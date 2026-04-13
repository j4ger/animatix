const PREC = {
  closure: 9,
  conditional: 8,
  compare: 4,
  sum: 5,
  product: 6,
  power: 7,
  unary: 8,
  call: 10,
  path: 11,
};

module.exports = grammar({
  name: 'animatix',

  extras: $ => [
    /[\s\uFEFF\u2060\u200B]+/,
  ],

  word: $ => $.identifier,

  conflicts: $ => [
    [$.closure_expression, $.parenthesized_expression],
    [$.closure_parameters, $.parenthesized_expression],
    [$.closure_parameters, $._expression],
  ],

  rules: {
    source_file: $ => repeat($._top_level_item),

    _top_level_item: $ => choice(
      $.absolute_keyframe,
      $.relative_keyframe,
      $._statement,
    ),

    _statement: $ => choice(
      $.comment,
      $.let_declaration,
      $.import_statement,
      $.labeled_always_statement,
      $.always_statement,
      $.if_statement,
      $.for_statement,
      $.component_definition,
      $.text_statement,
      $.math_statement,
      $.code_statement,
      $.svg_statement,
      $.image_statement,
      $.assignment,
      $.actor_declaration,
      $.action_statement,
    ),

    comment: _ => token(seq('//', /[^\r\n]*/)),

    absolute_keyframe: $ => prec.right(seq(
      '#',
      field('time', $.duration_literal),
      repeat($._statement),
    )),

    relative_keyframe: $ => prec.right(seq(
      '#+',
      field('offset', $.duration_literal),
      repeat($._statement),
    )),

    let_declaration: $ => seq(
      'let',
      field('name', $.identifier),
      '=',
      field('value', $._expression),
    ),

    import_statement: $ => seq(
      'import',
      field('path', $.string),
    ),

    assignment: $ => seq(
      field('target', $.assignment_target),
      '=',
      field('value', $._expression),
      optional(field('modifiers', $.modifier_list)),
    ),

    assignment_target: $ => seq(
      field('base', $.identifier),
      repeat1(seq('.', field('segment', $.identifier))),
    ),

    text_statement: $ => seq(
      optional(seq(field('label', $.identifier), ':')),
      'Text',
      field('properties', $.property_block),
      optional(field('modifiers', $.modifier_list)),
    ),

    math_statement: $ => seq(
      optional(seq(field('label', $.identifier), ':')),
      'Math',
      field('properties', $.property_block),
      optional(field('modifiers', $.modifier_list)),
    ),

    code_statement: $ => seq(
      optional(seq(field('label', $.identifier), ':')),
      'Code',
      field('properties', $.property_block),
      optional(field('modifiers', $.modifier_list)),
    ),

    svg_statement: $ => seq(
      optional(seq(field('label', $.identifier), ':')),
      'Svg',
      field('properties', $.property_block),
    ),

    image_statement: $ => seq(
      optional(seq(field('label', $.identifier), ':')),
      'Image',
      field('properties', $.property_block),
    ),

    actor_declaration: $ => seq(
      optional(field('visibility', 'pub')),
      field('label', $.identifier),
      ':',
      field('type', $.type_identifier),
      optional(field('properties', alias($.declaration_property_list, $.property_list))),
      optional(field('modifiers', $.modifier_list)),
      optional(field('children', $.inline_children_block)),
    ),

    action_statement: $ => prec.right(seq(
      field('verb', $.identifier),
      repeat1(field('target', $.identifier)),
      optional(field('modifiers', $.modifier_list)),
    )),

    always_statement: $ => seq(
      'always',
      field('body', $.block),
    ),

    labeled_always_statement: $ => seq(
      field('label', $.identifier),
      ':',
      'always',
      field('body', $.block),
    ),

    if_statement: $ => seq(
      'if',
      field('condition', $._expression),
      field('consequence', $.block),
      optional(seq('else', field('alternative', $.block))),
    ),

    for_statement: $ => seq(
      'for',
      field('variable', $.identifier),
      'in',
      field('iterable', $._expression),
      field('body', $.block),
    ),

    component_definition: $ => seq(
      optional(field('visibility', 'pub')),
      'component',
      field('name', $.identifier),
      optional(field('parameters', $.parameter_list)),
      field('body', $.block),
    ),

    parameter_list: $ => seq(
      '(',
      commaSep($.parameter_definition),
      optional(','),
      ')',
    ),

    parameter_definition: $ => seq(
      field('name', $.identifier),
      ':',
      field('default', choice($.string, $.null)),
    ),

    block: $ => seq('{', repeat($._statement), '}'),

    property_block: $ => seq('{', commaSep($.property), optional(','), '}'),

    property_list: $ => prec.left(seq(
      commaSep1($.property),
      optional(','),
    )),

    declaration_property_list: $ => prec.right(seq(
      ',',
      $.property,
      repeat(seq(',', $.property)),
      optional(','),
    )),

    property: $ => seq(
      field('name', $.identifier),
      ':',
      field('value', $._expression),
    ),

    modifier_list: $ => seq('[', commaSep($.modifier), optional(','), ']'),

    modifier: $ => choice(
      $.named_modifier,
      $.duration_literal,
      $._expression,
    ),

    named_modifier: $ => seq(
      field('name', $.identifier),
      ':',
      field('value', $._expression),
    ),

    inline_children_block: $ => seq('{', commaSep($.inline_item), optional(','), '}'),

    inline_item: $ => choice(
      $.inline_labeled_item,
      $.inline_anonymous_item,
      $.property,
    ),

    inline_labeled_item: $ => seq(
      field('label', $.identifier),
      ':',
      field('type', $.type_identifier),
      optional(field('modifiers', $.modifier_list)),
      optional(field('children', $.inline_children_block)),
    ),

    inline_anonymous_item: $ => seq(
      field('type', $.type_identifier),
      optional(field('modifiers', $.modifier_list)),
      optional(field('children', $.inline_children_block)),
    ),

    _expression: $ => choice(
      $.closure_expression,
      $.conditional_expression,
      $.comparison_expression,
      $.sum_expression,
      $.product_expression,
      $.power_expression,
      $.unary_expression,
      $.call_expression,
      $.path_expression,
      $.parenthesized_expression,
      $.tuple_expression,
      $.brace_array,
      $.number,
      $.percentage,
      $.string,
      $.boolean,
      $.null,
      $.identifier,
    ),

    closure_expression: $ => prec.right(PREC.closure, seq(
      field('parameters', $.closure_parameters),
      '=>',
      field('body', $._expression),
    )),

    closure_parameters: $ => choice(
      $.identifier,
      seq('(', commaSep($.identifier), optional(','), ')'),
    ),

    conditional_expression: $ => prec.right(PREC.conditional, seq(
      'if',
      field('condition', $._expression),
      '{',
      field('consequence', $._expression),
      '}',
      'else',
      '{',
      field('alternative', $._expression),
      '}',
    )),

    comparison_expression: $ => prec.left(PREC.compare, seq(
      field('left', choice($.sum_expression, $.product_expression, $.power_expression, $.unary_expression, $.call_expression, $.path_expression, $.parenthesized_expression, $.tuple_expression, $.brace_array, $.number, $.percentage, $.string, $.boolean, $.null, $.identifier)),
      field('operator', choice('>=', '<=', '==', '!=', '>', '<')),
      field('right', $._expression),
    )),

    sum_expression: $ => prec.left(PREC.sum, seq(
      field('left', choice($.product_expression, $.power_expression, $.unary_expression, $.call_expression, $.path_expression, $.parenthesized_expression, $.tuple_expression, $.brace_array, $.number, $.percentage, $.string, $.boolean, $.null, $.identifier)),
      field('operator', choice('+', '-')),
      field('right', choice($.product_expression, $.power_expression, $.unary_expression, $.call_expression, $.path_expression, $.parenthesized_expression, $.tuple_expression, $.brace_array, $.number, $.percentage, $.string, $.boolean, $.null, $.identifier)),
    )),

    product_expression: $ => prec.left(PREC.product, seq(
      field('left', choice($.power_expression, $.unary_expression, $.call_expression, $.path_expression, $.parenthesized_expression, $.tuple_expression, $.brace_array, $.number, $.percentage, $.string, $.boolean, $.null, $.identifier)),
      field('operator', choice('*', '/', '%')),
      field('right', choice($.power_expression, $.unary_expression, $.call_expression, $.path_expression, $.parenthesized_expression, $.tuple_expression, $.brace_array, $.number, $.percentage, $.string, $.boolean, $.null, $.identifier)),
    )),

    power_expression: $ => prec.right(PREC.power, seq(
      field('left', choice($.unary_expression, $.call_expression, $.path_expression, $.parenthesized_expression, $.tuple_expression, $.brace_array, $.number, $.percentage, $.string, $.boolean, $.null, $.identifier)),
      field('operator', '^'),
      field('right', $._expression),
    )),

    unary_expression: $ => prec.right(PREC.unary, seq(
      field('operator', choice('-', '!')),
      field('argument', choice($.call_expression, $.path_expression, $.parenthesized_expression, $.tuple_expression, $.brace_array, $.number, $.percentage, $.string, $.boolean, $.null, $.identifier)),
    )),

    call_expression: $ => prec(PREC.call, seq(
      field('function', $.identifier),
      field('arguments', $.argument_list),
    )),

    argument_list: $ => seq('(', commaSep($._expression), optional(','), ')'),

    path_expression: $ => prec.left(PREC.path, seq(
      field('base', $.identifier),
      repeat1(seq('.', field('segment', $.identifier))),
    )),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    tuple_expression: $ => seq(
      '(',
      $._expression,
      ',',
      commaSep($._expression),
      optional(','),
      ')',
    ),

    brace_array: $ => seq('{', commaSep($._expression), optional(','), '}'),

    boolean: $ => choice('true', 'false'),
    null: _ => 'null',

    string: _ => token(seq('"', repeat(/[^"\r\n]/), '"')),

    number: _ => token(/\d+(?:\.\d+)?/),

    percentage: _ => token(/\d+(?:\.\d+)?%/),

    duration_literal: _ => token(/\d+(?:\.\d+)?(?:ms|s)/),

    identifier: _ => token(prec(-1, /[A-Za-z_][A-Za-z0-9_]*(?:-[A-Za-z_][A-Za-z0-9_]*)*/)),

    type_identifier: _ => token(prec(1, /[A-Z][A-Za-z0-9_]*(?:-[A-Za-z_][A-Za-z0-9_]*)*/)),
  },
});

function commaSep(rule) {
  return optional(seq(rule, repeat(seq(',', rule))));
}

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)));
}
