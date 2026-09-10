# Editor support

The `fx50` binary is a language server, so the same executable provides
diagnostics, completion, hover and document symbols in any LSP-capable editor.
This directory holds ready-made integrations.

| Editor | Directory | What it adds |
| --- | --- | --- |
| VS Code | [`vscode/`](vscode) | TypeScript client extension + TextMate grammars |
| Zed | [`zed/`](zed) | Zed extension + language configs, with tree-sitter grammars in [`tree-sitter-fx/`](tree-sitter-fx) and [`tree-sitter-fxc/`](tree-sitter-fxc) |
| Neovim, Helix, Emacs, … | — | point your client at `fx50 lsp` (see below) |

## The contract

Every editor integration talks to the same process and uses the same two
language ids:

| Language | ID | Extension |
| --- | --- | --- |
| PRGM — what the calculator runs | `fx` | `.fx` |
| C-like source for the transpiler | `fxc` | `.fxc` |

```console
$ fx50 lsp
```

The server reads the language id from each `textDocument/didOpen` and routes to
the right front end: PRGM through the interpreter's own lexer/parser/mode
checker, `.fxc` through `fx-transpiler` (including `#include` resolution). If a
client does not send a language id, the server falls back to the file
extension, so the wiring is forgiving.

`--stdio` is accepted and ignored, because many clients append it.

## Building the server

All of these need the `fx50` binary. From the repository root:

```bash
cargo build            # -> target/debug/fx50
cargo build --release  # -> target/release/fx50
```

Put it on `PATH` (so editors find it by name), or point the editor's server
setting at the absolute path. The VS Code and Zed integrations both also look
for `target/release/fx50` / `target/debug/fx50` inside the opened workspace, so
developing in this repository works without installing anything.

## Neovim (built-in LSP)

Neovim 0.10+ needs no plugin. Register the filetypes and start the server on
demand:

```lua
-- ~/.config/nvim/after/ftdetect/fx.lua
vim.filetype.add({
  extension = { fx = "fx", fxc = "fxc" },
})
```

```lua
-- ~/.config/nvim/after/ftplugin/fx.lua   (and fx.fxc.lua)
vim.lsp.start({
  name = "fx-lsp",
  cmd = { "fx50", "lsp" },          -- or an absolute path to the binary
  root_dir = vim.fn.getcwd(),
})
```

With `nvim-lspconfig`, add a custom server instead:

```lua
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")
if not configs.fx_lsp then
  configs.fx_lsp = {
    default_config = {
      cmd = { "fx50", "lsp" },
      filetypes = { "fx", "fxc" },
      root_dir = lspconfig.util.root_pattern(".git"),
    },
  }
end
lspconfig.fx_lsp.setup({})
```

## Grammars

Zed requires a tree-sitter grammar for highlighting; the two grammars here are
ordinary tree-sitter grammars, so they also work with Neovim's
`nvim-treesitter` (or directly) if you want parser-based highlighting there.

Both parse every program shipped in [`examples/`](../examples) with no error
nodes:

```bash
cd editors/tree-sitter-fx && tree-sitter parse ../../examples/factorial.fx
cd editors/tree-sitter-fxc && tree-sitter parse ../../examples/factorial.fxc
```

| Grammar | Language | Highlights |
| --- | --- | --- |
| [`tree-sitter-fx/`](tree-sitter-fx) | `fx` (PRGM) | keywords, functions, statistics variables, base-n literals, `#mode`, calculator glyphs |
| [`tree-sitter-fxc/`](tree-sitter-fxc) | `fxc` | comments, `#mode`/`#include` directives, keywords, built-ins, `phys.` constants |

## Other clients

Anything that can run a command over stdio works. Configure it as:

* **command**: `fx50 lsp` (or `fx50 --lsp`, or add `--stdio`)
* **language ids**: `fx`, `fxc`
* **file extensions**: `.fx`, `.fxc`
