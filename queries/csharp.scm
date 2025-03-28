;; Query code is owned and licensed by https://github.com/tree-sitter/tree-sitter-c-sharp/blob/master/LICENSE

(class_declaration name: (identifier) @name) @definition.class

(property_declaration name: (identifier) @name) @definition.class.property

(constructor_declaration name: (identifier) @name) @definition.class.constructor

(field_declaration (variable_declaration (variable_declarator name: (identifier) @name))) @definition.class.field

(class_declaration (base_list (_) @name)) @reference.class

(interface_declaration name: (identifier) @name) @definition.interface

(interface_declaration (base_list (_) @name)) @reference.interface

(method_declaration name: (identifier) @name) @definition.method

(global_statement (local_function_statement name: (identifier) @name)) @definition.function

(object_creation_expression type: (identifier) @name) @reference.class

(type_parameter_constraints_clause (identifier) @name) @reference.class

(type_parameter_constraint (type type: (identifier) @name)) @reference.class

(variable_declaration type: (identifier) @name) @reference.class

(invocation_expression function: (member_access_expression name: (identifier) @name)) @reference.send

(namespace_declaration name: (identifier) @name) @definition.module
(namespace_declaration (qualified_name) @name) @definition.module

(record_declaration name: (identifier) @name) @definition.type

(enum_declaration name: (identifier) @name) @definition.enum

(enum_member_declaration name: (identifier) @name) @definition.enum.member

(delegate_declaration name: (identifier) @name) @definition.delegate
(event_field_declaration (variable_declaration (variable_declarator name: (identifier) @name))) @definition.event
