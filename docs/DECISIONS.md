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

**`free` under jumps or loops: `unsafe_free`.** A jump can re-enter code whose
memory has since been released and given to another variable, and so can a loop
body on its next iteration — `while (c) { print(x); free x; let y = 1; }` gives
`x` and `y` the same memory, so the second pass reads `y` as `x`. Either way the
single forward walk no longer describes the program and the linear model would
emit the wrong memory. Rather than ban the combination, a checked `free` inside
a loop body or in a program containing `goto`/`label` is an error, and
`unsafe_free` is the explicit way to say "I have checked this myself", borrowing
Rust's convention of making the unchecked operation visible. Like Rust's
`unsafe`, it waives one guarantee, not all checking: `unsafe_free` still rejects
double frees, unknown names and `const`s. A program with jumps or loops and no
`free` at all is unaffected, because then nothing is ever re-used, and a `free`
outside every loop is fine even when the program loops.

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

## ADR 0018 — Arrays use one memory per element, with compile-time indices

`.fxc` gained arrays (`let v[3] = {4, 8, 15};`, `v[1] = 16;`, `print(v[0]);`).

**Elements occupy memories, and the index must be a literal.** The calculator has
no indirect addressing: a memory is named by the keystroke that selects it, so
there is no way to express "the memory whose number is in `X`". A run-time index
would have to be compiled to an `If`/`IfEnd` chain over the elements.

We rejected the run-time form for a measured reason. The fx-50FH II has **680
bytes of program memory, shared by all four program areas** (P1–P4). An
`If`/`Else` chain over an array of `n` elements costs roughly `n` comparisons
plus the branch overhead *at every access*, and a loop that indexes an array pays
it on every iteration. Against a 680-byte budget that is unaffordable. With
compile-time indices the cost is the opposite: an element reference compiles to
**one memory letter and no instructions at all**. So the restriction is the
feature — the programmer gives up run-time indexing and gets zero-byte access,
which is the trade that matters on this machine.

The consequences, all deliberate:

* `let v[N]` takes `N` of the seven memories, chosen as the first `N` free ones,
  so a five-element array plus a loop counter plus an accumulator exactly fills
  the machine.
* The whole array is released at once by `free v;`. `free v[0];` is an error
  rather than allowed: the elements after it would be stranded in memories the
  allocator could never hand out again.
* An array and a scalar cannot share a name while it is live (in either
  direction), because the two would allocate different numbers of memories and a
  later reference could not tell them apart. `free` first.
* `const` cannot declare an array. A `const` is inlined and has no memory, which
  is precisely what an array is not; `#data` is the right home for a fixed table.
* Bounds are checked at transpile time, since the index is known there.

`a[i]` and `#data`'s `c.list[0]` share one syntax, one AST node
(`Expr::Data { accessors }`) and one parser rule; the allocator and emitter
decide between them by consulting the declared arrays and the data tables. A
`#data` table of the same name wins, preserving the existing behaviour.

## ADR 0019 — No register packing: the machine has no integer or fractional part

A natural response to "seven memories" is to pack two variables into one, using
the 15 significant digits the machine keeps internally. This is not
implementable, for a reason worth recording so it is not re-proposed.

**Precision is 15 significant digits** (mantissa `A.BCDEFGHIJKLMNO`), with
exponent `10^±99`, displayed to 10. That part was right. But the machine applies
**autocorrection** after *every* operation: if the last four significant digits
fall in `0000`–`0009` it rounds down to 11 significant figures, and if they fall
in `9991`–`9999` it rounds up to 11. Measured against this repository's own
`precision::normalize`:

| Input (15 digits) | After one operation |
| --- | --- |
| `1.23456789010005` | `1.23456789010000` (autocorrected) |
| `1.23456789012345` | unchanged |
| `1.2345678901` (11 digits) | unchanged |

So values of at most 11 significant digits pass through untouched, and only those
would be safe to pack.

**The blocking problem is unpacking.** Splitting `R = hi + lo × 10^-k` needs an
integer-part or fractional-part operation, and the fx-50FH II has neither. `Int`,
`Intg` and `Frac` do not exist on this model: they are absent from this
repository's token table (`src/token.rs`), from both reverse-engineering projects
the language notes cite (`KeroppiMomo/calsimtor` claims *all* COMP-mode tokens
and lists only `Rnd(`/`Abs(`; `throwingogo-hub/fx-50fh-ii` likewise), and from
the official fx-50F II / fx-4650F II manual's bracketed-function list
(`Abs(, Pol(, Rec(, arg(, Conjg(, Not(, Neg(, Rnd(`). The only rounding primitive
is `Rnd(`, which rounds to the *display* setting, not to an integer.

A narrow case does work: under `Fix 0`, with both halves non-negative integers
small enough that `hi = Rnd(R)` and `lo = (R - hi) × 10^h` are exact, two values
can share a memory. But it requires forcing `Fix 0` on the whole program — after
which every display is an integer, and the setting cannot be restored because
there is no way to query it. That is not a feature to build into the language; it
is a trick a programmer can apply by hand in the rare program where it fits.

Arrays therefore use one memory per element (ADR 0018), and the way to fit a
program is `const`, `#data` and `free` — not packing.

## ADR 0020 — Constant expressions are pre-calculated, and constant array loops are unrolled

Two byte-saving passes run after parsing and before allocation, both fed by the
fact that the machine has **680 bytes of program storage shared by all four
program areas**.

**Constant folding.** An expression built only from numbers is evaluated while
transpiling and replaced by its value: `2 * 3 + 4` emits `10`, not `2×3+4`; a
numeric `const` is inlined and then folded, so `const k = 6; print(k + 1)` emits
`7`; and a literal `for` limit is folded, so `i < 5` emits `To 4`, not `To 5-1`.

Folding had been partly rejected in ADR 0015, which keeps `const` symbolic. That
reason still holds where it applies, and this ADR does not overturn it: `pi`,
`e` and the `phys.` constants are **never** folded, because the machine keys
them in as their own symbols and folding `2 * pi` to a decimal would lose
precision and cost bytes. What is folded is arithmetic on plain numbers.

Folding must also agree with the machine's arithmetic, which keeps 15
significant digits and applies an auto-correction pass after every operation —
while a literal read from a program is *not* corrected. A value is therefore
folded only when the machine's own `normalize` would leave it unchanged; `1 / 3`
and `0.1 + 0.2` stay as written, because the machine's `0.333333333333333` and
`0.3` are not the values a naive `f64` fold produces. The predicate mirrors the
rules in `src/precision.rs` rather than duplicating them, and a test under the
`execute` feature asserts the two agree.

**Unrolling constant array loops.** ADR 0018 requires array indices to be
compile-time literals, which meant `for (let i… ) v[i] = input();` could not be
written at all. The transpiler now expands a `for` loop whose bounds are integer
literals after folding, replacing the counter with each of its values and
folding, so the index becomes a literal. Only loops whose body contains a
computed index that mentions the counter are expanded: a loop over plain scalars
keeps its native `For`/`Next` form, because unrolling it would cost bytes rather
than save them.

The shape is deliberately narrow. A run-time bound, a `break`, `goto`/`label`, a
declaration, or a `free` in the body stops the unroll, and the index is then
reported as the compile-time error ADR 0018 describes. The expansion is bounded
so a large loop cannot silently produce a giant program.

**What happens to the counter.** A `for (let i… )` counter that nothing outside
the loop mentions is dropped, exactly as writing the loop out by hand drops it —
that is where the byte saving comes from, and re-declaring it would also make the
machine display its value at the end of the program, which the loop itself does
not. If the counter *is* read or freed afterwards, or was an existing variable
rather than a `let` in the header, it is kept with the value the machine's `For`
would have left (the first value past the limit), so the unrolled program means
exactly what the loop did.

## ADR 0021 — Every PRGM key has an `.fxc` spelling

The `.fxc` language began as a C-like shorthand for the common PRGM constructs:
arithmetic, `If`/`While`/`For`, the prefix functions, `const`, `#data` and
memory management. The machine's full key vocabulary was **not** reachable —
`Ran#`, `x√(`, `x²`/`x³`/`x⁻¹`, `!`, `%`, `┘`, `nPr`/`nCr`, `∠`, `Pol(`/`Rec(`,
`arg`, `Conjg`, `Not`/`Neg`, the `stat.` values, `DT`, `Fix`/`Sci`/`Norm`,
`Deg`/`Rad`/`Gra`, `Hex`/`Bin`/`Oct`/`Dec`, `▶a+b𝑖`/`▶r∠θ`, `ClrMemory`,
`ClrStat`, `FreqOn`/`FreqOff`, `M+`/`M-`, the bitwise words and the `⇒` key had
no spelling at all. A program that needed one had to be written twice, once in
`.fxc` and once by hand.

**The front end now covers the whole machine.** The transpiler is a front end
for the calculator, so anything the calculator can key in, `.fxc` can express.
The mapping was chosen to keep one uniform rule rather than to mirror the
keyboard's shape:

* **Value keys are calls.** Prefix, postfix and infix keys alike are written
  `name(args)`, so `sqr(x)` is `x²`, `fact(x)` is `x!`, `frac(a, b)` is `a┘b`,
  `npr(n, r)` is `n nPr r`, and `polar(r, θ)` is `r∠θ`. The emitter puts the
  operator back between its arguments with the machine's own precedence, so the
  output is a real keystroke program. `root(n, x)` is the one place the argument
  order is not the spelling's order — the `x√(` key writes the index *before*
  the radical, and `root` follows the key rather than pretending otherwise.
* **Keys that act on the machine are statements.** `deg();`, `fix(3);`,
  `clrmemory();`, `dt(x, y);`, `mplus(x);` end in `;` and emit a bare key. This
  keeps them out of the expression grammar, where `Fix 3` is not a value.
* **Namespaces mirror `phys.`.** `phys.hbar` was already how a scientific
  constant is written, so statistical values are `stat.sumx`, `stat.meanx`,
  `stat.regA`, … A bare `sumx` stays an ordinary variable, so adding the
  namespace changed no existing program.
* **`Ran#` is reachable as `ran()`.** It was the only random source, and
  excluding it was arbitrary. It stays deterministic (a fixed-seed xorshift),
  which is what makes it usable in `#tests`.
* **The bitwise words are operators, not calls**, because they are infix on the
  keypad too: `a and b`, `a or b`, `a xor b`, `a xnor b`. `and` binds tighter
  than the others, and all four bind looser than the comparisons. Base-tagged
  literals are written the way other languages write them (`0x1F`, `0b1010`,
  `0o17`) and emitted the way the calculator does (`1Fh`, `1010b`, `17o`).
* **`cond => stmt;` is the `⇒` key.** It guards a single assignment, `print` or
  expression statement; anything larger is a transpile error pointing at `if`,
  because `⇒` on the machine guards exactly one statement.
* **The fixed `M` memory is reserved, not shared.** `mplus`/`mminus`/`mvalue`
  address `M` by its PRGM letter, so the allocator keeps `M` out of its pool for
  the whole program when they appear. Otherwise a `.fxc` variable could be
  placed in `M` and silently clobber the accumulator. This costs one of the
  seven memories only for programs that ask for the accumulator.

**Mode checking now mirrors the interpreter for every mode.** Previously only
BASE was validated, because `.fxc` was real-number-only. With the full key set
reachable, the transpiler applies the same rules as `src/check.rs`: complex keys
need CMPLX, statistics need SD/REG (and the `y`/regression values need REG),
base keys need BASE, setup needs a non-BASE mode, and `pol`/`rec` need COMP or
CMPLX. A construct the mode does not offer is a **transpile** error, with a line
and column, rather than a `Mode ERROR` on the calculator.

**Completeness is tested, not assumed.** `crates/fx-transpiler/tests/tokens.rs`
maps every element of the interpreter's own `FuncName::ALL`, `Postfix::ALL`,
`BinOp::ALL` and `StatVar::ALL` to an `.fxc` spelling and asserts it
transpiles. Adding a key to the machine without a front end for it therefore
fails the build.

## ADR 0022 — Functions are inlined; `fn main` is the entry point

`.fxc` gained user-defined functions:

```c
fn square(x) = x * x;                 // expression form
fn sum_to(n) { … return total; }      // procedure form
fn main() { print(square(4)); }
```

**PRGM has no call instruction, so a function is inlined, not called.** There
is no stack, no return address and no indirect addressing, so a real call is
impossible. A call is therefore replaced by the function's body while
transpiling: `square(x) = x * x` with `square(a)` becomes `a × a`. That makes a
call cost exactly its body and an uncalled function cost nothing — the same
bargain as `const` and `#data`, and the only one this machine can afford.

**Arguments are passed by name.** A parameter reference is replaced by the
argument expression at each mention. We chose this over evaluating arguments
into hidden temporaries for three reasons. It is free: no memory per argument,
which matters with seven memories, and it lets an argument that is mentioned
once inline with no trace. It is predictable: the program contains at each
mention exactly what the call wrote. And it makes `fn inc(x) { x = x + 1; }`
with `inc(a)` write back through the argument, which is how a procedure returns
more than one value:

```c
fn minmax(a, b, lo, hi) { lo = a; hi = b; if (a > b) { lo = b; hi = a; } }
```

The cost is that an argument mentioned `n` times is emitted and evaluated `n`
times, so `twice(ran())` draws two numbers. That is the documented meaning, not
a bug: the alternative — an invisible temporary per argument — spends memories
the machine does not have. Assigning to a parameter therefore requires an
assignable argument (a variable or array element); assigning an expression is
an error.

**A function owns its names.** Locals are hygienic: each expansion renames a
declared name to a private `f$name$N`, so two calls, or a caller variable of the
same name, cannot collide. Every function except `main` is also **closed**:
`f`'s body may use only its parameters, its locals, a top-level `const`/`#data`
value, another function or a built-in. It cannot read or create a global. This
is what makes a library safe to include: it contributes function names and
nothing else, so it cannot change the includer's behaviour by reaching into its
memories.

**`fn main` is the universal entry point.** An earlier revision of this change
kept the classic form for files with no `fn`, so the existing corpus was
untouched. That was rejected: it left the language with two shapes, and it left
the old failure mode reachable — an included fragment's loose statements ran in
the includer's seven memories and could silently redefine its variables. The
final rule is one shape for every program:

* `fn main()` is required. A file with no `fn` at all is an error, not a
  top-level script.
* Only `fn` definitions and top-level `const` declarations sit beside it. A
  loose statement is an error telling you to move it into `main`.
* `main` is the one body that is **not** closed. It keeps the classic
  order-free scoping — using a name declares it — so a program's loose
  statements move into `main` unchanged. Wrapping them emits exactly the PRGM
  they emitted at the top level.

`main` is the exception on purpose. Closing it too would have forced every name
in the corpus to be declared before use and broken `print(a);`, which is a
property of the *entry point*, not of functions in general. A function called
from another file is where isolation matters, and it is exactly those functions
that are closed.

**What is rejected, and why.** Recursion is rejected because there is no stack.
A `return` that is not the last statement is rejected because leaving a function
early would need a jump; two results are written with output parameters, which
call-by-name makes natural. A procedure (statement body) called from a
`while`/`for` condition is rejected because inlining would hoist its statements
out of the loop. A parameter used as an array is rejected because there is no
faithful inlining for it.

**The passes.** `functions.rs` runs first, right after parsing: it collects the
definitions, resolves the entry point, checks scoping, rejects cycles and
arity mismatches (against the definitions' own positions, before inlining
rewrites them), and substitutes each call. Bodies are re-positioned to the call
site so the allocator's byte-range bindings stay valid. The existing
`fold`/`unroll`/`alloc`/`emit` passes then see a flat, function-free program.

## ADR 0023 — Optimisation passes, and the `Options::optimize` switch

Three optional passes now run between parsing and emission, on top of the
required `fold` (ADR 0020) and `unroll`:

* **`simplify`** removes operators that provably cannot change a result —
  `x + 0`, `x - 0`, `x * 1`, `x / 1`, `x ^ 1`, `-(-x)`, `x == x`, and (when the
  operand cannot raise) `x * 0`, `x - x`, `x ^ 0`.
* **`propagate`** replaces a *read* of a name that is initialised to a constant
  and never assigned with that constant, which lets `fold` collapse the
  surrounding arithmetic (`let n = 3; … i < n` becomes `To 3`). It also decides
  a constant `if`/`while`, drops code after an unconditional `goto`, and — see
  below — removes stores nothing reads.
* **`size`** is not a pass but the metric: it counts the keys a listing costs,
  because the machine stores one byte per key out of a 680-byte budget shared by
  all four program areas.

They are on by default: the machine is too small for a program to be left
unoptimised. `Options::optimize = false` turns off `simplify` and `propagate`
only — **not** `fold` and `unroll`, since folding a `const`'s value is required
for the emitter and unrolling is required for an array element to name a memory
at all. The CLI spells this `--no-optimize`.

### Why the switch exists at all

Without it the golden tests would stop testing what they were written to test.
Measured on the real output:

| construct | unoptimised | optimised |
| --- | --- | --- |
| `print(a != b)` | `A≠B◢` | `1◢` |
| `if (1) {…} else {…}` | `If 1 / Then / … / Else / … / IfEnd` | the taken branch only |
| `print(sqr(a + 1))` | `(A+1)²◢` | `2²◢` |
| `print(-(-a))` | `-(-A)◢` | `A◢` |

`A≠B` and the `If`/`Else` chain are exactly what those tests exist to check, and
folding erases them. So the golden tests pin the **translation** with
`optimize: false`, and the optimiser is tested on its own terms in
`tests/simplify.rs` and `tests/propagate.rs`. `tests/optimize.rs` checks the
switch itself and, end to end, that both forms compute the same thing.

### A store is kept unless nothing reads it

Memory is observable: a memory's final value can be read by a later program or
by the user, so an optimiser may not quietly delete stores (ADR 0016). The one
exception is a store to a name that is **never read anywhere in the program** —
it cannot affect the output, which is a program's contract. Measured, that is
worth a lot: a corpus of ten programs that declare values they do not use goes
from 90 keys to 55 (−38%).

Getting this sound took four rules, each found by a failing test rather than by
inspection, and each worth recording because the tidy version of the pass is
wrong:

1. **No reads anywhere.** A name read only inside a loop or a branch still
   counts as read.
2. **Side-effect-free initialisers only.** `input()` (the prompt is observable),
   `ran()` (it advances the sequence), `mplus`/`mminus`, and any call that can
   raise `Math ERROR` keep their store even if nothing reads it.
3. **Never remove a declaration that is freed, or declared more than once.**
   This is about *diagnostics*, not size. `let t = 1; free t; free t;` is a
double-free error and `let x = 1; let x = 2;` a re-declaration error, yet in both
cases the name is never read, so the tidy pass removes the declaration — and then
the `free`s, since `free` is name-based and would otherwise dangle — leaving a
program with no error at all. The first version of this pass did exactly that and
turned three language-server diagnostics into silence. This pass runs *after*
checking, so it must never delete the thing a diagnostic is about.
4. **Nothing that `ans` could see, and nothing that could be displayed.** Two
non-reads are still observable. Evaluating any expression updates the hidden
result memory that `ans()` reads, so removing `6*7→A` changes what a later
`ans()` returns. And a program that ends without `◢` displays the last value it
computed, so a store in that position is visible — `let a = 1;` on its own
displays `1`. Either one disables the pass for the whole program.

The last two rules are the reason this pass is off in the golden tests even
though it is on by default: `Options::optimize = false` documents the
translation, and `tests/propagate.rs` tests the optimiser's own behaviour
including every one of these refusals.

Measured effect of the passes as a whole, in keys:

| program | raw | optimised |
| --- | --- | --- |
| `let n = 3; print(n + 1);` | 7 | 5 |
| `let a = input(); print(a * 1); print(a + 0);` | 11 | 7 |
| `if (1) { print(9); } else { print(8); }` | 9 | 2 |
| `while (0) { print(1); } print(2);` | 7 | 2 |
| a corpus of ten such programs | 90 | 62 (−31%) |
| `examples/functions.fxc` | 62 | 50 (−19%) |

The examples that are dominated by `input()` and loops (`determinant`, `arrays`,
`quadratic`) gain nothing, which is the honest result: there is no constant in
them to find.

## ADR 0024 — Program size is measured in keys, not characters

`Size::measure` counts the keys a PRGM listing costs, because that is what the
machine stores: **one byte per key** against a 680-byte budget shared by all
four program areas (P1–P4). Character count would be wrong in both directions —
`log(` is one key but four characters, while `10` is two keys but two
characters.

The consequence that matters is that a multi-character token costs one key. The
scan mirrors the interpreter's own lexer (`src/lexer.rs`), which is the
authority on the key set, but reimplements it rather than depending on that
crate, so `fx-transpiler` still builds with `--no-default-features` and zero
dependencies. Where the machine's accounting is genuinely ambiguous we chose the
reading that matches the manual and documented it: a statement separator is
free for a newline (listings emit one statement per line) but costs a key for a
literal `:`; the `#mode` header is free, since the MODE key would have been
pressed before typing the program rather than stored in it.

`fx50 size FILE` prints the total, the statement count, the largest single
statement, and whether it fits — and always prints what the optimiser saved, so
the number is meaningful next to what it would have been.

## ADR 0025 — The interpreter's numeric hot path has two implementations

`precision::normalize` runs after **every** arithmetic operation, and it was
85% of interpreter runtime — `format!`-then-`parse`, i.e. two string allocations
and three float↔string conversions per operation.

**First fix: the allocations.** Both directions now format into a fixed-size
stack buffer, and `normalize` fuses the two round-trips that
`autocorrect(round15(x))` implies into one. Fusing is exact for normal inputs:
with `s = format15(x)` and `y = parse(s)`, `|y − s| ≤ ½·ulp(y) < 1.111e-15·10^e`
against a 15-digit decimal spacing of `5.000e-15·10^e`, so `y` is strictly
nearer to `s` than any other 15-digit decimal and renders back to the same
string. Values below `1e-300` fall back, since there `y` can be subnormal and
the bound fails. Worth **2.6×**.

**Second fix: an integer-mantissa fast path.** Even after that, `normalize` was
still ~73% of runtime at ~185 ns per operation — slower than CPython doing the
same arithmetic, which is not a good look for an interpreter whose whole job is
arithmetic. The cost is the decimal conversion itself.

The 15 significant digits of `x` are the integer `T = |x|·10^(14-e10)`, so
instead of rendering and re-parsing, the fast path scales by an *exactly
representable* power of ten (one correctly-rounded multiply or divide), rounds
to a whole number, applies the autocorrection to that integer, and scales back.
Every subsequent step is arithmetic on a value below `2^53`, which is exact.

It is taken only when the result is **provably** identical to the round-trip:

* `|14 - e10| ≤ 22`, because `10^22` is the last power of ten `f64` represents
  exactly (`10^23` rounds to `99999999999999991611392`);
* the scaled value must land strictly inside `[10^14, 10^15)` — which is also
  what validates the decimal exponent, so an off-by-one `log10` is caught rather
  than trusted;
* the fractional part must be more than `0.15` from the rounding boundary. The
  scaling contributes at most ½ ulp ≈ 0.0625 of error, so this leaves a ~2.4×
  margin. It also excludes exact ties, which matter because decimal parsing
  breaks ties to **even** while `f64::round` breaks them away from zero.

The window is narrow — `e10 ∈ [-8, 36]` — so a cheap check on the exponent bits
rejects everything else before any work is done. Without that early-out,
out-of-window values were **21% slower** than before (paying for a fast path
they could not use); with it, they are unchanged.

Measured per call:

| corpus | before | after | |
| --- | --- | --- | --- |
| `1e-10 .. 1e20` (ordinary calculations) | 207 ns | 50 ns | **4.1× faster**, 93% coverage |
| `1e-99 .. 1e99`, even by decade | 183 ns | 143 ns | 1.28× faster, 22% coverage |
| `1e40 .. 1e89` (outside the window) | 169 ns | 169 ns | unchanged, 0% coverage |

End to end on a million-iteration loop of four arithmetic operations each:
1.02 s → 0.35 s. Against CPython 3.14 running the identical loop, fx50 went from
5.3× slower to **1.8× slower**.

**The bug this found, and why it is worth recording.** An early version returned
`10` where the answer was `1` for `0.999999999999999`: when the autocorrection
carries out of 15 digits, the value becomes `1 × 10^(e10+1)`, whose 15-digit
mantissa is `10^14` — not the `10^15` (16 digits) that the rounding produces.
The existing oracle corpus passed straight through that case, which is the
lesson: **a corpus test proves nothing unless the corpus reaches the code**. The
fix added >12 000 probes built to sit exactly on the autocorrection boundaries
(`LMNO` in `0..=20` and `9980..=9999` across 101 exponents and three mantissa
prefixes), plus explicit carry cases, and a check that every decade of the
machine's range agrees.
