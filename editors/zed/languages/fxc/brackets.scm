; Bracket matching for the fx-50FH II C-like language.
;
; Zed paints each matching pair, so the `{ }` of a block and the `( )` of a
; call or condition are easy to pair up.

("(" @open ")" @close)
("{" @open "}" @close)
("[" @open "]" @close)
