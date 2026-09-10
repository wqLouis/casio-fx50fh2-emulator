# fx-lsp

A stdio [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
server for **CASIO fx-50FH II PRGM** source (`.fx`).

It is a thin shell around the core interpreter crate: diagnostics come from
`casio_fx50fh2::lexer::lex` and `casio_fx50fh2::parser::parse`, and all of the
pure logic lives in [`src/logic.rs`](src/logic.rs) so it can be unit-tested
without a client.

## Capabilities

| LSP method | Behaviour |
| --- | --- |
| `textDocument/publishDiagnostics` | Lex + parse on open/change. Each error is published as an ERROR diagnostic whose code is the calculator label (`Syntax ERROR`, `Math ERROR`, …) and whose range is the offending byte offset mapped to line + UTF-16 column. |
| `textDocument/completion` | Context-free list of keywords (`If Then Else IfEnd For To Step Next While WhileEnd Break Goto Lbl ClrMemory ClrStat` …), functions (`sin( cos( tan( log( ln( √( ∛( Abs( Pol( Rec( Rnd(` …), variables (`A B C D X Y M Ans`), constants (`π e i`), operators and the ASCII aliases (`-> => disp <> <= >=`). |
| `textDocument/hover` | Markdown description of the token under the cursor. |
| `textDocument/documentSymbol` | One `Lbl N` symbol per jump label, with the label's range. Works even if the rest of the document has a syntax error. |

`initialize` advertises exactly these four capabilities plus full-text document
sync; nothing else.

### Deliberately out of scope

* **No semantic / runtime checking.** `Math ERROR`, `Go ERROR` (missing label)
  and similar are runtime concerns; this server only reports lex and parse
  errors.
* **No completion context filtering.** The keyboard vocabulary is small, so the
  whole list is always offered.
* **No formatting, rename, definition, or code actions.**
* Diagnostics are published for the open document only; there is no workspace
  indexing.

## Building and testing

```bash
# From the repository root. Use a private target dir to avoid clashing with
# the other workstreams.
CARGO_TARGET_DIR=/tmp/target-lsp cargo build -p fx-lsp
CARGO_TARGET_DIR=/tmp/target-lsp cargo test  -p fx-lsp
```

The unit tests cover the pure logic (offset ↔ position including UTF-16
surrogates, diagnostics, completion, hover, symbols) and
`tests/server.rs` boots the real binary, performs the `initialize` →
`didOpen` → `shutdown` sequence and asserts the published `Syntax ERROR`.

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
  cmd = { "/absolute/path/to/fx-lsp" }, -- or "fx-lsp" if it is on $PATH
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
      cmd = { "/absolute/path/to/fx-lsp" },
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
    { command: "/absolute/path/to/fx-lsp" },
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
CARGO_TARGET_DIR=/tmp/target-lsp cargo build -p fx-lsp
python3 crates/fx-lsp/scripts/smoke.py /tmp/target-lsp/debug/fx-lsp
```

Expected output:

```
initialize capabilities: {"completionProvider": {"triggerCharacters": ["("]}, "documentSymbolProvider": true, "hoverProvider": true, "textDocumentSync": 1}
diagnostic: Syntax ERROR {'end': {'character': 2, 'line': 0}, 'start': {'character': 1, 'line': 0}} Syntax ERROR at byte 1: unexpected character `@`
server exited with 0
```

> The same conversation is asserted by `tests/server.rs`, so `cargo test -p
> fx-lsp` covers it without needing Python.

## Layout

```
crates/fx-lsp/
├── src/lib.rs        crate root; re-exports `run_server`
├── src/logic.rs      all pure logic + unit tests
├── src/server.rs     tower-lsp stdio server (`run_server`)
├── src/main.rs        thin `fx-lsp` binary shim
├── tests/server.rs   end-to-end stdio smoke test
├── scripts/smoke.py  manual stdio check (standard library only)
└── README.md
```

## Launching

The server is normally started through the unified CLI:

```bash
fx50 lsp          # or: fx50 --lsp
```

The standalone `fx-lsp` binary remains available and simply calls
`fx_lsp::run_server()`; both use the same code path.

## Known deviation: shutdown relies on the client closing stdin

The server shuts down when its stdin reaches EOF. All real clients (VS Code,
Neovim, Helix) close the connection right after sending `exit`, so this is not
observable in practice. tower-lsp 0.20's `Server::serve` loop has no `exit`
hook, so a client that sends `exit` but keeps the pipe open will not
immediately terminate the process. See `docs/DECISIONS.md`.
