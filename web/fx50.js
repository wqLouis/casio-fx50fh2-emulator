// The JavaScript side of the fx-50FH II WebAssembly module.
//
// The module exports exactly three functions — `fx_alloc`, `fx_free` and
// `fx_call` — and this file is the whole protocol between them and JavaScript:
//
//   1. encode the request as UTF-8 and `fx_alloc` that many bytes,
//   2. write the bytes into the module's memory,
//   3. `fx_call(pointer, length)` and take the returned buffer,
//   4. read its 4-byte little-endian length prefix and decode the rest,
//   5. `fx_free` both buffers.
//
// The response is always JSON, so `call` parses it and hands back an object.
// Nothing here knows what a transpiler is; the operations are documented in
// `crates/fx-wasm/src/api.rs`.

/** A loaded module, with a `call` bound to its exports. */
export class Fx50 {
  #exports;

  constructor(exports) {
    this.#exports = exports;
  }

  /**
   * Send one request and return the parsed response.
   *
   * @param {object} request an object with an `op`, e.g.
   *   `{ op: "transpile", source: "fn main() { print(1); }" }`
   * @returns {object} the response, with `ok` set
   */
  call(request) {
    return JSON.parse(this.callRaw(request));
  }

  /**
   * Send one request and return the response text.
   *
   * Used by tests and by the page's "raw" view; `call` is what normal code
   * wants.
   */
  callRaw(request) {
    const { fx_alloc, fx_free, fx_call, memory } = this.#exports;
    const encoded = new TextEncoder().encode(JSON.stringify(request));

    // `memory.buffer` is re-read at every use: allocating can grow the memory,
    // which detaches any view taken before it.
    const input = fx_alloc(encoded.length);
    new Uint8Array(memory.buffer, input, encoded.length).set(encoded);

    const output = fx_call(input, encoded.length);
    fx_free(input, encoded.length);

    const length = new DataView(memory.buffer, output).getUint32(0, true);
    const text = new TextDecoder().decode(
      new Uint8Array(memory.buffer, output + 4, length),
    );
    fx_free(output, length + 4);

    return text;
  }

  /** What the module is, and the limits of the machine it models. */
  version() {
    return this.call({ op: "version" });
  }

  /** `.fxc` → PRGM. `options` may carry `entry`, `files`, `mode`, `ascii`, `optimize`. */
  transpile(source, options = {}) {
    return this.call({ op: "transpile", source, ...options });
  }

  /** Transpile if needed, run, and report the calculator's displays and state. */
  run(source, options = {}) {
    return this.call({ op: "run", source, ...options });
  }

  /** Run a program's embedded `#tests` table. */
  tests(source, options = {}) {
    return this.call({ op: "tests", source, ...options });
  }

  /** Editor markers, positioned for Monaco or any LSP-style client. */
  diagnostics(source, options = {}) {
    return this.call({ op: "diagnostics", source, ...options });
  }

  /** The completion list for `fxc` (default) or `fx`. */
  completions(language = "fxc") {
    return this.call({ op: "completions", language });
  }

  /** Documentation at a 0-based position. */
  hover(source, line, character, options = {}) {
    return this.call({
      op: "hover",
      source,
      position: { line, character },
      ...options,
    });
  }

  /** The document outline. */
  symbols(source, options = {}) {
    return this.call({ op: "symbols", source, ...options });
  }

  /** The 40 scientific constants. */
  constants() {
    return this.call({ op: "constants" });
  }

  /** Evaluate one expression, without a program around it. */
  evaluate(expression, options = {}) {
    return this.call({ op: "eval", source: expression, ...options });
  }
}

/**
 * Instantiate the module.
 *
 * `input` may be an `ArrayBuffer`, a typed array, or a URL to fetch. Use
 * `WebAssembly.instantiate` rather than `instantiateStreaming`: the latter needs
 * the server to send `application/wasm`, which a plain static file server often
 * does not, and the failure is a confusing one.
 *
 * @param {ArrayBuffer|ArrayBufferView|string|URL} input
 * @returns {Promise<Fx50>}
 */
export async function load(input) {
  const bytes =
    typeof input === "string" || input instanceof URL
      ? await (await fetch(input)).arrayBuffer()
      : input;

  // No imports: the module needs nothing from its host.
  const { instance } = await WebAssembly.instantiate(bytes, {});
  return new Fx50(instance.exports);
}

export default load;
