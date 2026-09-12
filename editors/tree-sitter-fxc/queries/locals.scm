; Locals for the CASIO fx-50FH II C-like language.
;
; `.fxc` names map to the seven PRGM memories and have no runtime block
; scoping, but blocks are useful scopes for outline/navigation purposes.

[
  (block)
  (for_statement)
  (while_statement)
  (if_statement)
] @local.scope

(let_statement (identifier) @local.definition)
(array_declaration (identifier) @local.definition)
(parameter (identifier) @local.definition)
(for_init (identifier) @local.definition)

(identifier) @local.reference
