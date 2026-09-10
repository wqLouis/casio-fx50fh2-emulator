; Locals for the fx-50FH II PRGM language.
;
; PRGM has no block scoping: the seven memories (A B C D X Y M) and Ans are
; global for the whole program, so there is a single scope.  The left side of
; an assignment (`expr → memory`) introduces the definition.

(source_file) @local.scope

(assignment . (expression) @local.definition)
