(signature name: (variable) @name) @definition.function

(patterns (constructor) @name) @definition.function

(data_type name: (name) @name) @definition.class

(record name: (constructor) @name) @definition.method

(class name: (name) @name) @definition.class

(signature name: (prefix_id (operator) @name)) @definition.method

(instance
  name: (name)
  patterns: (type_patterns (name) @name)) @definition.class

