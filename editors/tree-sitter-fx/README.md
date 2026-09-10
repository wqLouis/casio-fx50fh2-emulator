# tree-sitter-fx

Tree-sitter grammar for the **CASIO fx-50FH II PRGM** language (`.fx`).

PRGM is a keystroke language: each key is a token, statements are separated by
`:` or a newline, and `◢` displays the value on its left. Source may be written
with the calculator's glyphs (`→`, `◢`, `⇒`, `┘`) or with the ASCII aliases the
reference implementation accepts (`->`, `disp`, `=>`, `/`, `<>`, `<=`, `>=`).

## Layout

```
grammar.js            the grammar
tree-sitter.json      tree-sitter CLI / editor metadata (name `fx`, scope `source.fx`)
package.json          npm metadata (name `fx`, scope `source.fx`)
src/parser.c          generated C parser
src/grammar.json      generated grammar description
queries/highlights.scm  syntax highlighting
queries/locals.scm      (optional) locals
```

## Building

The generated parser is checked in. To regenerate it with the tree-sitter CLI:

```bash
tree-sitter generate
```

Parse a file:

```bash
tree-sitter parse ../../examples/quadratic.fx
```

## Coverage

* Numbers: `3`, `3.14`, `1.5E3`, `.5` and base-tagged literals `1Fh`, `1010b`,
  `17o`, `42d`.
* Memories `A B C D X Y M` and `Ans`.
* Constants `π`/`pi`, `e`, `i`, plus the 40 scientific constants by ASCII name
  (`hbar`, `C0`, `eq`) and by display symbol (`ħ`, `R∞`, `μμ`).
* Prefix functions `sin cos tan sin⁻¹ … log ln √ ∛ 10^ e^ Abs Pol Rec Rnd arg
  Conjg Not Neg` (and the ASCII spellings `asin`, `sqrt`, …).
* Statistical variables `Σx Σx² Σy Σy² Σxy x̄ ȳ σx σy sx sy minX maxX minY maxY
  regA regB regR` and their ASCII aliases.
* Operators `+ - × ÷ ┘ ∠ nPr nCr = ≠ < > ≥ ≤ and or xor xnor`, postfix
  `x² x³ x⁻¹ ! %`, and `^(`, `x√(`.
* Separators `:`, `→`/`->`, `◢`/`disp`, `⇒`/`=>`, `;`, `?`, and the
  `#mode COMP|CMPLX|BASE|SD|REG` directive.
* Program keywords `Goto Lbl If Then Else IfEnd For To Step Next While WhileEnd
  Break` and the setup keys.

`extras` hold horizontal whitespace and `//` line comments.  Newlines are
explicit separator tokens, because PRGM treats a newline exactly like `:`.

`Ran#` is modelled as a value (so `Ran#◢` highlights and parses) rather than a
bare keyword.
