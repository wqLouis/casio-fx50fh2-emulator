# Architecture decisions

Short records of the choices that shaped the workspace, especially where an
existing crate was adopted instead of hand-rolling code.

## ADR 0001 — One `fx50` binary in its own crate

**Context.** The toolkit has three concerns: interpreting PRGM, transpiling a
C-like language to PRGM, and language-server support. The first two naturally
live in libraries.

**Decision.** The user-facing binary is `crates/fx-cli`, published as the
single command `fx50`. The core interpreter is the library `casio-fx50fh2`;
the transpiler and LSP are also libraries (`fx_transpiler`, `fx_lsp`) driven
by `fx50 build`/`run` and `fx50 lsp`.

**Why not put `fx50` in the core crate?** `fx-transpiler` and `fx-lsp` both
depend on the core library. If the binary lived in the core package it would
have to depend on them, producing the cycle
`casio-fx50fh2 → fx-transpiler → casio-fx50fh2`. A separate leaf crate breaks
the cycle cleanly. The core package therefore sets `autobins = false`.

**Consequences.** `fx50` is a thin dispatcher; every capability is a library
call. The transpiler and LSP can each still be built and tested on their own.

**One binary, and `default-members` so `cargo build` finds it.** The
transpiler and LSP first carried thin `fxc`/`fx-lsp` shim binaries for their
own testing. Those were removed: the project ships exactly one binary, so
`cargo build` produces `fx50` and nothing else, and `cargo run` is
unambiguous. Their end-to-end tests moved to where they can exercise the real
tool: the LSP stdio test now lives in `crates/fx-cli/tests/lsp_server.rs` and
launches `fx50 lsp`.

Because the workspace root is a package, a bare `cargo build` would otherwise
select only the root library and never produce the binary; `[workspace]
default-members` lists every package so that `cargo build`, `cargo run` and
`cargo test` act on the whole workspace.

## ADR 0002 — Adopt `clap` instead of a hand-rolled argument parser

**Context.** The first cut of the CLI hand-parsed `std::env::args`.

**Decision.** Use `clap` (derive API) plus `clap_complete`.

**Why.** Hand-rolling reproduced a subset of `clap` badly: no `--help`
generation, no `--version` wiring, no shell completions, inconsistent error
messages, and fragile handling of `--`. `clap` gives all of that, is the de
facto standard, and its derive API keeps the subcommand definitions declarative
and in one place.

**Consequence.** `fx50 completions <shell>` exists for free.

## ADR 0003 — Adopt `rustyline` for the REPL

**Context.** The REPL read lines with `std::io::stdin().read_line`, so it had
no history, no line editing, and no cursor movement.

**Decision.** Use `rustyline`, saving history to
`<data_dir>/fx50/history.txt` (via `dirs`).

**Alternatives considered.** `reedline` (the nushell line editor) is more
capable but pulls a much larger dependency tree and a different programming
model; that is overkill for an expression REPL.

## ADR 0004 — Adopt `codespan-reporting` for diagnostics

**Context.** `CalcError::Syntax` carries a byte offset, but the CLI printed a
plain one-line message, discarding the position.

**Decision.** Render errors with `codespan-reporting`, giving the familiar
rustc-style caret under the offending source.

**Alternatives considered.** `miette` is excellent but expects the error type
to implement `miette::Diagnostic`. That would add a dependency *and* a trait
impl to the core crate, which the project keeps dependency-free (std only).
`ariadne` can do the job but is oriented at multi-span reports; the
`codespan-reporting` label API matches a single byte offset most directly.

## ADR 0005 — Keep the hand-written lexer and parser

**Context.** Parser generators (`pest`, `lalrpop`, `logos`) exist.

**Decision.** Keep the hand-written scanner and recursive-descent parser.

**Why.** The fx-50FH II grammar is context-sensitive in ways a generated
parser fights: `M+` is a statement only when standalone, `4AC` must lex as
`4 × A × C` (longest-keyword matching), closing parentheses may be omitted
before a statement separator, and `⇒` takes a *statement* target rather than
an expression. The recursive-descent parser encodes the user's guide priority
sequence directly and is covered by tests. A generated parser would add a
build-time dependency and a less direct mapping to the hardware's rules.

## ADR 0006 — No third-party dependencies in the core crate

**Decision.** `casio-fx50fh2` depends only on `std`. Dependencies live in the
leaf crates (`fx-cli`, `fx-lsp`).

**Why.** The interpreter is the part most likely to be embedded (in a WASM
build, an editor plugin, a test harness); keeping it pure `std` keeps that
easy and keeps the hardware-fidelity code free of unrelated churn.

## ADR 0007 — `tower-lsp` for the language server; shutdown relies on stdin EOF

**Context.** The LSP workstream needed a JSON-RPC server. Options were
`tower-lsp` (async, the de-facto standard), `lsp-server` (sync, simpler), or a
hand-rolled loop.

**Decision.** Use `tower-lsp` with `tokio`, but expose the server as a
library function `fx_lsp::run_server()` that owns its runtime, so the
synchronous `fx50` CLI can start it without an async runtime of its own.

**Known deviation.** `tower-lsp` 0.20's `Server::serve` loop terminates when
its input stream reaches EOF; its `LanguageServer` trait has no `exit` hook. So
a client that sends the `exit` notification but leaves the pipe open will not
see the process terminate immediately. Every real client (VS Code, Neovim,
Helix) closes the stream right after `exit`, and the bundled
`tests/server.rs` verifies that path. Making `exit` itself terminate the
process would require wrapping stdin in a framing-aware `AsyncRead` that
synthesises EOF, which risks introducing subtle bugs into the transport for a
case real clients do not hit. The deviation is documented in
`crates/fx-lsp/README.md`.

## ADR 0008 — Operating modes are declared in the source with `#mode`

**Context.** The fx-50FH II has five compute modes: COMP, CMPLX, BASE, SD and
REG. The mode is not a runtime concept you can set from a program — you pick it
with the MODE key before you start, and it decides which keys exist. In
particular there is no `i` key in COMP mode, and `√(-4)` is a `Math ERROR`
rather than `2i`.

An interpreter that lets every feature be used everywhere is therefore *wrong*
about the machine in an important way: it silently answers questions the
calculator cannot be asked.

**Decision.** A program declares its mode in a header directive, defaulting to
COMP:

```text
#mode CMPLX
(3+4i)×(1-2i)◢
```

The directive is lexed as its own token and parsed into `Stmt::Mode(Mode)`,
which (a) keeps `parser::parse -> Vec<Stmt>` unchanged, (b) lets the interpreter
apply the mode before anything runs, and (c) lets a static checker walk the
flattened program. The checker lives in `src/check.rs` and reports violations
as `CalcError::Mode`, labelled `Mode ERROR`. The mode can also be supplied from
outside via `compile_with(source, Some(mode))`, which is what the CLI's
`--mode/-m` flag uses.

Modes are modelled as a small capability table rather than a list of allowed
statements:

| Predicate | True for |
| --- | --- |
| `allows_complex` | CMPLX |
| `allows_stats` | SD, REG |
| `allows_regression` | REG |
| `allows_base` | BASE |
| `allows_float_math` | everything except BASE |
| `allows_setup` | everything except BASE |

**Why a header rather than a runtime command?** Because the real mode is fixed
for the life of a program. A mid-program `#mode` would model something the
hardware cannot do, so the parser rejects a directive that is not first.

**Why a new error label?** The hardware has no `Mode ERROR` screen; it prevents
these situations by not offering the key. When reading source, though, the
mistake is real and worth a precise message, so it gets its own label rather
than being disguised as a `Syntax ERROR`. It is documented as a source-level
diagnostic in `docs/LANGUAGE.md`.

**Consequence for `.fxc`.** The transpiler accepts the same header and emits it
into the generated PRGM, so the round trip preserves the mode. Because the
C-like language is real-only, its checker mostly matters for BASE, where
floating-point built-ins and `pi`/`e` are rejected. The transpiler keeps its own
copy of the `Mode` enum so it still builds with `--no-default-features` (no core
dependency).

## ADR 0009 — The 40 scientific constants live in one table

The calculator has 40 built-in scientific constants, reached on the real unit
with `CONST` and a two-digit number. They are modelled as a single table:

```rust
pub struct PhysicalConstant {
    pub code: u8,               // menu number, 1..=40
    pub name: &'static str,     // ASCII spelling in source, e.g. "hbar"
    pub symbol: &'static str,   // what the display shows, e.g. "ħ"
    pub value: f64,
    pub unit: &'static str,
    pub description: &'static str,
}
pub const CONSTANTS: [PhysicalConstant; 40] = [ /* … */ ];
```

`ConstName` grows exactly one variant, `Physical(u8)`, holding the menu number;
the value is resolved through `constants::by_code`. **Why a table rather than
forty variants?** Forty variants (or forty `pub const`s) would spread the data
across the token namespace and make a value correction a fifty-line diff. The
menu number is also the most faithful key: it is what the user's guide prints
next to each constant.

**Spellings.** A constant is written with its ASCII `name` or its display
`symbol`, matched by `constants::lookup`. The elementary charge is the one
exception: the calculator shows it as `e`, but source text already uses `e` for
Euler's number, so it is only reachable as `eq`. The symbol is still reported
for display purposes.

**Values** are the 2010 CODATA revision, the one this calculator shipped with.
Two of them differ from an older manual revision still found in the wild — the
proton gyromagnetic ratio is `2.675222005×10⁸` (not `10⁻⁸`) and the muon
magnetic moment is `−4.49044807×10⁻²⁶` (not the neutron's value). Because they
are real numbers they are available in COMP, CMPLX, SD and REG, and rejected in
BASE by the same `allows_float_math` predicate that covers `π` and `e`.

**Consequence for `.fxc`.** The transpiler cannot read the core table (it must
build with `--no-default-features`), so it keeps its own copy of the forty
names and symbols. Drift is prevented by a test that compares the two tables
entry by entry and, for every constant, transpiles `phys.<name>` and checks the
result against the interpreter. In `.fxc` the constants are reached through a
`phys.` namespace — `phys.h`, `phys.C0` — so that forty global names do not
collide with the user's variables; `phys` is a reserved word. Bare names are
deliberately *not* recognised.

## ADR 0010 — One language server for two languages, selected by `languageId`

`.fx` (PRGM) and `.fxc` (C-like) are different languages with different
front ends, and there was a choice between shipping two servers or one. There
is one process, `fx50 lsp`, and it routes per document.

**What selects the front end.** The `languageId` from
`textDocument/didOpen`, with the URI's file extension as a fallback and `fx` as
the final default. The ids are `fx` and `fxc` — the same ids VS Code, Neovim
and Zed are configured with, so the editor side and the server side agree on a
single vocabulary.

**Why one server.** The two languages are alternatives for the same artifact —
`.fxc` is simply a friendlier way to write the PRGM program — so a user
routinely has both open, and the diagnostics, completion and hover logic share
almost all of their plumbing (position mapping, diagnostic shape, symbol
walking). Two processes would also mean two entries in every editor
configuration.

**Cost.** `logic.rs` grows a `Language` parameter on its four entry points, and
the server must remember a document's language (the client sends it once, at
`didOpen`). That is a small, local cost for a much smaller editor setup.

**BASE-mode and `#include` diagnostics.** For `.fxc`, the server calls
`fx_transpiler::transpile_with_base` with the document's directory, which means
it performs real `#include` file I/O. That is deliberate: a missing or circular
include is one of the most common mistakes in a `.fxc` file, and the only way
to report it is to attempt the resolution. The failure is turned into a
diagnostic (`Include ERROR`, naming the path) and can never panic the server.

## ADR 0011 — Zed's language id comes from `config.toml`'s `name`

Zed derives the LSP `languageId` it sends from the language config's
human-readable `name`, lowercased (`LanguageName::lsp_id`), with no separate id
field. A name like `fx-50FH II PRGM` would therefore be sent as
`fx-50fh ii prgm`, and a naive `[language_servers] languages = ["fx", "fxc"]`
would not even match, because that list is matched against the same `name`.

The fix follows the pattern the official Vue extension uses: keep the readable
`name`, list those exact names in `languages`, and map them back to the short
ids with an explicit `language_ids` table:

```toml
[language_servers.fx50]
languages = ["fx-50FH II PRGM", "fx-50FH II C-like"]
language_ids = { "fx-50FH II PRGM" = "fx", "fx-50FH II C-like" = "fxc" }
```

The server would still work if this were wrong, because it falls back to the
file extension, but relying on that fallback would leave `languageId` reporting
the wrong string to every other consumer.

**Zed extension packaging.** The extension is a `cdylib` built for
`wasm32-wasip2`, and it needs an empty `[workspace]` table in its
`Cargo.toml`: without it, cargo walks up, finds this repository's workspace,
and refuses to build the package. It builds and lints clean with
`cargo build --target wasm32-wasip2`.

## ADR 0012 — Grammars live in this repository

Zed grammars are normally separate repositories, and `extension.toml` takes a
`repository` + `rev`. Zed's `GrammarManifestEntry` also supports a `path`, so
the two grammars live here under `editors/tree-sitter-fx` and
`editors/tree-sitter-fxc`, referenced by `rev` pointing at this repository.

**Why.** The grammar and the language cannot drift apart if they are reviewed
and versioned together: a change to `.fxc` syntax and the grammar that
highlights it land in the same commit. Zed needs a concrete Git revision, and a
commit cannot name its own SHA, so `rev` is pinned to the commit that
introduced the grammars — updating a grammar therefore means bumping `rev` to a
newer commit that contains it.

**Acceptance test.** Both grammars are checked against the programs the project
actually ships — every `examples/*.fx` and `examples/*.fxc`, including
`examples/lib/*.fxc` — and must parse with zero `ERROR`/`MISSING` nodes. The
generated `src/parser.c` is committed, as Zed's build expects it.

**One deviation from the obvious grammar.** PRGM treats a newline as the `:`
statement separator, so a newline cannot be an `extra` the way it is in most
grammars: with it as trivia, an expression happily swallows the next
statement's first token as implied multiplication. It is modelled as an
explicit separator token instead, which parses all six `.fx` examples cleanly.

## ADR 0013 — Highlight queries are duplicated, and that duplication is tested

Zed does not read `highlights.scm` from the grammar repository. It reads it from
the **language** directory (`editors/zed/languages/<lang>/highlights.scm`),
resolved relative to the language's `config.toml`. The queries therefore exist
twice:

* `editors/tree-sitter-{fx,fxc}/queries/highlights.scm` — the grammar-side copy,
  for editors that load queries from the grammar checkout (Neovim, Helix, …);
* `editors/zed/languages/{fx,fxc}/highlights.scm` — the copy Zed applies.

**Why keep both.** The grammars are ordinary tree-sitter grammars and are also
useful outside Zed, so they keep their canonical `queries/` directory. Zed's
lookup location is a Zed-specific detail that belongs with the Zed extension.

**Why a test.** Getting this wrong is silent: the grammar still parses, and the
only symptom is that nothing is coloured. The two copies also have no reason to
differ, so `crates/fx-lsp/tests/editor_assets.rs` compares their bodies and
fails on drift (the headers may differ — each points at the other). The same
test file checks the other cross-file assumptions that are easy to break: that
`extension.toml` lists every configured language name verbatim, maps it back to
the `fx`/`fxc` id, carries a real 40-character `rev`, and that each grammar
ships the generated `parser.c` that Zed compiles.

**The related trap.** A wasm extension cannot stat the filesystem. Zed's
`worktree` API (unchanged through `since_v0.8.0`) offers only `which`,
`root-path`, `read-text-file`, `shell-env` and `id`, and `read-text-file`
refuses absolute paths and only reads text. Any `std::path::Path::is_file()`
check silently returns false in the sandbox. The extension detects a worktree
build by reading Cargo's dep-info file (`target/debug/fx50.d`), and falls back
to recognising this repository by its root `Cargo.toml` — necessary because
`target/` is gitignored and the worktree file API cannot see ignored paths.

## ADR 0014 — JSON is hand-written, and compile-time data is a language feature

`#data` lets a `.fxc` program read JSON while it is transpiled, and `#tests` is
the same mechanism under a shorter name. Since `fx-transpiler` must build with
`--no-default-features` and **zero** dependencies (ADR 0006), and `#data` cannot
be hidden behind a cargo feature without the language changing shape with the
build configuration, the JSON parser is part of the crate
(`crates/fx-transpiler/src/json.rs`).

**Why not `serde_json`.** It was already an optional dependency behind
`testing`, but keeping two JSON implementations — one for tests, one for
`#data` — would let them disagree about what a document means. One parser is
strictly better than two, so `serde` and `serde_json` were removed from the
crate entirely and `testing` now uses `json` too. `cargo tree -p fx-transpiler
--no-default-features` is now empty; even the core crate is optional.

**How the parser is kept honest.** Its input is compiled into a calculator
program, so silent coercion is worse than a clear error: duplicate object keys,
comments, trailing commas, leading zeros, out-of-range numbers and lone `\u`
surrogates are all rejected. Beyond its unit tests it was checked against a
reference implementation — 3000 generated documents parsed by this crate and by
Python's `json`, re-parsed and compared structurally. Every difference (89 of
3000) was an `f64` representation case, e.g. `9007199254740993`, which no
`f64` can hold; there were no structural, type, string or key-order
disagreements. Numbers are `f64` because that is what the calculator computes
with.

**`#data` is textual, like `#include`.** Directives are extracted after
`#include` expansion and before lexing. The directive text is blanked —
newlines preserved — so every line number still refers to the line the user
wrote, and the existing line map keeps diagnostics pointing at the right file.
A **top-level JSON string means "read this file"**, which is what makes
`#tests = "cases.json";` work; nested strings stay strings. Values are only
usable as numbers and booleans, so nothing of the JSON reaches the calculator.

**`#tests` is not special.** It is literally `#data tests = ...`, so the test
runner is a consumer of the data facility rather than a parallel
implementation. The cases live in the program; a standalone `.tests.json` is
still read when a program has no `#tests` table.

## ADR 0015 — `const` is inlined, not allocated

`const NAME = <expr>;` declares a fixed value. It is replaced by its expression
at every use and never reaches the allocator, so it costs none of the seven
memories. This is the main way a `.fxc` program stays within the calculator's
register budget without heroics.

The expression must be constant: numbers, `pi`, `e`, `phys.` constants, `#data`
values and earlier `const`s, combined with unary minus and the binary
operators. Variables, `input()` and built-in calls are rejected with an
explanation, because inlining a variable would silently capture its value at
the point of the declaration, which is never what `const` means. Definitions
must precede use, which also makes cycles impossible.

**Alternatives rejected.** Folding every `const` to an `f64` at transpile time
would turn `const c = 2 * pi;` into a long decimal, losing the symbolic `π` the
calculator would otherwise key in — and folding in `f64` can differ in the last
digit from the calculator's own 15-digit arithmetic. Inlining keeps the
program's arithmetic exactly as written.

## ADR 0016 — `free` releases a memory; the transpiler never guesses

The calculator has seven memories, and a program may need more live values than
that. The question is who decides when a memory can be given to someone else.

**Implicit reuse is unsound here, and a test proved it.** The first version of
this allocator reused a memory once a name was no longer *read*, collapsing
`let a = 1; let b = 2;` onto `A`. The golden test asserting seven names map to
`A B C D X Y M` caught it. The reasoning is genuinely different from a normal
compiler's: a memory's final value is **observable** — PRGM leaves its answer in
one, a later program can read it, and so can the user — so no name is ever
provably dead. Dropping `a` silently changes what the program did.

**So the programmer says when.** `free name;` releases the memory, and the next
new variable takes it. This is the same bargain as any explicit-free language:
the compiler cannot prove liveness, so it does not pretend to; the human who
knows the value is finished states it. `let a = 1; let b = 2;` therefore still
uses two memories, and

```c
let first = 5;
print(first);
free first;
let second = 7;
```

uses one, with `second` allocated to the memory `free first` released.

**The register table is the point.** Allocation keeps a table of which variable
occupies each memory, in order, and that is what makes the classic lifetime
mistakes *transpile* errors instead of wrong answers on the calculator: use after
free (including using the name again as an assignment target), double free,
freeing a `const`/`#data`/unknown name, and running out of memories. Each message
names the offending variable and where it was written.

**`let` declares, and a freed name can be declared again.** A `let` introduces a
name, so a second `let` of a name that is *still live* is an error rather than a
silent shadow — that is the mistake the user asked to be caught. But once the
name has been released, declaring it again is exactly the intended way to reuse
a name, and it takes a fresh binding:

```c
let x = input();
free x;
let x = input();   // a second, independent life for `x`
```

For that to work the emitter cannot hold one name→memory table: `x` may have had
two memories over the program's life. A binding is therefore recorded as a byte
range (`from`..`to`), and a reference resolves to the binding whose range
contains the reference's offset. That is why `Allocator::register_at` takes a
position and the emitter passes each name's source offset.

Two smaller rules keep that story coherent:

* **A declaration cannot see its own name.** `let x = x + 1;` — including after a
  `free` — is an error, because a declaration does not take effect until its
  value is computed. This is the same rule as Rust's `let x = x;`.
* **A declaration's name is bound before its initializer is walked**, so
  allocation follows source order: `let a = -b * c;` gives `a` the first memory.

**`free` and jumps: `unsafe_free`.** A jump can re-enter code whose memory has
since been released and given to another variable, so a single forward walk no
longer describes the program — the linear model would emit the wrong memory.
Rather than ban the combination, a checked `free` in a program containing
`goto`/`label` is an error, and `unsafe_free` is the explicit way to say "I have
checked this myself", borrowing Rust's convention of making the unchecked
operation visible. Like Rust's `unsafe`, it waives one guarantee, not all
checking: `unsafe_free` still rejects double frees, unknown names and `const`s.
A program with jumps and no `free` at all is unaffected, because then nothing is
ever re-used.

**What this replaced.** An earlier revision shipped a `#reg NAME = M` directive
for pinning a variable to a chosen memory. It answered "how do I make my program
fit" with "pick the memory yourself", which is manual allocation the transpiler
should be doing, and it forced the user to know the letter for every variable.
`free` addresses the actual problem — too many *live* values — and leaves the
letter choice to the allocator. A `#reg` in an existing program is reported with
a message pointing at `free` rather than as an unknown directive.

`const` (ADR 0015) and `#data` remain the first answers to memory pressure,
because a value that is inlined needs no memory and therefore no `free`.

## ADR 0017 — Extensions do not bundle the language server

The VS Code and Zed integrations both need `fx50` running as a language server.
Neither ships it; both look for a binary the user installed. That is not a
preference — bundling is impossible.

**A Zed extension is a WebAssembly module.** Zed compiles extensions to
`wasm32-wasip2`, and `extension.toml` has no field for shipping files: a
language extension registers `languages/`, `[grammars.*]` (a Git repository, a
`rev` and a `path`) and `[language_servers.*]` (a name and the languages it
applies to). There is no resources directory and no way to declare an
executable. The only file-facing capability in `zed_extension_api` is

```rust
download_file(url, path, DownloadedFileType)  // Gzip | GzipTar | Zip | Uncompressed
```

i.e. fetch one at runtime into the extension's working directory, plus
`make_file_executable`. Nothing can read bytes out of the bundle, and the
`Worktree` API is read-only (`which`, `root_path`, `read_text_file`,
`shell_env`) — so there is no path from "bundled file" to "executable on disk"
even in principle. A VS Code extension has the same shape of problem: it is a
zip of JavaScript, with no place to put a platform binary.

**The rejected alternative is runtime download**, which is what many Zed
extensions do and what this project should eventually do: `latest_github_release`
→ `current_platform` → `download_file` → `make_file_executable` →
`set_language_server_installation_status`, with `language_server_command`
returning the downloaded path. It is not wired up yet because it needs
infrastructure this repository does not have: a release pipeline cross-compiling
`fx50` for roughly six OS/architecture pairs on every tag, with the binaries
attached as release assets. The extension code itself is small; the release
pipeline is the work. Two further objections to doing it badly are worth
recording: shipping or fetching executables is a known malware vector (the
reason the API is download-only in the first place), and an unsigned binary
materialised on macOS is quarantined by Gatekeeper.

**What we do instead.** `cargo install` puts `fx50` on `PATH`:

```bash
cargo install --git https://github.com/wqLouis/casio-fx50fh2-emulator fx-cli
cargo install --path crates/fx-cli     # from a clone
```

The resolution order is then `PATH` (via `Worktree::which`) → a dep-info probe
for a build inside the opened worktree → the worktree-root `Cargo.toml` marker
heuristic, so anyone hacking on this repository needs no install at all. When a
release pipeline exists, the download becomes a further fallback and the install
step becomes optional. The user-facing instructions live in
[`editors/README.md`](../editors/README.md) and each editor's README.
