# fx-transpiler

`fx-transpiler` translates a small C-like language (`.fxc`) into CASIO
fx-50FH II **PRGM** source. It is a library (`fx_transpiler`); the user-facing
commands are subcommands of the unified `fx50` binary:

```bash
fx50 build program.fxc          # print PRGM (calculator glyphs) to stdout
fx50 build --ascii program.fxc  # print ASCII aliases instead
fx50 run   program.fxc          # transpile, then execute (? reads from stdin)
```

The crate is independent of the interpreter: with

```bash
cargo build -p fx-transpiler --no-default-features
```

it has **no** dependency on `casio-fx50fh2`. The `execute` feature (on by
default) is what lets the interpreter-backed tests in `tests/execute.rs`
compile and run; it is also what the `fx50` binary uses for `fx50 run`.

## CLI

```bash
fx50 build program.fxc          # print PRGM (calculator glyphs) to stdout
fx50 build --ascii program.fxc  # print ASCII aliases instead
fx50 run   program.fxc          # transpile, then execute (? reads from stdin)
```

Example:

```console
$ cat factorial.fxc
let n = input();
let result = 1;
for (let i = 1; i <= n; i = i + 1) { result = result * i; }
print(result);

$ fx50 build factorial.fxc
?→A
1→B
For 1→C To A Step 1
B×C→B
Next
B◢

$ echo 5 | fx50 run factorial.fxc
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

> Authoring `.fxc` source, especially from an AI agent? Read
> [`docs/AI-AGENTS.md`](../../docs/AI-AGENTS.md) first. It is a rule-oriented
guide covering the grammar, the seven-variable limit, the modes, and the
gotchas that most often make generated programs fail to transpile.

## Modes

The calculator forces an operating mode before it will compute. A program may
declare its mode with a leading `#mode` directive, which must be the first
non-comment, non-blank line:

```c
#mode CMPLX
let a = input();
print(a + 1);
```

The valid names are `COMP`, `CMPLX`, `BASE`, `SD` and `REG` (case-insensitive;
`STAT` is an alias for `SD`). `COMP` is the default, so a program without a
directive emits no header. When a header is present it is re-emitted as the
first line of the PRGM, and the program is checked against the mode's
capabilities. `.fxc` is real-number-only, so the only restriction is `BASE`,
which rejects the floating-point built-ins `sqrt sin cos tan asin acos atan log
ln rnd` and the constants `pi`/`e` (`/` maps to `÷`, which is fine):

```c
#mode BASE
print(a / b);   // ok
print(sqrt(a)); // error: `sqrt` is not available in BASE mode
```

The mode can also be forced from Rust with [`Options::mode`](#library), which
takes precedence over any header in the source.

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

## Testing programs with JSON

A program can be regression-tested against a JSON suite that pairs inputs with
the display lines they should produce. Put the suite next to the program as
`<name>.tests.json`:

```json
{
  "program": "factorial.fxc",
  "cases": [
    { "name": "5! = 120", "input": [5], "output": ["120"] },
    { "name": "0! = 1",   "input": [0], "output": ["1"] },
    { "name": "no input", "input": [],  "error": "Argument ERROR" }
  ]
}
```

```console
$ fx50 test examples/factorial.fxc
factorial
  ok    5! = 120
  ok    1! = 1
  ok    0! = 1
  ok    10! = 3628800
  ok    no input given is an Argument ERROR
5 passed, 0 failed
```

The command exits non-zero when any case fails, so it drops straight into CI.
It also accepts a `.tests.json` path directly, and takes `--filter TEXT` (run
only cases whose name contains `TEXT`) and `--json` (machine-readable report):

```bash
fx50 test examples/quadratic.tests.json
fx50 test prog.fxc --filter "roots"
fx50 test prog.fxc --json
```

A failing case shows both sides:

```text
prog.tests.json
  ok    doubles 21
  FAIL  deliberately wrong
        expected: 11
        actual: 10
1 passed, 1 failed
```

### Suite schema

| Field | Required | Meaning |
| --- | --- | --- |
| `name` | no | Suite label shown in the report. |
| `program` | one of | Path to a `.fxc` file, resolved relative to the JSON file. |
| `source` | one of | Inline `.fxc` source instead of a file. |
| `mode` | no | Operating mode override (`COMP`, `CMPLX`, `BASE`, `SD`, `REG`). |
| `ascii` | no | Transpile with ASCII aliases. Default `false`. |
| `cases` | yes | The list of cases. |

`program` and `source` are mutually exclusive; when both are absent, the
sibling `.fxc` of the suite file is used. Each case takes:

| Field | Required | Meaning |
| --- | --- | --- |
| `name` | no | Case label. Defaults to `case N`. |
| `input` | no | Numbers fed to `?` prompts, in order. Default `[]`. |
| `output` | one of | Expected `◢` display lines, in order. |
| `error` | one of | Expected error, matched case-insensitively against the label. |

A case must give exactly one of `output` or `error`. `"output": []` asserts
that the program displays nothing — note that a program ending in an
assignment still displays its value, because the calculator shows the last
computed value when a program ends without `◢`.

### Library

The runner is behind the `testing` feature (on by default, and implied by
`execute`):

```rust
use fx_transpiler::testing::{run_suite_file, parse_suite};
use std::path::Path;

let report = run_suite_file(Path::new("examples/factorial.tests.json"), None)?;
assert!(report.is_success());
println!("{}", report.to_json_pretty());

// Or build a suite from inline JSON.
let suite = parse_suite(
    r#"{"source": "print(1);", "cases": [{"output": ["1"]}]}"#,
    "inline",
    Path::new("."),
    None,
)?;
assert!(fx_transpiler::testing::run_suite(&suite).is_success());
# Ok::<(), fx_transpiler::testing::TestError>(())
```

`TestSuite`, `TestCase`, `SuiteReport` and `CaseResult` are public, and the
report serialises to JSON.

## Library

```rust
use fx_transpiler::{transpile, transpile_with, Mode, Options};

// Calculator glyphs (default).
let prgm = transpile("let a = input(); print(a * 2);")?;
assert_eq!(prgm, "?→A\nA×2◢\n");

// ASCII aliases.
let prgm = transpile_with("print(a <= b);", Options { ascii: true, ..Default::default() })?;
assert_eq!(prgm, "A<=Bdisp\n");

// Force BASE mode (overrides any `#mode` header).
let prgm = transpile_with("print(a / b);", Options { mode: Some(Mode::Base), ..Default::default() })?;
assert_eq!(prgm, "#mode BASE\nA÷B◢\n");
# Ok::<(), fx_transpiler::error::TranspileError>(())
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
