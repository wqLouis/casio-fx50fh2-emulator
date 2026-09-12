//! The C ABI a wasm host talks to.
//!
//! Three exports, and nothing else:
//!
//! * `fx_alloc(len)` — reserve `len` bytes for a request.
//! * `fx_free(ptr, len)` — release them.
//! * `fx_call(ptr, len)` — handle the request and return a response buffer.
//!
//! A response buffer is `[len: u32 little-endian][json bytes]`, so the host can
//! read the length without knowing anything else about it; it then calls
//! `fx_free` on the whole thing. Self-describing is worth the four bytes: the
//! alternative is a second export to ask how long the last response was, which
//! is both stateful and racy.
//!
//! # Why raw exports rather than `wasm-bindgen`
//!
//! The interface is strings and numbers, which the WASM ABI already has, and
//! serialising both is what `serde` and `serde_json` are for — both already in
//! the dependency graph for the language logic's sake (ADR 0031). Using
//! `wasm-bindgen` would add a dependency *and* require its CLI at a version that
//! must match the crate exactly — for a boundary that is a JSON string in and a
//! JSON string out. This way the artifact is a plain
//! `wasm32-unknown-unknown` module that any host can `WebAssembly.instantiate`
//! with no tooling at all, which is also what makes it testable from Node.
//!
//! The whole module works on the host too, so `cargo test` exercises the real
//! pointer dance rather than a stand-in.

use crate::api;

/// Reserve `len` bytes for a request.
///
/// The host writes the request there and passes the pointer to [`fx_call`]. The
/// block is a leaked `Vec`, so it must eventually be released with [`fx_free`]
/// and must not have been allocated any other way.
///
/// # Safety
///
/// The returned pointer is valid for exactly `len` bytes and must be freed with
/// [`fx_free`] using the same `len`.
#[unsafe(no_mangle)]
pub extern "C" fn fx_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// Release a block from [`fx_alloc`] (or a response from [`fx_call`]).
///
/// # Safety
///
/// `pointer` must have come from [`fx_alloc`] or [`fx_call`] and `len` must be
/// the length it was allocated with — `len + 4` for a response, because of the
/// length prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fx_free(pointer: *mut u8, len: usize) {
    if pointer.is_null() {
        return;
    }
    // SAFETY: the caller guarantees `pointer` came from a leaked `Vec<u8>` of
    // this length, so reconstructing it here takes back exactly what was leaked.
    unsafe {
        drop(Vec::from_raw_parts(pointer, len, len));
    }
}

/// Handle the request in `[pointer, pointer + len)` and return a response.
///
/// # Safety
///
/// `pointer` must be a block of at least `len` initialised bytes, as returned by
/// [`fx_alloc`]. The response must be freed with `fx_free(response, len + 4)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fx_call(pointer: *const u8, len: usize) -> *mut u8 {
    // SAFETY: the caller guarantees `pointer` is `len` initialised bytes.
    let request = unsafe { std::slice::from_raw_parts(pointer, len) };
    // A request that is not UTF-8 is not a JSON request. Saying so beats
    // panicking across the boundary, which would trap the module and lose the
    // host's whole session.
    let response = match std::str::from_utf8(request) {
        Ok(text) => api::call(text),
        Err(e) => {
            format!(r#"{{"ok":false,"error":{{"message":"request was not valid UTF-8: {e}"}}}}"#)
        }
    };
    pack(&response)
}

/// A response buffer: a `u32` little-endian length, then the JSON bytes.
fn pack(text: &str) -> *mut u8 {
    let bytes = text.as_bytes();
    let total = 4 + bytes.len();
    let mut buffer = Vec::<u8>::with_capacity(total);
    buffer.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buffer.extend_from_slice(bytes);
    debug_assert_eq!(buffer.len(), total);
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Send one request through the real ABI and read the response back.
    fn round_trip(request: &str) -> String {
        let bytes = request.as_bytes();
        let input = fx_alloc(bytes.len());
        // SAFETY: `input` is `bytes.len()` writable bytes from `fx_alloc`.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), input, bytes.len()) };
        // SAFETY: `input`/`bytes.len()` are a valid request block.
        let response = unsafe { fx_call(input, bytes.len()) };
        // SAFETY: `input` came from `fx_alloc` with this length.
        unsafe { fx_free(input, bytes.len()) };

        // SAFETY: `response` is a packed buffer; its first four bytes are the
        // length of what follows.
        let header = unsafe { std::slice::from_raw_parts(response, 4) };
        let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
        // SAFETY: the buffer holds `4 + length` bytes by construction.
        let body = unsafe { std::slice::from_raw_parts(response.add(4), length) };
        let text = String::from_utf8(body.to_vec()).expect("response is UTF-8");
        // SAFETY: `response` came from `fx_call`, allocated with `4 + length`.
        unsafe { fx_free(response, 4 + length) };
        text
    }

    #[test]
    fn the_length_prefix_describes_the_body() {
        let text = round_trip(r#"{"op":"version"}"#);
        assert!(text.starts_with('{'), "{text}");
        assert!(text.contains("\"ok\":true"), "{text}");
        assert!(text.contains("programKeys"), "{text}");
    }

    #[test]
    fn a_malformed_request_comes_back_as_json_not_a_trap() {
        let text = round_trip("{not json");
        assert!(text.contains("\"ok\":false"), "{text}");
        assert!(text.contains("malformed"), "{text}");
    }

    #[test]
    fn an_empty_request_is_reported_rather_than_panicking() {
        let text = round_trip("");
        assert!(text.contains("\"ok\":false"), "{text}");
    }

    #[test]
    fn freeing_a_null_pointer_is_harmless() {
        // A host that failed to allocate may still call `fx_free`.
        // SAFETY: a null pointer is explicitly allowed here.
        unsafe { fx_free(std::ptr::null_mut(), 0) };
    }

    #[test]
    fn a_request_that_is_not_utf8_is_reported() {
        let bytes = [0xff, 0xfe, 0xfd];
        let input = fx_alloc(bytes.len());
        // SAFETY: `input` is `bytes.len()` writable bytes.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), input, bytes.len()) };
        // SAFETY: valid request block.
        let response = unsafe { fx_call(input, bytes.len()) };
        // SAFETY: from `fx_alloc`.
        unsafe { fx_free(input, bytes.len()) };

        // SAFETY: packed response, four-byte length prefix.
        let header = unsafe { std::slice::from_raw_parts(response, 4) };
        let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
        // SAFETY: `4 + length` bytes by construction.
        let body = unsafe { std::slice::from_raw_parts(response.add(4), length) };
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("valid UTF-8"), "{text}");
        // SAFETY: from `fx_call`.
        unsafe { fx_free(response, 4 + length) };
    }
}
