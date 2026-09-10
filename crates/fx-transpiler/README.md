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

## Scientific constants

The calculator's 40 built-in scientific constants are reached through the
`phys` namespace: `phys.NAME`, where `NAME` is the constant's ASCII name or the
symbol the display shows. `phys` is a reserved word, and a bare `h` or `hbar`
is an ordinary variable, so the namespace never pollutes the seven memories.

```c
print(phys.h);      // Planck constant
print(phys.ħ);      // the same constant, by its display symbol
print(phys.C0);     // speed of light in vacuum
print(phys.e);      // elementary charge (namespaced, so not Euler's e)
```

```
h◢
ħ◢
C0◢
eq◢
```

Glyph output uses the display symbol (`ħ`, `R∞`, `μμ`); `--ascii` uses the
ASCII name (`hbar`, `Rinf`, `mumu`). The elementary charge is the exception:
its display symbol `e` would re-lex as Euler's number, so it is emitted as `eq`
in both styles. `phys.e` and `phys.eq` are the elementary charge; Euler's number
remains just `e`.

The full table (menu number, ASCII name, display symbol) is available as
`fx_transpiler::constants::CONSTANTS`.

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
ln rnd` and the constants `pi`/`e` as well as every `phys.` scientific constant
(`/` maps to `÷`, which is fine):

```c
#mode BASE
print(a / b);   // ok
print(sqrt(a)); // error: `sqrt` is not available in BASE mode
print(phys.h);  // error: `h` (Planck constant) is not available in BASE mode
```

The mode can also be forced from Rust with [`Options::mode`](#library), which
takes precedence over any header in the source.

### Variable allocation

PRGM has exactly seven memories — `A B C D X Y M`. Every distinct `.fxc` name is
assigned one of them in **first-seen order** (the whole program is scanned before
emitting), and a name keeps its memory until the program ends or you release it
with `free`.

Names are never given a shared memory implicitly. A memory's final value is part
of the program's observable result — PRGM leaves its answer in one, a later
program or the user can read it — so the transpiler cannot prove a name is dead.
Only you know when a value is finished with, and saying so is what `free` is for.

The way to fit a program is therefore, in order of preference:

* **`const NAME = <expr>;`** declares a fixed value that is **inlined** at every
  use and occupies no memory. The expression must be constant — numbers, `pi`,
  `e`, `phys.` constants, `#data` values and other `const`s.
* **`#data NAME = <json>;`** resolves a JSON value at transpile time, likewise
  producing literals. See [Compile-time data](#compile-time-data).
* **`free NAME;`** releases a variable's memory so the next new variable can use
  it:

  ```c
  let first = 5;
  print(first);
  free first;        // release the memory
  let second = 7;    // reuses it
  print(second);
  ```

  `free` emits nothing; it hands the memory back to the allocator. The value
  left in the memory is untouched, exactly as on the calculator.

`fx50 regs FILE` reports the plan and the memories left over:

```console
$ fx50 regs examples/compiletime.fxc
Memory plan for examples/compiletime.fxc
  A  first  → second   (reused after `free first`)

  1 of 7 memories used; free: B C D X Y M
  released with `free`: first
  1 const (no memory): scale
  2 data table(s) (no memory): config, tests
```

### Errors the register table catches

Allocation keeps a table of which variable occupies each memory, so these are
transpile errors rather than surprises on the calculator:

| Mistake | Example | Message |
| --- | --- | --- |
| Use after free | `free a; print(a);` | `` `a` is not defined here: it was freed `` |
| Already declared | `let x = 1; let x = 2;` | `` `x` is already declared `` |
| Own initializer | `let x = x + 1;` | `` `x` cannot be used in its own initializer `` |
| Double free | `free a; free a;` | `` `a` was already freed (double free) `` |
| `free` with a jump | `free a; goto 1; label 1;` | `` use `unsafe_free` `` |
| Freeing what has no memory | `const k = 1; free k;` | `` `k` is a `const`, which uses no memory `` |
| Freeing an unknown name | `free nope;` | `` `nope` is not a variable `` |
| Running out | an eighth live variable | `` no free memory for `z`: … `` |

**`let` declares.** A `let` introduces a name, so a second `let` of a name that
is *still live* is an error rather than a silent shadow. Once the name has been
released, declaring it again is the intended way to reuse a name, and it takes a
fresh binding:

```c
let x = input();
free x;
let x = input();   // a second, independent life for `x`
```

A plain assignment (`x = ...`) never declares, so assigning to a released name
is an error. And a declaration cannot see its own name — `let x = x + 1;` is an
error, like Rust's `let x = x;`.

**Jumps: `unsafe_free`.** A jump can re-enter code whose memory has since been
released and given to another variable, so a checked `free` in a program
containing `goto`/`label` is an error. `unsafe_free` says "I have checked this
myself", borrowing Rust's convention:

```c
unsafe_free x;   // no control-flow check; still checked for double free etc.
```

Like Rust's `unsafe`, it waives one guarantee, not all checking: `unsafe_free`
still rejects double frees, unknown names and `const`s. Programs with jumps and
no `free` are unaffected, since nothing is ever re-used.

### Translation rules

| `.fxc`                             | PRGM (glyph / ASCII)                                   |
| ---------------------------------- | ------------------------------------------------------ |
| `let x = input();` / `x = input();`| `?→X` / `?->X`                                         |
| `x = e;`                           | `<e>→X` / `<e>->X`                                     |
| `const k = 12;`                    | *(nothing; `k` is replaced by `12` at each use)*       |
| `config.n` / `xs[0]`               | the number it resolves to                              |
| `free x;`                          | *(nothing; the allocator releases the memory)*         |
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

## Compile-time data

`#data` reads a JSON value — inline or from a file — while transpiling, so the
values become literals and use none of the seven memories:

```c
#data config = { "base": 2, "offsets": [10, 20, 30] };
const scale = config.base;

let total = config.offsets[1] * scale;
print(total);
```

```console
$ fx50 build offsets.fxc
20×2→A
A◢
```

A **top-level JSON string is a file path**, resolved relative to the file
containing the directive:

```c
#data offsets = "offsets.json";
```

Values are reached with `.field` and `[index]`; indices must be whole-number
literals. Only numbers and booleans can be used — a boolean is `1` or `0` — and
anything else is a transpile error naming the path that failed. The JSON parser
is the crate's own (`fx_transpiler::json`), so `fx-transpiler` has no
dependencies; it is strict, and rejects duplicate object keys, comments,
trailing commas and lone surrogates rather than guessing.

`#tests` is exactly `#data tests = ...`, which is how the test runner below is
an ordinary consumer of this facility rather than a special case.

## Sharing code with `#include`

A program can pull in another file's text at transpile time:

```c
// main.fxc
let n = input();
#include "lib/squares.fxc"
print(result);
```

```c
// lib/squares.fxc
let n_squared = n * n;
#include "increment.fxc"
```

`fx50 build main.fxc` inlines both fragments before compiling, so the
calculator only ever sees one flat program:

```text
?→A
A×A→B
B+1→C
C◢
```

This is the `.fxc` analogue of C's `#include` or Rust's `include_str!`. It is
useful for sharing a computation between several calculator programs.

Rules:

* The directive is `#include "path"` and must be the first thing on its line
  (leading whitespace is fine, and a trailing `// comment` is allowed).
* The path is resolved **relative to the file containing the directive**, so a
  fragment can include its own neighbours without knowing who included it.
* Includes nest, and a cycle is reported with the chain rather than looping.
* Expansion is textual and unguarded, exactly like C: including a file twice
  includes its text twice.
* **A fragment is statements, not a function.** `.fxc` has no user-defined
  functions, so a fragment reads and writes the same seven calculator memories
  as whatever included it. Names are allocated in expanded source order, so a
  fragment's variables are numbered where the `#include` line sits.
* `#mode` may only appear in the root file, since the mode applies to the whole
  program. A fragment that declares one is an error.

The include itself never reaches the calculator — only the expanded program
does. Diagnostics are reported against the file and line that actually caused
them, even when the error is inside a fragment:

```console
$ fx50 build main.fxc
fx50: unknown function `nope` (lib/squares.fxc:2:11)
```

From the library, `transpile_file` resolves includes relative to the file,
while `transpile_with_base` does the same for text you already hold:

```rust
# use std::path::Path;
# use fx_transpiler::{transpile_file, transpile_with_base, Options};
let prgm = transpile_file(Path::new("examples/include.fxc"), Options::default())?;
let prgm = transpile_with_base("let a = 1;\n#include \"frag.fxc\"", Options::default(), Path::new("src"))?;
# Ok::<(), fx_transpiler::error::TranspileError>(())
```

Note that plain [`transpile`]/[`transpile_with`] resolve includes relative to
the **current directory**, since a bare string carries no location.

## Testing programs with JSON

A program can carry its own test cases in a `#tests` table — the same
compile-time data facility as `#data`:

```c
// factorial.fxc
let n = input();
let result = 1;
for (let i = 1; i <= n; i = i + 1) { result = result * i; }
print(result);

#tests = [
  { "name": "5! = 120", "input": [5], "output": ["120"] },
  { "name": "0! = 1",   "input": [0], "output": ["1"] },
  { "name": "no input", "input": [],  "error": "Argument ERROR" }
];
```

```console
$ fx50 test examples/factorial.fxc
factorial.fxc
  ok    5! = 120
  ok    1! = 1
  ok    0! = 1
  ok    10! = 3628800
  ok    no input given is an Argument ERROR
5 passed, 0 failed
```

The command exits non-zero when any case fails, so it drops straight into CI.
It also accepts a standalone `.tests.json` path (loaded when the program has no
`#tests` table), and takes `--filter TEXT` (run only cases whose name contains
`TEXT`) and `--json` (machine-readable report):

```bash
fx50 test examples/quadratic.fxc
fx50 test prog.fxc --filter "roots"
fx50 test prog.fxc --json
```

A failing case shows both sides:

```text
prog.fxc
  ok    doubles 21
  FAIL  deliberately wrong
        expected: 11
        actual: 10
1 passed, 1 failed
```

### Suite schema

The `#tests` table is an array of cases, or an object:

| Field | Required | Meaning |
| --- | --- | --- |
| `name` | no | Suite label shown in the report. |
| `mode` | no | Operating mode override (`COMP`, `CMPLX`, `BASE`, `SD`, `REG`). |
| `ascii` | no | Transpile with ASCII aliases. Default `false`. |
| `cases` | yes | The list of cases. |

A standalone `.tests.json` file additionally takes `program` (a path, resolved
relative to the JSON file) **or** `source` (inline text); the two are mutually
exclusive. When both are absent, the sibling `.fxc` of the suite file is used.
Those fields are not allowed in an embedded `#tests` table, where the program is
the file the table lives in.

Each case takes:

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

let report = run_suite_file(Path::new("examples/factorial.fxc"), None)?;
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
