/**
 * Tree-sitter grammar for the CASIO fx-50FH II PRGM language.
 *
 * PRGM is a keystroke language: every key press is a token, so `sin(` is a
 * single key but this grammar (like the reference lexer in `src/lexer.rs`)
 * splits the name from the parenthesis.  Statements are separated by `:` or
 * by a newline, and a trailing `◢` displays the value on its left.
 *
 * The grammar is intentionally permissive and token oriented: an expression is
 * a flat run of value/operator tokens.  This mirrors the calculator, where
 * implicit multiplication (`4AC`, `2A`) is just juxtaposition, and it keeps
 * every shipped example free of ERROR/MISSING nodes.
 */

module.exports = grammar({
  name: 'fx',

  extras: $ => [
    /[ \t\r]+/,
    $.comment,
  ],

  rules: {
    source_file: $ => repeat($._statement),

    _statement: $ => choice(
      $.mode_directive,
      $.input_statement,
      $.display_statement,
      $.assignment,
      $.conditional_statement,
      $.goto_statement,
      $.label_statement,
      $.keyword_statement,
      $.expression_statement,
      $._separator,
    ),

    // Statements allowed on the right-hand side of `⇒`.
    statement: $ => choice(
      $.input_statement,
      $.display_statement,
      $.assignment,
      $.conditional_statement,
      $.goto_statement,
      $.label_statement,
      $.keyword_statement,
      $.expression_statement,
    ),

    // Newlines are the primary statement separator, exactly like `:` on the
    // calculator, so they are tokens rather than whitespace.
    _separator: $ => choice(':', ';', $._newline),

    _newline: $ => token(/\r?\n/),

    // #mode COMP | CMPLX | BASE | SD | REG (case-insensitive, `=` optional).
    mode_directive: $ => seq(
      token(prec(3, /#[mM][oO][dD][eE]/)),
      optional('='),
      $.mode_name,
    ),
    mode_name: $ => token(prec(2, /[A-Za-z][A-Za-z0-9_-]*/)),

    comment: $ => token(seq('//', /[^\n]*/)),

    input_statement: $ => seq('?', optional(seq($._assign_arrow, $.expression))),
    display_statement: $ => seq($.expression, $._display),
    assignment: $ => prec.right(seq($.expression, $._assign_arrow, $.expression)),
    conditional_statement: $ => prec.right(seq($.expression, $._condjump, $.statement)),

    _assign_arrow: $ => choice('→', '->'),
    _display: $ => choice('◢', 'disp'),
    _condjump: $ => choice('⇒', '=>'),

    goto_statement: $ => seq($._goto, $.number),
    label_statement: $ => seq($._lbl, $.number),
    _goto: $ => choice('Goto', 'goto'),
    _lbl: $ => choice('Lbl', 'lbl'),

    keyword_statement: $ => choice(
      'If', 'Then', 'Else', 'IfEnd',
      'For', 'To', 'Step', 'Next',
      'While', 'WhileEnd', 'Break',
      'ClrMemory', 'ClrStat', 'FreqOn', 'FreqOff',
      'Deg', 'Rad', 'Gra', 'Fix', 'Sci', 'Norm',
      'Dec', 'Hex', 'Bin', 'Oct',
      'DT', 'M+', 'M-',
    ),

    expression_statement: $ => $.expression,

    // A flat run of calculator tokens.  Implicit multiplication is just
    // adjacency, so `4AC` is four tokens inside one expression.  The rule is
    // left-recursive so that operators and their neighbours stay grouped.
    expression: $ => choice(
      $._expression_item,
      prec.left(1, seq($.expression, $._expression_item)),
    ),

    _expression_item: $ => choice(
      $.number,
      $.variable,
      $.constant,
      $.scientific_constant,
      $.stat_variable,
      $.function,
      $.random,
      $.binary_operator,
      $.postfix_operator,
      $.complex_format,
      '^',
      '(',
      ')',
      ',',
    ),

    number: $ => choice($.base_number, $.decimal_number),

    decimal_number: $ => token(prec(1, choice(
      /[0-9]+(\.[0-9]*)?([eE][+-]?[0-9]+)?/,
      /\.[0-9]+([eE][+-]?[0-9]+)?/,
    ))),

    base_number: $ => token(prec(2, choice(
      /[0-9A-Fa-f]+[hH]/,
      /[01]+b/,
      /[0-7]+o/,
      /[0-9]+d/,
    ))),

    variable: $ => choice('A', 'B', 'C', 'D', 'X', 'Y', 'M', 'Ans'),

    constant: $ => choice('π', 'pi', 'e', 'i'),

    // The calculator's 40 scientific constants, by ASCII name and by the
    // symbol the display shows.  `eq` is the elementary charge (its display
    // symbol `e` belongs to Euler's number).
    scientific_constant: $ => choice(
      'mp', 'mn', 'me', 'mmu', 'a0', 'h', 'muN', 'muB', 'hbar', 'alpha',
      're', 'lc', 'gp', 'lcp', 'lcn', 'Rinf', 'u', 'mup', 'mue', 'mun',
      'mumu', 'F', 'eq', 'NA', 'k', 'Vm', 'R', 'C0', 'C1', 'C2',
      'sigma', 'eps0', 'mu0', 'phi0', 'g', 'G0', 'Z0', 'tK', 'G', 'atm',
      'mμ', 'μN', 'μB', 'ħ', 'α', 'λc', 'γp', 'λcp', 'λcn', 'R∞',
      'μp', 'μe', 'μn', 'μμ', 'σ', 'ε0', 'μ0', 'φ0', 't',
    ),

    stat_variable: $ => choice(
      'Σx', 'Σx²', 'Σy', 'Σy²', 'Σxy', 'x̄', 'ȳ', 'σx', 'σy',
      'sx', 'sy', 'minX', 'maxX', 'minY', 'maxY', 'regA', 'regB', 'regR',
      'n', 'sumx', 'sumx2', 'sumy', 'sumy2', 'sumxy', 'meanx', 'meany',
      'sigmax', 'sigmay', 'minx', 'maxx', 'miny', 'maxy',
      'rega', 'regb', 'regr',
    ),

    function: $ => choice(
      // Inverse hyperbolic / trigonometric keys, longest first.
      'sinh⁻¹', 'cosh⁻¹', 'tanh⁻¹', 'sin⁻¹', 'cos⁻¹', 'tan⁻¹',
      'sinh^-1', 'cosh^-1', 'tanh^-1', 'sin^-1', 'cos^-1', 'tan^-1',
      'sinh', 'cosh', 'tanh', 'sin', 'cos', 'tan',
      'asin', 'acos', 'atan', 'asinh', 'acosh', 'atanh',
      'log', 'ln', 'sqrt', 'cbrt', 'Abs', 'Pol', 'Rec', 'Rnd',
      'arg', 'Conjg', 'Not', 'Neg',
      '√', '∛', 'x√', '10^', 'e^',
    ),

    binary_operator: $ => choice(
      '+', '-', '*', '/', '×', '÷', '┘', '∠',
      'nPr', 'nCr',
      '=', '==', '≠', '<>', '>', '<', '≥', '<=', '≤', '>=', 'div',
      'and', 'or', 'xor', 'xnor',
    ),

    postfix_operator: $ => choice(
      '²', '³', '⁻¹', '!', '%', '^2', '^3', '^-1',
    ),

    // `Ran#` is a value (a random number), so it may be displayed with `◢`.
    random: $ => 'Ran#',

    // `▶a+b𝑖`, `▶r∠θ` and their ASCII spellings.
    complex_format: $ => choice(
      '▶a+b𝑖', '▶a+b𝒾', '▶a+bi', '▶r∠θ', '>a+bi', '>rangle',
    ),
  },
});
