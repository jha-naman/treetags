; Derivative of tree-sitter-php v0.23.11's queries/tags.scm:
; https://github.com/tree-sitter/tree-sitter-php/blob/v0.23.11/queries/tags.scm
; Upstream source license (MIT):
; https://github.com/tree-sitter/tree-sitter-php/blob/master/LICENSE
; Extended here with additional PHP definition captures for the basic tag style.
(namespace_definition
  name: (namespace_name) @name) @definition.module

(interface_declaration
  name: (name) @name) @definition.interface

(trait_declaration
  name: (name) @name) @definition.trait

(class_declaration
  name: (name) @name) @definition.class

(class_interface_clause [(name) (qualified_name)] @name) @reference.implementation

(property_declaration
  (property_element (variable_name (name) @name))) @definition.field

(const_element (name) @name) @definition.constant

(namespace_use_clause alias: (name) @name) @definition.alias
(namespace_use_clause !alias (qualified_name (name) @name)) @definition.alias
(namespace_use_clause !alias (name) @name) @definition.alias

; Assignments in the program or a braced namespace are global variables.
; Assignments inside routines are intentionally excluded.
(program
  (expression_statement
    (assignment_expression left: (variable_name (name) @name)))) @definition.variable
(namespace_definition
  body: (compound_statement
    (expression_statement
      (assignment_expression left: (variable_name (name) @name))))) @definition.variable

(function_definition
  name: (name) @name) @definition.function

(method_declaration
  name: (name) @name) @definition.function

(object_creation_expression
  [
    (qualified_name (name) @name)
    (variable_name (name) @name)
  ]) @reference.class

(function_call_expression
  function: [
    (qualified_name (name) @name)
    (variable_name (name)) @name
  ]) @reference.call

(scoped_call_expression
  name: (name) @name) @reference.call

(member_call_expression
  name: (name) @name) @reference.call
