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
4. **There are no C-style logic operators and no modulo.** No `&&`, `||`, `!`,
   `%`, `++`, `--`, `+=`. Comparisons produce `1` or `0`; the calculator keys
   behind them are calls (`fact(x)`, `pct(x)`, `not(x)`), except the base-n
   words `and`/`or`/`xor`/`xnor`, which are real infix operators in `#mode BASE`.
5. **Everything is a floating-point number.** There is no integer type and no
   string type. Arrays exist (one memory per element, literal indices only),
   and so do user-defined functions — which are inlined, not called. **Every
   program needs a `fn main()` entry point**; only `fn` definitions and
   top-level `const` may sit at the top level. See §3.7.

---

## 2. Complete grammar

```text
program    := directive* (funndef | conststmt)*
             -- the top level holds only `fn` definitions and `const`
             -- declarations; every program needs `fn main()`
fndef     := 'fn' NAME '(' [NAME (',' NAME)*] ')' ('=' expr ';' | body)
conststmt  := 'const' NAME '=' expr ';'
directive  := '#mode' MODENAME             -- must be first if present
            | '#data' NAME '=' JSON ';'    -- may appear anywhere
            | '#tests' '=' JSON ';'        -- sugar for `#data tests = ...`
stmt       := 'let' NAME '=' expr ';'
            | 'let' NAME '[' INTEGER? ']' ('=' '{' expr (',' expr)* '}')? ';'
            | 'const' NAME '=' expr ';'
            | NAME '=' expr ';'
            | NAME '[' expr ']' '=' expr ';'    -- array element (index folds to a literal)
            | 'free' NAME ';'
            | 'unsafe_free' NAME ';'      -- `free` without the control-flow check
            | 'return' [expr] ';'         -- last statement of a `fn` body
            | 'print' [ '(' expr ')' | expr ] ';'   -- parens optional
            | 'if' '(' expr ')' body ('else' body)?
            | 'while' '(' expr ')' body
            | 'for' '(' forinit ';' expr ';' NAME '=' expr ')' body
            | 'break' ';'
            | 'goto' DIGIT ';'
            | 'label' DIGIT ';'
            | 'mplus' | 'mminus' '(' expr ')' ';'  -- expr M+ / M-
            | 'clrmemory' | 'clrstat' | 'freqon' | 'freqoff' '(' ')' ';'
            | 'deg' | 'rad' | 'gra' | 'dec' | 'hex' | 'bin' | 'oct'
            | 'to_cartesian' | 'to_polar' | 're_im' '(' ')' ';'
            | 'reg_lin' | 'reg_log' | 'reg_exp' | 'reg_pwr' | 'reg_inv'
            | 'reg_quad' | 'reg_abexp' '(' ')' ';'
            | 'dms' '(' ')' ';'              -- the bare °′″ conversion key
            | 'fix' | 'sci' | 'norm' '(' INTEGER ')' ';'
            | 'dt' '(' expr (',' expr (',' expr)?)? ')' ';'  -- statistics data
            | expr '=>' stmt                 -- conditional jump (⇒)
            | '{' stmt* '}'
            | ';'                            -- empty statement
            | expr ';'                       -- evaluate, do not display
body       := stmt | '{' stmt* '}'           -- braces optional for one statement
forinit    := ['let'] NAME '=' expr

JSON       := a JSON value, or a string naming a file to read
dataref    := NAME accessor+
accessor   := '.' NAME | '[' expr ']'

primary    := NUMBER | '0x' HEX | '0b' BIN | '0o' OCT | 'pi' | 'e'
            | 'input()' | NAME | dataref
            | NAME '(' [expr (',' expr)*] ')'
            | 'phys' '.' CONSTNAME | 'stat' '.' STATNAME | '(' expr ')'
```

**Binding strength, tightest first:** `(...)` / calls → `^ **` → unary `-` →
`* /` → `+ -` → `< <= > >=` → `== !=` → `and` → `or xor xnor` (loosest).

Note that unary minus binds *looser* than power, so `-2 ^ 2` is `-(2 ^ 2) = -4`,
not `(-2) ^ 2`.

A `for` loop whose bounds are integer constants is unrolled when its body needs
a computed array index, so `for (let i = 0; i < 3; i = i + 1) { v[i] = input(); }`
is fine. See [arrays](#arrays) for the shapes that unroll.

### Lexical details

| Item | Rule |
| --- | --- |
| Whitespace | Insignificant, including newlines. Indent freely. |
| Comments | `// to end of line` and `/* block */`. An unterminated block comment is an error. |
| Identifiers | `[A-Za-z_][A-Za-z0-9_]*`. Keywords are reserved. |
| Numbers | Decimal: `123`, `1.5`, `.5`, `1e10`, `2.5E-2`. In `#mode BASE` also base-tagged: `0x1F`, `0b1010`, `0o17`. |
| Keywords | `let const free unsafe_free if else while for break goto label print fn return phys stat and or xor xnor` |
| Punctuation | `+ - * / ^ ** = == != < <= > >= => ( ) { } [ ] ; , .` |
| Not available | As *operators*: `% & | ~ ! ++ -- += -= *= /= && \|\| << >> ?:`. The keys behind them have call spellings (`pct`, `fact`, `not`, `and`, `or`, `xor`, `xnor`). |

---

## 3. Statements

### Assignment and input

```c
let a = 1;        //  1→A          declaration
let v[3] = {1,2,3}; //  1→A 2→B 3→C  an array: one memory per element
v[1] = 9;         //  9→B          write one element
a = a + 1;        //  A+1→A        assignment to a declared name
let b = input();  //  ?→B          prompt for a number
```

`let` **declares** — it introduces the name and allocates its memory. A plain
`=` assigns to a name that already exists and never declares one. Declaring a
live name twice is an error; after `free` it can be declared again. See
[Variables](#5-variables-seven-memories) below for the full rules.

### Arrays

An array gives a group of values one memory each. **The index must be a
literal**, because PRGM has no indirect addressing — `v[k]` cannot be looked up
from a variable at run time, so `k` has to be known while transpiling.

```c
let v[3];                 // declare 3 elements (A B C)
let w[3] = {4, 8, 15};    // declare and initialise
let u[] = {1, 2, 3};      // size inferred from the list
print(w[0] + w[2]);       // read an element
w[1] = 16;                // write an element
free w;                   // release every element at once
```

The payoff is that an element reference is **free**: `w[1]` emits the single
memory letter `B`, with no instructions at all. The cost is that an index must
be known while transpiling. You can still walk an array with a loop whose
bounds are constant — the transpiler unrolls it and substitutes each counter
value:

```c
let v[3];
for (let i = 0; i < 3; i = i + 1) { v[i] = input(); }
```

```text
?→A
?→B
?→C
```

The loop must be the canonical `for` shape with integer bounds after
compile-time evaluation. A run-time bound (`i < n`), a `break`, `goto`/`label`,
a declaration, or a `free` in the body stops the unroll, and the computed index
is then a compile-time error — unroll those by hand. A loop that only touches
scalars keeps its native `For`/`Next` form.

Rules that will bite you:

* **Seven memories is the whole budget**, shared with everything else. `let v[5]`
  plus a loop counter and an accumulator fills all seven. Prefer `#data` for a
  constant table — it costs no memory.
* **An array is freed as a whole.** `free v[0];` is an error on purpose: the
  elements after it would be stranded in memories nothing can reuse.
* **A declaration with no initialiser emits nothing.** `let v[3];` just reserves
  the memories; the values are whatever you write there.
* **`const` cannot declare an array** (it is inlined, so it has no memory).
* **A name is either a scalar or an array**, not both, while it is live.

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

### Conditional jump (`⇒`)

`cond => stmt;` runs a single statement when `cond` is non-zero. It is PRGM's
`⇒` key; use `if` for anything larger than one statement.

```c
let x = input();
x > 0 => print(1);       // x>0⇒1◢
```

---

### 3.7 Functions

A program **is** a set of functions. They are **inlined at each call**, so they
cost no memory and an unused one costs nothing.

```c
fn square(x) = x * x;                 // expression function, no locals

fn sum_to(n) {                        // procedure: locals and a return
    let total = 0;
    for (let i = 1; i <= n; i = i + 1) { total = total + i; }
    return total;
}

fn main() {
    print(square(4));
    print(sum_to(10));
}
```

Rules that matter when generating code:

* **Every program needs `fn main()`.** The top level holds only `fn` definitions
  and `const` declarations; a loose statement is an error telling you to move it
  into `main`.
* **`main` keeps the classic order-free rules.** Inside `main`, using a name
  declares it, so `print(a);` alone is valid. `main` is where a program's loose
  statements live, and moving them there does not change what is emitted.
* **Every other function is closed.** Its body may only use its parameters, its
  locals, top-level `const`/`#data` values, other functions and the built-in
  keys. It cannot read or create a global — `fn f(x) = x + a;` is an error
  unless `a` is a parameter, a local or a top-level `const`.
* **Locals are hygienic.** They are renamed per call (`f$t$1`), so two calls and
  a caller variable of the same name cannot collide.
* **Arguments are passed by name** (substituted at each mention). A variable
  argument is effectively passed by reference; `twice(ran())` draws two
  numbers; an argument mentioned `n` times is emitted `n` times.
* **A parameter that is assigned needs an assignable argument** (a variable or
  array element), not an expression.
* **`return` is the last statement only**, and there is one return value. For
  two results, assign through output parameters:
  `fn minmax(a, b, lo, hi) { lo = a; hi = b; … }`.
* **No recursion** (direct or indirect) — there is no call stack.
* **A procedure cannot be called from a `while`/`for` condition**; use an
  expression function or precompute into a variable.
* A function name cannot shadow a built-in (`fn sqrt(…)` is an error), and a
  parameter cannot also be a local.

---

## 4. Expressions and built-ins

Every PRGM key has an `.fxc` spelling. Value keys are calls; keys that act on
the machine are statements ending in `;`.

| Call | Emits | Notes |
| --- | --- | --- |
| `sqrt(x)` | `√(x)` | |
| `cbrt(x)` | `∛(x)` | |
| `root(n, x)` | `nx√(x)` | the `x√(` key; index first |
| `pow10(x)` | `10^(x)` | |
| `exp(x)` | `e^(x)` | |
| `abs(x)` | `Abs(x)` | |
| `sin cos tan` | `sin(x)` … | angle unit set by `deg`/`rad`/`gra` |
| `asin acos atan` | `sin⁻¹(x)` … | |
| `sinh cosh tanh` | `sinh(x)` … | |
| `asinh acosh atanh` | `sinh⁻¹(x)` … | |
| `log(x)` | `log(x)` | base 10 |
| `log(a, b)` | `log(a,b)` | log of `b` to base `a` — argument order matters |
| `ln(x)` | `ln(x)` | natural log |
| `rnd(x)` | `Rnd(x)` | round to 10 significant digits |
| `pol(x, y)` / `rec(x, y)` | `Pol(x,y)` / `Rec(x,y)` | COMP or CMPLX; write `X`/`Y` |
| `arg(x)` / `conjg(x)` | `arg(x)` / `Conjg(x)` | CMPLX |
| `polar(r, θ)` | `r∠θ` | CMPLX |
| `dms(deg, min, sec)` | `deg°min′sec″` | sexagesimal literal; the three arguments must be literals |
| `not(x)` / `neg(x)` | `Not(x)` / `Neg(x)` | BASE |
| `inv(x)` / `sqr(x)` / `cube(x)` | `x⁻¹` / `x²` / `x³` | postfix keys |
| `fact(x)` / `pct(x)` | `x!` / `x%` | postfix keys |
| `frac(a, b)` | `a┘b` | the fraction key |
| `npr(n, r)` / `ncr(n, r)` | `<n>nPr<r>` / `<n>nCr<r>` | |
| `ran()` | `Ran#` | pseudo-random in `[0, 1)`; fixed seed |
| `i()` | `i` | CMPLX; a bare `i` is still a variable |
| `ans()` | `Ans` | the previous result |
| `mvalue()` | `M` | the fixed `M` memory (reserves it; see §5) |

Statement keys (all end in `;`): `mplus(x);`/`mminus(x);` → `x M+`/`x M-`,
`clrmemory();` → `ClrMemory`, `clrstat();` → `ClrStat`, `freqon();`/`freqoff();`
→ `FreqOn`/`FreqOff`, `dt(x[; y[; f]]);` → `x DT`/`x,y DT`/`x,y;f DT`,
`deg();`/`rad();`/`gra();` → `Deg`/`Rad`/`Gra`, `fix(n);`/`sci(n);` → `Fix n`/`Sci n`
(`n` 0–9), `norm(n);` → `Norm n` (1–2), `dec();`/`hex();`/`bin();`/`oct();` →
`Dec`/`Hex`/`Bin`/`Oct`, `to_cartesian();`/`to_polar();` → `▶a+b𝑖`/`▶r∠θ`,
`re_im();` → `Re⇔Im`, `reg_lin();`…`reg_abexp();` → `Lin`…`AB-Exp`, and
`dms();` → `°′″` (the bare decimal ⇄ sexagesimal conversion key).

Statistical variables use the `stat.` namespace: `stat.sumx`, `stat.meanx`,
`stat.sigmax`, `stat.regA`, `stat.regC`, … (glyph `Σx`, `x̄`, `σx`, `regA`,
`regC`). A bare `sumx` would be an ordinary variable, so always write the
namespace.

In `#mode REG`, `reg_lin()`/`reg_log()`/`reg_exp()`/`reg_pwr()`/`reg_inv()`/
`reg_quad()`/`reg_abexp()` select one of the manual's seven regression models;
`stat.rega`/`stat.regb` are the fitted coefficients, `stat.regc` is the
quadratic `c`, and `stat.regr` is the correlation coefficient. `dms(deg, min,
sec)` is a sexagesimal literal and `dms()` toggles the display.

Constants: `pi` → `π` (or `pi` with `--ascii`), `e` → `e`, and the 40
scientific constants under the `phys.` namespace (below).

In `#mode BASE`, `and`, `or`, `xor` and `xnor` are infix operators and integer
literals may be base-tagged (`0x1F`, `0b1010`, `0o17`, emitted `1Fh`, `1010b`,
`17o`). `and` binds tighter than `or`/`xor`/`xnor`.

The only exponentiation operators are `^` and its alias `**`. There is no
`min`/`max`, and no integer division or modulo (call `frac(a, b)` for the
calculator's `┘` key). Unknown function names are a
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

**First, do not hand-optimise.** The transpiler already pre-calculates constant
expressions, propagates a value that never changes into its uses, decides a
constant `if`/`while`, and drops stores nothing reads. Write the program clearly
and let it do that — `2 * 3 + 4` becomes `10`, and `let n = 3; for (…; i < n; …)`
becomes `To 3` without help.

Measure rather than guess. `fx50 size program.fxc` reports the keys a program
costs out of the **680-byte store shared by all four program areas**, and prints
what the optimiser saved so the number is meaningful:

```console
$ fx50 size program.fxc
Program size for program.fxc
  22 key(s) in 6 statement(s), largest statement 8 keys
  fits: 22 of 680 bytes used, 658 left
  optimiser: nothing to remove (22 keys either way)
```

When you do need to shrink a program, in this order of preference:

1. **Use `const` for fixed values.** A `const` is inlined where it is used and
   consumes no memory:

   ```c
   const scale = 3;
   let total = 7 * scale;   // emits `7×3→A`; only `total` uses a memory
   ```

2. **Use `#data` for values that come from JSON.** Those become literals too.
   An array is the other option for a table of values, but each element costs a
   memory, so `#data` is preferable for anything fixed. See
   [Arrays](#arrays).
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
| Use after free | `free a; print(a);` | `` `a` is not defined here: it was freed `` |
| Already declared | `let x = 1; let x = 2;` | `` `x` is already declared `` |
| Own initializer | `let x = x + 1;` | `` `x` cannot be used in its own initializer `` |
| Double free | `free a; free a;` | `` `a` was already freed (double free) `` |
| Freeing what has no memory | `const k = 1; free k;` | `` `k` is a `const`, which uses no memory `` |
| Freeing an unknown name | `free nope;` | `` `nope` is not a variable `` |

### `let` declares, so a name can have two lives

A `let` **introduces** a name. Declaring a name that is still live is an error,
not a silent shadow:

```c
let x = input();
let x = input();   // ERROR: `x` is already declared
```

Once the name has been `free`d, declaring it again is the intended way to reuse
it, and it gets a fresh memory:

```c
let x = input();
free x;
let x = input();   // fine: a second, independent `x`
```

A plain assignment never declares, so after a `free` you must use `let` to bring
the name back:

```c
free x;
x = 1;             // ERROR: `x` is not defined here
```

And a declaration cannot see its own name — the value is computed before the
declaration takes effect:

```c
let x = x + 1;     // ERROR: `x` cannot be used in its own initializer
```

### `free` under jumps or loops needs `unsafe_free`

A `goto` can re-enter code whose memory has since been re-used, and a loop body
re-enters on every iteration, so a checked `free` inside a loop body or in a
program containing `goto`/`label` is an error. `unsafe_free` says you have
checked it yourself:

```c
unsafe_free x;
```

`unsafe_free` still rejects double frees, unknown names and `const`s — it only
waives the control-flow check. Programs with jumps or loops and no `free` are
unaffected, because nothing is ever re-used. A `free` outside every loop is
fine even when the program loops.

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
| `CMPLX` | `CPLX`, `COMPLEX` | unlocks `i()`, `arg`, `conjg`, `polar`, `to_cartesian`/`to_polar`, `re_im` |
| `BASE` | `BASEN`, `BASE-N` | integer arithmetic; bitwise words, base literals, `dec`/`hex`/`bin`/`oct`, `not`/`neg`; **rejects every float built-in and `pi`/`e`/`phys.*`** |
| `SD` | `STAT`, `STATS`, `STATISTICS` | unlocks `stat.*`, `dt`, `clrstat`, `freqon`/`freqoff` |
| `REG` | `REGRESSION` | everything SD has, plus the `y`/regression stats (`stat.sumy`, `stat.regA`, …) and the regression model keys (`reg_lin`…`reg_abexp`) |

Rules:

* The directive must be the **first** non-comment, non-blank line. A `#mode`
  anywhere else is an error.
* Case-insensitive, and `#mode=CMPLX` is accepted.
* With no header and no `--mode` flag, the emitted PRGM has no header and runs
  in COMP.
* A header (or `--mode`) is **copied into the output**, so the mode survives the
  round trip.
* The mode is checked at transpile time, exactly as the interpreter checks it:
  using `stat.sumx` in COMP, `arg` in COMP, `not` in COMP, `0xFF` in COMP, or
  `sqrt` in BASE is a **transpile error**, not a `Mode ERROR` at run time.
  `pol`/`rec` are the one COMP/CMPLX-only pair — they are not offered in
  SD/REG either.

In `BASE`, all float built-ins and `pi`/`e`/`phys.*` are rejected at transpile
time:

```console
$ fx50 build base.fxc      # with: #mode BASE / print(sqrt(4));
fx50: `sqrt` is not available in BASE mode (needs COMP, CMPLX, SD or REG) (line 2, column 7)
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
2. **`for` bounds are adjusted, then folded.** `i < limit` becomes `To limit-1`,
   and `i > limit` becomes `To limit+1 Step -step`. A literal limit is folded, so
   `for (i = 0; i < 5; ...)` emits `For 0→A To 4 Step 1`, while a variable limit
   keeps the subtraction: `To A-1`. The adjustment is correct but surprising if
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
7. **Every key is a call, not an operator.** `x²` is `sqr(x)`, `x!` is
   `fact(x)`, `%` is `pct(x)`, `┘` is `frac(a, b)`, `nPr` is `npr(n, r)`, `∠`
   is `polar(r, θ)`. The `and`/`or`/`xor`/`xnor` words are the exception: those
   are real infix operators (BASE mode only). `i` alone is still a variable;
   the imaginary unit is `i()`.
8. **Error timing differs.** Malformed source, an eighth variable, `input()` in
   an expression, an unknown function, a mode violation, and `free` under
   re-entrant control flow are all caught at **transpile time** with a line and
   column. `break` outside a loop, a `goto` with no matching label, and
   division by zero are **runtime** errors (`Argument ERROR`, `Go ERROR`,
   `Math ERROR`). Do not assume a clean `fx50 build` means the program will run
   — run it if you can.
9. **The fixed `M` memory is shared.** `mplus`/`mminus`/`mvalue` use the
   calculator's `M`. The allocator reserves `M` for the whole program when they
   appear, so a program that uses them has only six memories for its own
   variables.

---

## 8. Splitting a program with `#include`

A program can inline another file's text at transpile time:

```c
// main.fxc
#include "lib/squares.fxc"
fn main() { let n = input(); print(square(n)); }
```

`fx50 build main.fxc` substitutes the fragment before compiling, so the
calculator still receives one flat program. This is the `.fxc` analogue of
C's `#include` or Rust's `include_str!`.

Rules that matter when generating code:

* Syntax is exactly `#include "path"`, starting the line.
* The path is relative to the file containing the directive, and includes
  nest.
* **A fragment may hold statements or `fn` definitions.** A statement is
  spliced in place and shares the includer's seven memories; a `fn` is scoped to
  its own parameters and locals, so an included **library** contributes only its
  function names and cannot pollute the caller:

  ```c
  // lib/geometry.fxc
  fn square(x) = x * x;

  // main.fxc
  #include "lib/geometry.fxc"
  fn main() { print(square(4)); }
  ```

* Including a file that defines a `fn` makes the program function-based, so it
  then needs `fn main()` and cannot have loose top-level statements.
* Variables are allocated in **expanded** source order, so the names inside a
  spliced statement fragment are numbered where the `#include` line sits. Place includes after
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

Prefer `#include` for sharing code, and put the shared code in `fn`s so the
caller's memories are untouched. A fragment of bare statements is also allowed,
but it must be included **inside a function body** (normally `main`) — nothing
runs at the top level. Such a fragment has no parameters and no return value;
use a `fn` when you want an interface.

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
* Reference values with `.field` and `[index]`: `config.offsets[1]`. An index
  may be a constant expression (`config.offsets[1 + 1]`), but it must resolve to
  a whole number while transpiling; a run-time index is an error.
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
- [ ] Every program has a `fn main()` entry point, and no loose top-level
      statements (top-level `const` is fine). Every function except `main` only
      uses its parameters, its locals, top-level `const`/`#data` values, other
      functions and built-ins — no globals, no recursion, and `return` only as
      the last statement. Inside `main`, the classic order-free rules still
      apply, so `print(a);` alone is valid.
- [ ] `#mode` (if used) is the very first line, and the mode is spelled
      correctly.
- [ ] At most **seven live** variables at any point; fixed values use `const`
      (and tables use `#data`) so they cost no memory, and `free` releases a
      variable that is finished with.
- [ ] `#data` paths resolve to numbers or booleans; array indices are literals.
- [ ] No use after free, no double free, no re-declaring a live name, and no
      `free` inside a loop body or alongside `goto`/`label` (use `unsafe_free`
      if you have verified the control flow).
- [ ] `input()` appears only as a complete assignment right-hand side.
- [ ] Calculator keys are written as calls, not operators: `sqr(x)`, `fact(x)`,
      `pct(x)`, `frac(a, b)`, `npr(n, r)`, `polar(r, θ)`, `ran()`, `i()`. The
      imaginary unit is `i()`, not a bare `i`. No `%`, `&&`, `||`, `!`, `++`,
      `--`, `+=`, or strings.
- [ ] Every array index resolves to a **literal** while transpiling — either
      written out, folded from a constant expression, or produced by a
      constant-bound `for` the transpiler can unroll. A run-time index is an
      error. Indices are in range, and the array plus everything else fits in
      seven memories — `fx50 regs` shows the plan. Arrays are freed whole
      (`free v;`), never element by element.
- [ ] Only built-ins from the table in §4 are called, with the right arity
      (`log` takes 1 or 2 arguments; `root`, `pol`, `rec`, `frac`, `npr`, `ncr`,
      `polar` take 2 and `dms` takes 3; `ran`, `i`, `ans`, `mvalue` take 0;
      everything else takes 1).
- [ ] `goto`/`label` use a single digit `0`–`9`. A `=>` guards one statement
      only; use `if` for more.
- [ ] Loop bodies that need more than one statement use `{ }` — braces are
      optional for a single statement and it is easy to lose the rest.
- [ ] Every `goto` has a matching `label`, and `break` only appears inside a
      loop.
- [ ] In `#mode BASE`, no float built-ins, no `pi`, no `e`, no `phys.`
      constant; base literals (`0x1F`) and the bitwise words are welcome.
- [ ] Mode-specific keys match the mode: complex keys (`i()`, `arg`, `conjg`,
      `polar`, `to_cartesian`, `re_im`) need CMPLX, `stat.*` and `dt` need SD/REG
      (`stat.sumy`… and the `reg_*` model keys need REG), base keys need BASE,
      setup and `dms` need a non-BASE mode, `pol`/`rec` need COMP or CMPLX.
- [ ] Scientific constants are written `phys.<name>` and statistical variables
      `stat.<name>`, always with the namespace — never as a bare name, which
      would become a variable.
- [ ] `#include` paths exist, start the line, and contain no `#mode`; the
      seven-memory budget is respected *after* expansion.
- [ ] If you can run commands, a `#tests` table (or a `.tests.json` suite)
      covers the happy path **and** at least one error case, and `fx50 test`
      reports `0 failed`.
- [ ] No hand-optimising was needed: constant expressions, constant conditions
      and unused stores are the transpiler's job, and `fx50 size` was used to
      check the 680-byte budget rather than estimating it.

If you can, verify with the compiler before declaring success:

```bash
fx50 build your.fxc >/dev/null && echo "transpiles"
fx50 regs  your.fxc            # check the seven-memory budget
fx50 size  your.fxc            # check the 680-byte budget
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
