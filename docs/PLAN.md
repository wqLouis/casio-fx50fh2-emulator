# fx-50FH II workspace plan

> **Status: all three workstreams are complete.** CORE (complex numbers,
> statistics, base-n, numeric fidelity), TRANSPILER and LSP have all landed.
> The plan below is kept as the record of what was specified and agreed; see
> the repository `README.md` for the current feature matrix and
> `docs/DECISIONS.md` for the design choices (including where an existing
> crate was adopted over hand-rolled code).

This document is the shared contract for three parallel workstreams. Read it
completely before starting. If something here is ambiguous, prefer the real
calculator's behaviour and document any deviation.

## Repository layout

```
Cargo.toml                  root package `casio-fx50fh2` + [workspace]
src/                        CORE interpreter (lexer, parser, ast, runtime)
crates/fx-transpiler/       TRANSPILER crate (lib `fx_transpiler` + bin `fxc`)
crates/fx-lsp/              LSP crate (bin `fx-lsp`)
examples/  docs/  tests/
```

Run everything from the repository root. Use a **private target directory** to
avoid clashing with the other workstreams:

```bash
CARGO_TARGET_DIR=/tmp/target-<yourname> cargo build
CARGO_TARGET_DIR=/tmp/target-<yourname> cargo test
```

## Hard constraints

* **CORE adds no third-party dependencies** (std only).
* Keep these public items stable — other workstreams depend on them:
  `compile`, `run`, `evaluate`, `lexer::lex`, `parser::parse`, `ast::*`,
  `token::*`, `CalcError`, `Interpreter`, `Host`, `MockHost`, `Environment`.
  You may **add** variants/fields, but do not rename or remove existing ones.
* Any code that matches on `ast::Expr` / `ast::Stmt` / `token::TokenKind` must
  use a `_ =>` wildcard arm so new variants do not break other crates.

## Source syntax agreed by all workstreams

Programs may be written with calculator glyphs or ASCII aliases. Canonical
ASCII aliases (the transpiler emits these; the LSP completes them):

| Purpose | Glyph | ASCII alias |
| --- | --- | --- |
| assign | `→` | `->` |
| statement separator | `:` | `:` or newline |
| display | `◢` | `disp` |
| conditional jump | `⇒` | `=>` |
| fraction | `┘` | `/` |
| power | `^(` | `^(` or `^` |
| not equal | `≠` | `<>` |
| ≤ / ≥ | `≤` `≥` | `<=` `>=` |
| multiply / divide | `×` `÷` | `*` `/` |

---

# Workstream CORE — features 1–4

Scope: `src/`, `tests/`, `examples/`. Keep `cargo test` green after each
feature; leave the tree compiling when you pause.

## Feature 1 — complex numbers

Introduce a value type so every expression can be real or complex.

* New `src/value.rs` with `pub enum Value { Real(f64), Complex(f64, f64) }`
  and the arithmetic helpers: `add sub mul div neg abs arg conjg sqrt cube
  square inverse is_zero is_real` plus `Display`-style formatting helpers.
  `runtime::Interpreter::eval` returns `Value`; comparisons output `Real(0/1)`.
* `<expr> i` is the imaginary unit; `i` is a constant.
* `r∠θ` is a polar literal (infix `∠`). Angle unit follows `Deg`/`Rad`/`Gra`.
* `Abs(x)`, `arg(x)`, `Conjg(x)`.
* `√` of a negative real yields a complex result.
* Arithmetic `+ − × ÷ x² x³ x⁻¹` must work on complex operands.
* `=`, `≠` are valid on complex; `<`, `>`, `≤`, `≥` on a complex operand are
  `Math ERROR`. Real-only functions (`sin`, `log`, `nPr`, `!`, …) given a
  complex argument are `Math ERROR`.
* Display of complex uses `Environment.complex_format`:
  `ComplexFormat::Cartesian` → `a+b𝑖` (omit the real part when 0, use `-` when
  the imaginary part is negative) and `ComplexFormat::Polar` → `r∠θ`.
  Add setup commands `▶a+b𝒾` / `▶r∠θ` with ASCII aliases `>a+bi` / `>rangle`
  that set the format. (Document that real calculators switch with these keys.)
* `Pol(r, θ)`/`Rec(x, y)` still write real results into `X` and `Y`.

## Feature 2 — statistics

New `src/stats.rs`, owned by the core.

* `pub struct Stats` storing data as `(x, y, freq)` triples, plus `freq_on`
  and a regression selector (`RegType::{Lin, …}` — implement `Lin` fully and
  leave the others as explicit `todo`-free errors or implement if cheap).
* `add(x, y, freq)` appends; `clr()` clears.
* Accessors returning `f64`: `n`, `sum_x`, `sum_x2`, `sum_y`, `sum_y2`,
  `sum_xy`, `mean_x`, `mean_y`, `sigma_x`, `sigma_y` (population),
  `s_x`, `s_y` (sample), `min_x`, `max_x`, `min_y`, `max_y`,
  and for linear regression `reg_a`, `reg_b`, `reg_r`, `est_y(x)`, `est_x(y)`.
* New statement `Stmt::DataEntry { x, y: Option<Expr>, freq: Option<Expr> }`
  parsed from `x DT` (SD) or `x,y;f DT` (REG) — ASCII `DT` keyword already
  lexes to `TokenKind::DT`.
* New expression `Expr::StatVar(StatVar)` for `n Σx Σx² Σy Σy² Σxy x̄ ȳ
  σx σy sx sy minX maxX minY maxY regA regB regR`. ASCII names:
  `n sumx sumx2 sumy sumy2 sumxy meanx meany sigmax sigmay sx sy
  minx maxx miny maxy rega regb regr`. Add `TokenKind::StatVar(StatVar)`.
* `ClrStat` clears; `FreqOn`/`FreqOff` toggle frequency mode.
* Statistics always operate on real values; a complex operand is `Math ERROR`.

## Feature 3 — base-n

New `src/bases.rs`, owned by the core.

* `pub enum Base { Dec, Hex, Bin, Oct }` with `radix()`, `bits()`
  (Dec 32, Hex 32, Bin 10, Oct 30), `wrap(f64) -> f64` (signed word),
  `to_word(f64) -> u32`, `from_word(u32) -> f64`, `format(f64) -> String`
  (suffix `d`, `h`, `b`, `o`), and `parse(text) -> Option<f64>` for tagged
  literals like `1Fh`, `1010b`, `17o`.
* `Environment.base: Option<Base>`; `None` means ordinary arithmetic.
* Commands `Dec`, `Hex`, `Bin`, `Oct` (`Stmt::Setup` variants) set `base`.
* While `base` is `Some`, integer arithmetic wraps to the word size and
  results display in that base with the suffix.
* `and`, `or`, `xor`, `xnor` become **bitwise** operators, valid only while a
  base is selected (otherwise `Math ERROR`). `Not(x)` and `Neg(x)` functions
  are added for base mode.
* Update the existing logical-op tests accordingly.

## Feature 4 — numeric fidelity

New `src/precision.rs`, owned by the core.

* `round15(x: f64) -> f64` — round to 15 significant digits.
* `autocorrect(x: f64) -> f64` — implement the fx-50FH II autocorrection
  described in `KeroppiMomo/calsimtor`'s `DOC.md` (summarised below):
  * if the last four significant digits `LMNO` satisfy `0 ≤ LMNO ≤ 9`,
    round down to 11 significant figures;
  * if `9991 ≤ LMNO ≤ 9999`, round up to 11 significant figures;
  * if the first 13 significant digits are zero, normalise to zero.
* `normalize(x) -> f64` = `autocorrect(round15(x))`, used after **every**
  arithmetic operation (replace the current `r15`).
* Implement `Norm1`/`Norm2`/`Fix`/`Sci` display thresholds exactly
  (Norm1 switches to scientific outside `[1e-2, 1e10)`, Norm2 outside
  `[1e-9, 1e10)`).
* Add tests directly from the `DOC.md` observations
  (`0.8 + 1E-15` style cases).

## Definition of done for CORE

* `cargo test` passes (all existing 18 tests still pass, adjusted only where
  this plan explicitly changes behaviour).
* `cargo clippy --all-targets` clean.
* New end-to-end tests in `tests/` for each feature.
* `docs/LANGUAGE.md` updated: remove finished items from the TODO list and
  document the new syntax.

---

# Workstream TRANSPILER — `fxc`

Scope: `crates/fx-transpiler/` only. This crate must build with
`--no-default-features` (no dependency on the interpreter). The `execute`
feature (default on) adds `fxc run` via the core crate.

## Input language (`.fxc`)

A small C-like language over real numbers:

```c
// line comment
let a = input();            // ?→A
a = a + 1;                  // a+1→A
print(a * 2);               // (a*2)◢
if (a > 0) { print(1); } else { print(0); }
while (a > 0) { a = a - 1; print(a); }
for (let i = 0; i < 5; i = i + 1) { print(i); }
break;
label 1;
goto 1;
```

* Statements: `let NAME = expr;`, `NAME = expr;`, `print(expr);`,
  `input()` (expression), `if/else`, `while`, `for`, `break`, `goto N`,
  `label N;`, blocks `{ ... }`, empty `;`.
* Expressions: `+ - * /`, `^` or `**` (power), unary `-`, comparisons
  `< <= > >= == !=`, parentheses, numeric literals, `pi`, `e`, names,
  calls `sqrt(x) abs(x) sin(x) cos(x) tan(x) asin(x) acos(x) atan(x)
  log(x) ln(x) rnd(x)`.
* Types: all values are real (doubles). No strings.

## Variable allocation

PRGM has exactly seven variables `A B C D X Y M`. Assign each declared `let`
name a unique calculator variable (deterministic order). If more than seven
are live at once, emit a clear error. Reuse is allowed once a name leaves
scope *only if* you can do so soundly; otherwise error.

## Translation rules

* `let x = input()` → `?→X`; `x = input()` → `?→X`.
* `print(e)` → `<e>◢`.
* `x = e` → `<e>→X`.
* `+ - * /` → `+ - × ÷`; `^`/`**` → `^( ... )`.
* `== != < <= > >=` → `= ≠ < ≤ > ≥`.
* `if (c) {..} else {..}` → `If <c>` `Then` `..`
  `Else` `..` `IfEnd` (always emit `Then`).
* `while (c)` → `While <c>` … `WhileEnd`.
* `for (let i = A; i < B; i = i + C)` → `For <A>→<var> To (B-1)
  Step C` … `Next` when the update is a positive `+C`; use `<=`→`To B`,
  and `>`/`>=` → negative step. If the shape does not match, fall back to a
  `While` loop.
* `break` → `Break`; `goto N` → `Goto N`; `label N;` → `Lbl N`.
* Statements separated by `:` (or newlines) in the emitted PRGM.

Output the **glyph** form by default and an ASCII-alias form with `--ascii`.

## API and binary

```rust
pub fn transpile(source: &str) -> Result<String, TranspileError>;
pub fn transpile_with(source: &str, opts: Options) -> Result<String, TranspileError>;
```

`TranspileError` must carry a message and a byte/line position.

`fxc build <file.fxc>` prints PRGM to stdout; `fxc run <file.fxc>` transpiles
and executes it (feature `execute`), reading `?` inputs from stdin.

## Definition of done for TRANSPILER

* Golden tests: `.fxc` → expected PRGM for each construct.
* Execution tests (feature `execute`) that run transpiled programs and check
  the output, e.g. factorial and a loop sum.
* `cargo test -p fx-transpiler --no-default-features` and with the feature.
* A short `crates/fx-transpiler/README.md` with examples.

---

# Workstream LSP — `fx-lsp`

Scope: `crates/fx-lsp/` only. Depends on the core crate for `lexer::lex` and
`parser::parse`.

## Server

Implement a stdio JSON-RPC language server. `tower-lsp` + `tokio` +
`lsp-types` are available from crates.io; `lsp-server` is also acceptable.
Hand-rolled JSON-RPC over stdio is fine if you prefer no async runtime.

## Capabilities

* `textDocument/publishDiagnostics`: lex and parse the document; map each
  error's byte offset to an LSP `Position` (line + UTF-16 code unit) and
  publish a diagnostic with the calculator's error label
  (`Syntax ERROR`, `Math ERROR`, …).
* `textDocument/completion`: keywords (`If Then Else IfEnd For To Step Next
  While WhileEnd Break Goto Lbl ClrMemory ClrStat`), functions (`sin( cos(
  tan( log( ln( √( ∛( Abs( Pol( Rec( Rnd(` …), variables (`A B C D X Y M Ans`),
  and the ASCII aliases from the table above.
* `textDocument/hover`: describe the token under the cursor.
* `textDocument/documentSymbol`: one symbol per `Lbl`/label in the program.
* `initialize` must advertise exactly the capabilities implemented.

Put the pure logic (offset→position, diagnostics from source, completion
items, hover text) in a testable module separate from the server loop.

## Definition of done for LSP

* Unit tests for the pure logic (a bad program yields the expected
  diagnostic range; completion contains expected labels).
* The server starts, answers `initialize`, and shuts down cleanly.
* A short `crates/fx-lsp/README.md` with editor setup (Neovim / VS Code) and
  an end-to-end manual check.

---

# Coordination protocol

* Stay inside your scope. The other agents are editing other directories.
* Before your final turn, make sure your own crate builds and its tests pass.
* If building the **core** crate fails because of another workstream's
  in-progress edits (errors in `src/`), that is expected: retry after a short
  sleep, up to ~10 minutes, before reporting a blocker.
* Report back with: what you implemented, the exact test command you ran, the
  result, and anything you deliberately left out.
