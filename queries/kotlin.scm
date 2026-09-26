; Basic Kotlin definitions from the downloaded grammar shared with the walker.

(package_header (identifier) @name) @definition.package

(class_declaration "class" (type_identifier) @name) @definition.class
(class_declaration "interface" (type_identifier) @name) @definition.interface
(object_declaration (type_identifier) @name) @definition.object
(type_alias (type_identifier) @name) @definition.type

(function_declaration (simple_identifier) @name) @definition.function

(property_declaration
  (variable_declaration (simple_identifier) @name)) @definition.variable
(property_declaration
  (multi_variable_declaration
    (variable_declaration (simple_identifier) @name))) @definition.variable
(class_parameter
  (binding_pattern_kind)
  (simple_identifier) @name) @definition.variable
