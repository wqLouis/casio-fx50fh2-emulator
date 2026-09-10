# tree-sitter-fxc

Tree-sitter grammar for the **CASIO fx-50FH II C-like** language (`.fxc`).

`.fxc` is a small C-like language that transpiles to PRGM. It has `//` and
`/* */` comments, the directives `#mode`, `#include` and `#data`/`#tests`, the
statements `let`/`const`/assignment/`free`/`unsafe_free`/`print`, `if`/`while`/
`for` with optional braces, `break`/`goto`/`label`, data paths (`config.size`,
`weights[0]`), and arithmetic/comparison expressions with `^`/`**`
exponentiation. Scientific constants are reached through the `phys.` namespace.

The language itself is documented in [`docs/FXC.md`](../../docs/FXC.md); this
README covers only the grammar.

## Layout

```
grammar.js            the grammar
tree-sitter.json      tree-sitter CLI / editor metadata (name `fxc`, scope `source.fxc`)
package.json          npm metadata (name `fxc`, scope `source.fxc`)
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
tree-sitter parse ../../examples/factorial.fxc
```

## Coverage

* `//` line comments and `/* … */` block comments.
* Directives: `#mode NAME` (case-insensitive, optional `=`),
  `#include "path"`, and the compile-time data tables `#data NAME = <json>;`
  and `#tests = <json>;` (the JSON is parsed by the grammar so the rest of the
  file still highlights).
* Statements: `let NAME = expr;`, `const NAME = expr;`, `NAME = expr;`,
  `free NAME;`, `unsafe_free NAME;`, `print(expr);`, `print expr;`, expression
  statements, `if (e) … else …` (braces optional), `while (e) …`,
  `for (let i = 0; i < 5; i = i + 1) …`, `break;`, `goto N;`, `label N;`,
  blocks and stray `;`.
* Expressions: numbers (`1`, `1.5`, `.5`, `1e10`, `2.5E-2`, `1e-9`),
  identifiers, `input()`, calls `name(args…)`, `phys.NAME`, data references
  with `.field`/`[index]`, unary `-`, binary `+ - * / ^ ** == != < <= > >=`,
  and parentheses.
* Keywords `let const free unsafe_free if else while for break goto label
  print`; built-ins `sqrt cbrt abs sin cos tan asin acos atan sinh cosh tanh
  asinh acosh atanh log ln rnd`; `input`; `pi`/`e`; `phys` and the 40 constant
  names/symbols after the dot.

Because `.fxc` has no user-defined functions, built-ins are ordinary
identifiers in the tree; the highlights query marks any called name as a
function.
