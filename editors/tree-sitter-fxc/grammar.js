/**
 * Tree-sitter grammar for the CASIO fx-50FH II C-like language (`.fxc`).
 *
 * `.fxc` is a small C-like language that transpiles to PRGM.  The grammar
 * follows `docs/AI-AGENTS.md`: `//` and block comments, `#mode`/`#include`
 * directives, `#data`/`#tests` compile-time JSON, `let` (scalar and array),
 * `const`, `free` (and `unsafe_free`), assignment and `print` statements,
 * `fn` definitions (expression and block form), `return`, `if`/`while`/`for`
 * with optional braces, `break`/`goto`/`label`, the `=>` conditional jump,
 * data paths with `.field`/`[index]`, base-tagged literals, and a
 * conventional expression grammar with `^`/`**` exponentiation and the
 * `and`/`or`/`xor`/`xnor` words.  Scientific constants live under the `phys.`
 * namespace and statistical variables under `stat.`.
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

    // A program is a run of top-level items.  `fn` definitions and `const`
    // declarations sit here; the directives may also appear inside a function
    // body, because an `#include`d fragment of statements is spliced in place.
    _top_level: $ => choice(
      $.mode_directive,
      $._statement,
    ),

    // The directives that may appear wherever a statement may: `#include`
    // splices a fragment in place, and `#data`/`#tests` are compile-time
    // tables.  `#mode` is deliberately *not* here -- it configures the whole
    // program, and the transpiler requires it first.
    _directive: $ => choice(
      $.include_directive,
      $.data_directive,
      $.tests_directive,
    ),

    // #mode NAME (case-insensitive, `=` optional).
    mode_directive: $ => seq(
      token(prec(3, /#[mM][oO][dD][eE]/)),
      optional('='),
      $.mode_name,
    ),
    // The mode name is only reachable here, directly after `#mode`, so it can
    // never be lexed as a general expression; a bare `main` stays an ordinary
    // identifier.
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
      $.array_declaration,
      $.const_statement,
      $.free_statement,
      $.assignment_statement,
      $.print_statement,
      $.if_statement,
      $.while_statement,
      $.for_statement,
      $.fn_statement,
      $.return_statement,
      $.break_statement,
      $.goto_statement,
      $.label_statement,
      $.conditional_statement,
      $.block,
      $.empty_statement,
      $.expression_statement,
      $._directive,
    ),

    let_statement: $ => seq('let', $.identifier, '=', $.expression, ';'),
    // `let name[size];`, `let name[size] = {e0, e1, …};` and the size-inferred
    // form `let name[] = {…};`.  The size is optional in the grammar, but the
    // transpiler rejects `let name[];` because there is nothing to infer from.
    array_declaration: $ => seq(
      'let', $.identifier, '[', optional($.number), ']',
      optional(seq(
        '=',
        '{', $.expression, repeat(seq(',', $.expression)), '}',
      )),
      ';',
    ),
    const_statement: $ => seq('const', $.identifier, '=', $.expression, ';'),
    // `free name;` releases the variable's memory for a later variable to use.
    // `unsafe_free name;` does the same without the jump safety check.
    free_statement: $ => seq(choice('free', 'unsafe_free'), $.identifier, ';'),
    // The target is a variable or an array element (`v[0] = 7;`).  Reusing
    // `data_reference` for the element form keeps `a[0]` identical on either
    // side of the `=`, so there is no ambiguity with `expression_statement`.
    assignment_statement: $ => seq(
      choice($.identifier, $.data_reference),
      '=',
      $.expression,
      ';',
    ),
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

    // `fn name(a, b) = expr;` and `fn name(a, b) { ... }`.  A function is
    // inlined at each call, so there is no callable value; the grammar only
    // describes the two surface forms.  A stray `;` after a block body is an
    // ordinary empty statement, which the parser discards.
    fn_statement: $ => seq(
      'fn', $.identifier, '(', optional($.parameters), ')',
      choice(
        seq('=', $.expression, ';'),
        $.block,
      ),
    ),
    parameters: $ => seq($.parameter, repeat(seq(',', $.parameter))),
    // `v` is a value parameter; `v[2]` is an array parameter, which names the
    // caller's array rather than copying it.  The size is written down because
    // it is the extent the body may index.
    parameter: $ => seq($.identifier, optional(seq('[', $.number, ']'))),

    // `return;` or `return expr;`.  The transpiler requires it to be the last
    // statement of a block function; that is a semantic check, not a
    // syntactic one.
    return_statement: $ => seq('return', optional($.expression), ';'),

    // `cond => stmt;` is the calculator's `=>` conditional jump.  It guards a
    // single statement (an `if` is how larger bodies are guarded).
    conditional_statement: $ => prec.right(seq($.expression, '=>', $._statement)),

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
      $.stat_ref,
      $.data_reference,
      $.parenthesized_expression,
      $.number,
      $.base_number,
      $.identifier,
    ),

    // `config.size`, `weights[0]`, and chains of those.  At least one accessor
    // is required, so a bare name is still an ordinary identifier.
    data_reference: $ => prec(8, seq($.identifier, repeat1($._accessor))),
    _accessor: $ => choice(
      seq('.', $.identifier),
      seq('[', $.expression, ']'),
    ),

    // Binding strength, loosest first: `or`/`xor`/`xnor`, `and`, equality,
    // comparison, `+ -`, `* /`, `^ **`.  The base-n words are reserved -- the
    // transpiler's lexer keywords them in every mode -- so `or` is an operator
    // here, never an identifier.
    binary_expression: $ => choice(
      prec.left(-2, seq($.expression, choice('or', 'xor', 'xnor'), $.expression)),
      prec.left(-1, seq($.expression, 'and', $.expression)),
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

    // `stat.NAME` reaches a statistical variable (`stat.n`, `stat.meanx`,
    // `stat.regA`); the transpiler validates the name against the machine's
    // table.
    stat_ref: $ => seq('stat', '.', $.identifier),
    constant_symbol: $ => choice(
      'mμ', 'μN', 'μB', 'ħ', 'α', 'λc', 'γp', 'λcp', 'λcn', 'R∞',
      'μp', 'μe', 'μn', 'μμ', 'σ', 'ε0', 'μ0', 'φ0',
    ),

    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,

    // Base-tagged integer literals, available in BASE mode: `0x1F` (hex),
    // `0b1010` (binary), `0o17` (octal).  Each alternative requires a digit,
    // so a bare `0x` still lexes as the number `0` followed by the name `x`.
    base_number: $ => token(choice(
      /0[xX][0-9a-fA-F]+/,
      /0[bB][01]+/,
      /0[oO][0-7]+/,
    )),

    number: $ => token(choice(
      /[0-9]+(\.[0-9]*)?([eE][+-]?[0-9]+)?/,
      /\.[0-9]+([eE][+-]?[0-9]+)?/,
    )),
  },
});
