; Syntax highlighting for the fx-50FH II PRGM language (tree-sitter-fx).
;
; Only captures known to Zed are used here.

; --- comments -------------------------------------------------------------

(comment) @comment

; --- directives -----------------------------------------------------------

(mode_directive) @keyword
(mode_name) @constant

; --- literals -------------------------------------------------------------

(number) @number

; --- values ---------------------------------------------------------------

(variable) @variable
(stat_variable) @variable
(constant) @constant
(scientific_constant) @constant
(random) @constant

; --- functions ------------------------------------------------------------

(function) @function

; --- keywords -------------------------------------------------------------

(keyword_statement) @keyword

; --- labels ---------------------------------------------------------------

(goto_statement (number) @label)
(label_statement (number) @label)

; --- program punctuation and operators ------------------------------------

(input_statement "?" @operator)

(assignment "→" @operator)
(assignment "->" @operator)

(display_statement "◢" @operator)
(display_statement "disp" @operator)

(conditional_statement "⇒" @operator)
(conditional_statement "=>" @operator)

(binary_operator) @operator
(postfix_operator) @operator
(complex_format) @operator

["(" ")"] @punctuation.bracket
["," ":" ";"] @punctuation.delimiter
