# fx-50FH II language notes

This is a working reference for the calculator's PRGM language as implemented
here. It is reconstructed from the sources below; where the machine and this
implementation differ it is noted.

## Sources

* CASIO fx-50FH II user's guide (priority sequence, command list).
* [`KeroppiMomo/calsimtor`](https://github.com/KeroppiMomo/calsimtor) — a
  detailed reverse-engineering of the interpreter, including `DOC.md` on
  numeric precision, the numeric/command stacks, `Ans` vs hidden memory, and
  the quirks of `⇒`, `Lbl`/`Goto`, `If` and `While`.
* [`sayatodev/progex`](https://github.com/sayatodev/progex) — a Crafting
  Interpreters–style implementation (Scanner/Parser/Interpreter) used as a
  model for this project's structure and for the token vocabulary.
* [`throwingogo-hub/fx-50fh-ii`](https://github.com/throwingogo-hub/fx-50fh-ii)
  — a complete machine reimplementation whose parser spells out the eleven
  priority levels.

## Tokens

The calculator is a keystroke machine: one key = one token. `sin(` is a single
token, not `sin` + `(`. This implementation tokens trees the name and the
parenthesis separately but the parser accepts an omitted opening parenthesis.

| Group | Tokens |
| --- | --- |
| Memories | `A B C D X Y M Ans` |
| Constants | `π` (`pi`), `e`, `i` |
| Prefix functions | `sin cos tan sin⁻¹ cos⁻¹ tan⁻¹ sinh cosh tanh sinh⁻¹ cosh⁻¹ tanh⁻¹ log ln √ ∛ 10^ e^ Abs Pol Rec Rnd arg Conjg Not Neg` |
| Postfix | `x² x³ x⁻¹ ! %` |
| Infix | `+ - × ÷ ┘ ∠ nPr nCr = ≠ > < ≥ ≤ and or xor xnor` |
| Parenthetical binary | `^(`, `x√(` |
| Program | `? → : ◢ ⇒ Goto Lbl If Then Else IfEnd For To Step Next While WhileEnd Break` |
| Setup | `Deg Rad Gra Fix n Sci n Norm n Dec Hex Bin Oct ▶a+b𝑖 ▶r∠θ ClrMemory ClrStat FreqOn FreqOff` |
| Statistics | `DT` and `n Σx Σx² Σy Σy² Σxy x̄ ȳ σx σy sx sy minX maxX minY maxY regA regB regR` (ASCII: `n sumx sumx2 sumy sumy2 sumxy meanx meany sigmax sigmay sx sy minx maxx miny maxy rega regb regr`) |

Only `A B C D X Y M` and `Ans` exist as variables. There is no `S`, `N`, etc.;
the lexer rejects them.

## Priority sequence

1. parenthetical functions / parentheses
2. postfix `x² x³ x⁻¹ ! %` and `^(`, `x√(`
3. fractions `┘`
4. prefix `-`
5. statistical estimated values (regression estimates live on
   [`crate::stats::Stats`] as `est_y`/`est_x`; there is no keystroke yet)
6. `nPr`, `nCr`
7. `×`, `÷` and omitted multiplication (`2π`, `4AC`, `2(3+4)`)
8. `+`, `-`
9. `=`, `≠`, `>`, `<`, `≥`, `≤`
10. `and`
11. `or`, `xor`, `xnor`

Note the consequences: `-2² = -4` (prefix `-` is looser than the power key),
and `1┘2+1┘3` groups the fractions first.

## Runtime semantics

* **Answers.** Evaluating an expression stores the result in both `Ans` and the
  "hidden" result memory. `?` stores only in the hidden memory, which is why
  `?→A` displays the input but leaves `Ans` unchanged on the real machine.
* **Display.** `◢` displays the current result. When a program ends without an
  explicit `◢`, the machine shows the last computed value.
* **`⇒`.** `expr ⇒ stmt` evaluates `expr`; if it is non-zero, `stmt` runs,
  otherwise it is skipped and execution continues after it.
* **`Goto`/`Lbl`.** Labels are a single digit `0`–`9`. `Goto n` jumps to the
  first `Lbl n`; a missing label is a `Go ERROR`.
* **Angle modes** affect `sin`/`cos`/`tan` and their inverses, `Pol`, `Rec`,
  `arg`, and the `∠` polar literal.

### Complex numbers

* `i` is the imaginary unit; `a+bi` is written with the imaginary unit as a
  suffix (`2+3i`, `-i`).  `<expr> i` is a constant, so `2i` is implicit
  multiplication.
* `r∠θ` is a polar literal.  `θ` follows the current `Deg`/`Rad`/`Gra` mode.
* `+ − × ÷`, `x²`, `x³`, `x⁻¹`, `Abs`, `arg`, `Conjg`, `√` and `=`/`≠` all work
  on complex values.  `√` of a negative real yields a purely imaginary result.
* Real-only functions (`sin`, `log`, `nPr`, `!`, `<`, `>`, `≤`, `≥`, …) given a
  complex operand raise `Math ERROR`.
* The display format is selected with `▶a+b𝑖` (ASCII `>a+bi`) or `▶r∠θ`
  (ASCII `>rangle`).  Cartesian omits a zero real part and writes a negative
  imaginary part with `-`; polar renders `r∠θ`.
* `Pol(`/`Rec(` still write real results into `X` and `Y`.

### Statistics

* `x DT` appends an SD data point; `x,y DT` appends a REG point;
  `x,y;f DT` also carries a frequency.  The `;` separator is only meaningful
  before `DT`.
* `FreqOn`/`FreqOff` turn frequency weighting on and off; `ClrStat` clears the
  data registers.
* `n`, `Σx`, `Σx²`, `Σy`, `Σy²`, `Σxy`, `x̄`, `ȳ`, `σx`, `σy`, `sx`, `sy`,
  `minX`, `maxX`, `minY`, `maxY`, `regA`, `regB`, `regR` are available as
  expressions.  `σ` is the population deviation, `s` the sample deviation.
  For linear regression `y = regA + regB·x`.
* Data is always real; a complex operand raises `Math ERROR`.  At most 40 data
  points are kept, after which `Data Full` is raised.

### Base-n

* `Dec`, `Hex`, `Bin` and `Oct` select a number base (`Environment.base`).
  While a base is selected, integer arithmetic wraps to a fixed word size
  (`Dec`/`Hex` 32 bits, `Oct` 30, `Bin` 10) and results display with a suffix
  (`d`, `h`, `b`, `o`).
* Tagged literals select their own base: `1Fh`, `1010b`, `17o`, `42d`.
* `and`, `or`, `xor` and `xnor` become bitwise operators, valid only in base
  mode; outside it they raise `Math ERROR`.  `Not(` and `Neg(` are the
  bitwise-complement and two's-complement helpers.

### Precision and autocorrection

Every arithmetic operation is normalised with `precision::normalize`, i.e.
`autocorrect(round15(x))`.  `round15` keeps 15 significant digits and
`autocorrect` implements the machine's rules:

* if the trailing four significant digits `LMNO` are `0000`–`0009`, round down
  to 11 significant figures;
* if they are `9991`–`9999`, round up to 11 significant figures;
* if the first 13 significant digits are zero, collapse to zero.

Numeric literals are *not* autocorrected, matching hardware.  `Norm1` switches
between decimal and scientific outside `[1e-2, 1e10)`; `Norm2` outside
`[1e-9, 1e10)`.  `Fix n` and `Sci n` set explicit formats.

## Known deviations / TODO

* The 40 physical constants and `ReP`/`ImP` are not implemented.
* Untagged integer literals are always read as decimal; only tagged literals
  (`1Fh`, `1010b`, …) select another base.  Hex literals must therefore start
  with a decimal digit (`1Fh`, not `FFh`).
* `Goto` clears the `If`/loop context, so jumping *out* of a loop works but
  jumping to a label *inside* the same loop does not (the real machine's
  behaviour here is itself subtle).
* Sexagesimal (`°′″`) output is not implemented.
* Statistical regression supports the linear model only; the other Casio
  models are rejected rather than approximated.
