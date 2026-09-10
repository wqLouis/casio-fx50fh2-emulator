# fx-lsp

A stdio [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
server for **CASIO fx-50FH II PRGM** source (`.fx`).

It is a thin shell around the core interpreter crate: diagnostics come from
`casio_fx50fh2::compile` (lex, parse and mode-check), and all of the pure
logic lives in [`src/logic.rs`](src/logic.rs) so it can be unit-tested without
a client.

## Capabilities

| LSP method | Behaviour |
| --- | --- |
| `textDocument/publishDiagnostics` | Lex, parse and mode-check on open/change. Each error is published as an ERROR diagnostic whose code is the calculator label (`Syntax ERROR`, `Mode ERROR`, `Math ERROR`, …) and whose range is the offending byte offset mapped to line + UTF-16 column. A mode violation has no single offending byte, so it is reported at the start of the document (range 0..0). |
| `textDocument/completion` | Context-free list of keywords (`If Then Else IfEnd For To Step Next While WhileEnd Break Goto Lbl ClrMemory ClrStat` …), the mode directives (`#mode`, `#mode COMP`, `#mode CMPLX`, `#mode BASE`, `#mode SD`, `#mode REG`), functions (`sin( cos( tan( log( ln( √( ∛( Abs( Pol( Rec( Rnd(` …), variables (`A B C D X Y M Ans`), constants (`π e i`), operators and the ASCII aliases (`-> => disp <> <= >=`). |
| `textDocument/hover` | Markdown description of the token under the cursor. |
| `textDocument/documentSymbol` | One `Lbl N` symbol per jump label, with the label's range. Works even if the rest of the document has a syntax error. |

`initialize` advertises exactly these four capabilities plus full-text document
sync; nothing else.

### Deliberately out of scope

* **No runtime checking.** `Math ERROR`, `Go ERROR` (missing label) and
  similar are runtime concerns; this server only reports lex, parse and
  mode errors.  Mode checking *is* static: `#mode COMP|CMPLX|BASE|SD|REG`
  selects the calculator mode (COMP is the default) and complex constructs
  outside CMPLX, statistics outside SD/REG, or base-n outside BASE are
  reported as `Mode ERROR`.
* **No completion context filtering.** The keyboard vocabulary is small, so the
  whole list is always offered.
* **No formatting, rename, definition, or code actions.**
* Diagnostics are published for the open document only; there is no workspace
  indexing.

## Building and testing

```bash
# From the repository root. `cargo build` produces the unified `fx50` binary.
cargo build
cargo test -p fx-lsp        # the crate's own unit tests
```

The unit tests cover the pure logic (offset ↔ position including UTF-16
surrogates, diagnostics including mode checking, completion, hover, symbols).
The end-to-end stdio test lives with the binary in
`crates/fx-cli/tests/lsp_server.rs`, because it drives the real `fx50 lsp`
process rather than an internal entry point.

## Editor setup

### Neovim (built-in LSP, 0.10+)

Tell Neovim about the `fx` filetype and start the server on demand:

```lua
-- ~/.config/nvim/after/ftdetect/fx.lua
vim.filetype.add({ extension = { fx = "fx" } })
```

```lua
-- ~/.config/nvim/after/ftplugin/fx.lua
vim.lsp.start({
  name = "fx-lsp",
  -- `fx50` is the single binary; `lsp` selects language-server mode.
  cmd = { "/absolute/path/to/fx50", "lsp" }, -- or "fx50" if it is on $PATH
  root_dir = vim.fn.getcwd(),
})
```

If you use `nvim-lspconfig`, the equivalent custom-server entry is:

```lua
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")
if not configs.fx_lsp then
  configs.fx_lsp = {
    default_config = {
      cmd = { "/absolute/path/to/fx50", "lsp" },
      filetypes = { "fx" },
      root_dir = lspconfig.util.root_pattern(".git"),
    },
  }
end
lspconfig.fx_lsp.setup({})
```

### VS Code

Add a tiny extension that launches the binary with
`vscode-languageclient`. `package.json`:

```json
{
  "name": "fx-lsp-client",
  "version": "0.1.0",
  "engines": { "vscode": "^1.80.0" },
  "activationEvents": ["onLanguage:fx"],
  "contributes": {
    "languages": [{ "id": "fx", "extensions": [".fx"] }]
  },
  "main": "./extension.js",
  "dependencies": { "vscode-languageclient": "^9.0.0" }
}
```

`extension.js`:

```js
const { LanguageClient } = require("vscode-languageclient/node");

let client;
function activate() {
  client = new LanguageClient(
    "fx-lsp",
    "fx-50FH II",
    { command: "/absolute/path/to/fx50", args: ["lsp"] },
    { documentSelector: [{ scheme: "file", language: "fx" }] }
  );
  client.start();
}
function deactivate() {
  return client ? client.stop() : undefined;
}
module.exports = { activate, deactivate };
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
initialize capabilities: {"completionProvider": {"triggerCharacters": ["("]}, "documentSymbolProvider": true, "hoverProvider": true, "textDocumentSync": 1}
diagnostic: Syntax ERROR {'end': {'character': 2, 'line': 0}, 'start': {'character': 1, 'line': 0}} Syntax ERROR at byte 1: unexpected character `@`
server exited with 0
```

> The same conversation is asserted by `crates/fx-cli/tests/lsp_server.rs`, so
> `cargo test` covers it without needing Python.

## Layout

```
crates/fx-lsp/
├── src/lib.rs        crate root; re-exports `run_server`
├── src/logic.rs      all pure logic + unit tests
├── src/server.rs     tower-lsp stdio server (`run_server`)
├── scripts/smoke.py  manual stdio check (standard library only)
└── README.md
```

This crate has no binary of its own: it is a library, and the `fx50` binary
calls `fx_lsp::run_server()` for its `lsp` subcommand.

## Launching

The server is started through the unified CLI:

```bash
fx50 lsp          # or: fx50 --lsp
```

## Known deviation: shutdown relies on the client closing stdin

The server shuts down when its stdin reaches EOF. All real clients (VS Code,
Neovim, Helix) close the connection right after sending `exit`, so this is not
observable in practice. tower-lsp 0.20's `Server::serve` loop has no `exit`
hook, so a client that sends `exit` but keeps the pipe open will not
immediately terminate the process. See `docs/DECISIONS.md`.
