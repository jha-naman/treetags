(mod_item
  name: (identifier) @name) @definition.module

(struct_item
  name: (type_identifier) @name) @definition.class

(enum_item
  name: (type_identifier) @name) @definition.class

(union_item
  name: (type_identifier) @name) @definition.class

(trait_item
  name: (type_identifier) @name) @definition.interface

(impl_item
  trait: (type_identifier)? @name
  type: (_) @name) @definition.implementation

(function_item
  name: (identifier) @name) @definition.function

(function_signature_item
  name: (identifier) @name) @definition.function

(macro_definition
  name: (identifier) @name) @definition.macro

(field_declaration
  name: (field_identifier) @name) @definition.field

(const_item
  name: (identifier) @name) @definition.constant

(static_item
  name: (identifier) @name) @definition.constant

(type_item
  name: (type_identifier) @name) @definition.type

(enum_variant
  name: (identifier) @name) @definition.enumerator

(associated_type
  name: (type_identifier) @name) @definition.type
