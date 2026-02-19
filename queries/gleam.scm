;; File sourced from https://github.com/gleam-lang/tree-sitter-gleam 
;; It's under Apache License

; Functions
(function
  name: (identifier) @name) @definition.function
(external_function
  name: (identifier) @name) @definition.function

; Types
(type_definition
  (type_name
    name: (type_identifier) @name)) @definition.type
(type_definition
  (data_constructors
    (data_constructor
      name: (constructor_name) @name))) @definition.constructor
(external_type
  (type_name
    name: (type_identifier) @name)) @definition.type

