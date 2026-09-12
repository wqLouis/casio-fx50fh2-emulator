//! A WebAssembly build of the fx-50FH II toolkit: the transpiler, the
//! interpreter, and the same language logic the editor extensions use.
//!
//! The point of this crate is to make the whole toolkit usable where there is no
//! filesystem and no process — a web page. `fx50` needs a disk to read
//! `#include`d libraries from and stdin for the `?` prompts; a browser has
//! neither, so this crate supplies both from JavaScript and reuses everything
//! else unchanged.
//!
//! Three layers, so that the interesting part is testable on the host:
//!
//! * [`api`] — every operation as `&str -> String`, plain Rust, no wasm types.
//!   This is where the behaviour lives, and `cargo test` covers it directly.
//! * `abi` — the three `extern "C"` exports, and the pointer dance.
//! * `mod.js`/the web page — reads the JSON and draws it.
//!
//! # Calling it
//!
//! ```js
//! const { instance } = await WebAssembly.instantiate(bytes, {});
//! const { fx_alloc, fx_free, fx_call, memory } = instance.exports;
//!
//! function call(request) {
//!   const bytes = new TextEncoder().encode(JSON.stringify(request));
//!   const input = fx_alloc(bytes.length);
//!   new Uint8Array(memory.buffer, input, bytes.length).set(bytes);
//!   const response = fx_call(input, bytes.length);
//!   fx_free(input, bytes.length);
//!
//!   const view = new DataView(memory.buffer, response);
//!   const length = view.getUint32(0, true);
//!   const text = new TextDecoder().decode(
//!     new Uint8Array(memory.buffer, response + 4, length));
//!   fx_free(response, length + 4);
//!   return JSON.parse(text);
//! }
//!
//! call({ op: "transpile", source: "fn main() { print(2 + 2); }" });
//! ```
//!
//! The operations and the request/response shapes are documented on [`api`].

mod abi;
pub mod api;
