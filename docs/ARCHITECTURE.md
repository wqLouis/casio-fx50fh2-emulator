# How the project is put together

This is the technical companion to the [main README](../README.md): the workspace
layout, the interpreter pipeline, and what is implemented. Design *choices* (and
the alternatives rejected) are in [DECISIONS.md](DECISIONS.md); the language
itself is documented in [LANGUAGE.md](LANGUAGE.md) (PRGM) and
[FXC.md](FXC.md) (`.fxc`).

## Workspace layout

The repository is a Cargo workspace:

```
src/                        the interpreter library (casio-fx50fh2)
crates/fx-transpiler/       `.fxc` -> PRGM (library `fx_transpiler`)
crates/fx-lsp/              language server (library `fx_lsp`)
crates/fx-cli/              the single `fx50` binary, dispatching on subcommands
examples/  docs/  tests/    programs, documentation, integration tests
editors/                    VS Code and Zed integrations, tree-sitter grammars
```

The binary lives in its own crate because both `fx-transpiler` and `fx-lsp`
depend on the core library; putting `fx50` in the core crate would create a
dependency cycle. It is the **only** binary the workspace produces — the
transpiler and language server are libraries that `fx50` drives through its
`build`/`run` and `lsp` subcommands.

`Cargo.toml` lists every package in `default-members`, so a bare `cargo build` at
the root builds the whole workspace (and therefore `fx50`), `cargo run` runs it,
and `cargo test` runs every crate's tests.

## The PRGM interpreter

```text
source ──► lexer ──► parser ──► flat Vec<Stmt> ──► tree-walking interpreter
          src/lexer  src/parser      src/ast           src/runtime
```

* **`src/token.rs`** — the key vocabulary (`VarName`, `BinOp`, `FuncName`, …)
  and `TokenKind`.
* **`src/lexer.rs`** — hand-written scanner. Accepts both Casio glyphs
  (`→ ◢ ⇒ ┘ ≠ ≥ ≤ π √`) and ASCII aliases (`-> disp => /`). Uses
  *longest-keyword matching*, which is why `4AC` lexes as `4 × A × C` while
  `Abs` and `Ans` stay single keys. Newlines are treated as `:`.
* **`src/parser.rs`** — recursive descent following the user's guide priority
  order (1 = tightest): atoms and parenthetical functions → postfix/`^(`/`x√(`
  → fractions → prefix `-` → `nPr`/`nCr` → `×`/`÷`/implicit `×` → `+`/`-` →
  relational → `and` → `or`/`xor`/`xnor`.
* **`src/runtime.rs`** — flattens the program to a `Vec<Stmt>` with an explicit
  program counter. `Lbl`/`Goto` are an index table; `If`/`While`/`For` are
  marker statements matched up into jump tables before execution.
* **`src/value.rs`** — `Value::{Real, Complex}` and complex arithmetic.
* **`src/precision.rs`** — 15-significant-digit rounding and the machine's
  autocorrection, applied after every arithmetic operation.
* **`src/stats.rs`** — the SD/REG data set and the S-SUM/S-VAR accessors.
* **`src/bases.rs`** — `Dec`/`Hex`/`Bin`/`Oct` word sizes and formatting.
* **`src/format.rs`** — approximates the two-line display (`Norm1`/`Norm2`,
  `Fix n`, `Sci n`).

The core crate is **std-only** by policy (see [DECISIONS.md](DECISIONS.md)).

## The `.fxc` front end

```text
source ──► #include ──► #data/#tests ──► lexer ──► parser ──► fold ──► unroll ──► validate ──► emit
           include.rs     data.rs         lexer.rs   parser.rs  fold.rs   unroll.rs  validate.rs  emit.rs
                                                          └── alloc.rs (memories)
```

Preprocessing is textual and happens before lexing: `#include` splices files,
then `#data`/`#tests` extract JSON and blank the directive (keeping newlines, so
diagnostics still point at the line the user wrote). Two byte-saving passes run
next: `fold.rs` pre-calculates constant expressions, and `unroll.rs` expands a
constant `for` loop whose body indexes an array, so its indices become literals.
`alloc.rs` then walks the program building the register table — which variable
occupies each of the seven memories — and rejects the lifetime mistakes (`free`
of an undeclared name, double free, use after free, running out of memory, a
checked `free` under re-entrant control flow — inside a loop body, or in a
program with `goto`/`label` — and an array index that is still not a literal)
before anything is emitted.

The language is documented in [FXC.md](FXC.md); the crate API in
[`crates/fx-transpiler/README.md`](../crates/fx-transpiler/README.md).

## The language server

`crates/fx-lsp` is one server for both languages. It routes per document on the
LSP `languageId` (`fx` → the interpreter's own lexer/parser/mode checker, `fxc` →
`fx-transpiler`), falling back to the file extension when a client sends none.
The CLI exposes it as `fx50 lsp`. Editor wiring is described in
[`editors/README.md`](../editors/README.md).

## Implemented features

| Area | Status |
| --- | --- |
| Arithmetic, implicit multiplication, fractions | ✅ |
| Trig / inverse trig / hyperbolic, `log`, `ln`, `√`, `∛`, `10^`, `e^`, `Abs`, `Pol`, `Rec`, `Rnd` | ✅ |
| Postfix `x² x³ x⁻¹ ! %` | ✅ |
| `nPr`, `nCr`, comparisons, `and`/`or`/`xor`/`xnor` | ✅ |
| Variables `A B C D X Y M Ans`, `M+`/`M-`, `ClrMemory` | ✅ |
| `?`, `→`, `:`, `◢` | ✅ |
| `Goto`/`Lbl`, `⇒`, `If/Then/Else/IfEnd`, `For/To/Step/Next`, `While/WhileEnd`, `Break` | ✅ |
| `Deg`/`Rad`/`Gra`, `Fix`/`Sci`/`Norm` | ✅ |
| Complex numbers (`i`, `∠`, `a+b𝒾`, `arg`, `Conjg`, `▶a+b𝒾`/`▶r∠θ`) | ✅ (CMPLX mode) |
| Statistics (`DT`, `Σx`, `x̄`, `σx`, `regA`…, `ClrStat`, `FreqOn`) | ✅ (SD/REG mode) |
| Base-n (`Dec`/`Hex`/`Bin`/`Oct`, `FFh`, bitwise ops) | ✅ (BASE mode) |
| Forced modes with a `#mode` header, CLI `--mode` and static checks | ✅ |
| 40 scientific constants (2010 CODATA) | ✅ |
| C-like front end: `const`, `free`/`unsafe_free`, `#data`, `#tests`, `#include` | ✅ |
| Full PRGM key coverage: postfix/infix keys, `stat.` and `phys.` namespaces, setup/clear/data keys, `and`-family, `⇒`, base literals | ✅ |
| `#include` for sharing fragments (transpile-time, C-style) | ✅ |
| `#data` / `#tests` compile-time JSON, and a zero-dependency JSON parser | ✅ |
| JSON test suites, embedded in the program or standalone (`fx50 test`) | ✅ |
| Memory plan report (`fx50 regs`) | ✅ |
| Language server (`fx50 lsp`) | ✅ (PRGM **and** `.fxc`) |
| 15-digit rounding + autocorrection | ✅ (f64-based — see note) |
| Exact decimal-arithmetic chains from the reference notes | ⚠️ approximate |
