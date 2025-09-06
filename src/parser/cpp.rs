use super::helper::{self, iterate_children, Break, Continue, LanguageContext, TagKindConfig};
use super::Parser;
use indexmap::IndexMap;
use tree_sitter::{Node, TreeCursor};

use crate::tag;

// Represents the type of scope for context tracking
#[derive(Debug)]
enum ScopeType {
    Namespace,
    Class,
    Struct,
    Union,
    Enum,
    Function,
}

/// Enhanced Context for C++ with scope tracking
struct CppContext<'a> {
    base: helper::Context<'a>,
    scope_stack: Vec<(ScopeType, String)>,
}

impl<'a> CppContext<'a> {
    fn new(
        source_code: &'a str,
        lines: Vec<Vec<u8>>,
        file_name: &'a str,
        tags: &'a mut Vec<tag::Tag>,
        tag_config: &'a TagKindConfig,
        user_config: &'a crate::config::Config,
    ) -> Self {
        Self {
            base: helper::Context {
                source_code,
                lines,
                file_name,
                tags,
                tag_config,
                user_config,
            },
            scope_stack: Vec::new(),
        }
    }

    // Build extension fields based on the current scope stack
    fn create_extension_fields(&self) -> IndexMap<String, String> {
        let mut fields = IndexMap::new();
        let mut namespace_path = Vec::new();

        for (scope_type, name) in &self.scope_stack {
            match scope_type {
                ScopeType::Namespace => namespace_path.push(name.clone()),
                ScopeType::Class => {
                    fields.insert(String::from("class"), name.clone());
                }
                ScopeType::Struct => {
                    fields.insert(String::from("struct"), name.clone());
                }
                ScopeType::Union => {
                    fields.insert(String::from("union"), name.clone());
                }
                ScopeType::Enum => {
                    fields.insert(String::from("enum"), name.clone());
                }
                ScopeType::Function => {
                    fields.insert(String::from("function"), name.clone());
                }
            }
        }

        if !namespace_path.is_empty() {
            fields.insert(String::from("namespace"), namespace_path.join("::"));
        }

        fields
    }
}

impl<'a> LanguageContext for CppContext<'a> {
    type ScopeType = ScopeType;

    fn get_tag_config(&self) -> &TagKindConfig {
        self.base.tag_config
    }

    fn push_scope(&mut self, scope_type: Self::ScopeType, name: String) {
        self.scope_stack.push((scope_type, name));
    }

    fn pop_scope(&mut self) -> Option<(Self::ScopeType, String)> {
        self.scope_stack.pop()
    }

    fn process_node(&mut self, cursor: &mut TreeCursor) -> Option<(Self::ScopeType, String)> {
        process_node(cursor, self)
    }
}

impl Parser {
    pub fn generate_cpp_tags_with_full_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        tag_config: &helper::TagKindConfig,
        user_config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        helper::generate_tags_with_config(
            &mut self.ts_parser,
            tree_sitter_cpp::LANGUAGE.into(),
            code,
            file_path_relative_to_tag_file,
            |source_code, lines, cursor, tags| {
                let mut context = CppContext::new(
                    source_code,
                    lines,
                    file_path_relative_to_tag_file,
                    tags,
                    tag_config,
                    user_config,
                );
                helper::walk_generic(cursor, &mut context);
            },
        )
    }
}

// Dispatches node processing based on kind, returns scope info if node defines one
fn process_node(cursor: &mut TreeCursor, context: &mut CppContext) -> Option<(ScopeType, String)> {
    let node = cursor.node();

    match node.kind() {
        "namespace_definition" => process_namespace(cursor, context),
        "class_specifier" => process_class(cursor, context),
        "struct_specifier" => process_struct(cursor, context),
        "union_specifier" => process_union(cursor, context),
        "enum_specifier" => process_enum(cursor, context),
        "function_definition" => process_function_definition(cursor, context),
        "declaration" => process_declaration(cursor, context),
        "field_declaration" => process_field_declaration(cursor, context),
        "preproc_def" => process_macro_definition(cursor, context),
        "type_definition" => process_typedef(cursor, context),
        _ => None,
    }
}

// --- Tag Creation Helper ---

fn create_tag(
    name: String,
    kind_char: &str,
    node: Node,
    context: &mut CppContext,
    extra_fields: Option<IndexMap<String, String>>,
) {
    if name.is_empty() || name == "_" {
        return;
    }

    if !context.base.tag_config.is_kind_enabled(kind_char) {
        return;
    }

    let row = node.start_position().row;
    let address = helper::address_string_from_line(row, &context.base);
    let mut extension_fields = IndexMap::new();

    // 1. Kind field (k)
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("kind")
    {
        extension_fields.insert(String::from("kind"), kind_char.to_string());
    }

    // 2. Line number (n)
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("line")
    {
        extension_fields.insert(String::from("line"), (row + 1).to_string());
    }

    // 3. Access field (a)
    if let Some(extras) = &extra_fields {
        if let Some(access) = extras.get("access") {
            if context
                .base
                .user_config
                .fields_config
                .is_field_enabled("access")
            {
                extension_fields.insert("access".to_string(), access.clone());
            }
        }
    }

    // 4. File field (f) - only add if file scope is enabled
    if context.base.user_config.extras_config.file_scope {
        extension_fields.insert(String::from("file"), String::new());
    }

    // 5. Signature field (S)
    if let Some(extras) = &extra_fields {
        if let Some(signature) = extras.get("signature") {
            if context
                .base
                .user_config
                .fields_config
                .is_field_enabled("signature")
            {
                extension_fields.insert("signature".to_string(), signature.clone());
            }
        }
    }

    // 6. Scope information (s)
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("scope")
        || context.base.user_config.extras_config.qualified
    {
        let scope_fields = context.create_extension_fields();
        extension_fields.extend(scope_fields);
    }

    // 7. Typeref field (t)
    if let Some(extras) = &extra_fields {
        if let Some(typeref) = extras.get("typeref") {
            extension_fields.insert("typeref".to_string(), typeref.clone());
        }
    }

    // 8. End position (e)
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("end")
    {
        extension_fields.insert(
            String::from("end"),
            (node.end_position().row + 1).to_string(),
        );
    }

    // Handle remaining extra fields
    if let Some(extras) = extra_fields {
        for (key, value) in extras {
            if matches!(key.as_str(), "access" | "signature" | "typeref") {
                continue;
            }

            match key.as_str() {
                "class" | "struct" | "union" | "enum" | "namespace" | "function" => {
                    if context
                        .base
                        .user_config
                        .fields_config
                        .is_field_enabled("scope")
                        || context.base.user_config.extras_config.qualified
                    {
                        extension_fields.insert(key, value);
                    }
                }
                _ => {
                    if context
                        .base
                        .user_config
                        .fields_config
                        .is_field_enabled("scope")
                        || context.base.user_config.extras_config.qualified
                    {
                        extension_fields.insert(key, value);
                    }
                }
            }
        }
    }

    context.base.tags.push(tag::Tag {
        name,
        file_name: context.base.file_name.to_string(),
        address,
        kind: Some(String::from(kind_char)),
        extension_fields: if extension_fields.is_empty() {
            None
        } else {
            Some(extension_fields)
        },
    });
}

// --- Specific Node Processors ---

fn process_namespace(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["namespace_identifier"]) {
        create_tag(name.clone(), "n", node, context, None);
        Some((ScopeType::Namespace, name))
    } else {
        None
    }
}

fn process_class(cursor: &mut TreeCursor, context: &mut CppContext) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["type_identifier"]) {
        create_tag(name.clone(), "c", node, context, None);
        Some((ScopeType::Class, name))
    } else {
        None
    }
}

fn process_struct(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["type_identifier"]) {
        create_tag(name.clone(), "s", node, context, None);
        Some((ScopeType::Struct, name))
    } else {
        None
    }
}

fn process_union(cursor: &mut TreeCursor, context: &mut CppContext) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["type_identifier"]) {
        create_tag(name.clone(), "u", node, context, None);
        Some((ScopeType::Union, name))
    } else {
        None
    }
}

fn process_enum(cursor: &mut TreeCursor, context: &mut CppContext) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut enum_name = String::new();
    let mut extra_fields = IndexMap::new();
    let mut enum_values = Vec::new();

    iterate_children!(cursor, |child_node| {
        match child_node.kind() {
            "type_identifier" => {
                enum_name = context.base.node_text(&child_node).to_string();
                Continue
            }
            "primitive_type" | "sized_type_specifier" => {
                // This is the base type (e.g., uint8_t in "enum Color : uint8_t")
                extra_fields.insert(
                    "typeref".to_string(),
                    format!("typename:{}", context.base.node_text(&child_node)),
                );
                Continue
            }
            "enumerator_list" => {
                // Process enum values
                iterate_children!(cursor, |enumerator_child| {
                    if enumerator_child.kind() == "enumerator" {
                        if let Some(enum_value_name) =
                            helper::get_node_name(cursor, &context.base, &["identifier"])
                        {
                            enum_values.push((enum_value_name, enumerator_child));
                        }
                    }
                    Continue
                });
                Continue
            }
            _ => Continue,
        }
    });

    if !enum_name.is_empty() {
        // Create the enum tag
        create_tag(
            enum_name.clone(),
            "g",
            node,
            context,
            if extra_fields.is_empty() {
                None
            } else {
                Some(extra_fields)
            },
        );

        // Create tags for enum values
        for (value_name, value_node) in enum_values {
            let mut value_fields = IndexMap::new();
            value_fields.insert("enum".to_string(), enum_name.clone());
            create_tag(value_name, "e", value_node, context, Some(value_fields));
        }

        Some((ScopeType::Enum, enum_name))
    } else {
        None
    }
}

fn process_function_definition(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut extra_fields = IndexMap::new();
    let mut fn_name = String::new();

    iterate_children!(cursor, |child_node| {
        match child_node.kind() {
            "primitive_type"
            | "type_identifier"
            | "qualified_identifier"
            | "sized_type_specifier" => {
                extra_fields.insert(
                    "typeref".to_string(),
                    format!("typename:{}", context.base.node_text(&child_node)),
                );
                Continue
            }
            "reference_declarator" => {
                iterate_children!(cursor, |ref_child| {
                    if ref_child.kind() == "function_declarator" {
                        fn_name = extract_function_name_from_declarator(
                            cursor,
                            context,
                            &mut extra_fields,
                        );
                        Break
                    } else {
                        Continue
                    }
                });
                Continue
            }
            "function_declarator" => {
                fn_name = extract_function_name_from_declarator(cursor, context, &mut extra_fields);
                Continue
            }
            _ => Continue,
        }
    });

    create_tag(
        fn_name.clone(),
        "f",
        node,
        context,
        if extra_fields.is_empty() {
            None
        } else {
            Some(extra_fields)
        },
    );
    Some((ScopeType::Function, fn_name))
}

fn extract_function_name_from_declarator(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
    extra_fields: &mut IndexMap<String, String>,
) -> String {
    let mut fn_name = String::new();

    iterate_children!(cursor, |declarator_child| {
        match declarator_child.kind() {
            "identifier" | "field_identifier" => {
                fn_name = context.base.node_text(&declarator_child).to_string();
                Break
            }
            "qualified_identifier" => {
                iterate_children!(cursor, |qualified_identier_child| {
                    match qualified_identier_child.kind() {
                        "namespace_identifier" => {
                            extra_fields.insert(
                                "class".to_string(),
                                context
                                    .base
                                    .node_text(&qualified_identier_child)
                                    .to_string(),
                            );
                            Continue
                        }
                        "identifier" | "destructor_name" | "operator_name" => {
                            let operator_text = context
                                .base
                                .node_text(&qualified_identier_child)
                                .to_string();

                            if operator_text.starts_with("operator") && operator_text.len() > 8 {
                                fn_name = format!("operator {}", &operator_text[8..]);
                            } else {
                                fn_name = operator_text.to_string();
                            }
                            Break
                        }
                        _ => Continue,
                    }
                });
                Break
            }
            "operator_name" => {
                fn_name = context.base.node_text(&declarator_child).to_string();
                Break
            }
            _ => Continue,
        }
    });

    fn_name
}

fn process_declaration(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let mut type_info = String::new();
    let mut variable_names = Vec::new();

    iterate_children!(cursor, |child_node| {
        match child_node.kind() {
            // Type specifiers
            "primitive_type" | "type_identifier" | "sized_type_specifier" => {
                type_info = context.base.node_text(&child_node).to_string();
                Continue
            }
            // Template types like Box<Box<int>>
            "template_type" => {
                type_info = context.base.node_text(&child_node).to_string();
                Continue
            }
            // Qualified types like std::string
            "qualified_identifier" => {
                type_info = context.base.node_text(&child_node).to_string();
                Continue
            }
            // Variable declarators
            "init_declarator" => {
                iterate_children!(cursor, |declarator_child| {
                    match declarator_child.kind() {
                        "identifier" => {
                            let var_name = context.base.node_text(&declarator_child).to_string();
                            variable_names.push((var_name, declarator_child));
                            Continue
                        }
                        "reference_declarator" => {
                            iterate_children!(cursor, |ref_child| {
                                if ref_child.kind() == "identifier" {
                                    let var_name = context.base.node_text(&ref_child).to_string();
                                    variable_names.push((var_name, ref_child));
                                }
                                Continue
                            });
                            Break
                        }
                        _ => Continue,
                    }
                });
                Continue
            }
            // Direct identifier (for simple declarations)
            "identifier" => {
                let var_name = context.base.node_text(&child_node).to_string();
                variable_names.push((var_name, child_node));
                Continue
            }
            _ => Continue,
        }
    });

    // Create tags for all found variables
    for (var_name, var_node) in variable_names {
        if !var_name.is_empty() && var_name != "_" {
            let mut extra_fields = IndexMap::new();

            if !type_info.is_empty() {
                extra_fields.insert("typeref".to_string(), format!("typename:{}", type_info));
            }

            create_tag(
                var_name,
                "v",
                var_node,
                context,
                if extra_fields.is_empty() {
                    None
                } else {
                    Some(extra_fields)
                },
            );
        }
    }

    None
}

fn process_field_declaration(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) =
        helper::get_node_name(cursor, &context.base, &["field_identifier", "identifier"])
    {
        let mut extra_fields = IndexMap::new();

        // Try to get the type information
        if let Some(type_info) = get_declaration_type(node, &context.base) {
            extra_fields.insert("typeref".to_string(), format!("typename:{}", type_info));
        }

        create_tag(
            name,
            "m",
            node,
            context,
            if extra_fields.is_empty() {
                None
            } else {
                Some(extra_fields)
            },
        );
    }
    None
}

fn process_macro_definition(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
        create_tag(name, "d", node, context, None);
    }
    None
}

fn process_typedef(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["type_identifier"]) {
        create_tag(name, "t", node, context, None);
    }
    None
}

// --- Helper Functions ---

fn get_declaration_type(decl_node: Node, context: &helper::Context) -> Option<String> {
    // Look for type information in declarations
    for i in 0..decl_node.child_count() {
        if let Some(child) = decl_node.child(i) {
            match child.kind() {
                "primitive_type"
                | "type_identifier"
                | "qualified_identifier"
                | "sized_type_specifier" => {
                    let type_text = context.node_text(&child).to_string();
                    if !type_text.is_empty() {
                        return Some(type_text);
                    }
                }
                _ => continue,
            }
        }
    }
    None
}
