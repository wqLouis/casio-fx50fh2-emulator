# Writing `.fxc` programs — a guide for AI agents

This document is for an AI agent (or anyone) generating **`.fxc` source** for
the `fx-transpiler`. It is written to be read once and then obeyed: everything
here is checked against the implementation, and every example is real output
from `fx50 build`.

`.fxc` is a small C-like language that compiles to CASIO fx-50FH II **PRGM**
code. The compiled program then runs on the calculator (or in this
interpreter).

```bash
fx50 build program.fxc            # transpile, print PRGM to stdout
fx50 build --ascii program.fxc    # ...using keyboard-typable ASCII aliases
fx50 run   program.fxc            # transpile and execute (? reads from stdin)
fx50 run   --mode CMPLX program.fxc
```

As a library:

```rust
use fx_transpiler::{transpile, transpile_with, Options, Mode};

let prgm = transpile("let a = input(); print(a * 2);")?;
let prgm = transpile_with(src, Options { ascii: true, mode: Some(Mode::Cmplx) })?;
```

`TranspileError` carries `message`, `offset`, `line` and `column`.

---

## 1. The rules that matter most

If you remember five things, remember these — each one is a common way
generated code fails.

1. **Every statement ends with `;`.** There is no newline termination, and no
   automatic semicolon insertion. `print(a)` alone is a syntax error.
2. **At most seven mutable variables per program.** PRGM has exactly seven
   memories (`A B C D X Y M`). Names are assigned in **first-seen order** and a
   name keeps its memory for the whole program, so an eighth distinct name is a
   transpile-time error. `const` and `#data` values are inlined and use **no**
   memory, so a program full of fixed values fits easily.
3. **`input()` is only legal as the entire right-hand side of an assignment.**
   `let a = input();` is fine; `print(input());` and `let a = input() + 1;` are
   errors.
4. **There are no logic operators and no modulo.** No `&&`, `||`, `!`, `%`,
   `++`, `--`, `+=`, or bitwise operators. Comparisons produce `1` or `0`.
5. **Everything is a floating-point number.** There is no integer type, no
   string type, no arrays, and no user-defined functions.

---

## 2. Complete grammar

```text
program    := directive* stmt*
directive  := '#mode' MODENAME             -- must be first if present
            | '#data' NAME '=' JSON ';'    -- may appear anywhere
            | '#tests' '=' JSON ';'        -- sugar for `#data tests = ...`
stmt       := 'let' NAME '=' expr ';'
            | 'const' NAME '=' expr ';'
            | NAME '=' expr ';'
            | 'free' NAME ';'
            | 'print' [ '(' expr ')' | expr ] ';'   -- parens optional
            | 'if' '(' expr ')' body ('else' body)?
            | 'while' '(' expr ')' body
            | 'for' '(' forinit ';' expr ';' NAME '=' expr ')' body
            | 'break' ';'
            | 'goto' DIGIT ';'
            | 'label' DIGIT ';'
            | '{' stmt* '}'
            | ';'                            -- empty statement
            | expr ';'                       -- evaluate, do not display
body       := stmt | '{' stmt* '}'           -- braces optional for one statement
forinit    := ['let'] NAME '=' expr

JSON       := a JSON value, or a string naming a file to read
dataref    := NAME accessor+
accessor   := '.' NAME | '[' INTEGER ']'

primary    := NUMBER | 'pi' | 'e' | 'input()' | NAME | dataref
            | NAME '(' [expr (',' expr)*] ')' | '(' expr ')'

**Binding strength, tightest first:** `(...)` / calls → `^ **` → unary `-` →
`* /` → `+ -` → `< <= > >=` → `== !=` (loosest).

Note that unary minus binds *looser* than power, so `-2 ^ 2` is `-(2 ^ 2) = -4`,
not `(-2) ^ 2`.

### Lexical details

| Item | Rule |
| --- | --- |
| Whitespace | Insignificant, including newlines. Indent freely. |
| Comments | `// to end of line` and `/* block */`. An unterminated block comment is an error. |
| Identifiers | `[A-Za-z_][A-Za-z0-9_]*`. Keywords are reserved. |
| Numbers | Decimal only: `123`, `1.5`, `.5`, `1e10`, `2.5E-2`. **No** hex/binary literals. |
| Keywords | `let if else while for break goto label print` |
| Punctuation | `+ - * / ^ ** = == != < <= > >= ( ) { } ; ,` |
| Not available | `% & | ~ ! ++ -- += -= *= /= && || << >> ?:` |

---

## 3. Statements

### Assignment and input

```c
let a = 1;        //  1→A          first assignment; `let` is conventional
a = a + 1;        //  A+1→A
let b = input();  //  ?→B          prompt for a number
```

`let` and plain `=` behave identically at runtime; `let` is only a readability
signal. There is no block scoping — a name means the same memory everywhere.

### Display

```c
print(a);         //  A◢
print a * 2;      //  A×2◢         parentheses are optional
a + 1;            //  A+1          bare expression: computed, NOT displayed
```

`print` is the only way to show a value. A bare expression statement exists but
produces no output, which makes it nearly useless — prefer `print`.

### Conditionals

`if` bodies may be a single statement or a block, and `else if` chains nest
naturally.

```c
if (a > 0) print(1); else print(2);
```

```text
If A>0
Then
1◢
Else
2◢
IfEnd
```

### Loops

`while` and `for` compile to native PRGM loops when the shape is recognisable;
unusual shapes fall back to a `while` expansion (see §7.3).

```c
let i = 0;
while (i < 10) { print(i); i = i + 1; }

for (let i = 0; i < 5; i = i + 1) { print(i); }
```

`break;` exits the innermost loop and is only valid inside one (else a runtime
`Argument ERROR`).

### Labels and jumps

`goto`/`label` take a **single digit** `0`–`9`. Anything else is an error.

```c
label 1;
let a = input();
goto 1;
```

```text
Lbl 1
?→A
Goto 1
```

Prefer structured loops. `goto` exists for compatibility with hand-written
PRGM.

---

## 4. Expressions and built-ins

| Built-in | Emits | Notes |
| --- | --- | --- |
| `sqrt(x)` | `√(x)` | |
| `cbrt(x)` | `∛(x)` | |
| `abs(x)` | `Abs(x)` | |
| `sin cos tan` | `sin(x)` … | angle unit set by the calculator's mode |
| `asin acos atan` | `sin⁻¹(x)` … | |
| `sinh cosh tanh` | `sinh(x)` … | |
| `asinh acosh atanh` | `sinh⁻¹(x)` … | |
| `log(x)` | `log(x)` | base 10 |
| `log(a, b)` | `log(a,b)` | log of `b` to base `a` — argument order matters |
| `ln(x)` | `ln(x)` | natural log |
| `rnd(x)` | `Rnd(x)` | round to 10 significant digits |

Constants: `pi` → `π` (or `pi` with `--ascii`), `e` → `e`, and the 40
scientific constants under the `phys.` namespace (below).

The only exponentiation operators are `^` and its alias `**`. There is no
`min`/`max`, and no integer division or modulo. Unknown function names are a
parse error, so do not invent built-ins.

### Scientific constants

The calculator has 40 built-in scientific constants. They are reached through
a `phys.` namespace, which keeps their names from colliding with your
variables:

```c
// Planck's relation: E = h f, with the constant as `phys.h`.
let f = input();
let energy = phys.h * f;
print(energy);
```

```text
?→A
h×A→B
B◢
```

Use the ASCII name after the dot — `phys.h`, `phys.hbar`, `phys.mp`,
`phys.C0`, `phys.atm`, `phys.NA`. The display symbol also works when you can
type it (`phys.ħ`, `phys.μμ`, `phys.R∞`). `phys` alone is a reserved word and
is never a variable.

Two spellings are easy to get wrong:

* The elementary charge is `phys.eq`, **not** `phys.e` — bare `e` is Euler's
  number. (`phys.e` does work, but spell it `phys.eq` to keep the intent
  obvious.)
* `phys.h` is the Planck constant and `phys.hbar` the reduced one; `phys.g` is
  the standard acceleration of gravity and `phys.G` the gravitational
  constant.

Do not use a bare constant name (`h`, `mp`) — those are ordinary variable
names in `.fxc` and would be allocated to one of the seven memories. The
constants are real numbers, so BASE mode rejects them.

### Worked example: factorial

```c
// factorial.fxc
let n = input();
let acc = 1;
for (let i = 1; i <= n; i = i + 1) {
  acc = acc * i;
}
print(acc);
```

```console
$ fx50 build factorial.fxc
?→A
1→B
For 1→C To A Step 1
B×C→B
Next
B◢

$ printf '5\n' | fx50 run factorial.fxc
120
```

---

## 5. Variables: seven memories

Names are mapped to `A B C D X Y M` **in first-seen order**, and each name keeps
its memory for the whole program.

```c
let total = 1;      //  total → A
let count = 2;      //  count → B
print(total + count);
```

```text
1→A
2→B
A+B◢
```

Consequences to design around:

* **Budget seven live variables.** A name keeps its memory until the program
  ends, or until you `free` it.
* The order is by first *appearance in source order*, including inside
  expressions — `let a = b; let b = 1;` gives `b → A` and `a → B`.
* Reserved letters name nothing special. Calling a variable `x` does not make
  it the calculator's `X` unless it happens to be allocated there.

Naming them `a b c d x y m` in that order makes allocation obvious and
predictable. It is a good habit for generated code.

### Making a program fit

In this order of preference:

1. **Use `const` for fixed values.** A `const` is inlined where it is used and
   consumes no memory:

   ```c
   const scale = 3;
   let total = 7 * scale;   // emits `7×3→A`; only `total` uses a memory
   ```

2. **Use `#data` for values that come from JSON.** Those become literals too.
3. **`free` a variable when you are done with it.** The memory is handed to the
   next new variable:

   ```c
   let first = 5;
   print(first);
   free first;        // release the memory
   let second = 7;    // reuses the same memory
   print(second);
   ```

   Nothing is released implicitly: a memory's final value is observable (a
   later program or the user can read it), so only you know when a value is
   finished with.

`fx50 regs program.fxc` prints the plan without running anything:

```console
$ fx50 regs examples/compiletime.fxc
Memory plan for examples/compiletime.fxc
  A  first  → second   (reused after `free first`)

  1 of 7 memories used; free: B C D X Y M
  released with `free`: first
  1 const (no memory): scale
  2 data table(s) (no memory): config, tests
```

### `free` mistakes are caught at transpile time

The transpiler keeps a table of which variable occupies each memory, so these
never reach the calculator:

| Mistake | Example | Error |
| --- | --- | --- |
| Use after free | `free a; print(a);` | `` `a` was freed and cannot be used again `` |
| Double free | `free a; free a;` | `` `a` was already freed (double free) `` |
| Freeing what has no memory | `const k = 1; free k;` | `` `k` is a `const`, which uses no memory `` |
| Freeing an unknown name | `free nope;` | `` `nope` is not a variable `` |

Two further rules:

* **A freed name may not be revived.** `let t = 1; free t; let t = 2;` is an
  error — use a fresh name. A name resolves to exactly one memory, and a second
  `t` would need a second one.
* **`free` cannot be combined with `goto`/`label`.** A jump can re-enter code
  whose memory has since been re-used, which the transpiler cannot verify, so
  it refuses rather than miscompiling. Programs with jumps and no `free` are
  unaffected.

Running out of memories names the variables in the way:

```text
fx50: no free memory for `z`: all of A B C D X Y M are in use by A (`a`), B (`b`),
C (`c`), D (`d`), X (`x`), Y (`y`), M (`m`). Use `const` for fixed values, or
`free` a variable you no longer need — `fx50 regs` shows the plan
```

---

## 6. Modes

The calculator forces an operating mode, and a `.fxc` program may declare one
with a **header directive**:

```c
#mode CMPLX
```

| Name | Aliases | What it means for `.fxc` |
| --- | --- | --- |
| `COMP` | — | default; general real arithmetic |
| `CMPLX` | `CPLX`, `COMPLEX` | declares complex mode |
| `BASE` | `BASEN`, `BASE-N` | integer arithmetic; **rejects built-ins and `pi`/`e`** |
| `SD` | `STAT`, `STATS`, `STATISTICS` | declares statistics mode |
| `REG` | `REGRESSION` | declares regression mode |

Rules:

* The directive must be the **first** non-comment, non-blank line. A `#mode`
  anywhere else is an error.
* Case-insensitive, and `#mode=CMPLX` is accepted.
* With no header and no `--mode` flag, the emitted PRGM has no header and runs
  in COMP.
* A header (or `--mode`) is **copied into the output**, so the mode survives the
  round trip.
* Because `.fxc` is real-number-only, `CMPLX`/`SD`/`REG` change only the
  declared mode. **`BASE` is the one mode that changes what you may write.**

In `BASE`, all built-ins and `pi`/`e` are rejected at transpile time:

```console
$ fx50 build base.fxc      # with: #mode BASE / print(sqrt(4));
fx50: `sqrt` is not available in BASE mode (switch to COMP, CMPLX, SD or REG) (line 2, column 7)
```

> **Caveat.** `#mode BASE` declares integer mode, but `.fxc` has no syntax for
> selecting a number base, so `/` still compiles to `÷` and evaluates as
> *floating-point* division at runtime: `#mode BASE` with `let a = 7 / 2;`
> prints `3.5`, not `3`. If you need truncating integer division, do not rely on
> the mode — implement it (for example repeated subtraction in a `while` loop),
> or emit PRGM directly.

---

## 7. Gotchas that bite generated code

1. **No `%`, `&&`, `||`, `!`, `++`, `--`, compound assignment.** Write
   `i = i + 1`, not `i++`. Build boolean logic from comparisons and nesting:
   `if (a > 0) { if (b > 0) { ... } }` instead of `a > 0 && b > 0`.
2. **`for` bounds are adjusted.** `i < limit` becomes `To limit-1`, and
   `i > limit` becomes `To limit+1 Step -step`. So `for (i = 0; i < 5; ...)`
   emits `For 0→A To 5-1 Step 1`. The adjustment is correct but surprising if
   you are diffing output.
3. **Only four `for` shapes compile natively:** `<` with `+`, `<=` with `+`,
   `>` with `-`, `>=` with `-`. Anything else (for example `i != n`, or an
   update touching a different variable) falls back to an equivalent `while`
   loop. The fallback is correct, but note the update variable starts at `0`
   if it was never initialised.
4. **Numbers are printed without scientific notation.** `1e10` becomes
   `10000000000`, because the calculator's keypad cannot enter an exponent.
5. **No scoping.** A variable declared inside a `for` body is the same memory
   as one declared outside it. Reusing the loop counter after the loop is
   common and often a bug.
6. **Comparisons are not a boolean type.** `a > 0` evaluates to `1` or `0`, so
   you may write `x = (a > 0);` and later test it.
7. **Error timing differs.** Malformed source, an eighth variable, `input()` in
   an expression, an unknown function and BASE-mode built-ins are all caught at
   **transpile time** with a line and column. `break` outside a loop, a `goto`
   with no matching label, and division by zero are **runtime** errors
   (`Argument ERROR`, `Go ERROR`, `Math ERROR`). Do not assume a clean
   `fx50 build` means the program will run — run it if you can.

---

## 8. Splitting a program with `#include`

A program can inline another file's text at transpile time:

```c
// main.fxc
let n = input();
#include "lib/squares.fxc"
print(result);
```

`fx50 build main.fxc` substitutes the fragment before compiling, so the
calculator still receives one flat program. This is the `.fxc` analogue of
C's `#include` or Rust's `include_str!`.

Rules that matter when generating code:

* Syntax is exactly `#include "path"`, starting the line.
* The path is relative to the file containing the directive, and includes
  nest.
* **A fragment is statements, not a function.** There are no user-defined
  functions, so a fragment reads and writes the same seven memories as its
  includer. `#include "f.fxc"` where `f.fxc` says `let total = a + 1;` is
  exactly as if you had typed that line yourself — it can see `a` and leaves
  `total` behind.
* Variables are allocated in **expanded** source order, so the names inside a
  fragment are numbered where the `#include` line sits. Place includes after
  the inputs that should be allocated first if the numbering matters to you.
* The seven-memory budget is shared across all files. Two fragments plus the
  main program still fit in seven distinct names in total.
* A fragment may **not** contain `#mode`; only the root file may declare a mode.
* Expansion is unguarded, so including the same file twice duplicates its
  statements.

Errors inside a fragment are reported against that fragment, with its own line
numbers:

```text
fx50: unknown function `nope` (lib/squares.fxc:2:11)
```

Prefer `#include` when two programs share a computation. Do **not** use it to
simulate functions or parameter passing: there is no call, no arguments and no
return value.

---

## 9. Verify your program with a JSON test suite

You do not have to trust your own reading of a program: a `.fxc` file can carry
its own test cases in a `#tests` table, which is the same compile-time data
facility as `#data`.

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
$ fx50 test factorial.fxc
factorial.fxc
  ok    5! = 120
  ok    0! = 1
  ok    no input
3 passed, 0 failed
```

If you can run commands, **run this before reporting success**. It exits
non-zero when a case fails, and a failure tells you the exact expected and
actual values.

A separate `<name>.tests.json` file still works for suites that prefer to live
apart from their program; it is loaded only when the program has no `#tests`
table.

### Fields

The `#tests` table is an array of cases, or an object:

| Field | Required | Meaning |
| --- | --- | --- |
| `name` | no | Suite label. |
| `mode` | no | `COMP`, `CMPLX`, `BASE`, `SD` or `REG`. |
| `ascii` | no | Transpile with ASCII aliases. |
| `cases` | yes | The cases. |

A standalone `.tests.json` additionally takes `program` (a path, **relative to
the JSON file**) **or** `source` (inline text).

| Case field | Required | Meaning |
| --- | --- | --- |
| `name` | no | Label; defaults to `case N`. |
| `input` | no | Numbers fed to `?` prompts, **in order**. |
| `output` | one of | Expected `◢` lines, in order. |
| `error` | one of | Expected error, matched case-insensitively by substring. |

Each case gives exactly one of `output` or `error`.

### Getting the expectations right

These are the ways a hand-written suite usually goes wrong:

* **`output` is the displayed lines, exactly as formatted.** `print(10 / 3)`
  yields `"3.333333333"`, not `"3.33"`. Compute the expected string the same
  way the calculator would, and prefer whole numbers in tests.
* **A program ending in an assignment still displays.** `let a = 1;` produces
  `["1"]`, because the calculator shows the last computed value when a program
  ends without `◢`. Use `"output": []` only for programs that compute nothing
  at all (such as one containing only `label 1;`).
* **Inputs are consumed in order.** A case with fewer `input` values than the
  program has `?` prompts fails with `Argument ERROR` — which is itself a
  useful case to assert.
* **Assert errors by label**, not by full message: `"error": "Math ERROR"`,
  `"Argument ERROR"`, `"Go ERROR"`. The match is a case-insensitive substring,
  so `"math"` works too.
* **Path resolution:** `program` is relative to the JSON, so a suite in
  `tests/` referring to `../src/prog.fxc` works.
* If the suite lists neither `program` nor `source`, the sibling `.fxc` of the
  suite file is used — so `fx50 test prog.fxc` with a `prog.tests.json` that
  omits `program` is valid.

Use `fx50 test prog.fxc --json` for a machine-readable report, and
`--filter TEXT` to run a subset while iterating.

---

## 10. Reading data at transpile time

A `.fxc` program can read JSON **while it is being transpiled** and use the
values as literals. This is the general facility behind `#tests`, and it is how
a program carries tables of numbers without spending memories on them.

```c
#data config = { "base": 2, "offsets": [10, 20, 30] };
const scale = config.base;

let total = config.offsets[1] * scale;   // emits `20×2→A`
print(total);                            // `A◢`
```

Rules:

* The directive is `#data NAME = VALUE;`. `VALUE` is JSON and may span lines.
* A **top-level JSON string means "read this file"**:
  `#data offsets = "offsets.json";` reads that file, resolved relative to the
  file containing the directive.
* Reference values with `.field` and `[index]`: `config.offsets[1]`. Indices
  must be whole-number literals, not expressions.
* Only **numbers and booleans** can reach the calculator. A boolean is `1` or
  `0`. A string, array or object used as a value is an error — index into it
  first.
* A bare data name (`print(config)`) is allowed only if the whole table is a
  single number or boolean.
* `#data` names share the namespace with variables and `const`s, so they must
  be unique.
* `#data` may appear anywhere; `#mode` must come first.

Because the values are resolved before emission, they contribute **nothing** to
the seven-memory budget. Prefer `#data` over writing a long list of literals by
hand when the values are already available as JSON.

---

## 11. Checklist before you emit `.fxc`

- [ ] Every statement ends with `;`.
- [ ] `#mode` (if used) is the very first line, and the mode is spelled
      correctly.
- [ ] At most **seven live** variables at any point; fixed values use `const`
      (and tables use `#data`) so they cost no memory, and `free` releases a
      variable that is finished with.
- [ ] `#data` paths resolve to numbers or booleans; array indices are literals.
- [ ] No use after free, no double free, and no `free` alongside
      `goto`/`label`.
- [ ] `input()` appears only as a complete assignment right-hand side.
- [ ] No `%`, `&&`, `||`, `!`, `++`, `--`, `+=`, hex literals, arrays, or
      strings.
- [ ] Only built-ins from the table in §4 are called, with the right arity
      (`log` takes 1 or 2 arguments; everything else takes 1).
- [ ] `goto`/`label` use a single digit `0`–`9`.
- [ ] Loop bodies that need more than one statement use `{ }` — braces are
      optional for a single statement and it is easy to lose the rest.
- [ ] Every `goto` has a matching `label`, and `break` only appears inside a
      loop.
- [ ] In `#mode BASE`, no built-ins, no `pi`, no `e`, no `phys.` constant.
- [ ] Scientific constants are written `phys.<name>` with the namespace — never
      as a bare name, which would become a variable.
- [ ] `#include` paths exist, start the line, and contain no `#mode`; the
      seven-memory budget is respected *after* expansion.
- [ ] If you can run commands, a `.tests.json` suite covers the happy path
      **and** at least one error case, and `fx50 test` reports `0 failed`.

- [ ] If you can run commands, a `#tests` table (or a `.tests.json` suite)
      covers the happy path **and** at least one error case, and `fx50 test`
      reports `0 failed`.

If you can, verify with the compiler before declaring success:

```bash
fx50 build your.fxc >/dev/null && echo "transpiles"
fx50 regs  your.fxc            # check the seven-memory budget
```

and, where inputs are known, execute it:

```bash
printf '5\n' | fx50 run your.fxc
```

Best of all, write test cases in the program itself (§9) and run them — that
checks many cases at once and leaves evidence behind:

```bash
fx50 test your.fxc
```
