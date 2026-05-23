; Comments
(comment) @comment

; Keywords
[
  "let"
  "import"
  "always"
  "if"
  "else"
  "for"
  "in"
  "pub"
  "component"
  "sequence"
  "stagger"
  "config"
  "true"
  "false"
] @keyword

(null) @keyword

; Literals
(string) @string
(number) @number
(percentage) @number
(duration_literal) @number
(boolean) @boolean

; Delimiters and operators
[
  "#"
  "#+"
  ":"
  ","
  "."
  "="
  "=>"
  "+"
  "-"
  "*"
  "/"
  "%"
  "^"
  ">"
  "<"
  ">="
  "<="
  "=="
  "!="
] @operator

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

; Names
(let_declaration name: (identifier) @variable)
(parameter_definition name: (identifier) @parameter)
(property name: (identifier) @property)
(named_modifier name: (identifier) @property)
(component_definition name: (identifier) @type)
(actor_declaration label: (identifier) @variable)
(actor_declaration type: (type_identifier) @type)
(action_statement verb: (identifier) @function)
(action_statement target: (identifier) @variable)
(assignment target: (dotted_identifier) @variable)
(dotted_identifier base: (identifier) @variable)
(dotted_identifier segment: (identifier) @property)
(path_expression base: (identifier) @variable)
(path_expression segment: (identifier) @property)
(inline_labeled_item_with_props label: (identifier) @variable)
(inline_labeled_item_with_props type: (type_identifier) @type)
(inline_labeled_item label: (identifier) @variable)
(inline_labeled_item type: (type_identifier) @type)
(inline_anon_with_props type: (type_identifier) @type)
(inline_anon type: (type_identifier) @type)
(text_shorthand label: (identifier) @variable)
(svg_statement label: (identifier) @variable)
(image_statement label: (identifier) @variable)
(scene_declaration name: (identifier) @type)
(labeled_always_statement label: (identifier) @variable)
(for_statement variable: (identifier) @variable)
(call_expression function: (identifier) @function)

; Type annotations (Num, Str, Bool, Vec2, Vec4, Color, Actor, Scene, List<T>)
(type_annotation) @type

; Type identifiers (builtin types like Text, Circle, Button, etc.)
(type_identifier) @type

; Generic identifiers
(identifier) @variable
