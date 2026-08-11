; Comments
(comment) @comment

; Keywords
[
  "config"
  "import"
  "as"
  "let"
  "pub"
  "component"
  "action"
  "sequence"
  "stagger"
  "always"
  "for"
  "in"
  "if"
  "else"
  "play"
] @keyword

(inline_slot_marker) @keyword

; Scene/keyframe prefix
(scene_declaration "#" @punctuation.special)
(keyframe "#" @punctuation.special)

; Slot fill prefix
(inline_slot_fill "@" @punctuation.special)

; Literals
(number) @number
(percentage) @number
(time_literal) @number
(string) @string
(boolean) @boolean
(null_literal) @constant.builtin

; Operators
[
  "+"
  "-"
  "*"
  "/"
  "%"
  "^"
  "=="
  "!="
  "<"
  ">"
  "<="
  ">="
  "&&"
  "||"
  "!"
  "="
  ":="
  "=>"
] @operator

; Punctuation
[
  ","
  ":"
  "."
] @punctuation

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

; Type annotations in parameters
(type_annotation) @type

; Identifiers used as types in actor declarations
(actor_declaration
  type: (identifier) @type)

; Text shorthand labels
(text_shorthand
  label: (identifier) @label)

; Identifiers used as function names in calls
(call_expression
  function: (identifier) @function)

; Action verbs
(action_invocation
  verb: (identifier) @function)

; Method calls
(method_call_expression
  method: (identifier) @function)

; Property names in property lists and assignments
(property
  name: (identifier) @property)

(property
  name: (string) @property)

; Modifier keys
(modifier
  key: (identifier) @property)

; Parameter names in component definitions
(parameter
  name: (identifier) @parameter)

; Variable names in let declarations
(let_declaration
  name: (identifier) @variable)

; Variable names in for loops
(for_block
  variable: (identifier) @variable)

; Scene names
(scene_declaration
  name: (identifier) @type)

; Action names
(action_definition
  name: (identifier) @function)

; Component names
(component_definition
  name: (identifier) @type)

; Slot fill names
(inline_slot_fill
  name: (identifier) @property)

; Path expressions (property access / assignment targets)
(path_expression
  base: (identifier) @label)

(path_expression
  name: (identifier) @property)

; Labels in actor declarations (definition site)
(actor_declaration
  label: (identifier) @label)

; Target identifiers in action invocations (reference site)
(target_list
  (identifier) @label)
