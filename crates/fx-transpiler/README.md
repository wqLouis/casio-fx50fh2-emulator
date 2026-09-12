# fx-transpiler

`fx-transpiler` translates the `.fxc` language into CASIO fx-50FH II **PRGM**
source. It is a library (`fx_transpiler`); the user-facing commands are
subcommands of the unified `fx50` binary.

**The language itself is documented in [`docs/FXC.md`](../../docs/FXC.md)** —
grammar, statements, the seven-memory model, `free`/`unsafe_free`, user-defined
`fn`s (inlined, with a required `fn main()` entry point), compile-time data, and
`#include`. This README covers the crate: its CLI, its Rust API and its tests.

```bash
fx50 build program.fxc          # print PRGM (calculator glyphs) to stdout
fx50 build --ascii program.fxc  # print ASCII aliases instead
fx50 run   program.fxc          # transpile, then execute (? reads from stdin)
fx50 regs  program.fxc          # show how the seven memories are used
fx50 size  program.fxc          # show how many of the 680 program bytes it needs
fx50 test  program.fxc          # run the cases the program carries
```

## Dependencies

`serde_json` (for `#data` and `#tests`), `serde` and `thiserror`. There is still
**no** dependency on `casio-fx50fh2`: the interpreter sits behind the optional
`execute`/`testing` features, so

```bash
cargo build -p fx-transpiler --no-default-features
```

builds the transpiler on its own. What that configuration no longer means is
"no dependencies" — the blanket ban was retired in
[ADR 0032](../../docs/DECISIONS.md) because it had produced a hand-written JSON
parser to no purpose, and [ADR 0014](../../docs/DECISIONS.md) is what survives of
it: `#data` is a language feature rather than an add-on, and JSON is parsed
strictly and in exactly one place.

The `execute` feature (on by default) is what lets the interpreter-backed tests
in `tests/execute.rs` compile and run; it is also what the `fx50` binary uses
for `fx50 run`. `testing` (also on by default, and implied by `execute`) adds
the `#tests` runner.

## CLI

The examples below use `factorial.fxc`:

```c
fn main() {
    let n = input();
    let result = 1;
    for (let i = 1; i <= n; i = i + 1) { result = result * i; }
    print(result);
}
```

```console
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

`fx50 build` resolves `#include` relative to the file it is given, and
`fx50 regs` runs the same front end without emitting, so the two never disagree
about the memory plan.

## Optimisation

The emitted program is optimised by default. The machine has **680 bytes of
program storage shared by all four program areas** and stores one byte per key,
so a smaller program is always the goal:

| Pass | Does | Optional? |
| --- | --- | --- |
| `fold` | pre-calculates constant expressions (`2*3+4` → `10`) | no |
| `simplify` | drops operators that cannot change a result (`a*1`, `a+0`, `-(-a)`) | yes |
| `propagate` | replaces a read of a never-assigned constant with its value; decides a constant `if`/`while`; drops stores nothing reads | yes |
| `unroll` | expands a constant `for` loop that indexes an array, so the index is a literal | no |

`fold` and `unroll` are not optional: a `const`'s value has to be folded for the
emitter, and an array element can only name a memory if its index is known while
transpiling. Set `Options { optimize: false, .. }` (CLI: `--no-optimize`) to get
the raw translation, in which each construct appears as written:

```console
$ fx50 build --no-optimize c.fxc
If 1
Then
9◢
Else
8◢
IfEnd
$ fx50 build c.fxc
9◢
```

Both forms compute the same thing; `tests/optimize.rs` checks that, and
`tests/simplify.rs` / `tests/propagate.rs` test the passes themselves.

`fx50 size` measures the result, so the saving is a number rather than a claim:

```console
$ fx50 size factorial.fxc
Program size for factorial.fxc
  22 key(s) in 6 statement(s), largest statement 8 keys
  fits: 22 of 680 bytes used, 658 left
  optimiser: nothing to remove (22 keys either way)
```

## Library

```rust
use fx_transpiler::{transpile, transpile_with, Mode, Options};

// Calculator glyphs (default).
let prgm = transpile("fn main() { let a = input(); print(a * 2); }")?;
assert_eq!(prgm, "?→A\nA×2◢\n");

// ASCII aliases.
let prgm = transpile_with("fn main() { print(a <= b); }", Options { ascii: true, ..Default::default() })?;
assert_eq!(prgm, "A<=Bdisp\n");

// Force BASE mode (overrides any `#mode` header).
let prgm = transpile_with("fn main() { print(a / b); }", Options { mode: Some(Mode::Base), ..Default::default() })?;
assert_eq!(prgm, "#mode BASE\nA÷B◢\n");
# Ok::<(), fx_transpiler::error::TranspileError>(())
```

The entry points differ only in where `#include` and `#data` paths are resolved:

| Function | Paths resolved against |
| --- | --- |
| `transpile(source)` / `transpile_with(source, opts)` | the current directory |
| `transpile_with_base(source, opts, dir)` | `dir` |
| `transpile_file(path, opts)` | the file's own directory |

`Options` has three fields: `ascii` (glyph vs. ASCII spellings), `mode` (override
the `#mode` header) and `optimize` (the optional passes above, **on** by
default). `size::measure(prgm)` returns the key count `fx50 size` prints,
including `Size::CAPACITY` (680) and `Size::fits()`.

```rust
# use std::path::Path;
# use fx_transpiler::{transpile_file, transpile_with_base, Options};
let prgm = transpile_file(Path::new("examples/include.fxc"), Options::default())?;
let prgm = transpile_with_base(
    "fn main() {\nlet a = 1;\n#include \"frag.fxc\"\n}",
    Options::default(),
    Path::new("src"),
)?;
# Ok::<(), fx_transpiler::error::TranspileError>(())
```

`TranspileError` carries a message plus the byte offset, 1-based line and column
of the offending source, and names the file when the error came from an included
fragment. `analyze(source, dir)` returns the memory plan that `fx50 regs` prints,
without emitting. The pipeline stages are exposed as `lexer::lex`,
`parser::parse`, `ast`, `data`, `json` and `builtins::lookup`.

Between parsing and allocation three internal passes shape the program:
`functions.rs` resolves a `fn main()` entry point and inlines every user
function call, `fold.rs` pre-calculates constant expressions (leaving
`pi`/`e`/`phys.` symbolic, and refusing anything the machine's 15-digit
arithmetic would round), and `unroll.rs` expands a constant `for` loop whose
body indexes an array, so its indices become literals. All three are private;
`transpile`/`analyze` apply them automatically.

### Testing API

The `#tests` runner is behind the `testing` feature:

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

`TestSuite`, `TestCase`, `SuiteReport` and `CaseResult` are public. The case
schema is in [`docs/FXC.md`](../../docs/FXC.md#suite-schema).

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
`tests/compiletime.rs` covers `const`, `#data`, `free`/`unsafe_free` and the
lifetime errors. `tests/arrays.rs` covers arrays end to end, including the
constant loops that are unrolled so `v[i]` becomes a literal.
`tests/functions.rs` covers `fn`: inlining, call-by-name, hygiene, the
`fn main()` entry point, scoping, and `#include`d libraries. `tests/tokens.rs`
is the catalogue of the full PRGM key surface and asserts that every element of
the interpreter's own `FuncName::ALL`, `Postfix::ALL`, `BinOp::ALL` and
`StatVar::ALL` has an `.fxc` spelling that transpiles.

## Reference works

The language and the choices behind it are described in the project docs:

* [`docs/FXC.md`](../../docs/FXC.md) — the language manual.
* [`docs/AI-AGENTS.md`](../../docs/AI-AGENTS.md) — rule-oriented authoring guide.
* [`docs/DECISIONS.md`](../../docs/DECISIONS.md) — why JSON is hand-written,
  why `const` is inlined, why memories are never shared implicitly.
