; Basic Swift definitions from the same downloaded grammar used by the walker.

(class_declaration
  name: (type_identifier) @name) @definition.type

(class_declaration
  name: (user_type (type_identifier) @name)) @definition.type

(protocol_declaration
  name: (type_identifier) @name) @definition.interface

(function_declaration
  name: (simple_identifier) @name) @definition.function

; The grammar also labels return types as "name", so restrict this capture
; to operator text. Built-in operators are anonymous tokens.
(function_declaration
  name: _ @name
  (#match? @name "^[+*/%=<>!&|^~-]+$")) @definition.function

(protocol_function_declaration
  name: (simple_identifier) @name) @definition.function

(init_declaration
  "init" @name) @definition.method

(deinit_declaration
  "deinit" @name) @definition.method

(subscript_declaration
  "subscript" @name) @definition.method

(property_declaration
  (pattern (simple_identifier) @name)) @definition.variable

(protocol_property_declaration
  name: (pattern bound_identifier: (simple_identifier) @name)) @definition.variable

(enum_entry
  name: (simple_identifier) @name) @definition.constant

(typealias_declaration
  name: (type_identifier) @name) @definition.type

(associatedtype_declaration
  name: (type_identifier) @name) @definition.type

(operator_declaration
  (custom_operator) @name) @definition.operator

(operator_declaration
  (bang) @name) @definition.operator
