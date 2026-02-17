(class_declaration
  (type_identifier) @name) @definition.class

(object_declaration
  (type_identifier) @name) @definition.class

(function_declaration
  (simple_identifier) @name) @definition.function

(function_declaration
  (modifiers
    (member_modifier))
  (simple_identifier) @name) @definition.method

(enum_entry
  (simple_identifier) @name) @definition.constant

(class_declaration
  (enum_class_body
    (enum_entry
      (simple_identifier) @name) @definition.constant))
