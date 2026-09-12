# casio-fx50fh2

An interpreter, a C-like transpiler and a language server for the **CASIO
fx-50FH II** programmable calculator.

It runs the calculator's own **PRGM** keystroke language, compiles a friendlier
**C-like language** (`.fxc`) down to it, and serves both through one language
server — so you can write, check and run calculator programs in your editor or
from the command line.

```console
$ fx50 eval --mode CMPLX "(3+4i)×(1-2i)"
11-2𝑖
$ echo 5 | fx50 run examples/factorial.fxc
120
```

## Build

Requires a Rust toolchain new enough for edition 2024 (**Rust 1.85+**).

```bash
git clone https://github.com/wqLouis/casio-fx50fh2-emulator
cd casio-fx50fh2-emulator

cargo build --release     # -> target/release/fx50
```

`cargo build` at the repository root builds everything; the workspace produces
exactly one binary.

To put it on your `PATH`:

```bash
# from a clone:
cargo install --path crates/fx-cli

# or without cloning:
cargo install --git https://github.com/wqLouis/casio-fx50fh2-emulator fx-cli
```

Either way you get `~/.cargo/bin/fx50`. Installing system-wide is also what the
editor extensions need — they cannot bundle the server, so they look for `fx50`
on your `PATH` (see [Editor support](#editor-support)).

During development `cargo run -- <args>` runs it without installing, and
`cargo test` runs the whole test suite. If `fx50` is not on `PATH`, the editor
extensions also look for `target/debug/fx50` or `target/release/fx50` inside the
workspace folder you open.

## Usage

```bash
# interactive REPL (state persists between lines, with history)
fx50

# run a program — inputs for `?` are read from stdin
fx50 run examples/factorial.fx
fx50 run examples/factorial.fxc      # C-like source is transpiled first

# evaluate a single expression
fx50 eval "2+3×4"
fx50 eval --mode CMPLX "(3+4i)×(1-2i)"

# transpile C-like source to PRGM
fx50 build program.fxc > program.fx
fx50 build --ascii program.fxc       # keyboard-typable output

# run the test cases a program carries
fx50 test examples/factorial.fxc

# show how a program uses the seven memories (A B C D X Y M)
fx50 regs examples/compiletime.fxc

# show how many of the 680 program bytes it needs
fx50 size examples/determinant.fxc

# language server over stdio, and shell completions
fx50 lsp
fx50 completions bash
```

Every command also has a flag form (`--eval/-e`, `--build/-b`, `--lsp/-l`).
Errors are rendered with a caret at the offending source position.

> Put options **before** the expression: `fx50 eval --mode CMPLX "…"`. Because
> `eval` accepts expressions that begin with `-` (as in `fx50 eval -5+3`), it
> treats everything after the subcommand as the expression, so a trailing
> `--mode` would be read as part of it.

## Editor support

`fx50 lsp` is a language server for **both** languages. It provides diagnostics
(including mode violations and `.fxc` `#include`/`#data` resolution),
completion, hover and document symbols.

The two languages use fixed ids, which every integration below relies on:

| Language | Editor language id | Files |
| --- | --- | --- |
| PRGM (what the calculator runs) | `fx` | `.fx` |
| C-like source for the transpiler | `fxc` | `.fxc` |

### VS Code

A client extension plus TextMate grammars live in
[`editors/vscode/`](editors/vscode).

The extension cannot ship the language server, so install `fx50` first — then
build and install the extension itself:

```bash
# 1. install the server (puts `fx50` in ~/.cargo/bin)
cargo install --git https://github.com/wqLouis/casio-fx50fh2-emulator fx-cli

# 2. build the extension
cd editors/vscode
npm install
npm run compile

# 3. package and install it
npx @vscode/vsce package        # -> fx50-vscode-<version>.vsix
code --install-extension fx50-vscode-*.vsix
```

Then open any `.fx` or `.fxc` file. The extension looks for the server in the
`fx50.serverPath` setting, then `PATH`, then the workspace's `target/`, so
working inside this repository needs no install at all. To iterate on the
extension, open `editors/vscode` and press **F5** ("Run Extension").

Full details: [`editors/vscode/README.md`](editors/vscode/README.md).

### Zed

A Zed extension with tree-sitter grammars for both languages lives in
[`editors/zed/`](editors/zed).

Zed extensions are WebAssembly modules and cannot bundle a binary, so install
`fx50` first (installing from a clone with `cargo install --path crates/fx-cli`
works too, as does `cargo build` if you are working in this repository), then
install the extension as a dev extension:

```bash
cargo install --git https://github.com/wqLouis/casio-fx50fh2-emulator fx-cli
```

1. In Zed, run the `zed: extensions` action.
2. Click **Install Dev Extension** and choose the `editors/zed` directory.
3. Open a `.fx` or `.fxc` file — the status bar should read
   `fx-50FH II PRGM` / `fx-50FH II C-like`.

If the server does not start, check `zed: open log` while launching Zed from a
terminal (`zed --foreground`). After changing the extension itself, run
`zed: reload extensions`.

Full details: [`editors/zed/README.md`](editors/zed/README.md).

### Neovim and other clients

Any LSP client works — point it at the command `fx50 lsp` with the filetypes
`fx` and `fxc`. Neovim 0.10+ needs no plugin; the exact config, plus notes for
using the tree-sitter grammars with `nvim-treesitter`, is in
[`editors/README.md`](editors/README.md).

## The two languages

**PRGM** is the calculator's native language: one key = one token, `→` to store,
`◢` to display.

```text
?→A
1→B
For 1→C To A Step 1
B×C→B
Next
B◢
```

**`.fxc`** is a C-like source language that transpiles to the above. It adds
`const` for compile-time values, `#data` for reading JSON while transpiling,
`free` for releasing one of the seven memories, and `#tests` for cases that
travel with the program. **Every PRGM key has an `.fxc` spelling** — the
postfix and infix keys (`sqr(x)`, `fact(x)`, `frac(a, b)`, `npr(n, r)`,
`polar(r, θ)`), the statistical values (`stat.meanx`), setup and data keys
(`fix(3);`, `dt(x, y);`), the bitwise words, `Ran#` as `ran()`, and the `⇒` key
as `cond => stmt;`. It also has **user-defined functions** (`fn`), inlined at
each call: **every program needs a `fn main()` entry point**, and every
function except `main` is scoped to its own parameters and locals. Constant
expressions are pre-calculated while transpiling — the machine has only 680
bytes of program storage — and a constant `for` loop that indexes an array is
unrolled so its indices become literals.

```c
// A program is a set of functions with a `fn main()` entry point. `const`
// values are visible everywhere and cost no memory.
const max_input = 12;

fn main() {
    let n = input();
    if (n > max_input) { n = max_input; }
    let result = 1;
    for (let i = 1; i <= n; i = i + 1) { result = result * i; }
    print(result);
}

#tests = [
  { "name": "5! = 120", "input": [5], "output": ["120"] }
];
```

## Run it in a browser

The whole toolkit also builds to WebAssembly, so the transpiler, the interpreter
and the editor features run on a web page with no server behind it:

```bash
rustup target add wasm32-unknown-unknown   # once
cd web
bun install
bun run build:wasm                         # Rust -> web/static/fx_wasm.wasm
bun run dev                                # then open the printed URL
```

The Rust and JavaScript sides build **independently**: `bun run build:wasm` is
only needed when `src/` or `crates/` changes, so working on the page needs no
Rust toolchain at all.

[`web/`](web) is a workbench, and it also serves the documentation you are
reading now as a website. Write `.fxc` with completions and hover, build it into
PRGM when you ask, see the bytes it costs out of the machine's 680, see which of
the seven memories each variable got, and run it with values for the `?` prompts.
The build is deliberately manual: the diagnostics stay live, but the listing, the
size and the memory plan only change when you press Build, and they say
`out of date` when the editor has moved on rather than quietly showing a
different program. There is a switch for the size optimiser, a terminal-like
REPL whose session remembers the mode and the memories, resizable and hideable
panels, and a light and dark theme. Each example carries its own `#tests` and
can be run from the page.

Nothing is reimplemented in JavaScript — it is the same Rust crates the CLI and
the editor extensions use, behind three `extern "C"` functions and a JSON string
([why not `wasm-bindgen`](docs/DECISIONS.md#adr-0031--the-webassembly-interface-is-a-json-string-over-three-exports)).

`crates/fx-wasm` is the module itself; [`web/README.md`](web/README.md) covers
the page, and `build/fx_wasm.wasm` can be dropped into any other site behind the
same three functions.

## Documentation

| Document | What it covers |
| --- | --- |
| [`docs/LANGUAGE.md`](docs/LANGUAGE.md) | **Language manual — PRGM**: tokens, modes, priority, runtime semantics, precision, base-n |
| [`docs/FXC.md`](docs/FXC.md) | **Language manual — `.fxc`**: grammar, statements, the memory model and `free`, functions, arrays, the optimiser, compile-time data, `#include`, `#tests` |
| [`docs/AI-AGENTS.md`](docs/AI-AGENTS.md) | Rule-oriented authoring guide for `.fxc`, written for AI agents (and a good checklist for anyone) |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Workspace layout, interpreter and transpiler pipelines, feature matrix |
| [`docs/DECISIONS.md`](docs/DECISIONS.md) | Design decisions and the alternatives rejected (ADRs) |
| [`docs/PLAN.md`](docs/PLAN.md) | The original specification, kept as a record |
| [`editors/README.md`](editors/README.md) | LSP contract, editor setup, grammars |
| [`web/README.md`](web/README.md) | The browser workbench: building the wasm module, testing it, embedding it |
| [`crates/fx-transpiler/README.md`](crates/fx-transpiler/README.md) | The transpiler crate: CLI, Rust API, tests |
| [`crates/fx-lsp/README.md`](crates/fx-lsp/README.md) | The language-server crate |
| [`crates/fx-wasm/README.md`](crates/fx-wasm/README.md) | The WebAssembly crate: the ABI, the operations |

## Examples

[`examples/`](examples) contains working programs.

* **PRGM** (`fx50 run <file>`): `factorial.fx`, `fibonacci.fx`, `gcd.fx`,
  `quadratic.fx`, `complex_quadratic.fx`, `statistics.fx`.
* **`.fxc`** (`fx50 run` / `build` / `test`): `factorial.fxc`, `quadratic.fxc`,
  `constants.fxc` (the 40 scientific constants), `compiletime.fxc` (`#data`,
  `const`, `free`), `arrays.fxc` (one memory per element), `determinant.fxc`
  (a 3x3 determinant against the seven-memory limit), `functions.fxc`
  (user-defined functions, inlined at each call), `packing.fxc` (eight reals in
  four memories, by packing each pair into a complex number), and `include.fxc`,
  which pulls in the [`examples/lib/`](examples/lib) libraries — including
  `pack.fxc`, the packing library itself. A library is a `.fxc` file of `fn`
  definitions with no `fn main()`; it builds on its own and is included at the top
  level.

The `.fxc` examples carry their own test cases, so they can be checked with:

```bash
fx50 test examples/factorial.fxc
```

## Tests

```bash
cargo test                          # everything
cargo test -p fx-transpiler --no-default-features   # transpiler with zero deps
cd web && bun test                   # the real .wasm module, under bun
```

The last one matters more than it looks: it is the only place the actual artifact
a browser downloads is exercised, and a `cargo test` cannot catch a broken export
or a wrong length prefix.

## License

MIT.
