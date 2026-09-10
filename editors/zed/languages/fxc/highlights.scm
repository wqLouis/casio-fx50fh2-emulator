; Syntax highlighting for the fx-50FH II C-like language (tree-sitter-fxc).
;
; Zed loads this from the language directory (languages/fxc/), NOT from the
; grammar repository, so this is the copy Zed actually applies. It is kept
; byte-identical to `editors/tree-sitter-fxc/queries/highlights.scm`, which
; serves editors that read queries from the grammar checkout.
;
; Only captures known to Zed are used.

; --- comments -------------------------------------------------------------

(comment) @comment

; --- directives -----------------------------------------------------------

(mode_directive) @keyword
(mode_name) @constant
(include_directive) @keyword
(include_directive (string) @string)

; --- keywords -------------------------------------------------------------

[
  "let"
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

["(" ")" "{" "}"] @punctuation.bracket
[";" "," "."] @punctuation.delimiter
