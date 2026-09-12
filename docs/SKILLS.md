# Skill: write `.fxc` calculator programs

You are writing **`.fxc`**, a small C-like language that transpiles to CASIO
fx-50FH II **PRGM** keystrokes. The PRGM program then runs on the calculator or
on the `fx50` emulator. Everything here is checked against the implementation;
for depth read [`docs/FXC.md`](FXC.md), which this document distils.

```bash
fx50 build program.fxc          # transpile to PRGM on stdout (calculator glyphs)
fx50 build --ascii program.fxc  # ...with keyboard-typable aliases
fx50 run   program.fxc          # transpile, then execute (? reads stdin)
fx50 size  program.fxc          # keys used out of the 680-byte program store
fx50 regs  program.fxc          # which of the seven memories each name got
fx50 test  program.fxc          # run the cases in the program's `#tests` table
```

## 1. Skeleton

A program **is a set of functions**. `fn main()` is required; the top level may
contain **only** `fn` definitions and `const` declarations. A loose statement
there is an error telling you to move it into `main`.

```c
const limit = 12;

fn main() {
    let n = input();
    if (n > limit) { n = limit; }
    print(n);
}
```

`main` takes no parameters. Inside `main` a name is declared on first use
(order-free, like a script); every other function is closed (below). A file with
no `fn main()` is a **library** — it defines functions for another file to
`#include` and builds to nothing.

Every statement ends with `;`. There is no newline termination and no automatic
semicolon insertion.

## 2. Functions and call by name

Functions are **inlined at each call** while transpiling: no call instruction
reaches the calculator, and a function costs no memory of its own.

```c
fn square(x) = x * x;                 // expression function

fn bump(x) { x = x + 1; }             // procedure: assigns its parameter
```

**Every function except `main` is closed** over its own names. Its body may use
only its parameters, its own `let`/`const`/array locals, top-level
`const`/`#data` values, other functions and built-ins. It cannot read a variable
from `main` or create a global:

```c
fn f(x) = x + a;                      // ERROR: `a` is not defined in `f`
fn main() { let a = 1; print(f(2)); }
```

Fix it by adding `a` as a parameter or making it a top-level `const`.

**Arguments are passed by name**, substituted wherever the parameter is
mentioned. There is no call stack and no copy:

- A **variable argument behaves like a reference.** `bump(a)` becomes
  `a = a + 1`, so the caller's `a` changes.
- An **expression argument is re-evaluated at every mention.** `twice(ran())`
  emits `Ran#+Ran#` and draws two random numbers.
- A parameter the function **assigns needs an assignable argument** — a variable
  or an array element, not an expression. `bump(1 + 2)` is an error.
- A parameter mentioned `n` times is emitted `n` times, which costs program
  bytes. Mention it once.

Locals are hygienic: the transpiler renames them per call, so a local cannot
collide with a caller or another call.

To return more than one value, pass variables and assign through them — `return`
gives one value and must be the **last statement** of the body:

```c
fn minmax(a, b, lo, hi) {
    lo = a; hi = b;
    if (a > b) { lo = b; hi = a; }
}
```

## 3. Memory: seven memories, 680 bytes

PRGM has exactly **seven memories** — `A B C D X Y M` — and **680 bytes of
program storage shared by all four program areas**. Names are allocated to
`A B C D X Y M` in first-seen source order, and a name keeps its memory for the
whole program (or until it is freed), so an eighth distinct name is a
transpile-time error.

- `let` **declares** and allocates. Re-declaring a live name is an error.
- A plain `=` assigns to a name that already exists; it never declares.
- `free name;` **releases** the memory so the next new name can reuse it.
  Nothing is released implicitly. Free an array as a whole: `free v;`, never
  `free v[0];`.
- `const` and `#data` values are inlined and use **no memory**. Prefer them for
  fixed values; `const` cannot declare an array.

```c
fn main() {
    let first = 5;
    print(first);          // emits `5→A`, `A◢`
    free first;
    let second = 7;        // reuses memory A
    print(second);         // emits `7→A`, `A◢`
}
```

Naming variables `a b c d x y m` in that order makes allocation predictable.

Use `fx50 regs` to see the plan and `fx50 size` to check the 680-byte budget.
The transpiler already folds constants, propagates values that never change,
decides constant conditions and drops unread stores — write clearly and measure,
do not hand-optimise. Running out of memories names the culprits; `const`,
`#data` and `free` are the levers.

## 4. Arrays and array parameters

An array gives each element its own memory, and **the index must be a
compile-time literal** — PRGM has no indirect addressing. An element reference
is then free: `w[1]` emits just the memory letter `B`.

```c
let v[3];                 // A B C, no initialiser, emits nothing
let w[3] = {4, 8, 15};    // declare and initialise
let u[] = {1, 2, 3};      // size inferred from the list
print(w[0] + w[2]);       // read an element
w[1] = 16;                // write an element
free w;                   // release every element
```

A `for` loop with constant integer bounds that needs a computed index is
**unrolled**, so the index becomes a literal:

```c
let v[3];
for (let i = 0; i < 3; i = i + 1) { v[i] = input(); }   // emits ?→A ?→B ?→C
```

A run-time bound, or a `break`/`goto`/declaration/`free` in the body, stops the
unroll and the index becomes a compile-time error. A truly run-time index
(`v[k]`, `k` from `input()`) is always an error.

**Array parameters** name the caller's array instead of copying it. Write the
size: `v[2]`. The argument is the *name* of an array, not an expression and not
an element. It costs no memory of its own, and writes go through.

```c
fn setfirst(v[2]) { v[0] = 9; }
fn main() {
    let a[] = {1, 2};
    setfirst(a);          // writes a[0]
    print(a[0]);          // emits `9→A`, `A◢`
}
```

`f(a)` is accepted; `f(a[0])` and `f(1 + 2)` are errors. In `fn f(v[2])`, `v[2]`
is out of bounds. A bare `v` has no value — index it.

## 5. Control flow and what is rejected

`if`/`else`, `while`, `for`, `break`, single-digit `goto`/`label`, and
`cond => stmt;` (the `⇒` key). Bodies use `{ }` for more than one statement.

```c
for (let i = 0; i < 5; i = i + 1) { print(i); }
while (a > 0) { a = a - 2; }
if (x > 0) print(1); else print(2);
x > 0 => print(1);
```

Rejected at transpile time:

- **Recursion**, direct or indirect. There is no call stack.
- **`return` anywhere but the last statement** of a function body.
- **A procedure called from a `while`/`for` condition** (inlining would move its
  statements out of the loop). Use an expression function or precompute into a
  variable.
- **A parameter that is also a local** of the same function.
- **A function name that shadows a built-in** (`fn sqrt(…)`).
- `input()` anywhere except the entire right-hand side of an assignment.
- `free` inside a loop or in a program with `goto`/`label`; use `unsafe_free`
  when you have checked the control flow yourself.

Prefer structured loops to `goto`.

## 6. Built-ins you are most likely to need

Every key is a call, not an operator. `x²` is `sqr(x)`, `x!` is `fact(x)`,
`%` is `pct(x)`, `┘` is `frac(a, b)`, `nPr` is `npr(n, r)`, `∠` is
`polar(r, θ)`. Comparisons produce `1` or `0`; there is no boolean type.

| Call | Emits |
| --- | --- |
| `sqrt(x)` / `cbrt(x)` / `root(n, x)` | `√(x)` / `∛(x)` / `nx√(x)` |
| `pow10(x)` / `exp(x)` / `ln(x)` | `10^(x)` / `e^(x)` / `ln(x)` |
| `log(x)` / `log(a, b)` | base 10 / log of `b` to base `a` |
| `abs(x)` / `rnd(x)` | `Abs(x)` / `Rnd(x)` |
| `sin cos tan asin acos atan` | trig; angle unit set by `deg()`/`rad()`/`gra()` |
| `inv(x)` / `sqr(x)` / `cube(x)` | `x⁻¹` / `x²` / `x³` |
| `fact(x)` / `pct(x)` / `frac(a, b)` | `x!` / `x%` / `a┘b` |
| `npr(n, r)` / `ncr(n, r)` | nPr / nCr |
| `ran()` / `ans()` | `Ran#` / `Ans` |
| `pi` / `e` | `π` / `e` (constants, no arguments) |
| `phys.h`, `phys.C0`, `phys.NA`, … | the 40 scientific constants |

There is no `%`, `&&`, `||`, `!`, `++`, `--`, `+=`, and no integer division or
modulo. Build boolean logic by nesting `if`s. Unknown function names are a parse
error, so do not invent built-ins.

## 7. Compile-time data, tests and includes

`const` inlines a compile-time value; `#data` reads JSON while transpiling and
inlines numbers and booleans. Neither uses memory.

```c
#data config = { "base": 2, "offsets": [10, 20, 30] };
const scale = config.base;

fn main() {
    let total = config.offsets[1] * scale;   // emits `20×2→A`
    print(total);                            // `A◢`
}
```

`#include "lib/squares.fxc"` splices in a **library** at the top level (never
inside a `fn` body). A library is `.fxc` with `fn` definitions and no
`fn main()`; it is checked on its own, and an included file may not declare
`#mode` (only the program's own root file may).

`#tests` carries test cases in the program. **Write tests and run
`fx50 test` before reporting success** — it checks many cases at once and exits
non-zero on failure.

```c
#tests = [
  { "name": "5! = 120", "input": [5], "output": ["120"] },
  { "name": "no input", "input": [],  "error": "Argument ERROR" }
];
```

Each case gives exactly one of `output` (the exact `◢` display lines) or
`error` (matched case-insensitively by substring).

## 8. Modes

A program may declare an operating mode. The directive must be the **very first**
non-comment, non-blank line, and the mode's capabilities are checked at
transpile time.

```c
#mode CMPLX
fn main() { print(i() * i()); }
```

| Mode | Unlocks |
| --- | --- |
| `COMP` (default) | general real arithmetic; `pol`/`rec` |
| `CMPLX` | `i()`, `arg`, `conjg`, `rep`/`imp`, `polar`, `to_cartesian`/`to_polar`, `re_im` |
| `BASE` | base literals (`0x1F`), `and`/`or`/`xor`/`xnor`, `not`/`neg`, `hex`/`bin`/`oct`; **rejects every float built-in, `pi`, `e`, `phys.*`** |
| `SD` / `REG` | `stat.*`, `dt`, `clrstat`, `freqon`/`freqoff`; `REG` adds `stat.sumy`… and the `reg_*` models |

With no directive the output has no header and runs in COMP. A directive is
copied into the output. Use `rep(z)` / `imp(z)` for the real and imaginary
parts — this model has no `ReP`/`ImP` key.

## 9. Worked example

```c
// scale each pair:  print((v[0] + v[1]) * 2)
const scale = 2;

fn dot(v[2], k) {
    return (v[0] + v[1]) * k;    // array parameter: no memory of its own
}

fn main() {
    let v[] = {input(), input()};
    print(dot(v, scale));
}

#tests = [
  { "name": "2,3 scaled", "input": [2, 3], "output": ["10"] },
  { "name": "0,0",        "input": [0, 0], "output": ["0"] }
];
```

```console
$ fx50 build demo.fxc
?→A
?→B
(A+B)×2→C
C◢

$ fx50 test demo.fxc
demo.fxc
  ok    2,3 scaled
  ok    0,0
2 passed, 0 failed
```

## 10. Checklist

- [ ] Every statement ends with `;`.
- [ ] `fn main()` exists; the top level holds only `fn` and `const`.
- [ ] Every other function is closed: parameters, locals, top-level
      `const`/`#data`, functions, built-ins only. No globals.
- [ ] No recursion; `return` only as the last statement; no procedure in a loop
      condition; no parameter that is also a local; no shadowed built-in.
- [ ] An assigned parameter gets a variable or array element, not an expression.
- [ ] At most seven live names; `const`/`#data` for fixed values; `free` what is
      finished with.
- [ ] Every array index resolves to a literal; array parameters are written
      `v[n]` and passed an array *name*.
- [ ] `input()` is only a complete assignment right-hand side.
- [ ] Built-ins come from §6 with the right arity.
- [ ] Mode-specific keys match the declared mode.
- [ ] `#include` paths exist, sit at the top level, and declare no `#mode`.
- [ ] `fx50 test` reports `0 failed`; `fx50 size` and `fx50 regs` fit.

For the full grammar, every statement and every built-in, see
[`docs/FXC.md`](FXC.md); for the rule-oriented long form, see
[`docs/AI-AGENTS.md`](AI-AGENTS.md).
