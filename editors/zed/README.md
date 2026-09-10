# CASIO fx-50FH II — Zed extension

Zed language support for the CASIO fx-50FH II:

| Language | Grammar | Files | Description |
| --- | --- | --- | --- |
| `fx` | `editors/tree-sitter-fx` | `*.fx` | the on-calculator PRGM language |
| `fxc` | `editors/tree-sitter-fxc` | `*.fxc` | the C-like source that transpiles to PRGM |

It provides:

* syntax highlighting (`queries/highlights.scm` in each grammar),
* bracket matching and comment configuration,
* the `fx50` language server, launched as **`fx50 lsp`**, attached to both
  languages.

## Requirements

The language server is the repository's own `fx50` binary. Build it from the
repository root:

```bash
cargo build --release     # produces target/release/fx50
# or
cargo build               # produces target/debug/fx50
```

The LSP is behind the `lsp` feature; the workspace's default build enables it
(see `crates/fx-cli`). If your build does not include it, use:

```bash
cargo build --release -p fx50 --features lsp
```

## How the server binary is located

`language_server_command` resolves `fx50` in this order:

1. a worktree-local build:
   `<worktree root>/target/release/fx50`, then
   `<worktree root>/target/debug/fx50` (and the `.exe` variants on Windows);
2. `fx50` found on `$PATH` (via Zed's `Worktree::which`);
3. the bare name `fx50`, resolved by the operating system when the server is
   spawned.

The command is always started with the single argument `lsp`, i.e. `fx50 lsp`.

## Installing (dev extension)

1. Build `fx50` (above) so that `target/release/fx50` or `target/debug/fx50`
   exists in the repository you open in Zed, or install `fx50` on your `PATH`.
2. In Zed, run the `zed: extensions` action to open the Extensions page.
3. Click **Install Dev Extension** and choose this directory,
   `editors/zed`.
4. Open a `.fx` or `.fxc` file. Zed should report the language in the status
   bar (`fx-50FH II PRGM` / `fx-50FH II C-like`).

If the server does not start, check Zed's log (`zed: open log`) while launching
Zed from a terminal with `zed --foreground`.

## `PLACEHOLDER_REV`

`extension.toml` registers both grammars against this repository:

```toml
[grammars.fx]
repository = "https://github.com/wqLouis/casio-fx50fh2-emulator"
rev = "PLACEHOLDER_REV"
path = "editors/tree-sitter-fx"
```

Zed needs a concrete Git revision for `rev`. After the commit that contains
`editors/tree-sitter-fx` and `editors/tree-sitter-fxc` is pushed, replace each
`PLACEHOLDER_REV` with that commit's full SHA:

```bash
git rev-parse HEAD
```

The `path` key points at the grammar subdirectory inside the repository, so a
single repository can host both grammars (and the extension).

## Files

```
editors/zed/
  extension.toml            manifest: id, grammars, language server
  Cargo.toml                wasm cdylib crate for the extension
  src/fx50.rs               Extension impl + binary resolution
  languages/fx/config.toml  language metadata for `fx`
  languages/fxc/config.toml language metadata for `fxc`
  README.md                 this file
```

## Notes

* The `fx` grammar accepts `//` line comments in `extras` so that the shipped
  `.fx` examples parse, even though the calculator's keypad has no comment key.
  Because of that, `languages/fx/config.toml` leaves `line_comments = []`
  (toggle-comment is disabled for `fx`); change it to `["// "]` if you would
  rather have toggle-comment insert `//`.
* `line_comments` in `[language_servers]` is `["fx", "fxc"]` as required by
  this repository's fixed contract. Zed's documentation notes that entries
  should match the `name` field of the language's `config.toml`; if the server
  does not attach, use the display names instead
  (`["fx-50FH II PRGM", "fx-50FH II C-like"]`).
