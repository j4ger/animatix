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

(slot_marker) @keyword

; Literals
(number) @number
(time_literal) @number
(string) @string
(boolean) @boolean

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

; Identifiers used as types in actor declarations
(actor_declaration
  type: (identifier) @type)

(actor_declaration
  type: (string) @type)

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
(slot_fill
  name: (identifier) @property)

; Labels in actor declarations
(actor_declaration
  label: (identifier) @variable)

; Target identifiers in action invocations
(target_list
  (identifier) @variable)
