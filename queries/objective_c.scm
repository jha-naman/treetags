; Basic definitions from the same downloaded Objective-C grammar as the walker.

(preproc_def name: (identifier) @name) @definition.macro
(preproc_function_def name: (identifier) @name) @definition.macro

(class_interface . (identifier) @name) @definition.interface
(class_implementation . (identifier) @name) @definition.implementation
(protocol_declaration . (identifier) @name) @definition.protocol

; The grammar splits keyword selectors into separate identifiers. A query can
; expose the first selector component; the walker builds the complete selector.
(method_declaration (method_type) . (identifier) @name) @definition.method
(method_definition (method_type) . (identifier) @name) @definition.method

(property_declaration
  (struct_declaration (struct_declarator (identifier) @name))) @definition.property
(property_declaration
  (struct_declaration (struct_declarator
    (pointer_declarator declarator: (identifier) @name)))) @definition.property

(instance_variable
  (struct_declaration (struct_declarator (identifier) @name))) @definition.ivar
(instance_variable
  (struct_declaration (struct_declarator
    (pointer_declarator declarator: (identifier) @name)))) @definition.ivar

(struct_specifier name: (type_identifier) @name body: (field_declaration_list)) @definition.struct
(union_specifier name: (type_identifier) @name body: (field_declaration_list)) @definition.union
(enum_specifier name: (type_identifier) @name) @definition.enum
(enumerator name: (identifier) @name) @definition.enumerator
(field_declaration declarator: (field_identifier) @name) @definition.member

(type_definition declarator: (type_identifier) @name) @definition.typedef
(type_definition declarator: (pointer_declarator declarator: (type_identifier) @name)) @definition.typedef
(type_definition declarator: (function_declarator
  declarator: (parenthesized_declarator
    (pointer_declarator declarator: (type_identifier) @name)))) @definition.typedef

(function_definition declarator: (function_declarator declarator: (identifier) @name)) @definition.function
(function_definition declarator: (pointer_declarator
  declarator: (function_declarator declarator: (identifier) @name))) @definition.function

(declaration declarator: (identifier) @name) @definition.variable
(declaration declarator: (pointer_declarator declarator: (identifier) @name)) @definition.variable
(declaration declarator: (init_declarator declarator: (identifier) @name)) @definition.variable
(declaration declarator: (init_declarator
  declarator: (pointer_declarator declarator: (identifier) @name))) @definition.variable
