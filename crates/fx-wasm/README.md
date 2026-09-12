# fx-wasm

A WebAssembly build of the whole fx-50FH II toolkit: the transpiler, the
interpreter, and the same language logic the editor extensions use.

It exists because a browser has no filesystem and no processes. `fx50` reads
`#include`d libraries from disk and `?` prompts from stdin; this crate supplies
both from JavaScript and reuses everything else unchanged. The browser workbench
in [`web/`](../../web) is its consumer.

```bash
rustup target add wasm32-unknown-unknown
cargo build --profile wasm --target wasm32-unknown-unknown -p fx-wasm
# -> target/wasm32-unknown-unknown/wasm/fx_wasm.wasm
```

`web` wraps that in `bun run build:wasm`, which also copies the module next to the
page.

## The interface

Three `extern "C"` functions, and a JSON string either way:

```rust
fx_alloc(len: usize) -> *mut u8
fx_free(ptr: *mut u8, len: usize)
fx_call(ptr: *const u8, len: usize) -> *mut u8   // [len: u32 LE][json]
```

There is deliberately no `wasm-bindgen` here (see [ADR 0031](../../docs/DECISIONS.md)):
the boundary is a JSON string, and serialising it is what `serde` and `serde_json`
are for — both already in the dependency graph for the language logic's sake. This
way the module is a plain `wasm32-unknown-unknown` binary with **no imports at
all** that any host can instantiate with no tooling. `web/src/lib/fx50.ts` is the
JavaScript side, and typed.

## Three layers

| Module | |
| --- | --- |
| [`api`](src/api.rs) | every operation as `&str -> String`. Plain Rust, no wasm types, so `cargo test` covers the behaviour directly. **Start here.** |
| [`abi`](src/abi.rs) | the exports and the pointer marshalling |
| `web/src/lib/fx50.ts` | the typed client the page imports |

## Operations

| `op` | does |
| --- | --- |
| `version` | what this build is, plus the machine's limits |
| `transpile` | `.fxc` → PRGM, with its size and memory plan |
| `run` | transpile if needed, run, and report the displays and state |
| `tests` | run a program's embedded `#tests` table |
| `diagnostics` | errors for the editor (markers) |
| `completions` | the completion list for a language |
| `hover` | documentation at a position |
| `symbols` | the document outline |
| `constants` | the 40 scientific constants |
| `eval` | one expression |
| `replOpen` | open an interactive session |
| `replEval` | run one entry in a session |
| `replReset` | clear a session's memories |
| `replClose` | forget a session |

### Interactive sessions

The calculator is stateful, and `eval` is not: it builds a fresh interpreter
for every call, so `#mode CMPLX` on one line is forgotten by the next and a
variable set on one line is gone. The `repl*` operations mirror `fx50`'s
interactive REPL instead, keeping the environment between entries:

```json
{ "op": "replOpen" }
{ "op": "replEval", "id": 1, "source": "5→A", "inputs": [] }
{ "op": "replReset", "id": 1 }
{ "op": "replClose", "id": 1 }
```

`replOpen` returns a unique `id`, and the other three address a session by it.
A line with no `#mode` of its own inherits the session's mode, so
`#mode CMPLX` followed by `3+4i` works as it does on the command line. `outputs`
are the `◢` displays and `state` is the same `MachineState` shape `eval`
returns. On failure the error is reported and the stored environment is left
untouched.

Every request takes the same envelope, and uses what it needs:

```json
{
  "op": "transpile",
  "source": "fn main() { print(1 + 1); }",
  "entry": "main.fxc",
  "files": { "lib/pack.fxc": "fn pack(x, y) = x + y * i();" },
  "mode": "COMP", "ascii": false, "optimize": true,
  "inputs": [3, 4]
}
```

`source` may be omitted when `entry` names a file inside `files`, which is how a
whole project is built. Positions in responses are **0-based** (LSP-style) so an
editor can use them directly; the CLI prints 1-based columns for humans.

The full shapes, and the reasoning behind them, are on the
[`api` module](src/api.rs).

## Tests

```bash
cargo test -p fx-wasm        # the API and the ABI, on the host
cd ../web && bun test        # the real .wasm module, under bun
```

Both matter. The host tests cover behaviour; only instantiating the actual
artifact catches a broken export, a wrong length prefix or a double free.
