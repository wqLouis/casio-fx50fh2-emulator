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
crates/fx-wasm/             WebAssembly build of all of the above (library `fx_wasm`)
web/                        the browser workbench: a page, and the generated module
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

### The file seam

Two language features read files: `#include` pulls in a library of `fn`
definitions, and `#data`/`#tests` read a JSON value. Both go through
[`fx_transpiler::FileLoader`](../crates/fx-transpiler/src/loader.rs) rather than
calling `std::fs` directly.

The default is `FsLoader`, so nothing on a command line changed. `MemoryLoader`
replaces the filesystem with a map of path to text, which is what lets the same
transpiler run in a browser — where there is no filesystem at all — against an
editor's unsaved buffers. Path resolution is *lexical* (`.` and `..` are resolved
from the text of the path), so include-cycle detection gives the same answer
everywhere instead of depending on symlinks and what happens to exist.

Every entry point has a `*_with_loader` form; the plain ones are those with
`FsLoader`. See [ADR 0030](DECISIONS.md).

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
  autocorrection, applied after every arithmetic operation. This is the
  interpreter's hot path, so it has two implementations: an **integer-mantissa
  fast path** that computes the 15 digits by scaling with an exactly
  representable power of ten (one correctly-rounded multiply or divide), and the
  original decimal round-trip as a fallback. The fast path is taken only when it
  can be *proved* identical — the scaled value must land strictly inside
  `[10^14, 10^15)` and its fractional part must be far enough from the rounding
  boundary that one ulp of error cannot flip the decision, which also excludes
  exact ties (decimal parsing breaks them to even, `f64::round` away from zero).
  A cheap exponent check rejects values outside the window before doing any
  work, so large and tiny magnitudes pay nothing.

  The payoff depends on magnitude: **~4×** for the magnitudes ordinary
  calculations use (`1e-10..1e20`, where it applies ~93% of the time), ~1.3×
  spread evenly over the machine's full `1e-99..1e99`, and no change outside,
  since those values never enter the fast path. Coverage and correctness are
  tested against a verbatim copy of the original implementation over ~880 000
  values plus >12 000 probes sitting exactly on the autocorrection boundaries.
* **`src/stats.rs`** — the SD/REG data set and the S-SUM/S-VAR accessors.
* **`src/bases.rs`** — `Dec`/`Hex`/`Bin`/`Oct` word sizes and formatting.
* **`src/format.rs`** — approximates the two-line display (`Norm1`/`Norm2`,
  `Fix n`, `Sci n`).

The core crate is **std-only** by policy (see [DECISIONS.md](DECISIONS.md)).

## The `.fxc` front end

```text
source ─► #include ─► #data/#tests ─► lexer ─► parser ─► functions
         include.rs   data.rs        lexer.rs  parser.rs functions.rs
  ─► fold ─► simplify ─► propagate ─► fold ─► unroll ─► validate ─► emit
     fold.rs simplify.rs propagate.rs fold.rs unroll.rs validate.rs emit.rs
                                     └── alloc.rs (memories)
```

Preprocessing is textual and happens before lexing: `#include` splices files,
then `#data`/`#tests` extract JSON and blank the directive (keeping newlines, so
diagnostics still point at the line the user wrote). `#include` is a top-level
directive, and an included file is a **library** — `fn` definitions to be
called, never statements spliced into a body (ADR 0026). `functions.rs` then
resolves the `fn main()` entry point and inlines every user function call (see
ADR 0022). A file with no `main` is a library and expands to an empty program,
so a library is buildable and checkable on its own.

The passes that follow exist because the machine has only **680 bytes of program
storage shared by all four program areas**, and it stores one byte per key:

* `fold.rs` pre-calculates constant expressions, and `unroll.rs` expands a
  constant `for` loop whose body indexes an array, so its indices become
  literals. These are **required**, not optional — a `const`'s value has to be
  folded for the emitter, and an array index has to be a literal for the element
  to name a memory at all.
* `simplify.rs` drops operators that cannot change a result (`x * 1`, `x + 0`,
  `-(-x)`), and `propagate.rs` replaces a read of a never-assigned constant with
  its value — which lets `fold` collapse the arithmetic around it, decide a
  constant `if`/`while`, and drop stores nothing reads. These are **optional**
  and on by default; `Options::optimize = false` (CLI `--no-optimize`) turns
  them off, which is how the golden tests pin each construct's own spelling
  (ADR 0023).
* `size.rs` is not a pass but the metric: it counts the keys a listing costs, so
  an optimisation can be judged rather than assumed (ADR 0024). `fx50 size`
  reports it, always next to what the unoptimised translation would have cost.

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
| User-defined functions (`fn`, inlined; `fn main()` entry point, scoped bodies) | ✅ |
| Full PRGM key coverage: postfix/infix keys, `stat.` and `phys.` namespaces, setup/clear/data keys, `and`-family, `⇒`, base literals | ✅ |
| `#include` for sharing libraries (transpile-time, top-level, C-style) | ✅ |
| `#data` / `#tests` compile-time JSON, and a zero-dependency JSON parser | ✅ |
| JSON test suites, embedded in the program or standalone (`fx50 test`) | ✅ |
| Memory plan report (`fx50 regs`) | ✅ |
| Language server (`fx50 lsp`) | ✅ (PRGM **and** `.fxc`) |
| 15-digit rounding + autocorrection | ✅ (f64-based — see note) |
| Exact decimal-arithmetic chains from the reference notes | ⚠️ approximate |
