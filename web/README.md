# web — the browser workbench

A page that lets you write `.fxc`, watch it become PRGM, and run it on the
emulator. The transpiler, interpreter and editor logic are the same Rust crates
the `fx50` CLI and the editor extensions use, compiled to WebAssembly. **Nothing
is reimplemented in JavaScript.**

```bash
rustup target add wasm32-unknown-unknown   # once
./web/build.sh                             # build the module and bundle the examples

python3 -m http.server 8000                # from the repository root
# then open http://localhost:8000/web/
```

A static server is required — `file://` will not work, because the page fetches
the `.wasm` and loads Monaco as a module.

## What it does

* **Edit** `.fxc` with syntax highlighting, completions and hover, all answered
  by the wasm module (the same `fx_lsp::logic` the VS Code and Zed extensions
  call).
* **See the PRGM** the transpiler produces, live, as you type.
* **See the cost** — bytes used out of the machine's 680, and how many the
  optimiser saved.
* **See the memory plan** — which of `A B C D X Y M` each variable got, and
  which memories were freed and handed on.
* **Run** with values for the `?` prompts, and see the displays, every memory and
  `Ans` exactly as the calculator would show them (including its 10-digit
  rounding).
* **Run tests** — each example carries its own `#tests` table.
* **Browse the 40 constants**, switch between examples, and open the `lib/`
  files that `#include` resolves against.

## The files

| File | |
| --- | --- |
| `index.html`, `style.css` | the page |
| `app.js` | editor setup and the wiring between the page and the module |
| `fx50.js` | the JavaScript side of the wasm ABI — no domain logic |
| `test.mjs` | end-to-end tests, run under Node |
| `build.sh` | builds `fx_wasm.wasm` and generates `examples.js` |
| `fx_wasm.wasm` | **generated** — the module |
| `examples.js` | **generated** from `examples/*.fxc` |

Both generated files are in `.gitignore`. The examples are generated rather than
copied so the page can never show a stale or hand-edited version of a program the
test suite checks; `build.sh` re-reads `examples/*.fxc` every time.

## Testing

```bash
./web/build.sh && node web/test.mjs
```

This is the only place the real artifact — the `.wasm` file a browser downloads —
is exercised. A `cargo test` cannot catch a broken export, a wrong length prefix,
or memory freed twice; instantiating the module and calling it can. Node has a
full `WebAssembly` implementation, so no browser is needed, and the suite
includes running the embedded `#tests` of **every** bundled example.

It has already earned its keep: it is what caught the test runner still reading
`#include`d libraries from the filesystem, which worked locally and failed in
wasm.

## Notes on the design

**No `wasm-bindgen`.** The interface is a JSON string in and a JSON string out
over three `extern "C"` functions, so the only prerequisite is a rustup target.
`wasm-bindgen` would add a dependency *and* a CLI whose version must match the
crate exactly, for a boundary that is already strings. It also means the module
is a plain `wasm32-unknown-unknown` binary with **no imports at all** — a test
asserts that — which any host can instantiate. See
[ADR 0031](../docs/DECISIONS.md).

**No fallback editor logic.** If Monaco cannot be fetched the page says so and
switches to a plain textarea. Transpiling, running and diagnostics still work,
because none of them need an editor.

**Size.** The module is about 640 KB, 220 KB gzipped. The floor is the
interpreter and transpiler themselves (about 453 KB); the editor layer adds about
185 KB, most of it `serde`/`serde_json`/`url` pulled in by `lsp-types`.

## Embedding it elsewhere

`fx50.js` is standalone — copy it and `fx_wasm.wasm` and you have the whole
toolkit:

```js
import { load } from "./fx50.js";

const fx = await load("./fx_wasm.wasm");

fx.transpile("fn main() { print(2 + 2); }").prgm;   // "4◢"
fx.run("fn main() { let a = input(); print(a * 2); }", { inputs: [21] }).outputs;
// ["42"]

// A whole project, with `#include` resolved from the map rather than the disk:
fx.transpile('#include "lib/x.fxc"\nfn main() { print(one()); }', {
  entry: "main.fxc",
  files: { "lib/x.fxc": "fn one() = 1;" },
}).prgm;
```

The operations and every request/response shape are documented in
[`crates/fx-wasm/src/api.rs`](../crates/fx-wasm/src/api.rs).
