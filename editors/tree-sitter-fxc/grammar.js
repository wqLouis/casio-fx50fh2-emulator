/**
 * Tree-sitter grammar for the CASIO fx-50FH II C-like language (`.fxc`).
 *
 * `.fxc` is a small C-like language that transpiles to PRGM.  The grammar
 * follows `docs/AI-AGENTS.md`: `//` and block comments, `#mode`/`#include`
 * directives, `let`/assignment/`print` statements, `if`/`while`/`for` with
 * optional braces, `break`/`goto`/`label`, and a conventional expression
 * grammar with `^`/`**` exponentiation.  Scientific constants live under the
 * `phys.` namespace.
 */

module.exports = grammar({
  name: 'fxc',

  extras: $ => [
    /[ \t\r\n\f]+/,
    $.comment,
  ],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._top_level),

    _top_level: $ => choice(
      $.mode_directive,
      $.include_directive,
      $._statement,
    ),

    // #mode NAME (case-insensitive, `=` optional).
    mode_directive: $ => seq(
      token(prec(3, /#[mM][oO][dD][eE]/)),
      optional('='),
      $.mode_name,
    ),
    mode_name: $ => token(prec(2, /[A-Za-z][A-Za-z0-9_-]*/)),

    // #include "path/to/file.fxc"
    include_directive: $ => seq(
      token(prec(3, /#[iI][nN][cC][lL][uU][dD][eE]/)),
      $.string,
    ),
    string: $ => token(seq('"', /[^"\n]*/, '"')),

    comment: $ => token(choice(
      seq('//', /[^\n]*/),
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
    )),

    _statement: $ => choice(
      $.let_statement,
      $.assignment_statement,
      $.print_statement,
      $.if_statement,
      $.while_statement,
      $.for_statement,
      $.break_statement,
      $.goto_statement,
      $.label_statement,
      $.block,
      $.empty_statement,
      $.expression_statement,
    ),

    let_statement: $ => seq('let', $.identifier, '=', $.expression, ';'),
    assignment_statement: $ => seq($.identifier, '=', $.expression, ';'),
    print_statement: $ => seq('print', $.expression, ';'),

    if_statement: $ => prec.right(seq(
      'if', '(', $.expression, ')',
      $._body,
      optional(seq('else', $._body)),
    )),
    while_statement: $ => seq('while', '(', $.expression, ')', $._body),
    for_statement: $ => seq(
      'for', '(',
      $.for_init, ';', $.expression, ';', $.for_update,
      ')', $._body,
    ),
    for_init: $ => seq(optional('let'), $.identifier, '=', $.expression),
    for_update: $ => seq($.identifier, '=', $.expression),

    _body: $ => $._statement,

    break_statement: $ => seq('break', ';'),
    goto_statement: $ => seq('goto', $.label_number, ';'),
    label_statement: $ => seq('label', $.label_number, ';'),
    label_number: $ => token(/[0-9]/),

    block: $ => seq('{', repeat($._statement), '}'),
    empty_statement: $ => ';',
    expression_statement: $ => seq($.expression, ';'),

    // -- expressions -------------------------------------------------------

    expression: $ => choice(
      $.binary_expression,
      $.unary_expression,
      $.call_expression,
      $.input_expression,
      $.constant_ref,
      $.parenthesized_expression,
      $.number,
      $.identifier,
    ),

    binary_expression: $ => choice(
      prec.left(1, seq($.expression, choice('==', '!='), $.expression)),
      prec.left(2, seq($.expression, choice('<', '<=', '>', '>='), $.expression)),
      prec.left(3, seq($.expression, choice('+', '-'), $.expression)),
      prec.left(4, seq($.expression, choice('*', '/'), $.expression)),
      prec.right(5, seq($.expression, choice('^', '**'), $.expression)),
    ),

    unary_expression: $ => prec(6, seq('-', $.expression)),
    parenthesized_expression: $ => seq('(', $.expression, ')'),

    call_expression: $ => prec(7, seq(
      $.identifier, '(', optional($.arguments), ')',
    )),
    arguments: $ => seq($.expression, repeat(seq(',', $.expression))),

    // `input()` is the calculator's `?` prompt.
    input_expression: $ => seq('input', '(', ')'),

    // `phys.NAME` reaches one of the 40 scientific constants, by ASCII name
    // or by the symbol the display shows.
    constant_ref: $ => seq('phys', '.', choice($.identifier, $.constant_symbol)),
    constant_symbol: $ => choice(
      'mμ', 'μN', 'μB', 'ħ', 'α', 'λc', 'γp', 'λcp', 'λcn', 'R∞',
      'μp', 'μe', 'μn', 'μμ', 'σ', 'ε0', 'μ0', 'φ0',
    ),

    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,

    number: $ => token(choice(
      /[0-9]+(\.[0-9]*)?([eE][+-]?[0-9]+)?/,
      /\.[0-9]+([eE][+-]?[0-9]+)?/,
    )),
  },
});
