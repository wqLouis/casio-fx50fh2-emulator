# Architecture decisions

Short records of the choices that shaped the workspace, especially where an
existing crate was adopted instead of hand-rolling code.

## ADR 0001 — One `fx50` binary in its own crate

**Context.** The toolkit has three concerns: interpreting PRGM, transpiling a
C-like language to PRGM, and language-server support. The first two naturally
live in libraries.

**Decision.** The user-facing binary is `crates/fx-cli`, published as the
single command `fx50`. The core interpreter is the library `casio-fx50fh2`;
the transpiler and LSP are also libraries (`fx_transpiler`, `fx_lsp`) with
optional thin binaries.

**Why not put `fx50` in the core crate?** `fx-transpiler` and `fx-lsp` both
depend on the core library. If the binary lived in the core package it would
have to depend on them, producing the cycle
`casio-fx50fh2 → fx-transpiler → casio-fx50fh2`. A separate leaf crate breaks
the cycle cleanly. The core package therefore sets `autobins = false`.

**Consequences.** `fx50` is a thin dispatcher; every capability is a library
call. The transpiler and LSP can each still be built and tested on their own.

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
