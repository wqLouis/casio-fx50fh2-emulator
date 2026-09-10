# tree-sitter-fxc

Tree-sitter grammar for the **CASIO fx-50FH II C-like** language (`.fxc`).

`.fxc` is a small C-like language that transpiles to PRGM. It has `//` and
`/* */` comments, `#mode`/`#include` directives, `let`/assignment/`print`
statements, `if`/`while`/`for` with optional braces, `break`/`goto`/`label`,
and arithmetic/comparison expressions with `^`/`**` exponentiation. Scientific
constants are reached through the `phys.` namespace.

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
* Directives `#mode NAME` (case-insensitive, optional `=`) and
  `#include "path"` (strings appear only here).
* Statements: `let NAME = expr;`, `NAME = expr;`, `print(expr);`,
  `print expr;`, expression statements, `if (e) … else …` (braces optional),
  `while (e) …`, `for (let i = 0; i < 5; i = i + 1) …`, `break;`, `goto N;`,
  `label N;`, blocks and stray `;`.
* Expressions: numbers (`1`, `1.5`, `.5`, `1e10`, `2.5E-2`, `1e-9`),
  identifiers, `input()`, calls `name(args…)`, `phys.NAME`, unary `-`, binary
  `+ - * / ^ ** == != < <= > >=`, and parentheses.
* Keywords `let if else while for break goto label print`; built-ins `sqrt
  cbrt abs sin cos tan asin acos atan sinh cosh tanh asinh acosh atanh log ln
  rnd`; `input`; `pi`/`e`; `phys` and the 40 constant names/symbols after the
  dot.

Because `.fxc` has no user-defined functions, built-ins are ordinary
identifiers in the tree; the highlights query marks any called name as a
function.
