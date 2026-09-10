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
2. **At most seven distinct variables per program.** PRGM has exactly seven
   memories (`A B C D X Y M`). Names are assigned in **first-seen order** and
   **never reused**, so an eighth distinct name is a transpile-time error no
   matter where it lives or how short its lifetime.
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
program    := modedirective? stmt*
modedirective := '#mode' MODENAME          -- optional; if present, must be first
stmt       := 'let' NAME '=' expr ';'
            | NAME '=' expr ';'
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

expr       := equality
equality   := comparison (('==' | '!=') comparison)*
comparison := additive  (('<' | '<=' | '>' | '>=') additive)*
additive   := multiplicative (('+' | '-') multiplicative)*
multiplicative := unary (('*' | '/') unary)*
unary      := '-' unary | power
power      := primary (('^' | '**') unary)?   -- right-associative
primary    := NUMBER | 'pi' | 'e' | 'input()' | NAME | NAME '(' [expr (',' expr)*] ')'
            | '(' expr ')'
```

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

Constants: `pi` → `π` (or `pi` with `--ascii`), `e` → `e`.

The only exponentiation operators are `^` and its alias `**`. There is no
`min`/`max`, and no integer division or modulo. Unknown function names are a
parse error, so do not invent built-ins.

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

* **Budget seven names.** If a program needs more, restructure to reuse
  variables rather than adding names. There is no scope analysis, so a name
  that appears once at the bottom still consumes a memory.
* The order is by first *appearance in source order*, including inside
  expressions — `let a = b; let b = 1;` gives `b → A` and `a → B`.
* Reserved letters name nothing special. Calling a variable `x` does not make
  it the calculator's `X` unless it happens to be allocated there.

Naming them `a b c d x y m` in that order makes allocation obvious and
predictable. It is a good habit for generated code.

An eighth distinct name fails at transpile time, pointing at the offending name:

```text
fx50: too many variables: PRGM only has 7 memories (A B C D X Y M), but `z` is the 8th distinct name (line 1, column 61)
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

## 8. Checklist before you emit `.fxc`

- [ ] Every statement ends with `;`.
- [ ] `#mode` (if used) is the very first line, and the mode is spelled
      correctly.
- [ ] At most **seven** distinct variable names in the whole program.
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
- [ ] In `#mode BASE`, no built-ins, no `pi`, no `e`.

If you can, verify with the compiler before declaring success:

```bash
fx50 build your.fxc >/dev/null && echo "transpiles"
```

and, where inputs are known, execute it:

```bash
printf '5\n' | fx50 run your.fxc
```
