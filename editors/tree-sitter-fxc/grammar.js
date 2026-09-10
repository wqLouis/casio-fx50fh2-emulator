/**
 * Tree-sitter grammar for the CASIO fx-50FH II C-like language (`.fxc`).
 *
 * `.fxc` is a small C-like language that transpiles to PRGM.  The grammar
 * follows `docs/AI-AGENTS.md`: `//` and block comments, `#mode`/`#include`
 * directives, `#data`/`#tests` compile-time JSON, `let`/`const`/`free`
 * (and `unsafe_free`), assignment and `print` statements, `if`/`while`/`for` with optional braces,
 * `break`/`goto`/`label`, data paths with `.field`/`[index]`, and a
 * conventional expression grammar with `^`/`**` exponentiation.  Scientific
 * constants live under the `phys.` namespace.
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
      $.data_directive,
      $.tests_directive,
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

    // #data NAME = <json>;  and  #tests = <json>;
    //
    // The value is real JSON and may span lines.  The transpiler resolves and
    // removes the directive before the program itself is parsed; here it only
    // needs to be recognised so the rest of the file still highlights.
    data_directive: $ => seq(
      token(prec(3, /#[dD][aA][tT][aA]/)),
      $.identifier,
      '=',
      $.json_value,
      ';',
    ),
    tests_directive: $ => seq(
      token(prec(3, /#[tT][eE][sS][tT][sS]/)),
      '=',
      $.json_value,
      ';',
    ),

    json_value: $ => choice(
      $.json_object,
      $.json_array,
      $.string,
      $.json_number,
      $.json_literal,
    ),

    // JSON numbers may be negative; the program's `number` rule must not
    // accept a sign, or `a-1` would lex as `a` followed by `-1`.
    json_number: $ => token(
      /-?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?/
    ),
    json_object: $ => seq(
      '{',
      optional(seq($.json_pair, repeat(seq(',', $.json_pair)))),
      '}',
    ),
    json_pair: $ => seq($.string, ':', $.json_value),
    json_array: $ => seq(
      '[',
      optional(seq($.json_value, repeat(seq(',', $.json_value)))),
      ']',
    ),
    json_literal: $ => choice('true', 'false', 'null'),

    string: $ => token(seq(
      '"',
      repeat(choice(/\\./, /[^"\\\n]/)),
      '"',
    )),

    comment: $ => token(choice(
      seq('//', /[^\n]*/),
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
    )),

    _statement: $ => choice(
      $.let_statement,
      $.const_statement,
      $.free_statement,
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
    const_statement: $ => seq('const', $.identifier, '=', $.expression, ';'),
    // `free name;` releases the variable's memory for a later variable to use.
    // `unsafe_free name;` does the same without the jump safety check.
    free_statement: $ => seq(choice('free', 'unsafe_free'), $.identifier, ';'),
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
      $.data_reference,
      $.parenthesized_expression,
      $.number,
      $.identifier,
    ),

    // `config.size`, `weights[0]`, and chains of those.  At least one accessor
    // is required, so a bare name is still an ordinary identifier.
    data_reference: $ => prec(8, seq($.identifier, repeat1($._accessor))),
    _accessor: $ => choice(
      seq('.', $.identifier),
      seq('[', $.number, ']'),
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
