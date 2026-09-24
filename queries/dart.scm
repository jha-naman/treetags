; Basic Dart definitions from the downloaded grammar shared with the walker.

(class_declaration name: (identifier) @name) @definition.class
(class_declaration (mixin_application_class . (identifier) @name)) @definition.class
(mixin_declaration name: (identifier) @name) @definition.mixin
(enum_declaration name: (identifier) @name) @definition.enum
(enum_constant name: (identifier) @name) @definition.constant
(extension_declaration name: (identifier) @name) @definition.extension
(extension_declaration !name class: (type (type_identifier) @name)) @definition.extension
(extension_type_declaration name: (extension_type_name . (identifier) @name)) @definition.type

(type_alias . (type_identifier) @name) @definition.type

(function_declaration signature: (function_signature name: (identifier) @name)) @definition.function
(local_function_declaration (function_signature name: (identifier) @name)) @definition.function
(getter_declaration signature: (getter_signature name: (identifier) @name)) @definition.property
(setter_declaration signature: (setter_signature name: (identifier) @name)) @definition.property

(method_declaration signature: (method_signature (function_signature name: (identifier) @name))) @definition.method
(method_declaration signature: (method_signature (getter_signature name: (identifier) @name))) @definition.property
(method_declaration signature: (method_signature (setter_signature name: (identifier) @name))) @definition.property
(declaration (function_signature name: (identifier) @name)) @definition.method
(declaration (getter_signature name: (identifier) @name)) @definition.property
(declaration (setter_signature name: (identifier) @name)) @definition.property

; A named constructor has two name fields. Use its second identifier as the
; navigation target, while the one-name form captures the class name.
(declaration (constructor_signature name: (identifier) @name . parameters: (formal_parameter_list))) @definition.constructor
(declaration (constructor_signature name: (identifier) name: (identifier) @name)) @definition.constructor
(declaration (constant_constructor_signature name: (identifier) @name)) @definition.constructor
(declaration (redirecting_factory_constructor_signature name: (identifier) name: (identifier) @name)) @definition.constructor
(method_declaration signature: (method_signature (factory_constructor_signature name: (identifier) name: (identifier) @name))) @definition.constructor
(method_declaration signature: (method_signature (operator_signature operator: (binary_operator) @name))) @definition.operator

(top_level_variable_declaration (initialized_identifier_list (initialized_identifier name: (identifier) @name))) @definition.variable
(top_level_variable_declaration (static_final_declaration_list (static_final_declaration name: (identifier) @name))) @definition.variable
(declaration (initialized_identifier_list (initialized_identifier name: (identifier) @name))) @definition.field
(declaration (static_final_declaration_list (static_final_declaration name: (identifier) @name))) @definition.field
(extension_type_representation name: (identifier) @name) @definition.field
