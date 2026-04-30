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
      $.config_statement,
      $.let_declaration,
      $.import_statement,
      $.labeled_always_statement,
      $.always_statement,
      $.if_statement,
      $.for_statement,
      $.component_definition,
      $.svg_statement,
      $.image_statement,
      $.assignment,
      $.actor_declaration,
      $.text_shorthand,
      $.action_statement,
      $.sequence_statement,
      $.stagger_statement,
    ),

    comment: _ => token(seq('//', /[^\r\n]*/)),

    // ============================================================
    // Config: config { colorscheme: "editorial-dark" }
    // ============================================================
    config_statement: $ => seq(
      'config',
      '{',
      commaSepTrailing($.property),
      '}',
    ),

    // ============================================================
    // Keyframes: #0s, #1.5s, #+500ms
    // ============================================================
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

    // ============================================================
    // Declarations
    // ============================================================
    let_declaration: $ => seq(
      optional('pub'),
      'let',
      field('name', $.identifier),
      '=',
      field('value', $._expression),
    ),

    import_statement: $ => seq(
      'import',
      field('path', $.string),
      optional(seq('as', field('alias', $.identifier))),
    ),

    // ============================================================
    // Assignments: auto1.radius = 48 [700ms, ease: ease-in-out]
    // ============================================================
    assignment: $ => seq(
      field('target', $.dotted_identifier),
      '=',
      field('value', $._expression),
      optional(field('modifiers', $.modifier_list)),
    ),

    dotted_identifier: $ => seq(
      field('base', $.identifier),
      repeat1(seq('.', field('segment', $.identifier))),
    ),

    // ============================================================
    // SVG/Image statements
    // ============================================================
    svg_statement: $ => seq(
      optional(seq(field('label', $.identifier), ':')),
      'Svg',
      '{',
      commaSepTrailing($.property),
      '}',
    ),

    image_statement: $ => seq(
      optional(seq(field('label', $.identifier), ':')),
      'Image',
      '{',
      commaSepTrailing($.property),
      '}',
    ),

    // ============================================================
    // Text shorthand: title: "Slide 1"
    // ============================================================
    text_shorthand: $ => seq(
      field('label', $.identifier),
      ':',
      field('value', $.string),
      optional(field('modifiers', $.modifier_list)),
    ),

    // ============================================================
    // Actor declaration:
    // label: Text, text: "Hello", font_size: 20, color: text.primary
    // label: Text, text: "Hello" { children }
    // pub label: Text
    // ============================================================
    actor_declaration: $ => seq(
      optional('pub'),
      field('label', $.identifier),
      ':',
      field('type', $.type_identifier),
      optional($._actor_properties),
      optional(field('modifiers', $.modifier_list)),
      optional(field('children', $.inline_children_block)),
    ),

    // Actor properties - comma-separated after type
    _actor_properties: $ => seq(
      ',',
      $.property,
      repeat(seq(',', $.property)),
    ),

    // ============================================================
    // Action: move btn to (100, 100) [2s]
    // ============================================================
    action_statement: $ => prec.right(seq(
      field('verb', $.identifier),
      repeat1(field('target', $.identifier)),
      optional(field('modifiers', $.modifier_list)),
    )),

    // ============================================================
    // Sequence: sequence { ... }
    // ============================================================
    sequence_statement: $ => seq(
      'sequence',
      '{',
      repeat($._statement),
      '}',
    ),

    // ============================================================
    // Stagger: stagger [150ms] { ... }
    // ============================================================
    stagger_statement: $ => seq(
      'stagger',
      optional(field('modifiers', $.modifier_list)),
      '{',
      repeat($._statement),
      '}',
    ),

    // ============================================================
    // Always blocks
    // ============================================================
    always_statement: $ => seq(
      'always',
      '{',
      repeat($._statement),
      '}',
    ),

    labeled_always_statement: $ => seq(
      field('label', $.identifier),
      ':',
      'always',
      '{',
      repeat($._statement),
      '}',
    ),

    // ============================================================
    // Conditionals
    // ============================================================
    if_statement: $ => seq(
      'if',
      field('condition', $._expression),
      '{',
      field('consequence', repeat($._statement)),
      '}',
      optional(seq(
        'else',
        '{',
        field('alternative', repeat($._statement)),
        '}',
      )),
    ),

    // ============================================================
    // For loop
    // ============================================================
    for_statement: $ => seq(
      'for',
      field('variable', $.identifier),
      'in',
      field('iterable', $._expression),
      '{',
      field('body', repeat($._statement)),
      '}',
    ),

    // ============================================================
    // Component definition
    // ============================================================
    component_definition: $ => seq(
      optional('pub'),
      'component',
      field('name', $.identifier),
      optional(seq(
        '(',
        commaSepTrailing($.parameter_definition),
        ')',
      )),
      '{',
      repeat($._statement),
      '}',
    ),

    parameter_definition: $ => seq(
      field('name', $.identifier),
      ':',
      optional(field('default', choice($.string, 'null'))),
    ),

    // ============================================================
    // Inline children block for containers (Row, Col, Grid)
    // Items are separated by newlines, commas are part of items with properties
    // ============================================================
    inline_children_block: $ => seq(
      '{',
      repeat($._inline_item),
      '}',
    ),

    // Inline items - no commas between items, commas are part of items with properties
    _inline_item: $ => choice(
      $.inline_labeled_item_with_props,
      $.inline_anon_with_props,
      $.inline_labeled_item,
      $.inline_anon,
    ),

    // Labeled inline item with properties: label: Type, prop1: val1, prop2: val2
    inline_labeled_item_with_props: $ => seq(
      field('label', $.identifier),
      ':',
      field('type', $.type_identifier),
      ',',
      $.property,
      repeat(seq(',', $.property)),
    ),

    // Labeled inline item without properties: label: Type
    inline_labeled_item: $ => seq(
      field('label', $.identifier),
      ':',
      field('type', $.type_identifier),
    ),

    // Anonymous inline item with properties: Type, prop1: val1, prop2: val2
    inline_anon_with_props: $ => seq(
      field('type', $.type_identifier),
      ',',
      $.property,
      repeat(seq(',', $.property)),
    ),

    // Anonymous inline item without properties: Type
    inline_anon: $ => field('type', $.type_identifier),

    // ============================================================
    // Properties
    // ============================================================
    property: $ => seq(
      field('name', choice($.dotted_identifier, $.identifier)),
      ':',
      field('value', $._expression),
    ),

    // ============================================================
    // Modifiers: [2s], [delay: 500ms, ease: bounce]
    // ============================================================
    modifier_list: $ => seq(
      '[',
      commaSepTrailing($.modifier),
      ']',
    ),

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

    // ============================================================
    // Expressions
    // ============================================================
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
      seq('(', commaSepTrailing($.identifier), ')'),
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
      field('left', $._expression),
      field('operator', choice('>=', '<=', '==', '!=', '>', '<')),
      field('right', $._expression),
    )),

    sum_expression: $ => prec.left(PREC.sum, seq(
      field('left', $._expression),
      field('operator', choice('+', '-')),
      field('right', $._expression),
    )),

    product_expression: $ => prec.left(PREC.product, seq(
      field('left', $._expression),
      field('operator', choice('*', '/', '%')),
      field('right', $._expression),
    )),

    power_expression: $ => prec.right(PREC.power, seq(
      field('left', $._expression),
      '^',
      field('right', $._expression),
    )),

    unary_expression: $ => prec.right(PREC.unary, seq(
      field('operator', choice('-', '!')),
      field('argument', $._expression),
    )),

    call_expression: $ => prec(PREC.call, seq(
      field('function', $.identifier),
      '(',
      commaSepTrailing($._expression),
      ')',
    )),

    path_expression: $ => prec.left(PREC.path, seq(
      field('base', $.identifier),
      repeat1(seq('.', field('segment', $.identifier))),
    )),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    tuple_expression: $ => seq(
      '(',
      $._expression,
      ',',
      commaSepTrailing($._expression),
      optional(','),
      ')',
    ),

    brace_array: $ => seq(
      '{',
      commaSepTrailing($._expression),
      optional(','),
      '}',
    ),

    boolean: _ => choice('true', 'false'),
    null: _ => 'null',

    string: _ => token(seq('"', repeat(/[^"\r\n]/), '"')),

    number: _ => token(/\d+(?:\.\d+)?/),

    percentage: _ => token(/\d+(?:\.\d+)?%/),

    duration_literal: _ => token(/\d+(?:\.\d+)?(?:ms|s)/),

    // Identifiers allow hyphens: [A-Za-z_][A-Za-z0-9_]*(?:-[A-Za-z_][A-Za-z0-9_]*)*
    identifier: _ => token(prec(-1, /[A-Za-z_][A-Za-z0-9_]*(?:-[A-Za-z_][A-Za-z0-9_]*)*/)),

    // Type identifiers start with uppercase (higher precedence to win over identifier)
    type_identifier: _ => token(prec(1, /[A-Z][A-Za-z0-9_]*(?:-[A-Za-z_][A-Za-z0-9_]*)*/)),
  },
});

// Helper function for comma-separated lists with optional trailing comma
function commaSepTrailing(rule) {
  return optional(seq(rule, repeat(seq(',', rule)), optional(',')));
}
