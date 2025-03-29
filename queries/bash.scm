(function_definition name: (word) @name) @definition.method

(heredoc_redirect (heredoc_start) @name) @definition.heredoc

(
  (command
    name: (command_name
      (word) @doc)
    argument: (concatenation
      (word) @name))
  (#eq? @doc "alias")
) @definition.alias
