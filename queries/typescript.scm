; TypeScript definitions for basic tags.

[
  (function_declaration name: (identifier) @name)
  (generator_function_declaration name: (identifier) @name)
  (function_signature name: (identifier) @name)
] @definition.function

[
  (class_declaration name: (type_identifier) @name)
  (abstract_class_declaration name: (type_identifier) @name)
] @definition.class

(interface_declaration
  name: (type_identifier) @name) @definition.interface

(type_alias_declaration
  name: (type_identifier) @name) @definition.type

(enum_declaration
  name: (identifier) @name) @definition.enum

(enum_body
  name: (property_identifier) @name @definition.constant)

(enum_assignment
  name: (property_identifier) @name) @definition.constant

(import_alias
  . (identifier) @name) @definition.alias

[
  (module name: (identifier) @name)
  (internal_module name: (identifier) @name)
] @definition.module

(variable_declarator
  name: (identifier) @name) @definition.variable

[
  (method_definition name: (property_identifier) @name)
  (method_signature name: (property_identifier) @name)
  (abstract_method_signature name: (property_identifier) @name)
] @definition.method

(public_field_definition
  name: (property_identifier) @name) @definition.property

(property_signature
  name: (property_identifier) @name) @definition.property
