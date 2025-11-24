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
    sequence_counter: u16,
    filename_hash: String,
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
            filename_hash: Self::calculate_filename_hash(file_name),
            base: helper::Context {
                source_code,
                lines,
                file_name,
                tags,
                tag_config,
                user_config,
            },
            scope_stack: Vec::new(),
            sequence_counter: 1,
        }
    }

    // Calculate djb2 hash of filename
    fn calculate_filename_hash(filename: &str) -> String {
        let mut hash: u32 = 5381;
        for byte in filename.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
        }
        format!("{:08x}", hash)
    }

    // Generate anonymous identifier
    fn generate_anonymous_name(&mut self, kind_id: u8) -> String {
        let name = format!(
            "__anon{}{:02x}{:02x}",
            self.filename_hash, self.sequence_counter, kind_id
        );
        self.sequence_counter += 1;
        name
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
        "preproc_function_def" => process_macro_function_definition(cursor, context),
        "type_definition" => process_typedef(cursor, context),
        "preproc_include" => process_preproc_include(cursor, context),
        "labeled_statement" => process_labeled_statement(cursor, context),
        "namespace_alias_definition" => process_namespace_alias_definition(cursor, context),
        "using_declaration" => process_using_declaration(cursor, context),
        "template_declaration" => process_template_declaration(cursor, context),
        // Module support is not released to crates.io
        // https://github.com/tree-sitter/tree-sitter-cpp/issues/341
        // "module_declaration" => process_module_declaration(cursor, context),
        // "module_partition" => process_module_partition(cursor, context),
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

// --- Helper Functions ---

fn process_named_item(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
    identifier_kinds: &[&str], // e.g., &["type_identifier"], &["identifier"]
    tag_kind: &str,            // e.g., "n", "c", "s", "u", "d", "t"
    scope_type: Option<ScopeType>, // Some(scope_type) for scoped items, None for non-scoped
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child_node| {
        if identifier_kinds.contains(&child_node.kind()) {
            name = context.base.node_text(&child_node).to_string();
            Break
        } else {
            Continue
        }
    });

    if !name.is_empty() {
        create_tag(name.clone(), tag_kind, node, context, None);
        if let Some(scope_type) = scope_type {
            Some((scope_type, name))
        } else {
            None
        }
    } else {
        None
    }
}

// --- Specific Node Processors ---

fn process_namespace(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    process_named_item(
        cursor,
        context,
        &["namespace_identifier"],
        "n",
        Some(ScopeType::Namespace),
    )
}

fn process_class(cursor: &mut TreeCursor, context: &mut CppContext) -> Option<(ScopeType, String)> {
    let mut name = "".to_string();

    iterate_children!(cursor, |child_node| {
        match child_node.kind() {
            "type_identifier" => {
                name = context.base.node_text(&child_node).to_string();
                create_tag(name.clone(), "c", child_node, context, None);
                Continue
            }
            _ => Continue,
        }
    });

    if name.is_empty() {
        None
    } else {
        Some((ScopeType::Class, name))
    }
}

fn process_struct(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    // Check if this struct declaration is local (inside a function)
    let is_local = context
        .scope_stack
        .iter()
        .any(|(scope_type, _)| matches!(scope_type, ScopeType::Function));

    // Skip processing local struct declarations entirely
    if is_local {
        return None;
    }

    let result = process_named_item(
        cursor,
        context,
        &["type_identifier"],
        "s",
        Some(ScopeType::Struct),
    );

    // Handle anonymous struct
    if result.is_none() {
        let anon_name = context.generate_anonymous_name(8);
        let node = cursor.node();
        create_tag(anon_name.clone(), "s", node, context, None);
        return Some((ScopeType::Struct, anon_name));
    }

    result
}

fn process_union(cursor: &mut TreeCursor, context: &mut CppContext) -> Option<(ScopeType, String)> {
    process_named_item(
        cursor,
        context,
        &["type_identifier"],
        "u",
        Some(ScopeType::Union),
    )
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
            "identifier" | "field_identifier" | "destructor_name" => {
                fn_name = context.base.node_text(&declarator_child).to_string();
                Continue
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
                            Continue
                        }
                        _ => Continue,
                    }
                });
                Continue
            }
            "operator_name" => {
                fn_name = context.base.node_text(&declarator_child).to_string();
                Continue
            }
            "parameter_list" => {
                process_parameter_list(cursor, context, &fn_name);
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
    let mut is_extern = false;
    iterate_children!(cursor, |child_node| {
        match child_node.kind() {
            "storage_class_specifier" => {
                if context.base.node_text(&child_node) == "extern" {
                    is_extern = true;
                }
                Continue
            }
            // Type specifiers
            "primitive_type"
            | "type_identifier"
            | "sized_type_specifier"
            | "template_type"
            | "qualified_identifier" => {
                type_info = format!(
                    "typename:{}",
                    context.base.node_text(&child_node).to_string()
                );
                Continue
            }
            // Handle struct declarations like "struct rectangle r;"
            "struct_specifier" => {
                iterate_children!(cursor, |struct_child| {
                    if struct_child.kind() == "type_identifier" {
                        type_info = format!("struct:{}", context.base.node_text(&struct_child));
                        Break
                    } else {
                        Continue
                    }
                });
                Continue
            }
            // Function declarator - handle function prototypes
            "function_declarator" => {
                let fn_name =
                    extract_function_name_from_declarator(cursor, context, &mut IndexMap::new());
                if !fn_name.is_empty() {
                    let mut proto_fields = IndexMap::new();
                    if !type_info.is_empty() {
                        proto_fields.insert("typeref".to_string(), type_info.clone());
                    }
                    create_tag(fn_name, "p", child_node, context, Some(proto_fields));
                }
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
            // Declarator (for simple variable declarations)
            "declarator" => {
                iterate_children!(cursor, |decl_child| {
                    match decl_child.kind() {
                        "identifier" => {
                            let var_name = context.base.node_text(&decl_child).to_string();
                            variable_names.push((var_name, decl_child));
                        }
                        _ => {}
                    }
                    Continue
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
            // Determine if this is a local variable (inside function) or global variable
            let is_local = context
                .scope_stack
                .iter()
                .any(|(scope_type, _)| matches!(scope_type, ScopeType::Function));

            let kind = if is_extern {
                "x"
            } else if is_local {
                "l"
            } else {
                "v"
            };

            let mut extra_fields = IndexMap::new();

            if !type_info.is_empty() {
                extra_fields.insert("typeref".to_string(), type_info.clone());
            }

            create_tag(
                var_name,
                kind,
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
    let mut field_name = String::new();
    let mut type_info = String::new();
    let mut is_method_prototype = false;
    let mut is_pointer = false;

    iterate_children!(cursor, |child_node| {
        match child_node.kind() {
            "field_identifier" | "identifier" => {
                field_name = context.base.node_text(&child_node).to_string();
                Continue
            }
            "pointer_declarator" => {
                is_pointer = true;
                iterate_children!(cursor, |ptr_child| {
                    if ptr_child.kind() == "field_identifier" {
                        field_name = context.base.node_text(&ptr_child).to_string();
                        Break
                    } else {
                        Continue
                    }
                });
                Continue
            }
            "struct_specifier" => {
                iterate_children!(cursor, |struct_child| {
                    if struct_child.kind() == "type_identifier" {
                        type_info = format!("struct:{}", context.base.node_text(&struct_child));
                        Break
                    } else {
                        Continue
                    }
                });
                Continue
            }
            "primitive_type"
            | "type_identifier"
            | "qualified_identifier"
            | "sized_type_specifier" => {
                type_info = context.base.node_text(&child_node).to_string();
                Continue
            }
            "function_declarator" => {
                is_method_prototype = true;
                field_name = extract_method_name_from_declarator(cursor, context);
                Continue
            }
            "reference_declarator" => {
                iterate_children!(cursor, |ref_child| {
                    if ref_child.kind() == "function_declarator" {
                        is_method_prototype = true;
                        field_name = extract_method_name_from_declarator(cursor, context);
                    }
                    Continue
                });
                Continue
            }
            _ => Continue,
        }
    });

    if !field_name.is_empty() {
        let mut extra_fields = IndexMap::new();

        if !type_info.is_empty() {
            let typeref_value = if is_pointer {
                format!("{} *", type_info)
            } else {
                format!("typename:{}", type_info)
            };
            extra_fields.insert("typeref".to_string(), typeref_value);
        }

        // Add struct scope information for members
        if let Some((ScopeType::Struct, struct_name)) = context
            .scope_stack
            .iter()
            .rev()
            .find(|(scope_type, _)| matches!(scope_type, ScopeType::Struct))
        {
            if context
                .base
                .user_config
                .fields_config
                .is_field_enabled("scope")
            {
                extra_fields.insert("struct".to_string(), struct_name.clone());
            }
        }

        let tag_kind = if is_method_prototype {
            "p" // prototype
        } else {
            "m" // member
        };

        create_tag(
            field_name,
            tag_kind,
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
    process_named_item(cursor, context, &["identifier"], "d", None)
}

fn process_macro_function_definition(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let mut macro_name = String::new();
    iterate_children!(cursor, |child| {
        match child.kind() {
            "identifier" => {
                macro_name = context.base.node_text(&child).to_string();
                create_tag(macro_name.clone(), "d", child, context, None);
                Continue
            }
            "preproc_params" => {
                iterate_children!(cursor, |params_child| {
                    match params_child.kind() {
                        "identifier" => {
                            let param_name = context.base.node_text(&params_child).to_string();
                            let mut fields = IndexMap::new();
                            fields.insert("macro".to_string(), macro_name.clone());
                            create_tag(param_name, "D", params_child, context, Some(fields));
                            Continue
                        }
                        _ => Continue,
                    }
                });
                Break
            }
            _ => Continue,
        }
    });

    None
}

fn process_typedef(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut typedef_name = String::new();
    let mut type_info = String::new();
    let mut found_anonymous_struct = false;

    iterate_children!(cursor, |child_node| {
        match child_node.kind() {
            "type_identifier" => {
                typedef_name = context.base.node_text(&child_node).to_string();
                Continue
            }
            "function_declarator" => {
                iterate_children!(cursor, |func_child| {
                    match func_child.kind() {
                        "parenthesized_declarator" => {
                            iterate_children!(cursor, |paren_child| {
                                if paren_child.kind() == "pointer_declarator" {
                                    iterate_children!(cursor, |ptr_child| {
                                        if ptr_child.kind() == "type_identifier" {
                                            typedef_name =
                                                context.base.node_text(&ptr_child).to_string();
                                        }
                                        Continue
                                    });
                                }
                                Continue
                            });
                            Continue
                        }
                        "parameter_list" => {
                            let params = context.base.node_text(&func_child);
                            type_info = format!("typename:void (*){}", params);
                            Continue
                        }
                        _ => Continue,
                    }
                });
                Continue
            }
            "primitive_type" | "sized_type_specifier" | "qualified_identifier" => {
                type_info = format!("typename:{}", context.base.node_text(&child_node));
                Continue
            }
            "struct_specifier" => {
                iterate_children!(cursor, |struct_child| {
                    match struct_child.kind() {
                        "type_identifier" => {
                            type_info = format!("struct:{}", context.base.node_text(&struct_child));
                            Continue
                        }
                        "field_declaration_list" => {
                            // This indicates an anonymous struct
                            found_anonymous_struct = true;
                            Continue
                        }
                        _ => Continue,
                    }
                });
                Continue
            }
            _ => Continue,
        }
    });

    // Handle anonymous struct typedef
    if found_anonymous_struct && type_info.is_empty() && !typedef_name.is_empty() {
        let anon_name = context.generate_anonymous_name(8);
        type_info = format!("struct:{}", anon_name);
    }

    let mut extra_fields = IndexMap::new();
    if !type_info.is_empty() {
        extra_fields.insert("typeref".to_string(), type_info);
    }

    create_tag(
        typedef_name,
        "t",
        node,
        context,
        if extra_fields.is_empty() {
            None
        } else {
            Some(extra_fields)
        },
    );

    None
}

fn extract_method_name_from_declarator(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> String {
    let mut method_name = String::new();

    iterate_children!(cursor, |declarator_child| {
        match declarator_child.kind() {
            "identifier" | "field_identifier" => {
                method_name = context.base.node_text(&declarator_child).to_string();
                Break
            }
            "operator_name" => {
                let operator_text = context.base.node_text(&declarator_child).to_string();
                if operator_text.starts_with("operator") && operator_text.len() > 8 {
                    method_name = format!("operator {}", &operator_text[8..]);
                } else {
                    method_name = operator_text;
                }
                Break
            }
            "destructor_name" => {
                method_name = context.base.node_text(&declarator_child).to_string();
                Break
            }
            _ => Continue,
        }
    });

    method_name
}

fn process_preproc_include(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    iterate_children!(cursor, |child_node| {
        if child_node.kind() == "string_literal" || child_node.kind() == "system_lib_string" {
            let path = context.base.node_text(&child_node);
            let path = path.trim_matches(|c| c == '"' || c == '<' || c == '>');
            create_tag(path.to_string(), "h", node, context, None);
        }
        Continue
    });

    None
}

fn process_labeled_statement(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    iterate_children!(cursor, |child_node| {
        if child_node.kind() == "statement_identifier" {
            let label_name = context.base.node_text(&child_node).to_string();
            create_tag(label_name, "L", node, context, None);
            Break
        } else {
            Continue
        }
    });

    None
}

fn process_namespace_alias_definition(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut alias_name = String::new();
    iterate_children!(cursor, |child_node| {
        if child_node.kind() == "namespace_identifier" {
            alias_name = context.base.node_text(&child_node).to_string();
            // First one is the alias name
            Break
        } else {
            Continue
        }
    });
    if !alias_name.is_empty() {
        create_tag(alias_name, "A", node, context, None);
    }

    None
}

fn process_using_declaration(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();

    iterate_children!(cursor, |child_node| {
        match child_node.kind() {
            "qualified_identifier" | "namespace_identifier" | "identifier" => {
                create_tag(
                    context.base.node_text(&child_node).to_string(),
                    "U",
                    node,
                    context,
                    None,
                );
                Break
            }
            _ => Continue,
        }
    });

    None
}

fn process_template_declaration(
    cursor: &mut TreeCursor,
    context: &mut CppContext,
) -> Option<(ScopeType, String)> {
    iterate_children!(cursor, |child| {
        match child.kind() {
            "template_parameter_list" => {
                iterate_children!(cursor, |param_child| {
                    match param_child.kind() {
                        "parameter_declaration"
                        | "type_parameter_declaration"
                        | "optional_type_parameter_declaration" => {
                            iterate_children!(cursor, |name_child| {
                                if name_child.kind() == "type_identifier" {
                                    let name = context.base.node_text(&name_child).to_string();
                                    create_tag(name, "Z", name_child, context, None);
                                }
                                Continue
                            });
                        }
                        "template_template_parameter_declaration" => {
                            if let Some(name_node) = param_child.child_by_field_name("name") {
                                let name = context.base.node_text(&name_node).to_string();
                                create_tag(name, "Z", name_node, context, None);
                            }
                        }
                        _ => {}
                    }
                    Continue
                });
                Break
            }
            _ => Continue,
        }
    });

    None
}

// fn process_module_declaration(
//     cursor: &mut TreeCursor,
//     context: &mut CppContext,
// ) -> Option<(ScopeType, String)> {
//     process_named_item(cursor, context, &["identifier"], "M", None)
// }

// fn process_module_partition(
//     cursor: &mut TreeCursor,
//     context: &mut CppContext,
// ) -> Option<(ScopeType, String)> {
//     process_named_item(cursor, context, &["identifier"], "P", None)
// }

fn process_parameter_list(cursor: &mut TreeCursor, context: &mut CppContext, fn_name: &String) {
    iterate_children!(cursor, |param_child| {
        if param_child.kind() == "parameter_declaration" {
            let mut name = String::new();
            let mut type_info = String::new();
            let mut name_node_for_tag = param_child;

            iterate_children!(cursor, |decl_child| {
                match decl_child.kind() {
                    "primitive_type"
                    | "type_identifier"
                    | "qualified_identifier"
                    | "sized_type_specifier" => {
                        type_info = context.base.node_text(&decl_child).to_string();
                        Continue
                    }
                    "identifier" => {
                        name = context.base.node_text(&decl_child).to_string();
                        name_node_for_tag = decl_child;
                        Continue
                    }
                    "declarator" | "pointer_declarator" | "reference_declarator" => {
                        let mut current_node = decl_child;
                        loop {
                            if current_node.kind() == "identifier" {
                                name = context.base.node_text(&current_node).to_string();
                                name_node_for_tag = current_node;
                                break;
                            }

                            if let Some(child) = current_node.child_by_field_name("declarator") {
                                current_node = child;
                                continue;
                            }

                            // Fallback for unnamed declarators/identifiers
                            let mut found_next = false;
                            let mut temp_cursor = current_node.walk();
                            for child in current_node.children(&mut temp_cursor) {
                                match child.kind() {
                                    "identifier"
                                    | "declarator"
                                    | "pointer_declarator"
                                    | "reference_declarator" => {
                                        current_node = child;
                                        found_next = true;
                                        break;
                                    }
                                    _ => {}
                                }
                            }

                            if !found_next {
                                break;
                            }
                        }
                        Continue
                    }
                    _ => Continue,
                }
            });

            if !name.is_empty() {
                let mut extra_fields = IndexMap::new();
                match context.scope_stack.last() {
                    Some((ScopeType::Class, class_name)) => extra_fields.insert(
                        "function".to_string(),
                        format!("{}::{}", class_name, fn_name),
                    ),
                    _ => extra_fields.insert("function".to_string(), fn_name.clone()),
                };
                // extra_fields.insert("function".to_string(), fn_name.clone());
                if !type_info.is_empty() {
                    extra_fields.insert("typeref".to_string(), format!("typename:{}", type_info));
                }
                create_tag(
                    name,
                    "z",
                    name_node_for_tag,
                    context,
                    Some(extra_fields.clone()),
                );
            }
        }
        Continue
    });
}
