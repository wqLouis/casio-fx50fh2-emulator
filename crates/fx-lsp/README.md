# fx-lsp

A stdio [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
server for **both** CASIO fx-50FH II source languages.

| Language | ID | Extension | Source |
| --- | --- | --- | --- |
| PRGM | `fx` | `.fx` | The calculator's own keystroke language. |
| C-like | `fxc` | `.fxc` | The small C-like language that `fx-transpiler` lowers to PRGM. |

The client-supplied `languageId` is authoritative. When it is missing or
unrecognised the server falls back to the URI's extension: a case-insensitive
`.fxc` suffix selects `fxc`, anything else selects `fx`.

The server is a thin shell around the core interpreter and the transpiler:
PRGM diagnostics come from `casio_fx50fh2::compile`, C-like diagnostics from
`fx_transpiler::transpile_with_base` (including `#include` expansion). All of
the pure logic lives in [`src/logic.rs`](src/logic.rs) so it can be unit-tested
without a client.

## Capabilities

`initialize` advertises exactly these four capabilities plus full-text document
sync, with `(`, `#` and `.` as completion trigger characters:

| LSP method | PRGM (`fx`) | C-like (`fxc`) |
| --- | --- | --- |
| `textDocument/publishDiagnostics` | Lex, parse and mode-check via `compile`. | Transpile via `transpile_with_base`, including `#include` expansion. |
| `textDocument/completion` | The calculator's keyboard vocabulary. | The C-like grammar's keywords, built-ins, constants and operators. |
| `textDocument/hover` | Markdown description of the token under the cursor. | Markdown description of the identifier under the cursor. |
| `textDocument/documentSymbol` | One `Lbl N` symbol per jump label. | Every `label N` and declared variable. |

### Diagnostics

**PRGM.** `casio_fx50fh2::compile` lexes, parses and mode-checks the document.
Each error is published as an ERROR diagnostic whose code is the calculator
label (`Syntax ERROR`, `Mode ERROR`, `Math ERROR`, …) and whose range is the
offending byte offset mapped to line + UTF-16 column. A mode violation has no
single offending byte, so it is reported at the start of the document
(range 0..0).

**C-like.** `fx_transpiler::transpile_with_base` expands `#include` directives,
then lexes, parses, mode-validates and emits. A failure becomes a single ERROR
diagnostic from `fx-transpiler`:

* **`Transpile ERROR`** — a lex, parse or mode-validation problem in the
  document itself.
* **`Include ERROR`** — the failure involves another file: an `#include` that
  cannot be resolved or is circular, or a compile error inside an included
  library. The message names the offending path.

The range uses the error's 1-based line/column (`line-1`, `column-1 .. column`,
converted to UTF-16 columns) and falls back to the byte offset when those are
unknown. `#include` paths resolve relative to the document's directory, which
the server derives from the URI (a document without a directory resolves
against the current working directory). Because that resolution performs file
I/O, a missing include is reported as a diagnostic rather than being ignored;
it can never panic the server.

### Completion

**PRGM.** A context-free list of keywords (`If Then Else IfEnd For To Step Next
While WhileEnd Break Goto Lbl ClrMemory ClrStat` …), the mode directives
(`#mode`, `#mode COMP`, `#mode CMPLX`, `#mode BASE`, `#mode SD`, `#mode REG`),
functions (`sin( cos( tan( log( ln( √( ∛( Abs( Pol( Rec( Rnd(` …), variables
(`A B C D X Y M Ans`), constants (`π e i`), operators and the ASCII aliases
(`-> => disp <> <= >=`).

**C-like.** Keywords (`let if else while for break goto label print`),
`input()`, every built-in (`sqrt(`, `cbrt(`, `log(`, … — `log` takes one or two
arguments), `pi` and `e`, all 40 scientific constants as `phys.NAME` (with the
constant's name and menu code in the detail), the bare `phys.` namespace,
operators (`== != <= >= < > + - * / ^ ** = ;`), `#mode`/`#mode NAME`, and an
`#include ""` snippet. The two lists are kept separate, so PRGM-only items
(`IfEnd`, `Lbl`, …) never appear for a `.fxc` document and vice versa.

### Hover

**PRGM.** The markdown description of the token under the cursor.

**C-like.** Hover is tokenizer-independent: the identifier span under the cursor
is found directly, so it still works while the document is incomplete. It is
classified as follows:

* `phys.NAME` resolves to the scientific constant (description, ASCII name,
  display symbol and menu code).
* `phys` describes the constant namespace.
* `.fxc` keywords, `pi`/`e`, `input`, and built-ins describe themselves.
* Anything else is an ordinary variable. In particular a bare `hbar` is a
  variable (with a hint to write `phys.hbar`), not the reduced Planck constant —
  only `phys.hbar` is.

### Document symbols

**PRGM.** One `Lbl N` symbol per jump label, with the label's range. Works even
if the rest of the document has a syntax error.

**C-like.** Lexes and parses the document, then walks the AST (recursing through
`if`/`while`/`for`/blocks) emitting every `label N` as a function symbol and
every declared variable as a VARIABLE symbol. Names are de-duplicated (the first
occurrence wins) and symbols are sorted by position. A document that does not
lex or parse yields no symbols instead of an error.

### Deliberately out of scope

* **No runtime checking.** `Math ERROR`, `Go ERROR` (missing label), a `break`
  outside a loop and similar are runtime concerns; this server only reports
  lex, parse and mode errors. Mode checking *is* static: `#mode
  COMP|CMPLX|BASE|SD|REG` selects the mode (COMP is the default) and, for PRGM,
  complex constructs outside CMPLX, statistics outside SD/REG, or base-n
  outside BASE are reported as `Mode ERROR`. For `.fxc`, BASE rejects every
  floating-point built-in and `pi`/`e` at transpile time.
* **No completion context filtering.** Both vocabularies are small, so the whole
  list is always offered.
* **No formatting, rename, definition, or code actions.**
* Diagnostics are published for the open document only; there is no workspace
  indexing.

## Building and testing

```bash
# From the repository root. `cargo build` produces the unified `fx50` binary.
cargo build
cargo test -p fx-lsp        # unit tests plus tests/fxc.rs
```

The unit tests cover the pure logic (offset ↔ position including UTF-16
surrogates, PRGM diagnostics, PRGM completion/hover/symbols, the `Language`
enum and `range_from_line_col`). The `tests/fxc.rs` integration suite covers
the C-like path: language selection, `.fxc` diagnostics (including a real
`#include` resolved in a temp directory and a missing include reported as
`Include ERROR`), completion, hover, document symbols and `range_from_line_col`
edge cases. The end-to-end stdio test lives with the binary in
`crates/fx-cli/tests/lsp_server.rs`, because it drives the real `fx50 lsp`
process rather than an internal entry point.

## Editor setup

Editor integrations (VS Code, Zed, Neovim, …) and the tree-sitter grammars live
in the repository's [`editors/`](../../editors) directory — see
[`editors/README.md`](../../editors/README.md) for the shared contract and
per-editor setup. The server itself is started through the unified CLI:

```bash
fx50 lsp          # also: fx50 --lsp, or add --stdio
```

## Manual end-to-end check

The server speaks framed JSON-RPC on stdin/stdout. `scripts/smoke.py` drives a
complete conversation (`initialize` → `didOpen` with the invalid program
`A@B` → `shutdown` → `exit`) and prints the replies:

```bash
cd /path/to/casio-fx50fh2-emulator
cargo build
python3 crates/fx-lsp/scripts/smoke.py target/debug/fx50
```

Expected output:

```
initialize capabilities: {"completionProvider": {"triggerCharacters": ["(", "#", "."]}, "documentSymbolProvider": true, "hoverProvider": true, "textDocumentSync": 1}
diagnostic: Syntax ERROR {'end': {'character': 2, 'line': 0}, 'start': {'character': 1, 'line': 0}} Syntax ERROR at byte 1: unexpected character `@`
server exited with 0
```

> The same conversation is asserted by `crates/fx-cli/tests/lsp_server.rs`, so
> `cargo test` covers it without needing Python.

## Layout

```
crates/fx-lsp/
├── src/lib.rs        crate root; re-exports `Language` and `run_server`
├── src/logic.rs      all pure logic + unit tests
├── src/server.rs     tower-lsp stdio server (`run_server`)
├── tests/fxc.rs      integration tests for the C-like language
├── scripts/smoke.py  manual stdio check (standard library only)
└── README.md
```

This crate has no binary of its own: it is a library, and the `fx50` binary
calls `fx_lsp::run_server()` for its `lsp` subcommand.

## Known deviation: shutdown relies on the client closing stdin

The server shuts down when its stdin reaches EOF. All real clients (VS Code,
Neovim, Helix, Zed) close the connection right after sending `exit`, so this is
not observable in practice. tower-lsp 0.20's `Server::serve` loop has no `exit`
hook, so a client that sends `exit` but keeps the pipe open will not
immediately terminate the process. See `docs/DECISIONS.md`.
