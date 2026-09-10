; Auto-indentation for the fx-50FH II C-like language.
;
; Indenting the `block` node covers every construct that uses braces
; (`if`/`else`, `while`, `for`, and bare `{ }`). A single-statement body
; written without braces stays on its own line, so it needs no rule.

(block) @indent
