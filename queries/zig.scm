(variable_declaration
  . (identifier) @name
  (#not-eq? @name "_")) @definition.variable

(function_declaration
  name: (identifier) @name) @definition.function

(container_field
  name: (identifier) @name
  (#not-eq? @name "_")) @definition.property

(error_set_declaration
  (identifier) @name) @definition.constant

(test_declaration
  (identifier) @name) @definition.test

(test_declaration
  (string (string_content) @name)
  (#not-eq? @name "_")) @definition.test
