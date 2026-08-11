module.exports = grammar({
  name: 'animatix',

  externals: $ => [
    $.number,
    $.time_literal,
  ],

  extras: $ => [
    /\s/,
    $.comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    [$._expression, $.closure_expression],
    [$.binary_expression, $.closure_expression],
    [$.tuple_expression, $.parenthesized_expression],
    [$._statement, $._expression],
    [$._expression, $.path_expression],
    [$.action_invocation, $._expression],
    [$.set_expression, $.object_expression],
    [$.argument_list, $.tuple_expression, $.parenthesized_expression],
    [$.argument_list, $.tuple_expression],
    [$._expression, $.object_expression],
    [$._expression, $.index_value],
    [$.inline_anonymous_actor],
    [$.inline_actor_declaration],
    [$.inline_actor_declaration, $._expression],
    [$.inline_actor_declaration, $._expression, $.object_expression],
    [$.object_expression, $.children_block],
    [$.inline_actor_declaration, $.modifier],
    [$.property_list],
    [$.inline_property, $.property],
    [$.block, $.set_expression],
  ],

  rules: {
    source_file: $ => repeat($._statement),

    _statement: $ => choice(
      $.config,
      $.import_statement,
      // Deprecated: use_statement is not supported by the runtime PEG parser
      // $.use_statement,
      $.let_declaration,
      $.type_alias,
      $.component_definition,
      $.action_definition,
      $.scene_declaration,
      $.keyframe,
      $.actor_declaration,
      $.text_shorthand,
      $.typst_shorthand,
      $.property_assignment,
      $.reactive_binding,
      $.action_invocation,
      $.sequence_block,
      $.stagger_block,
      $.always_block,
      $.for_block,
      $.if_expression,
      $.match_expression,
      $.play_statement,
    ),

    comment: $ => seq('//', /[^\n]*/),

    config: $ => seq(
      'config',
      '{',
      optional($.property_list),
      '}'
    ),

    import_statement: $ => seq(
      'import',
      field('path', $.string),
      optional(seq('as', field('alias', $.identifier)))
    ),

    let_declaration: $ => seq(
      optional('pub'),
      'let',
      field('name', $.identifier),
      '=',
      field('value', $._expression)
    ),

    type_alias: $ => seq(
      optional('pub'),
      'type',
      field('name', $.identifier),
      '=',
      field('annotation', $.type_annotation)
    ),

    component_definition: $ => seq(
      optional('pub'),
      'component',
      field('name', $.identifier),
      optional($.parameter_list),
      $.block
    ),

    parameter_list: $ => seq(
      '(',
      optional(seq(
        $.parameter,
        repeat(seq(',', $.parameter))
      )),
      ')'
    ),

    parameter: $ => seq(
      field('name', $.identifier),
      optional(choice(
        seq(':', field('type', $.type_annotation), optional(seq('=', field('default', $._expression)))),
        seq(':', field('default', $._expression)),
        seq('=', field('default', $._expression))
      ))
    ),

    type_annotation: $ => prec(10, choice(
      seq($._type_annotation, repeat1(seq('|', $._type_annotation))),
      $._type_annotation
    )),

    _type_annotation: $ => prec(20, choice(
      'Num',
      'Str',
      'Bool',
      'Vec2',
      'Vec4',
      'Color',
      'Actor',
      'Scene',
      'Any',
      $.type_identifier,
      $.type_path,
      seq('List', '<', $.type_annotation, '>')
    )),

    action_definition: $ => seq(
      'action',
      field('name', $.identifier),
      optional($.parameter_list),
      $.block
    ),

    scene_declaration: $ => seq(
      '#',
      field('name', $.identifier)
    ),

    keyframe: $ => seq('#', optional('+'), choice($.time_literal, $.number)),

    time_unit: $ => choice('s', 'ms'),

    actor_declaration: $ => seq(
      optional('pub'),
      field('label', $.identifier),
      optional(seq('[', field('array_index', $._expression), ']')),
      ':',
      field('type', $.identifier),
      optional(seq(',', $.property_list)),
      optional($.modifier_block),
      optional($.children_block)
    ),

    text_shorthand: $ => seq(
      field('label', $.identifier),
      ':',
      field('text', $.string),
      optional($.modifier_block)
    ),

    typst_shorthand: $ => seq(
      field('label', $.identifier),
      ':',
      '$$',
      field('content', token(/[^$]*/)),
      '$$',
      optional($.modifier_block)
    ),

    property_assignment: $ => seq(
      field('target', choice($.path_expression, $.indexed_target_path)),
      '=',
      field('value', $._expression),
      optional($.modifier_block)
    ),

    action_invocation: $ => seq(
      field('verb', $.identifier),
      field('targets', $.target_list),
      optional($.modifier_block)
    ),

    target_list: $ => seq(
      choice($.identifier, $.path_expression, $.index_expression),
      repeat(seq(',', choice($.identifier, $.path_expression, $.index_expression)))
    ),

    sequence_block: $ => seq(
      'sequence',
      $.block
    ),

    stagger_block: $ => seq(
      'stagger',
      optional(seq('[', $._expression, ']')),
      $.block
    ),

    always_block: $ => seq(
      'always',
      $.block
    ),

    for_block: $ => seq(
      'for',
      field('variable', choice(
        $.identifier,
        seq('(', $.identifier, repeat(seq(',', $.identifier)), ')')
      )),
      optional(seq(',', field('index_variable', $.identifier))),
      'in',
      field('iterable', $._expression),
      $.block
    ),

    if_expression: $ => seq(
      'if',
      field('condition', $._expression),
      field('consequence', choice(
        $.block,
        $.expression_block
      )),
      optional(seq('else', field('alternative', choice(
        $.block,
        $.expression_block
      ))))
    ),

    // Single CST node used by both statement and expression forms. The
    // statement form (block values) may omit commas between arms; the
    // expression form keeps the Rust-style comma-separated arms. The converter
    // decides between Stmt::Match and Expr::Match from the surrounding context.
    match_expression: $ => seq(
      'match',
      field('scrutinee', $._expression),
      '{',
      optional(seq(
        repeat($.match_statement_arm),
        optional(seq(
          $.match_arm,
          repeat(seq(',', $.match_arm)),
          optional(',')
        ))
      )),
      '}'
    ),

    match_statement_arm: $ => seq(
      field('pattern', $.match_pattern),
      '=>',
      field('value', $.block),
      optional(',')
    ),

    match_arm: $ => seq(
      field('pattern', $.match_pattern),
      '=>',
      field('value', $._expression)
    ),

    match_pattern: $ => choice(
      $.match_wildcard,
      $.match_range,
      $.match_literal,
      $.match_or,
      $.match_tuple
    ),

    match_wildcard: $ => '_',

    match_num: $ => /[0-9]+(\.[0-9]+)?/,

    match_literal: $ => choice($.match_num, $.string, $.boolean),

    // Note: match_range uses $.match_num (regex token, not the external
    // NUMBER scanner) so that '0..=3' doesn't have the first '.' stolen
    // by the external scanner's decimal-point handling.
    match_range: $ => seq(
      field('low', $.match_num),
      '..=',
      field('high', $.match_num)
    ),

    match_or: $ => prec.left(seq(
      $.match_pattern,
      '|',
      $.match_pattern
    )),

    match_tuple: $ => seq(
      '(',
      optional(seq(
        $.match_pattern,
        repeat(seq(',', $.match_pattern)),
        optional(',')
      )),
      ')'
    ),

    expression_block: $ => seq(
      '{',
      $._expression,
      '}'
    ),

    play_statement: $ => seq(
      'play',
      field('scene', choice($.identifier, $.path_expression)),
      optional($.modifier_block)
    ),

    // Deprecated: not supported by the runtime PEG parser.
    // use_statement: $ => seq(
    //   'use',
    //   field('path', $.path_expression)
    // ),

    block: $ => seq(
      '{',
      repeat($._statement),
      '}'
    ),

    // ── Inline items (used inside children_block) ───────────────────────

    inline_items: $ => seq(
      $.inline_item,
      repeat(choice(
        seq(',', $.inline_item),
        $.inline_item
      )),
      optional(',')
    ),

    inline_item: $ => choice(
      $.inline_actor_declaration,
      $.inline_anonymous_actor,
      $.inline_property,
      $.inline_for_loop,
      $.inline_slot_marker,
      $.inline_slot_fill,
      $.inline_children_block,
    ),

    inline_actor_declaration: $ => seq(
      field('label', $.identifier),
      optional(seq('[', field('array_index', $._expression), ']')),
      ':',
      field('type', $.identifier),
      optional($.modifier_block),
      optional($.children_block)
    ),

    inline_anonymous_actor: $ => seq(
      field('type', $.identifier),
      optional($.modifier_block),
      optional($.children_block)
    ),

    inline_property: $ => seq(
      field('name', choice($.path_expression, $.identifier, $.string)),
      ':',
      field('value', $._expression)
    ),

    inline_for_loop: $ => seq(
      'for',
      field('variable', choice(
        $.identifier,
        seq('(', $.identifier, repeat(seq(',', $.identifier)), ')')
      )),
      optional(seq(',', field('index_variable', $.identifier))),
      'in',
      field('iterable', $._expression),
      $.children_block
    ),

    inline_slot_marker: $ => '@slot',

    inline_slot_fill: $ => seq(
      '@',
      field('name', $.identifier),
      $.children_block
    ),

    inline_children_block: $ => seq(
      '{',
      optional($.inline_items),
      '}'
    ),

    children_block: $ => seq(
      '{',
      optional($.inline_items),
      '}'
    ),

    property_list: $ => seq(
      $.property,
      repeat(seq(',', $.property)),
      optional(',')
    ),

    property: $ => seq(
      field('name', choice($.path_expression, $.identifier, $.string)),
      ':',
      field('value', $._expression)
    ),

    modifier_block: $ => seq(
      '[',
      optional($.modifier_list),
      ']'
    ),

    modifier_list: $ => seq(
      $.modifier,
      repeat(seq(',', $.modifier))
    ),

    modifier: $ => choice(
      $._expression,
      seq(
        field('key', $.identifier),
        ':',
        field('value', $._expression)
      )
    ),

    // Expressions
    _expression: $ => choice(
      $.number,
      $.percentage,
      $.time_literal,
      $.string,
      $.boolean,
      $.null_literal,
      $.identifier,
      $.path_expression,
      $.unary_expression,
      $.binary_expression,
      $.call_expression,
      $.index_expression,
      $.tuple_expression,
      $.array_expression,
      $.set_expression,
      $.closure_expression,
      $.object_expression,
      $.parenthesized_expression,
      $.method_call_expression,
      $.if_expression,
      $.match_expression,
    ),

    path_expression: $ => prec.left(seq(
      field('base', choice($.identifier, $.path_expression)),
      '.',
      field('name', $.identifier)
    )),

    indexed_target_path: $ => prec.left(seq(
      field('base', choice($.identifier, $.path_expression, $.indexed_target_path)),
      '[',
      field('index', $.index_value),
      ']',
      optional(seq('.', field('name', choice($.identifier, $.path_expression, $.indexed_target_path))))
    )),

    unary_expression: $ => prec.left(3, seq(
      field('operator', choice('-', '!')),
      field('operand', $._expression)
    )),

    binary_expression: $ => choice(
      prec.left(1, seq($._expression, choice('+', '-'), $._expression)),
      prec.left(2, seq($._expression, choice('*', '/', '%', '^'), $._expression)),
      prec.left(0, seq($._expression, choice('==', '!=', '<', '>', '<=', '>='), $._expression)),
      prec.left(0, seq($._expression, choice('&&', '||'), $._expression)),
    ),

    call_expression: $ => prec(4, seq(
      field('function', $.identifier),
      '(',
      optional($.argument_list),
      ')'
    )),

    method_call_expression: $ => prec(4, seq(
      field('object', $._expression),
      '.',
      field('method', $.identifier),
      '(',
      optional($.argument_list),
      ')'
    )),

    argument_list: $ => seq(
      $._expression,
      repeat(seq(',', $._expression))
    ),

    index_expression: $ => prec(5, seq(
      field('object', $._expression),
      token.immediate('['),
      field('index', $.index_value),
      ']'
    )),

    // Values that make sense as array/tuple indices.
    // Excludes time_literal and percentage to avoid ambiguity with modifier blocks.
    index_value: $ => choice(
      $.number,
      $.identifier,
      $.path_expression,
      $.parenthesized_expression
    ),

    tuple_expression: $ => seq(
      '(',
      optional(seq($._expression, repeat(seq(',', $._expression)))),
      ')'
    ),

    array_expression: $ => seq(
      '[',
      optional(seq($._expression, repeat(seq(',', $._expression)))),
      ']'
    ),

    set_expression: $ => seq(
      '{',
      optional(seq($._expression, repeat(seq(',', $._expression)))),
      '}'
    ),

    closure_expression: $ => choice(
      seq(
        '(',
        optional(seq($.identifier, repeat(seq(',', $.identifier)))),
        ')',
        '=>',
        $._expression
      ),
      // Single-identifier closure: x => expr (no parens)
      seq(
        field('param', $.identifier),
        '=>',
        $._expression
      )
    ),

    object_expression: $ => seq(
      field('type', $.identifier),
      '{',
      optional($.property_list),
      '}'
    ),

    parenthesized_expression: $ => seq(
      '(',
      $._expression,
      ')'
    ),

    // New statements
    // Note: use_statement is now deprecated and moved next to play_statement.

    reactive_binding: $ => seq(
      field('target', choice($.identifier, $.path_expression, $.indexed_target_path)),
      ':=',
      field('value', $._expression),
      optional($.modifier_block)
    ),

    // Literals
    // number and time_literal are handled by external scanner (scanner.c)
    // The scanner decides: digits → number, digits+s/ms → time_literal

    percentage: $ => prec(6, seq($.number, '%')),

    null_literal: $ => 'null',

    // time_literal is handled by external scanner (scanner.c)
    // It tokenizes '800ms', '2.5s', etc. as a single token

    string: $ => choice(
      seq('"', /([^"\\]|\\.)*/, '"'),
      seq("'", /([^'\\]|\\.)*/, "'")
    ),

    boolean: $ => choice('true', 'false'),

    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_-]*/,

    type_identifier: $ => /[A-Z][a-zA-Z0-9_-]*/,

    lower_identifier: $ => /[a-z_][a-zA-Z0-9_-]*/,

    colon_colon: $ => token(seq(':', ':')),

    type_path: $ => prec(30, seq(
      $.identifier,
      repeat(seq($.colon_colon, $.lower_identifier)),
      $.colon_colon,
      $.type_identifier
    )),
  }
});
