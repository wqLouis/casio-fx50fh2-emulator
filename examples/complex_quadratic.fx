// Quadratic formula with complex roots.
// Try: 1 0 1  →  x = ±i
//
// Complex results require CMPLX mode, exactly as on the calculator.
#mode CMPLX
?→A: ?→B: ?→C:
B²-4AC→D:
(-B+√(D))┘(2A)◢
(-B-√(D))┘(2A)◢
