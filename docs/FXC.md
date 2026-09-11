# The `.fxc` language

`.fxc` is a small C-like language that transpiles to CASIO fx-50FH II **PRGM**
source. This is its manual: the grammar, the statements, the memory model, and
the constructs that are checked while transpiling. To *run* any of it you need
the `fx50` binary — see the [main README](../README.md#build).

```bash
fx50 build program.fxc          # print PRGM (calculator glyphs) to stdout
fx50 build --ascii program.fxc  # print ASCII aliases instead
fx50 run   program.fxc          # transpile, then execute (? reads from stdin)
fx50 regs  program.fxc          # show how the seven memories are used
fx50 test  program.fxc          # run the cases the program carries
```

The crate that implements this is [`fx-transpiler`](../crates/fx-transpiler/README.md);
this document is about the language, not the Rust API.

> Authoring `.fxc` source, especially from an AI agent? Read
> [AI-AGENTS.md](AI-AGENTS.md) first — a rule-oriented guide covering the
> grammar, the memory limit, the modes, and the gotchas that most often make
> generated programs fail to transpile.

## Complete grammar

```text
program    := directive* item*
item       := funndef | 'const' NAME '=' expr ';'
             -- a file with no `fn main()` is a library: it defines functions
             -- for another program to `#include`, and builds to nothing
funndef    := 'fn' NAME '(' [NAME (',' NAME)*] ')' ('=' expr ';' | body)
directive  := '#mode' MODENAME             -- must be first if present
            | '#include' STRING             -- top-level only; a library
            | '#data' NAME '=' JSON ';'    -- may appear anywhere
            | '#tests' '=' JSON ';'        -- sugar for `#data tests = ...`
stmt       := 'let' NAME '=' expr ';'      -- declaration
            | 'let' NAME '[' INTEGER? ']'  -- array declaration
              ('=' '{' expr (',' expr)* '}')? ';'
            | 'const' NAME '=' expr ';'    -- compile-time constant
            | NAME '=' expr ';'            -- assignment
            | NAME '[' expr ']' '=' expr ';'   -- element assignment (index resolves to a literal)
            | 'free' NAME ';'              -- release a memory
            | 'unsafe_free' NAME ';'       -- release it without the control-flow check
            | 'return' [expr] ';'          -- last statement of a `fn` body
            | 'print' [ '(' expr ')' | expr ] ';'
            | 'if' '(' expr ')' body ('else' body)?
            | 'while' '(' expr ')' body
            | 'for' '(' forinit ';' expr ';' NAME '=' expr ')' body
            | 'break' ';'
            | 'goto' DIGIT ';'
            | 'label' DIGIT ';'
            | 'mplus' '(' expr ')' ';'      -- expr M+
            | 'mminus' '(' expr ')' ';'     -- expr M-
            | 'clrmemory' | 'clrstat' | 'freqon' | 'freqoff' '(' ')' ';'
            | 'deg' | 'rad' | 'gra' | 'dec' | 'hex' | 'bin' | 'oct'
            | 'to_cartesian' | 'to_polar' | 're_im' '(' ')' ';'
            | 'reg_lin' | 'reg_log' | 'reg_exp' | 'reg_pwr' | 'reg_inv'
            | 'reg_quad' | 'reg_abexp' '(' ')' ';'
            | 'dms' '(' ')' ';'               -- the bare °′″ conversion key
            | 'fix' | 'sci' | 'norm' '(' INTEGER ')' ';'
            | 'dt' '(' expr (',' expr (',' expr)?)? ')' ';'   -- x DT / x,y DT / x,y;f DT
            | expr '=>' stmt                -- conditional jump (⇒)
            | '{' stmt* '}'
            | ';'                            -- empty statement
            | expr ';'                       -- evaluate, do not display
body       := stmt | '{' stmt* '}'           -- braces optional for one statement
forinit    := ['let'] NAME '=' expr

expr       := bitwise_or
bitwise_or := bitwise_and (('or' | 'xor' | 'xnor') bitwise_and)*
bitwise_and:= equality ('and' equality)*
equality   := comparison (('==' | '!=') comparison)*
comparison := additive  (('<' | '<=' | '>' | '>=') additive)*
additive   := multiplicative (('+' | '-') multiplicative)*
multiplicative := unary (('*' | '/') unary)*
unary      := '-' unary | power
power      := primary (('^' | '**') unary)?   -- right-associative
primary    := NUMBER | BASENUMBER | 'pi' | 'e' | 'input()' | NAME | dataref
            | NAME '(' [expr (',' expr)*] ')' | 'phys' '.' CONSTNAME
            | 'stat' '.' STATNAME | '(' expr ')'
BASENUMBER := '0x' HEXDIGITS | '0b' BINARY | '0o' OCTAL   -- BASE mode
STATNAME   := 'n' | 'sumx' | 'sumx2' | … | 'rega' | 'regb' | 'regc' | 'regr'
dataref    := NAME accessor+
accessor   := '.' NAME | '[' expr ']'
```

**Binding strength, tightest first:** `(...)` / calls → `^ **` → unary `-` →
`* /` → `+ -` → `< <= > >=` → `== !=` → `and` → `or xor xnor` (loosest).

Notes:

* Every statement ends with `;`. There is no newline termination and no
  automatic semicolon insertion: `print(a)` is a syntax error.
* Comments are `// line` and `/* block */`.
* Identifiers are ASCII letters, digits and `_`, and may not start with a digit.
  The display symbols of scientific constants (`ħ`, `μμ`, `R∞`) are accepted
  only directly after `phys.` — a bare `π` is rejected rather than silently
  becoming a variable.
* There is no `&&`/`||`/`!`, no `++`/`--`/`+=`, and no strings. Comparisons
  produce `1` or `0`. The base-n words `and`/`or`/`xor`/`xnor` exist, but only
  in `#mode BASE`.
* **Every program is a set of functions with a `fn main()` entry point.**
  User-defined functions (`fn`) are inlined at each call, and no statement runs
  at the top level — only `fn` definitions and `const` declarations sit there.
  A file with no `fn` at all is an error. See [Functions](#functions).
* There is one numeric type: an `f64`, the same as the calculator computes with.
* An array or `#data` index is written as an expression but must resolve to a
  non-negative whole number while transpiling: constant expressions are folded,
  and a constant `for` loop that indexes an array is unrolled, but a run-time
  index is an error (see [Arrays](#arrays)).

## Statements

A program's statements live inside a function body, normally `fn main()`:

```c
// line comment, and /* block comments */
let a = input();          // read a number
let b = 2;                // declaration
let v[3] = {1, 2, 3};     // array: three elements, one memory each
v[1] = 9;                 // write one element
a = a + b * 2;            // assignment
print(a);                 // display with ◢
if (a > 0) { print(1); } else { print(0); }
while (a > 0) { a = a - 1; }
for (let i = 0; i < 5; i = i + 1) { print(i); }
break;
label 1;
goto 1;
deg();                    // angle unit: Deg
fix(3);                   // display: Fix 3
x > 0 => print(1);        // conditional jump
mplus(x);                 // x M+
```

* **`let` declares.** It introduces a name and allocates its memory. Declaring a
  name that is still live is an error, but after a `free` the name can be
  declared again — see [Memory and `free`](#memory-and-free). In `main`, a name
  used for the first time is declared on sight (the classic order-free rule);
  every other function is closed and needs a real declaration.
* **Arrays** are declared with a size in brackets and indexed with a literal:
  `let v[3];`, `let v[] = {1, 2, 3};`, `v[0] = 7;`. See
  [Arrays](#arrays) for why the index has to be a constant.
* **`input()`** is only legal as the entire right-hand side of an assignment.
  `let a = input();` is fine; `print(input());` and `let a = input() + 1;` are
  errors.
* **`goto`/`label`** take a single digit `0`–`9`.
* **`expr => stmt;`** is the calculator's `⇒` conditional jump: run `stmt`
  when `expr` is non-zero. It can guard a single assignment, `print`, or
  expression statement (use `if` for anything larger).
* **Calculator keys that act on the whole machine** are written as calls and
  end with `;`: `deg();`, `fix(3);`, `clrmemory();`, `dt(x, y);`, `mplus(x);`.
  They are listed under [Calculator keys](#calculator-keys).

Expressions support `+ - * /`, `^` / `**` (power), unary `-`, comparisons
`== != < <= > >=`, the base-n words `and`/`or`/`xor`/`xnor` (in `#mode BASE`),
parentheses, the constants `pi` and `e`, and every calculator key — see
[Calculator keys](#calculator-keys). Values are real numbers.

## Arrays

An array is a group of values that each take **one memory**. Declare one with a
size in brackets, then index it with a **literal**:

```c
fn main() {
    let v[3];                 // three elements, no values yet
    let w[3] = {4, 8, 15};    // three elements, initialised (A B C)

    print(w[0] + w[2]);       // read an element
    w[1] = 16;                // write an element
    free w;                   // release the whole array at once
    let u[] = {1, 2, 3};      // size inferred from the list, into `w`'s memories
}
```

`v[0]`, `v[1]`, … are ordinary memories; the report shows which:

```console
$ fx50 regs examples/arrays.fxc
Memory plan for examples/arrays.fxc
  A  data[0]  → again[0]   (reused after `free data`)
  B  data[1]  → again[1]   (reused after `free data`)
  C  data[2]  → again[2]   (reused after `free data`)
  D  sums[0]
  X  sums[1]

  5 of 7 memories used; free: Y M
  released with `free`: data
  1 data table(s) (no memory): tests
```

### Why the index must be a constant

**PRGM has no indirect addressing.** There is no way to say "the memory whose
number is in `X`" — a memory can only be named literally, `A`, `B`, `C`. So
`v[k]` cannot be a run-time lookup: the transpiler has to know `k` while it is
translating.

That restriction buys the thing this language is for. An element reference
compiles to **a single memory letter and no instructions at all**:

```c
fn main() {
    let v[3] = {10, 20, 30};
    print(v[1]);
}
```

```text
10→A
20→B
30→C
B◢
```

The alternative — a run-time index — would need an `If`/`Else` chain over every
element, and the machine has only **680 bytes of program memory shared by all
four program areas**. On a machine that small, `v[0]` being free and `v[i]`
costing a dozen bytes per lookup is the right trade.

To walk an array, use a loop whose bounds are known while transpiling. The
transpiler **unrolls** such a loop and replaces the counter with each of its
values, so the index becomes a literal:

```c
fn main() {
    let v[3];
    for (let i = 0; i < 3; i = i + 1) { v[i] = input(); }
}
```

```text
?→A
?→B
?→C
```

The loop must be the canonical `for` shape with integer bounds after
compile-time evaluation. A run-time bound (`i < n`), a `break`, `goto`/`label`,
a declaration, or a `free` in the body all stop the unroll, and the index is
reported as an error — write those loops out by hand. A loop that only touches
scalars keeps its compact native `For`/`Next` form; unrolling happens **only**
when an array index needs it, since otherwise it would cost bytes rather than
save them. A counter the rest of the program never mentions is dropped along
with the loop, as it would be if you unrolled the loop by hand; if it is read
afterwards, it keeps the value the `For` would have left.

### What the transpiler checks

| Mistake | Message |
| --- | --- |
| Index past the end | ``index 3 is out of range for `v` (length 3)`` |
| Indexing a scalar | `` `x` is not an array; it holds a single value`` |
| Using an array bare | `` `v` is an array; index it, as in `v[0]` `` |
| Assigning the name | `` `v` is an array; assign to an element, as in `v[0] = ...` `` |
| Wrong initialiser count | `` `v` is declared with 3 element(s) but has 2 initialiser(s)`` |
| No size and no list | `` `v[]` needs a size or an initialiser list…`` |
| `const v[3]` | `` `const` cannot declare an array; use `let v[…]` instead`` |
| Freeing one element | `` `free v[…]` releases one element, which would strand the others…`` |
| Not enough memories | ``no room for array `w`: it needs 3 memories but only 2 are free (Y M)…`` |
| Run-time index | `for (i < n) v[i];` | `` `v[…]` needs a compile-time index… `` |

Notes:

* **The budget is seven memories including everything else.** `let v[5]` plus a
  loop counter plus an accumulator fills all seven. A `#data` table is often the
  better home for a constant table, since it costs no memory at all.
* **An array is released as a whole.** There is deliberately no `free v[0];`:
  releasing one element would strand the others in memories nothing can hand out
  again.
* **An array with no initialiser list emits nothing**, like an unassigned
  variable. The memories exist from the declaration; the values are whatever you
  put there.
* **A `const` cannot be an array**, because a `const` is inlined and has no
  memory. Use `#data` for a fixed table of values.
* **A name cannot be both.** `let x = 1; let x[2] = {1,2};` is an error; use a
  fresh name, or `free` first.

## Scientific constants

The calculator's 40 built-in scientific constants are reached through the
`phys` namespace: `phys.NAME`, where `NAME` is the constant's ASCII name or the
symbol the display shows. `phys` is a reserved word, and a bare `h` or `hbar`
is an ordinary variable, so the namespace never pollutes the seven memories.

```c
fn main() {
    print(phys.h);      // Planck constant
    print(phys.ħ);      // the same constant, by its display symbol
    print(phys.C0);     // speed of light in vacuum
    print(phys.e);      // elementary charge (namespaced, so not Euler's e)
}
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
`fx_transpiler::constants::CONSTANTS`, and `fx50 constants` prints it.

## Calculator keys

The transpiler is a front end for the whole machine, so **every PRGM key** has
an `.fxc` spelling. Keys that produce a value are ordinary calls; keys that act
on the machine are statements ending in `;`. The emitter writes the calculator's
native keystroke back out, so a `.fxc` program can reach all of PRGM.

### Value keys

| `.fxc` | Emits | Notes |
| --- | --- | --- |
| `sqrt(x)` / `cbrt(x)` | `√(x)` / `∛(x)` | |
| `root(n, x)` | `nx√(x)` | the `x√(` key; **index first** |
| `pow10(x)` / `exp(x)` | `10^(x)` / `e^(x)` | |
| `abs(x)` / `rnd(x)` | `Abs(x)` / `Rnd(x)` | |
| `sin`…`atanh(x)` | `sin(x)`… | angle unit follows `deg`/`rad`/`gra` |
| `log(x)` / `log(b, x)` / `ln(x)` | `log(x)` / `log(b,x)` / `ln(x)` | |
| `inv(x)` / `sqr(x)` / `cube(x)` | `x⁻¹` / `x²` / `x³` | postfix keys |
| `fact(x)` / `pct(x)` | `x!` / `x%` | postfix keys |
| `frac(a, b)` | `a┘b` | the fraction key |
| `npr(n, r)` / `ncr(n, r)` | `<n>nPr<r>` / `<n>nCr<r>` | |
| `pol(x, y)` / `rec(x, y)` | `Pol(x,y)` / `Rec(x,y)` | COMP or CMPLX; write into `X`/`Y` |
| `arg(x)` / `conjg(x)` | `arg(x)` / `Conjg(x)` | CMPLX |
| `rep(z)` / `imp(z)` | `(z+Conjg(z))÷2` / `(z-Conjg(z))÷(2i)` | CMPLX; the real and imaginary parts — **no key on this model**, so they lower to arithmetic |
| `polar(r, θ)` | `r∠θ` | CMPLX |
| `dms(deg, min, sec)` | `deg°min′sec″` | sexagesimal literal; the three arguments must be literals |
| `not(x)` / `neg(x)` | `Not(x)` / `Neg(x)` | BASE |
| `ran()` | `Ran#` | pseudo-random, `[0, 1)`; deterministic from a fixed seed |
| `i()` | `i` | CMPLX; bare `i` is still an ordinary variable |
| `ans()` | `Ans` | the previous result memory |
| `mvalue()` | `M` | the fixed `M` memory; see below |

`frac`, `npr`, `ncr` and `polar` are **infix** keys, but are written as
two-argument calls for one uniform syntax; the emitter places them between their
arguments with the machine's own precedence (`┘` binds tighter than `×`, `nPr`
looser).

### Statement keys

| `.fxc` | Emits |
| --- | --- |
| `mplus(x);` / `mminus(x);` | `x M+` / `x M-` |
| `clrmemory();` | `ClrMemory` |
| `clrstat();` | `ClrStat` |
| `freqon();` / `freqoff();` | `FreqOn` / `FreqOff` |
| `dt(x);` / `dt(x, y);` / `dt(x, y, f);` | `x DT` / `x,y DT` / `x,y;f DT` |
| `deg();` / `rad();` / `gra();` | `Deg` / `Rad` / `Gra` |
| `fix(n);` / `sci(n);` | `Fix n` / `Sci n` (`n` = 0–9) |
| `norm(n);` | `Norm n` (`n` = 1 or 2) |
| `dec();` / `hex();` / `bin();` / `oct();` | `Dec` / `Hex` / `Bin` / `Oct` |
| `to_cartesian();` / `to_polar();` | `▶a+b𝑖` / `▶r∠θ` |
| `re_im();` | `Re⇔Im` |
| `reg_lin();` … `reg_abexp();` | `Lin` … `AB-Exp` |
| `dms();` | `°′″` |

The digit of `fix`/`sci`/`norm` must be a literal; the mode each key needs is in
the [Modes](#modes) table.

### Statistical variables

Statistical values are reached through the `stat.` namespace, mirroring `phys.`:
`stat.n`, `stat.sumx`, `stat.sumx2`, `stat.sumy`, `stat.sumy2`, `stat.sumxy`,
`stat.meanx`, `stat.meany`, `stat.sigmax`, `stat.sigmay`, `stat.sx`, `stat.sy`,
`stat.minx`, `stat.maxx`, `stat.miny`, `stat.maxy`, `stat.rega`, `stat.regb`,
`stat.regc`, `stat.regr`.

```c
#mode REG
fn main() {
    print(stat.meanx);   // x̄◢
    print(stat.regA);    // regA◢
}
```

Glyph output uses the display spelling (`Σx`, `x̄`, `σx`, `regA`); `--ascii` uses
the ASCII alias (`sumx`, `meanx`, `sigmax`, `rega`). A bad name is reported by
name rather than becoming a variable.

In `#mode REG` the regression model is selected with `reg_lin()`, `reg_log()`,
`reg_exp()`, `reg_pwr()`, `reg_inv()`, `reg_quad()` or `reg_abexp()`. The
fitted coefficients are `stat.rega`/`stat.regb`, plus `stat.regc` for
`reg_quad()`, and `stat.regr` is the correlation coefficient (the multiple
correlation for `reg_quad()`).

### Sexagesimal values

`dms(deg, min, sec)` writes a sexagesimal literal, `deg°min′sec″`. The three
arguments must be literals, because the machine's `°′″` key only accepts
digits:

```c
fn main() {
    print(dms(2, 15, 18));   // 2°15′18″◢
}
```

With no arguments, `dms()` is the bare conversion key: it toggles the displayed
value between decimal and sexagesimal. `dms()` belongs to every mode except
BASE.

### The fixed `M` memory

`mvalue()`, `mplus()` and `mminus()` address the calculator's fixed `M` memory
by letter, so the allocator **reserves** `M`: no `.fxc` variable is ever placed
there while the program uses those keys. `fn main() { let a = 1; mplus(a);
print(mvalue()); }` emits `1→A`, `A M+`, `M◢` — the variable takes `A`, and `M`
stays the accumulator.

### Bitwise operators and base literals

In `#mode BASE`, `and`, `or`, `xor` and `xnor` are infix operators, and integers
may be written `0x1F` (hex), `0b1010` (binary) or `0o17` (octal). The emitter
tags them the way the calculator does:

```c
#mode BASE
fn main() {
    print(0b1010 and 0b1100);   // 1010b and 1100b◢
}
```

`and` binds tighter than `or`/`xor`/`xnor`; both are looser than the comparisons.
In `--ascii`, a display key after a base literal keeps a space (`FFh disp`) so it
re-lexes as `FFh` then `disp`.

### `Ran#`

`ran()` emits the machine's `Ran#`, a pseudo-random number in `[0, 1)` drawn
from a fixed-seed xorshift generator. Running the same program twice gives the
same sequence, which is what makes it usable in `#tests`.

## Modes

The calculator forces an operating mode before it will compute. A program may
declare its mode with a leading `#mode` directive, which must be the first
non-comment, non-blank line:

```c
#mode CMPLX
fn main() {
    let a = input();
    print(a + 1);
}
```

The valid names are `COMP`, `CMPLX`, `BASE`, `SD` and `REG` (case-insensitive;
`STAT` is an alias for `SD`). `COMP` is the default, so a program without a
directive emits no header. When a header is present it is re-emitted as the
first line of the PRGM, and the program is checked against the mode's
capabilities — the same rules the interpreter enforces, so a construct the mode
does not offer is a **transpile error** rather than a `Mode ERROR` on the
calculator.

| Constructs | Need |
| --- | --- |
| `sqrt`, `sin`, trig/logs, `^`, `┘`, `!`, `%`, `npr`/`ncr`, `pol`/`rec`, `ran`, `pi`, `e`, `phys.*` | anything except BASE |
| `deg`/`rad`/`gra`, `fix`/`sci`/`norm` | anything except BASE |
| `hex`/`bin`/`oct`, base literals (`0x1F`), `and`/`or`/`xor`/`xnor`, `not`/`neg` | BASE |
| `i()`, `arg`, `conjg`, `polar`, `to_cartesian`/`to_polar` | CMPLX |
| `stat.*`, `dt`, `clrstat`, `freqon`/`freqoff` | SD or REG |
| `stat.sumy` … `stat.regr` (the `y` and regression values) | REG |
| `reg_lin` … `reg_abexp` (the regression model) | REG |
| `re_im()` | CMPLX |
| `dms(deg, min, sec)`, `dms()` | anything except BASE |
| `pol`/`rec` specifically | COMP or CMPLX |

```c
#mode BASE
fn main() {
    print(a / b);   // ok: `/` maps to `÷`, which BASE offers
    print(sqrt(a)); // error: `sqrt` is not available in BASE mode
    print(phys.h);  // error: `h` (Planck constant) is not available in BASE mode
    print(not(a));  // ok: `Not(` is a BASE key
}
```

The mode can also be forced from Rust with `Options::mode`, which takes
precedence over any header in the source (see the
[crate README](../crates/fx-transpiler/README.md#library)).

## Functions

A `.fxc` program can define functions. PRGM has no call instruction, so a
function is **inlined at each call** while transpiling — it never reaches the
calculator as a function, costs no memory of its own, and a function that is
never called costs nothing.

```c
fn square(x) = x * x;                 // expression function: no locals

fn sum_to(n) {                        // procedure: locals, loop, return
    let total = 0;
    for (let i = 1; i <= n; i = i + 1) { total = total + i; }
    return total;
}

fn main() {                           // the entry point
    print(square(4));                 // emits `4×4` then folds to `16◢`
    print(sum_to(10));
}
```

### `fn main()` is the entry point

**Every program is a set of functions with a `fn main()` entry point**, whether
or not it defines any other function. A file with no `fn` at all is an error,
not a top-level script:

* `fn main()` is **required**. Without it the transpiler reports that the
  program needs an entry point.
* Only `fn` definitions and top-level `const` declarations may sit beside it. A
  loose statement is an error telling you to move it into `main`.
* `main` takes no parameters.

`main` is the one body that is **not closed**: it keeps the classic order-free
scoping, so a name used for the first time is declared on sight. Loose code
moved into `main` therefore transpiles exactly as it did at the top level.
Every other function is closed (see the next section).

That boundary is what keeps an included library from running code or creating
variables in the includer's namespace.

### Every function except `main` is closed over its own names

A function body may use:

* its **parameters**,
* names it declares with `let`/`const`/`let v[…]` (including `for (let i …)`),
* top-level `const` values and `#data` tables (compile-time, no memory),
* other functions, and the built-in calculator keys.

Anything else is an error: a function cannot read or create a **global**.
`fn f(x) = x + a;` is rejected unless `a` is a parameter, a local or a
top-level `const`. This is deliberate: a library function cannot reach into the
caller's memories, so including it cannot change the caller's behaviour by
accident.

Names declared inside a function are **local and hygienic**. The transpiler
renames them per call (`sum_to$total$1`), so a caller variable, another
function, or a second call to the same function cannot collide with them. Two
calls to a procedure each get their own copies:

```c
fn add(a, b) { let s = a + b; return s; }
fn main() { print(add(1, 2)); print(add(3, 4)); }   // two independent `s`
```

### Arguments are passed by name

A parameter reference is replaced by the **argument expression at each
mention**, not evaluated once. This is why functions cost nothing to call, and
it has three consequences worth knowing:

* **A variable argument behaves like a reference.** `fn inc(x) { x = x + 1; }`
  with `inc(a)` becomes `a = a + 1`, so the caller's `a` changes.
* **An expression argument is re-evaluated.** `twice(ran())` draws two random
  numbers, because `ran()` is substituted into both mentions.
* **An argument mentioned `n` times is emitted `n` times**, so spelling it out
  more than once costs bytes. A single mention is free.

Because an assignment is written back through the name, a parameter that the
function **assigns** needs an assignable argument — a variable or an array
element, not an expression:

```c
fn bump(x) { x = x + 1; }
fn main() {
    let a = 5;
    bump(a);          // ok: a becomes 6
    bump(1 + 2);      // error: `x` is assigned, so its argument must be a variable
}
```

### Returning more than one value

`return` may only be the **last statement** of a function body, so there is one
return value. To return two results, pass variables and assign them — the
transpiler writes back through them:

```c
fn minmax(a, b, lo, hi) {
    lo = a;
    hi = b;
    if (a > b) { lo = b; hi = a; }
}

fn main() {
    let small = 0;
    let big = 0;
    minmax(3, 7, small, big);   // small = 3, big = 7
    print(small);
    print(big);
}
```

### What is not allowed

| Mistake | Why |
| --- | --- |
| Recursion, direct or indirect | PRGM has no call stack |
| A `return` that is not the last statement | It would need a jump out |
| A procedure called from a `while`/`for` condition | Inlining would move its statements out of the loop |
| A parameter used as an array (`x[0]`) | Pass the elements as separate scalars |
| A parameter that is also a local | Rename one of them |
| Redefining a built-in (`fn sqrt(…)`) | Choose another name |

See [`examples/functions.fxc`](../examples/functions.fxc) for a working program
and [`examples/lib/geometry.fxc`](../examples/lib/geometry.fxc) for a library.

## Memory and `free`

PRGM has exactly seven memories — `A B C D X Y M` — and no more. A `.fxc` name is
assigned one of them in **first-seen order**, and keeps it until the program ends
or you release it.

Names are never given a shared memory implicitly. A memory's final value is part
of the program's observable result — PRGM leaves its answer in one, a later
program or the user can read it — so the transpiler cannot prove a name is dead.
Only you know when a value is finished with, and saying so is what `free` is for.

The ways to fit a program, in order of preference:

* **`const NAME = <expr>;`** declares a fixed value that is **inlined** at every
  use and occupies no memory. The expression must be constant — numbers, `pi`,
  `e`, `phys.` constants, `#data` values and other `const`s.
* **`#data NAME = <json>;`** resolves a JSON value at transpile time, likewise
  producing literals. See [Compile-time data](#compile-time-data).
* **`free NAME;`** releases a variable's memory so the next new variable can use
  it:

  ```c
  fn main() {
      let first = 5;
      print(first);
      free first;        // release the memory
      let second = 7;    // reuses it
      print(second);
  }
  ```

  `free` emits nothing; it hands the memory back to the allocator. The value
  left in the memory is untouched, exactly as on the calculator.
* **Pack two values into one memory as a complex number**, in CMPLX mode. A
  complex value is two reals, so one memory can hold a pair — the way to fit
  *more live values than there are memories*, which `const`, `#data` and `free`
  cannot do:

  ```c
  let p = pack(x, y);   // one memory: x + yi
  free x; free y;       // or the packing has bought nothing
  print(unpack_x(p));
  ```

  See [`examples/lib/pack.fxc`](../examples/lib/pack.fxc) and
  [`examples/packing.fxc`](../examples/packing.fxc), which keeps eight reals live
  in four memories. The trade is bytes for memories — every `pack`/`unpack` call
  is inlined, so it always makes the program *longer*. Unpacking is `rep`/`imp`
  (see the built-ins table) and is exact for values of up to 14 significant
  digits; the display shows 10, so the difference is not visible.

`fx50 regs FILE` reports the plan and the memories left over:

```console
$ fx50 regs examples/compiletime.fxc
Memory plan for examples/compiletime.fxc
  A  value → value   (reused after `free value`)

  1 of 7 memories used; free: B C D X Y M
  released with `free`: value
  1 const (no memory): scale
  2 data table(s) (no memory): config, tests
```

### `let` declares, so a name can have two lives

A `let` **introduces** a name, so a second `let` of a name that is still live is
an error rather than a silent shadow:

```c
let x = input();
let x = input();   // ERROR: `x` is already declared
```

Once the name has been released, declaring it again is the intended way to reuse
a name, and it takes a fresh binding — possibly into the very memory its previous
life released:

```c
let x = input();
free x;
let x = input();   // fine: a second, independent `x`
```

A plain assignment never re-declares, so after a `free` you must bring the name
back with `let`:

```c
free x;
x = 1;             // ERROR: `x` is not defined here
```

A declaration cannot see its own name, because the value is computed before the
declaration takes effect:

```c
let x = x + 1;     // ERROR: `x` cannot be used in its own initializer
```

### `free` under re-entrant control flow needs `unsafe_free`

A `goto` can re-enter code whose memory has since been released and given to
another variable, and a loop body does the same on each iteration. For example,
`while (c) { print(x); free x; let y = 1; }` gives `x` and `y` the same memory,
so the second pass prints `y` as `x`. The transpiler walks a loop body once, so
it cannot verify the allocation of a program that both loops and frees either.

A checked `free` inside a loop body, and a checked `free` in a program
containing `goto`/`label`, is therefore an error. `unsafe_free` is how you say
you have checked it yourself — the same convention Rust uses for unchecked
operations:

```c
unsafe_free x;   // no control-flow check; every other check still applies
```

Like Rust's `unsafe`, it waives one guarantee, not all checking: `unsafe_free`
still rejects double frees, unknown names and `const`s. Programs with jumps or
loops and no `free` at all are unaffected, since nothing is ever re-used. A
`free` that is outside every loop is fine even when the program loops, because
no back-edge re-enters the released region.

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
| `free` inside a loop | `while (c) { free a; }` | `` use `unsafe_free` `` |
| Freeing what has no memory | `const k = 1; free k;` | `` `k` is a `const`, which uses no memory `` |
| Freeing an unknown name | `free nope;` | `` `nope` is not a variable `` |
| Running out | an eighth live variable | `` no free memory for `z`: … `` |

### Translation rules

| `.fxc`                             | PRGM (glyph / ASCII)                                   |
| ---------------------------------- | ------------------------------------------------------ |
| `let x = input();` / `x = input();`| `?→X` / `?->X`                                         |
| `x = e;`                           | `<e>→X` / `<e>->X`                                     |
| `const k = 12;`                    | *(nothing; `k` is replaced by `12` at each use)*       |
| `config.n` / `xs[0]`               | the number it resolves to                              |
| `free x;` / `unsafe_free x;`       | *(nothing; the allocator releases the memory)*         |
| `print(e);`                        | `<e>◢` / `<e>disp`                                     |
| `e;`                               | `<e>`                                                  |
| `+ - * /`                          | `+ - × ÷` / `+ - * /`                                  |
| `^` / `**`                         | `<base>^(<exp>)`                                       |
| `== != < <= > >=`                  | `= ≠ < ≤ > ≥` / `= <> < <= > >=`                       |
| `if (c) {..} else {..}`            | `If <c>` `Then` `..` `Else` `..` `IfEnd`               |
| `while (c) {..}`                   | `While <c>` `..` `WhileEnd`                            |
| `for (i = a; i < b; i = i + s)`    | `For <a>→<v> To <b>-1 Step <s>` `..` `Next`           |
| `break;` / `goto N;` / `label N;`  | `Break` / `Goto N` / `Lbl N`                           |
| `c => s;`                         | `<c>⇒<s>` / `<c>=><s>`                                 |
| `pi` / `e`                         | `π` / `e` (ASCII: `pi` / `e`)                          |
| `root(n, x)` / `polar(r, θ)`       | `nx√(x)` / `r∠θ`                                       |
| `inv(x)` / `sqr(x)` / `fact(x)`    | `x⁻¹` / `x²` / `x!` (ASCII: `x^-1` / `x^2`)            |
| `frac(a, b)` / `ncr(n, r)`         | `a┘b` / `nnCrr`                                        |
| `stat.NAME`                        | the statistical value (`Σx`, `x̄`, …)                   |
| `dms(deg, min, sec)`               | `deg°min′sec″`                                         |
| `dms();` / `re_im();`              | `°′″` / `Re⇔Im`                                        |
| `reg_quad();`                      | `Quad`                                                 |
| `0x1F` / `0b1010` / `0o17`         | `1Fh` / `1010b` / `17o`                                |
| `a and b`                          | `a and b` (BASE)                                       |
| `deg();` / `dt(x, y);`             | `Deg` / `x,y DT`                                       |
| `mplus(x);` / `clrmemory();`       | `x M+` / `ClrMemory`                                   |

`Then` is always emitted after every `If`. A `for` whose header does not match
the canonical shape (for example a `!=` bound, or an update that mutates a
different variable) falls back to an equivalent `While` loop. A literal bound is
folded, so `i < 5` emits `To 4` rather than `To 5-1`:

```c
fn main() {
    for (let i = 0; i != 5; i = i + 2) { print(i); }
}
```

```
0→A
While A≠5
A◢
A+2→A
WhileEnd
```

## Constant folding

The machine has 680 bytes of program storage shared by all four program areas,
and every operator in the emitted PRGM costs bytes. An expression built only
from numbers therefore has its value computed while transpiling and is replaced
by that value:

```c
const k = 6 * 7;         // -> k is 42
fn main() {
    print(2 * 3 + 4);    // -> 10◢, not 2×3+4◢
    print(k + 1);        // -> 43◢
}
```

The `for` limit is folded too, so `for (i = 0; i < 5; …)` emits `To 4`, not
`To 5-1`.

**Symbolic values are never folded.** `pi`, `e` and the `phys.` constants stay
symbolic, because the calculator keys them in as their own symbols: `2 * pi`
emits `2×π`, preserving both the precision and the cheaper spelling (ADR 0015).

**Nothing the machine would round is folded.** The machine keeps 15 significant
digits and auto-corrects after every operation, while a literal it reads from a
program is not corrected. So `1 / 3` and `0.1 + 0.2` are left as written — the
machine's `0.333333333333333` and `0.3` are not the values a naive fold would
produce. Only arithmetic whose result the machine would leave unchanged is
folded.

## Optimisation

The output is optimised by default, because the machine has only **680 bytes of
program storage shared by all four program areas** and stores one byte per key.
You do not have to hand-optimise constant expressions — write the program clearly
and let the transpiler do it:

```c
let n = 3;
for (let i = 0; i < n; i = i + 1) { print(i); }
```

```text
For 0→A To 3 Step 1
A◢
Next
```

What the passes do:

| Pass | Does | Example |
| --- | --- | --- |
| constant folding | pre-calculates arithmetic on numbers | `2 * 3 + 4` → `10` |
| constant propagation | replaces a read of a value that never changes, per binding | `i < n` → `To 3` |
| simplification | drops operators that cannot change a result | `a * 1` → `a`, `-(-a)` → `a` |
| constant conditions | decides an `if`/`while` whose condition is a constant | `if (1) {…} else {…}` → the taken branch |
| dead stores | removes a store to a name that is never read | `let unused = 1;` → nothing |

Three things are deliberately **not** done, and the reasons matter:

* **A symbolic value is never turned into a decimal.** `2 * pi` stays `2×π`: the
  calculator keys `π` in as its own key, so the decimal would be both less
  precise and *more* keys (ADR 0015).
* **A name that is assigned anywhere is never assumed constant**, even if the
  assignment is in a loop or a branch it might never reach. When in doubt the
  transpiler keeps the variable:

  ```c
  let n = 3;
  while (n < 6) { print(n); n = n + 1; }   // `n` stays a memory
  ```

* **A store whose initialiser has an effect is never removed** — `input()` shows
  a prompt, `ran()` advances the random sequence, and a call can raise an error.
  Such a store survives even if nothing reads it.

A store to a name that is genuinely never read *is* removed, since it cannot
affect what the program displays:

```c
let value = config.offsets[0] * scale;   // 20◢
print(value);
```

Two things still block that, and they are the subtle part:

* a program that reads **`ans`** — evaluating any expression updates the hidden
  result memory `ans` reads, so no store is purely local; and
* a program whose last statement is **not a display**. PRGM shows the value of
  the last *value-producing* statement it ran when a program ends without `◢`
  — a store, a bare expression or a `◢`, but not a control statement. So
  `let a = 5; while (0) { print(1); }` displays `5`, from the store, and
  removing that store would change the answer to `1`. Trimming is therefore
  skipped entirely for a program that does not end in `◢` on every path. A
  program that does end in `◢` keeps the full optimiser.

Memory is otherwise left alone: a memory's final value is observable, so a store
is not deleted merely because the name is not used again in a straight line (see
[ADR 0023](../docs/DECISIONS.md) and ADR 0027).

### Diagnostics do not depend on the optimiser

The transpiler checks the program **as you wrote it**, before optimising it, so a
mistake is reported whether or not the optimiser would have removed the code it
is about:

```c
let v = (v - v);   // error: `v` cannot be used in its own initializer
```

That program is rejected both with and without `--no-optimize`. Without the
ordering it would compile, because `simplify` folds `v - v` to `0` before the
allocator ever sees the self-reference. The same holds for a double free, a
re-declaration, a use after free, and an array index out of range — a program
that is rejected stays rejected.

### Checking the size

`fx50 size` reports what a program costs, so the budget is a measurement rather
than a guess, and it always compares against the unoptimised translation:

```console
$ fx50 size examples/functions.fxc
Program size for examples/functions.fxc
  50 key(s) in 15 statement(s), largest statement 9 keys
  fits: 50 of 680 bytes used, 630 left
  optimiser: 12 key(s) saved (62 without it, 19% smaller)
```

To see the raw translation — each construct as written, which is useful for
understanding what one construct compiles to on its own — pass `--no-optimize`.
Note that folding and unrolling still apply, since a `const`'s value has to be
folded for the emitter and an array index has to be literal for the element to
name a memory at all.

## Compile-time data

`#data` reads a JSON value — inline or from a file — while transpiling, so the
values become literals and use none of the seven memories:

```c
#data config = { "base": 2, "offsets": [10, 20, 30] };
const scale = config.base;

fn main() {
    let total = config.offsets[1] * scale;
    print(total);
}
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
anything else is a transpile error naming the path that failed. A bare data name
(`print(config)`) works only when the whole table is a single number or boolean.
Data names share the namespace with variables and `const`s, so they must be
unique, and `#data` may appear anywhere.

The JSON parser is the crate's own (`fx_transpiler::json`), so `fx-transpiler`
has no dependencies; it is strict, and rejects duplicate object keys, comments,
trailing commas and lone surrogates rather than guessing.

`#tests` is exactly `#data tests = ...`, which is how the test runner below is
an ordinary consumer of this facility rather than a special case.

## Sharing code with `#include`

A program can pull in a **library** — another file of `fn` definitions — at
transpile time:

```c
// main.fxc
#include "lib/squares.fxc"

fn main() {
    let n = input();
    print(square_plus_one(n));
}
```

```c
// lib/squares.fxc
#include "increment.fxc"

fn square(n) = n * n;
fn square_plus_one(n) = increment(square(n));
```

```c
// lib/increment.fxc
fn increment(x) = x + 1;
```

`fx50 build main.fxc` inlines both libraries before compiling, so the calculator
only ever sees one flat program:

```text
?→A
A×A+1◢
```

This is the `.fxc` analogue of C's `#include` or Rust's `include_str!`. It is
useful for sharing a computation between several calculator programs.

Rules:

* The directive is `#include "path"` and must be the first thing on its line
  (leading whitespace is fine, and a trailing `// comment` is allowed).
* **It is a top-level directive.** A library contributes `fn` definitions — and
  optionally compile-time `const`/`#data` values — to be *called*. It does not
  contribute statements to be spliced into a body: an `#include` inside a `fn`
  body is an error.
* The path is resolved **relative to the file containing the directive**, so a
  library can include its own neighbours without knowing who included it.
* Includes nest, and a cycle is reported with the chain rather than looping.
* Expansion is textual and unguarded, exactly like C: including a file twice
  includes its text twice.
* `#mode` may only appear in the root file, since the mode applies to the whole
  program. A library that declares one is an error.

The include itself never reaches the calculator — only the expanded program
does. Diagnostics are reported against the file and line that actually caused
them, even when the error is inside a library:

```console
$ fx50 build main.fxc
fx50: unknown function `nope` (lib/squares.fxc:2:11)
```

### A library is a file in its own right

A library has **no `fn main()`**, and that is not an error. It is a valid file
that defines functions, so it can be built, checked and opened in an editor on
its own:

```console
$ fx50 build lib/squares.fxc     # succeeds; prints nothing, because there is
                                 # no `main` and so no program to emit
$ fx50 run   lib/squares.fxc
fx50: nothing to run: `lib/squares.fxc` has no `fn main()`; it defines functions
      for another program to `#include`
```

Because a library cannot see its includer, its dependencies are all in its
signature. `fn square(n)` above takes `n` as a parameter rather than reading a
variable named `n` from whoever called it:

```c
// lib/broken.fxc
fn square() = n * n;
```

```console
$ fx50 build lib/broken.fxc
fx50: `n` is not defined in `square`; declare it with `let`/`const`, add it as a
      parameter, or make it a top-level `const` (lib/broken.fxc:1:15)
```

That error appearing when the *library* is built is the point: a mistake is
reported where it is written, not when some other program happens to include it.

That is the point. A library used to be a *fragment* of statements spliced into
whatever included it — sharing the includer's variables and memories — which
made a file's meaning depend on where it was included, let an `#include`
silently introduce or overwrite the includer's memories, and left a file that
defines nothing runnable unable to say so (ADR 0026).

## Testing a program with `#tests`

A program can carry its own test cases in a `#tests` table — the same
compile-time data facility as `#data`:

```c
// factorial.fxc
fn main() {
    let n = input();
    let result = 1;
    for (let i = 1; i <= n; i = i + 1) { result = result * i; }
    print(result);
}

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

The Rust API for running suites from your own code
(`run_suite_file`, `parse_suite`, `SuiteReport`) is documented in the
[crate README](../crates/fx-transpiler/README.md#testing-api).
