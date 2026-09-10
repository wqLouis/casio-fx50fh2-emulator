# casio-fx50fh2

A clean-room toolkit for the **CASIO fx-50FH II** programmable scientific
calculator, in Rust: an interpreter for the calculator's own PRGM language, a
transpiler from a friendly C-like language (`fx50 run program.fxc`), and a
language server for editor support.

The interpreter covers expression evaluation, variables, `?` input, `◢`
display, complex numbers, statistics, base-n, `Goto`/`Lbl`, `⇒`,
`If`/`Then`/`Else`/`IfEnd`, `For`/`Next`, `While`/`WhileEnd`, `Break`, memory
arithmetic and the setup commands.

Programs can be typed with the calculator's glyphs or with readable ASCII
aliases, so both of these are the same program:

```text
5→A: A+1◢
5->A: A+1disp
```

## Usage

The whole toolkit is one binary, `fx50`. With no arguments it behaves like
`python`: an interactive REPL when stdin is a terminal, or "run whatever is
piped in" otherwise.

```bash
cargo build --release

# interactive REPL (state persists between lines, with history)
./target/release/fx50

# run a program file — inputs for `?` are read from stdin
printf '5\n' | ./target/release/fx50 run examples/factorial.fx

# evaluate a single expression
./target/release/fx50 eval "2+3×4^2"

# write a program in C-like syntax and run it directly
./target/release/fx50 run examples/factorial.fxc

# transpile C-like source to PRGM
./target/release/fx50 build program.fxc > program.fx
./target/release/fx50 build --ascii program.fxc

# language server over stdio, and shell completions
./target/release/fx50 lsp
./target/release/fx50 completions bash
```

Every command also has a flag form (`--eval/-e`, `--build/-b`, `--lsp/-l`).
Errors are rendered with a caret at the offending source position.

As a library:

```rust
use casio_fx50fh2::{Interpreter, MockHost, compile};

let program = compile("?→A: A×2◢").unwrap();
let mut interp = Interpreter::new(program, MockHost::with_inputs([21.0]));
interp.run().unwrap();
assert_eq!(interp.host().output, vec!["42"]);
```

## The C-like language (`fx50 run x.fxc`)

Real numbers only, with `let`, assignment, `print`, `input`, `if`/`else`,
`while`, `for`, `break`, `goto`/`label`, and `sqrt/abs/sin/cos/tan/log/ln/rnd`
built-ins. The transpiler maps your names onto the seven real memories
`A B C D X Y M` and emits PRGM.

```c
let n = input();
let result = 1;
for (let i = 1; i <= n; i = i + 1) {
  result = result * i;
}
print(result);
```

```console
$ fx50 build examples/factorial.fxc
?→A
1→B
For 1→C To A Step 1
B×C→B
Next
B◢
$ printf '5\n' | fx50 run examples/factorial.fxc
120
```

See [`crates/fx-transpiler/README.md`](crates/fx-transpiler/README.md) for the
full language, and [`crates/fx-lsp/README.md`](crates/fx-lsp/README.md) for
editor setup.

## Design

The repository is a Cargo workspace:

```
src/                        the interpreter library (casio-fx50fh2)
crates/fx-transpiler/       C-like language -> PRGM (library `fx_transpiler`)
crates/fx-lsp/              language server (library `fx_lsp`)
crates/fx-cli/              the single `fx50` binary, dispatching on subcommands
examples/  docs/  tests/
```

The binary lives in its own crate because both `fx-transpiler` and `fx-lsp`
depend on the core library; putting `fx50` in the core crate would create a
dependency cycle.

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

The reference works used to reconstruct the language are listed in
[`docs/LANGUAGE.md`](docs/LANGUAGE.md); design decisions in
[`docs/DECISIONS.md`](docs/DECISIONS.md).

## Supported today

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
| Complex numbers (`i`, `∠`, `a+b𝒾`, `arg`, `Conjg`, `▶a+b𝒾`/`▶r∠θ`) | ✅ |
| Statistics (`DT`, `Σx`, `x̄`, `σx`, `regA`…, `ClrStat`, `FreqOn`) | ✅ |
| Base-n (`Dec`/`Hex`/`Bin`/`Oct`, `1Fh`, bitwise ops) | ✅ |
| 15-digit rounding + autocorrection | ✅ (f64-based — see note) |
| C-like front end (`fx50 run x.fxc`) | ✅ |
| Language server (`fx50 lsp`) | ✅ |
| 40 physical constants | ⛔ not yet |
| Exact decimal-arithmetic chains from the reference notes | ⚠️ approximate |

## Tests

```bash
cargo test --workspace
```

138 tests: interpreter end-to-end tests, plus per-feature suites for complex
numbers, statistics, base-n and numeric precision, 36 golden transpiler tests
with execution tests, and LSP logic + server tests.

## Examples

`examples/` contains working programs in PRGM (`factorial.fx`, `fibonacci.fx`,
`gcd.fx`, `quadratic.fx`, `complex_quadratic.fx`, `statistics.fx`) and in the
C-like language (`factorial.fxc`, `quadratic.fxc`).
