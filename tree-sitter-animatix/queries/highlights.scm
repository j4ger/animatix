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
  "true"
  "false"
] @keyword

; Statements / builtins
[
  "Text"
  "Math"
  "Code"
  "Svg"
  "Image"
] @type.builtin

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
(assignment_target base: (identifier) @variable)
(assignment_target segment: (identifier) @property)
(path_expression base: (identifier) @variable)
(path_expression segment: (identifier) @property)
(inline_labeled_item label: (identifier) @variable)
(inline_labeled_item type: (type_identifier) @type)
(inline_anonymous_item type: (type_identifier) @type)
(text_statement label: (identifier) @variable)
(math_statement label: (identifier) @variable)
(code_statement label: (identifier) @variable)
(svg_statement label: (identifier) @variable)
(image_statement label: (identifier) @variable)
(labeled_always_statement label: (identifier) @variable)
(for_statement variable: (identifier) @variable)
(call_expression function: (identifier) @function)

; Generic identifiers
(identifier) @variable
