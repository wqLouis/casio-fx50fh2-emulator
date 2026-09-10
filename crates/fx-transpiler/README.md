# fx-transpiler

`fx-transpiler` translates a small C-like language (`.fxc`) into CASIO
fx-50FH II **PRGM** source. It ships as a library (`fx_transpiler`) and a thin
command-line tool (`fxc`).

The crate is independent of the interpreter: with

```bash
cargo build -p fx-transpiler --no-default-features
```

it has **no** dependency on `casio-fx50fh2`. The default `execute` feature adds
`fxc run`, which pipes the generated PRGM through the interpreter.

## CLI

```bash
fxc build program.fxc          # print PRGM (calculator glyphs) to stdout
fxc build --ascii program.fxc  # print ASCII aliases instead
fxc run   program.fxc          # transpile, then execute (? reads from stdin)
```

Example:

```console
$ cat factorial.fxc
let n = input();
let result = 1;
for (let i = 1; i <= n; i = i + 1) { result = result * i; }
print(result);

$ fxc build factorial.fxc
?→A
1→B
For 1→C To A Step 1
B×C→B
Next
B◢

$ echo 5 | fxc run factorial.fxc
? 120
```

With `--ascii` the same program is emitted using only keyboard-typable
characters:

```
?->A
1->B
For 1->C To A Step 1
B*C->B
Next
Bdisp
```

## Language

```c
// line comment, and /* block comments */
let a = input();          // read a number
let b = 2;                // declaration
a = a + b * 2;            // assignment
print(a);                 // display with ◢
if (a > 0) { print(1); } else { print(0); }
while (a > 0) { a = a - 1; }
for (let i = 0; i < 5; i = i + 1) { print(i); }
break;
label 1;
goto 1;
```

Expressions support `+ - * /`, `^` / `**` (power), unary `-`, comparisons
`== != < <= > >=`, parentheses, the constants `pi` and `e`, and the built-ins
`sqrt cbrt abs sin cos tan asin acos atan sinh cosh tanh asinh acosh atanh
log ln rnd` (`log` accepts one or two arguments). Values are real numbers.

### Variable allocation

PRGM has exactly seven memories — `A B C D X Y M`. Every distinct `.fxc` name is
assigned one of them in **first-seen order** (the whole program is scanned
before emitting). A name keeps its memory for the whole program, so no scope
analysis is needed. An eighth distinct name is a compile error pointing at that
name.

### Translation rules

| `.fxc`                             | PRGM (glyph / ASCII)                                   |
| ---------------------------------- | ------------------------------------------------------ |
| `let x = input();` / `x = input();`| `?→X` / `?->X`                                         |
| `x = e;`                           | `<e>→X` / `<e>->X`                                     |
| `print(e);`                        | `<e>◢` / `<e>disp`                                     |
| `e;`                               | `<e>`                                                  |
| `+ - * /`                          | `+ - × ÷` / `+ - * /`                                  |
| `^` / `**`                         | `<base>^(<exp>)`                                       |
| `== != < <= > >=`                  | `= ≠ < ≤ > ≥` / `= <> < <= > >=`                       |
| `if (c) {..} else {..}`            | `If <c>` `Then` `..` `Else` `..` `IfEnd`               |
| `while (c) {..}`                   | `While <c>` `..` `WhileEnd`                            |
| `for (i = a; i < b; i = i + s)`    | `For <a>→<v> To <b>-1 Step <s>` `..` `Next`           |
| `break;` / `goto N;` / `label N;`  | `Break` / `Goto N` / `Lbl N`                           |
| `pi` / `e`                         | `π` / `e` (ASCII: `pi` / `e`)                          |

`Then` is always emitted after every `If`. A `for` whose header does not match
the canonical shape (for example a `!=` bound, or an update that mutates a
different variable) falls back to an equivalent `While` loop:

```c
for (let i = 0; i != 5; i = i + 2) { print(i); }
```

```
0→A
While A≠5
A◢
A+2→A
WhileEnd
```

## Library

```rust
use fx_transpiler::{transpile, transpile_with, Options};

// Calculator glyphs (default).
let prgm = transpile("let a = input(); print(a * 2);")?;
assert_eq!(prgm, "?→A\nA×2◢\n");

// ASCII aliases.
let prgm = transpile_with("print(a <= b);", Options { ascii: true })?;
assert_eq!(prgm, "A<=Bdisp\n");
# Ok::<(), fx_transpiler::TranspileError>(())
```

`TranspileError` carries a message plus the byte offset, 1-based line and
column of the offending source. The pipeline stages are exposed as
`lexer::lex`, `parser::parse`, `ast`, and `builtins::lookup`.

## Tests

```bash
# Library + golden tests only (no interpreter dependency).
cargo test -p fx-transpiler --no-default-features

# Everything, including execution tests that run transpiled programs.
cargo test -p fx-transpiler
```

`tests/golden.rs` pins the exact PRGM for every construct in both glyph and
ASCII modes. `tests/execute.rs` (feature `execute`) transpiles a factorial, a
loop sum, ascending/descending `for` loops, `while`/`if`/`break`, `goto` and the
built-ins, then runs them on the interpreter and checks the displayed output.
