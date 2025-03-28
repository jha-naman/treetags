;; Query code is owned and licensed by https://github.com/tree-sitter/tree-sitter-c-sharp/blob/master/LICENSE

(class_declaration name: (identifier) @name) @definition.class

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

(record_declaration name: (identifier) @name) @definition.type

(enum_declaration name: (identifier) @name) @definition.enum

