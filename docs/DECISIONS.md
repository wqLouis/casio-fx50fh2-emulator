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
