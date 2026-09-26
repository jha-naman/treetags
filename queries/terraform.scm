; HCL block labels are string literals. Capture their contents so tag names
; match the walker without surrounding quotes.
; tree-sitter-tags only accepts reserved capture names. @doc holds the block
; type for the predicates; basic tags do not expose documentation.
(block
  (identifier) @doc
  (string_lit (template_literal) @name)
  (#eq? @doc "variable")) @definition.variable

(block
  (identifier) @doc
  (string_lit (template_literal) @name)
  (#eq? @doc "module")) @definition.module

(block
  (identifier) @doc
  (string_lit (template_literal) @name)
  (#eq? @doc "output")) @definition.output

(block
  (identifier) @doc
  (string_lit (template_literal) @name)
  (#eq? @doc "provider")) @definition.provider

(block
  (identifier) @doc
  (string_lit)
  (string_lit (template_literal) @name)
  (#eq? @doc "data")) @definition.data

(block
  (identifier) @doc
  (string_lit)
  (string_lit (template_literal) @name)
  (#eq? @doc "resource")) @definition.resource

(block
  (identifier) @doc
  (body (attribute (identifier) @name) @definition.local)
  (#eq? @doc "locals"))

; Root attributes cover variable assignments in .tfvars files.
(config_file
  (body (attribute (identifier) @name) @definition.variable))
