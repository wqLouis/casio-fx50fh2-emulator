# CASIO fx-50FH II — Zed extension

Zed language support for the CASIO fx-50FH II:

| Language | Grammar | Files | Description |
| --- | --- | --- | --- |
| `fx` | `editors/tree-sitter-fx` | `*.fx` | the on-calculator PRGM language |
| `fxc` | `editors/tree-sitter-fxc` | `*.fxc` | the C-like source that transpiles to PRGM |

It provides:

* syntax highlighting. The queries live in `languages/<lang>/highlights.scm`, next
  to each language's `config.toml` — that is where Zed loads them from, **not**
  from the grammar checkout. They are kept byte-identical to
  `editors/tree-sitter-*/queries/highlights.scm` (which serves editors that read
  queries from the grammar), and `cargo test` asserts the copies match,
* bracket matching and auto-indentation for `.fxc`
  (`languages/fxc/brackets.scm`, `languages/fxc/indents.scm`),
* comment configuration for both languages,
* the `fx50` language server, launched as **`fx50 lsp`**, attached to both
  languages.

### Why `fx` has no `brackets.scm`

Zed's rainbow brackets need a pattern that captures the opening and closing
token as children of one node, e.g. `("(" @open ")" @close)`. The PRGM grammar
models `(` and `)` as flat tokens of an `expression`, each wrapped in its own
`expression` node, so no node has both as children and the pattern can never
match. Parentheses are still *highlighted* (`["(" ")"] @punctuation.bracket`
matches the tokens directly); only the pairing/rainbow colouring is missing.
Modelling a real `parenthesized_expression` in the grammar would enable it, at
the cost of regenerating the parser.

## Installing the language server

**Zed extensions cannot ship the server, so `fx50` has to be installed on your
system separately.** A Zed extension is a WebAssembly module: Zed compiles it to
`wasm32-wasip2`, and the only file-facing API is *downloading* a file at
runtime — there is no way to bundle an executable in the extension, and nowhere
in `extension.toml` to declare one. So the extension starts `fx50` by looking
for it (see [How the server binary is located](#how-the-server-binary-is-located)).

The easiest way to provide it is
[`cargo install`](https://doc.rust-lang.org/cargo/commands/cargo-install.html),
which puts `fx50` in `~/.cargo/bin` (make sure that is on your `PATH`):

```bash
# Straight from GitHub, no clone needed:
cargo install --git https://github.com/wqLouis/casio-fx50fh2-emulator fx-cli

# Or from a clone of this repository:
git clone https://github.com/wqLouis/casio-fx50fh2-emulator
cd casio-fx50fh2-emulator
cargo install --path crates/fx-cli
```

Either way you get a `fx50` binary; confirm it with `fx50 --version`.

If you are working *in this repository*, you do not need to install anything:
build with `cargo build` and the extension finds `target/debug/fx50` itself
(below). That is the workflow the
[dev-extension instructions](#installing-as-a-dev-extension) assume.

> Auto-downloading the server from a GitHub release, the way many Zed extensions
> do, is not wired up yet. When it is, this section becomes optional.

## How the server binary is located

A wasm extension cannot inspect the filesystem: Zed's `worktree` API offers
only `which`, `root-path`, `read-text-file`, `shell-env` and `id` — there is no
`stat`/`exists`, and `read-text-file` refuses absolute paths and only reads
text. So `language_server_command` resolves `fx50` in this order:

1. `fx50` on `$PATH` (via `Worktree::which`);
2. a build inside the opened worktree, detected by reading the dep-info file
   Cargo writes next to the binary (`target/release/fx50.d`, then
   `target/debug/fx50.d`);
3. a worktree whose root `Cargo.toml` names this project
   (`crates/fx-cli`), in which case `target/debug/fx50` is used. This is needed
   because `target/` is gitignored here, and the worktree file API cannot see
   ignored paths, so step 2 never fires in this repository itself;
4. otherwise a clear error explaining how to make the binary findable.

Steps 2 and 3 return an absolute path, so the server spawns regardless of the
working directory. The command is always started with the single argument
`lsp`, i.e. `fx50 lsp`.

## Installing as a dev extension

The extension is not on the Zed extension registry, so it is installed from
source as a **dev extension**. Either install `fx50` on your `PATH` (see
[Installing the language server](#installing-the-language-server)), or work in
this repository so the workspace build is found automatically.

1. Provide `fx50`: `cargo install --git https://github.com/wqLouis/casio-fx50fh2-emulator fx-cli`,
   or `cargo build` here so `target/debug/fx50` exists.
2. In Zed, run the `zed: extensions` action to open the Extensions page.
3. Click **Install Dev Extension** and choose this directory,
   `editors/zed`.
4. Open a `.fx` or `.fxc` file. Zed should report the language in the status
   bar (`fx-50FH II PRGM` / `fx-50FH II C-like`).

If the server does not start, check Zed's log (`zed: open log`) while launching
Zed from a terminal with `zed --foreground`, and run `zed: reload extensions`
after changing anything under `editors/zed/`.

## Grammar revision

`extension.toml` registers both grammars against this repository rather than a
separate grammar repo, so the grammar and the language are versioned together:

```toml
[grammars.fx]
repository = "https://github.com/wqLouis/casio-fx50fh2-emulator"
rev = "<commit that contains editors/tree-sitter-fx>"
path = "editors/tree-sitter-fx"
```

Zed needs a concrete Git revision, and a commit cannot name its own SHA, so
`rev` is pinned to the commit that introduced the grammars. The `path` key
points at the grammar subdirectory, which is what lets one repository host the
extension and both grammars.

**If you move or change a grammar**, update `rev` to a newer commit that
contains the change, otherwise Zed keeps building the old parser:

```bash
git rev-parse HEAD   # take the SHA of the commit with the grammar change
```

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
