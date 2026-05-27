module.exports = grammar({
  name: 'animatix',

  extras: $ => [
    /\s/,
    $.comment,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    [$._expression, $.object_expression],
    [$._expression, $.path_expression],
    [$._expression, $.call_expression],
    [$._expression, $.method_call_expression],
    [$._expression, $.closure_expression],
    [$._expression, $.tuple_expression],
    [$._expression, $.index_expression],
    [$._expression, $.parenthesized_expression],
    [$._expression, $.array_expression],
    [$._expression, $.type_annotation],
    [$.tuple_expression, $.parenthesized_expression],
    [$.binary_expression, $.closure_expression],
    [$.binary_expression, $.path_expression],
    [$.binary_expression, $.call_expression],
    [$.binary_expression, $.method_call_expression],
    [$.binary_expression, $.index_expression],
    [$._statement, $.actor_declaration],
    [$._statement, $.property_assignment],
    [$._statement, $.action_invocation],
    [$.target_list, $.argument_list],
  ],

  rules: {
    source_file: $ => repeat($._statement),

    _statement: $ => choice(
      $.comment,
      $.config,
      $.import_statement,
      $.let_declaration,
      $.component_definition,
      $.action_definition,
      $.scene_declaration,
      $.keyframe,
      $.actor_declaration,
      $.property_assignment,
      $.action_invocation,
      $.sequence_block,
      $.stagger_block,
      $.always_block,
      $.for_block,
      $.if_statement,
      $.play_statement,
      $.slot_marker,
      $.slot_fill,
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
      optional(seq(':', field('type', $.type_annotation), optional(seq('=', field('default', $._expression))))),
      optional(seq('=', field('default', $._expression)))
    ),

    type_annotation: $ => choice(
      'Num',
      'Str',
      'Bool',
      'Vec2',
      'Vec4',
      'Color',
      'Actor',
      'Scene',
      'Any',
      seq('List', '<', $.type_annotation, '>')
    ),

    action_definition: $ => seq(
      'action',
      field('name', $.identifier),
      $.block
    ),

    scene_declaration: $ => seq(
      '#',
      field('name', $.identifier)
    ),

    keyframe: $ => seq(
      '#',
      choice(
        seq(optional('+'), $.number, optional($.time_unit)),
        seq('+', $.number, optional($.time_unit))
      )
    ),

    time_unit: $ => choice('s', 'ms'),

    actor_declaration: $ => seq(
      field('label', $.identifier),
      ':',
      field('type', choice($.identifier, $.string)),
      optional(seq(',', $.property_list)),
      optional($.modifier_block),
      optional($.children_block)
    ),

    property_assignment: $ => seq(
      field('target', $.path_expression),
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
      $.identifier,
      repeat(seq(',', $.identifier))
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
      field('variable', $.identifier),
      'in',
      field('iterable', $._expression),
      $.block
    ),

    if_statement: $ => seq(
      'if',
      field('condition', $._expression),
      field('consequence', $.block),
      optional(seq('else', field('alternative', $.block)))
    ),

    play_statement: $ => seq(
      'play',
      field('scene', $.identifier),
      optional($.modifier_block)
    ),

    slot_marker: $ => '@slot',

    slot_fill: $ => seq(
      '@',
      field('name', $.identifier),
      $.block
    ),

    block: $ => seq(
      '{',
      repeat($._statement),
      '}'
    ),

    children_block: $ => seq(
      '{',
      repeat($._statement),
      '}'
    ),

    property_list: $ => seq(
      $.property,
      repeat(seq(',', $.property))
    ),

    property: $ => seq(
      field('name', choice($.identifier, $.string)),
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
      $.time_literal,
      $.string,
      $.boolean,
      $.identifier,
      $.path_expression,
      $.unary_expression,
      $.binary_expression,
      $.call_expression,
      $.index_expression,
      $.tuple_expression,
      $.array_expression,
      $.closure_expression,
      $.object_expression,
      $.parenthesized_expression,
      $.method_call_expression,
    ),

    path_expression: $ => prec.left(seq(
      field('base', choice($.identifier, $.path_expression)),
      '.',
      field('name', $.identifier)
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
      '[',
      field('index', $._expression),
      ']'
    )),

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

    closure_expression: $ => seq(
      '(',
      optional(seq($.identifier, repeat(seq(',', $.identifier)))),
      ')',
      '=>',
      $._expression
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

    // Literals
    number: $ => /\d+(\.\d+)?/,

    time_literal: $ => seq($.number, $.time_unit),

    string: $ => choice(
      seq('"', /[^"\\]*/, '"'),
      seq("'", /[^'\\]*/, "'")
    ),

    boolean: $ => choice('true', 'false'),

    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_-]*/,
  }
});
