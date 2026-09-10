// Greatest common divisor by repeated subtraction.
// Uses `⇒` for the two conditional assignments so no Goto is needed.
?→A
?→B
While B≠0
A>B⇒A-B→A
A≤B⇒B-A→B
WhileEnd
A◢
