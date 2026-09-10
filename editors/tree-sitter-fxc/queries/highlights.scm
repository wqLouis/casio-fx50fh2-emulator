; Syntax highlighting for the fx-50FH II C-like language (tree-sitter-fxc).
;
; This is the grammar-side copy, for editors that read queries from the grammar
; checkout (Neovim, Helix, ...). Zed ignores it and loads
; `editors/zed/languages/fxc/highlights.scm` instead, which is kept
; byte-identical to this file (the bodies must match; the headers differ).
;
; Only captures known to Zed are used.

(comment) @comment

; --- directives -----------------------------------------------------------
;
; `#mode`, `#data` and `#tests` are whole-line directives.  Capturing the node
; marks the directive keyword; the identifiers and JSON inside are re-captured
; below and take precedence.

(mode_directive) @keyword
(mode_name) @constant
(include_directive) @keyword
(data_directive) @keyword
(data_directive (identifier) @constant)
(tests_directive) @keyword

; --- keywords -------------------------------------------------------------

[
  "let"
  "const"
  "free"
  "if"
  "else"
  "while"
  "for"
  "break"
  "goto"
  "label"
  "print"
  "phys"
] @keyword

; --- JSON carried by `#data` / `#tests` -----------------------------------

(string) @string
(json_pair (string) @property)
(json_number) @number
(json_literal) @boolean

; --- literals -------------------------------------------------------------

(number) @number

; --- built-ins and calls --------------------------------------------------

(call_expression (identifier) @function)
(input_expression "input" @function)

; --- scientific constants -------------------------------------------------

(constant_ref (identifier) @constant)
(constant_ref (constant_symbol) @constant)

; --- variables and named constants ----------------------------------------

(identifier) @variable
((identifier) @constant
  (#match? @constant "^(pi|e)$"))

; --- labels ---------------------------------------------------------------

(goto_statement (label_number) @label)
(label_statement (label_number) @label)

; --- operators ------------------------------------------------------------

["+" "-" "*" "/" "^" "**" "==" "!=" "<" "<=" ">" ">=" "="] @operator

; --- punctuation ----------------------------------------------------------

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
[";" "," "." ":"] @punctuation.delimiter
