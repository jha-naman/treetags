use super::helper::{self, iterate_children, Break, Continue, LanguageContext, TagKindConfig};
use super::Parser;
use indexmap::IndexMap;
use tree_sitter::{Node, TreeCursor};

use crate::tag;

#[derive(Debug)]
enum ScopeType {
    Class,
    Interface,
    Enum,
    Module,
    Function,
}

struct TypeScriptContext<'a> {
    base: helper::Context<'a>,
    scope_stack: Vec<(ScopeType, String)>,
}

impl<'a> TypeScriptContext<'a> {
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
}

impl<'a> LanguageContext for TypeScriptContext<'a> {
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
    pub fn generate_typescript_tags_with_full_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        tag_config: &helper::TagKindConfig,
        user_config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        helper::generate_tags_with_config(
            &mut self.ts_parser,
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            code,
            file_path_relative_to_tag_file,
            |source_code, lines, cursor, tags| {
                let mut context = TypeScriptContext::new(
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

fn process_node(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            process_function_declaration(cursor, context)
        }
        "class_declaration" => process_class_declaration(cursor, context),
        "interface_declaration" => process_interface_declaration(cursor, context),
        "enum_declaration" => process_enum_declaration(cursor, context),
        "module" => process_module(cursor, context),
        "method_definition" => process_method_definition(cursor, context),
        "method_signature" => process_method_signature(cursor, context),
        "variable_declarator" => process_variable_declarator(cursor, context),
        "type_alias_declaration" => process_type_alias_declaration(cursor, context),
        "public_field_definition" => process_public_field_definition(cursor, context),
        "property_signature" => process_property_signature(cursor, context),
        "enum_body" => process_enum_body(cursor, context),
        "required_parameter" | "optional_parameter" => process_parameter(cursor, context),
        _ => None,
    }
}

fn create_tag(
    name: String,
    kind: &str,
    node: Node,
    context: &mut TypeScriptContext,
    extra_fields: Option<IndexMap<String, String>>,
) {
    if !context.base.tag_config.is_kind_enabled(kind) {
        return;
    }

    let row = node.start_position().row;
    let address = helper::address_string_from_line(row, &context.base);
    let mut extension_fields = IndexMap::new();

    // Kind
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("kind")
    {
        extension_fields.insert("kind".to_string(), kind.to_string());
    }

    // Line
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("line")
    {
        extension_fields.insert("line".to_string(), (row + 1).to_string());
    }

    // Roles
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("roles")
    {
        extension_fields.insert("roles".to_string(), "def".to_string());
    }

    if let Some(extras) = extra_fields {
        for (k, v) in extras {
            extension_fields.insert(k, v);
        }
    }

    // Scope
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("scope")
    {
        if let Some((scope_type, scope_name)) = context.scope_stack.last() {
            match scope_type {
                ScopeType::Class => {
                    extension_fields.insert("class".to_string(), scope_name.clone());
                }
                ScopeType::Interface => {
                    extension_fields.insert("interface".to_string(), scope_name.clone());
                }
                ScopeType::Enum => {
                    extension_fields.insert("enum".to_string(), scope_name.clone());
                }
                ScopeType::Module => {
                    extension_fields.insert("module".to_string(), scope_name.clone());
                }
                ScopeType::Function => {
                    extension_fields.insert("function".to_string(), scope_name.clone());
                }
            }
        }
    }

    // End
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("end")
    {
        extension_fields.insert("end".to_string(), (node.end_position().row + 1).to_string());
    }

    context.base.tags.push(tag::Tag {
        name,
        file_name: context.base.file_name.to_string(),
        address,
        kind: Some(kind.to_string()),
        extension_fields: if extension_fields.is_empty() {
            None
        } else {
            Some(extension_fields)
        },
    });
}

fn process_function_declaration(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "identifier" {
            name = context.base.node_text(&child).to_string();
            Break
        } else {
            Continue
        }
    });

    if !name.is_empty() {
        let kind = if node.kind() == "generator_function_declaration" {
            "G"
        } else {
            "f"
        };
        create_tag(name.clone(), kind, node, context, None);
        Some((ScopeType::Function, name))
    } else {
        None
    }
}

fn process_class_declaration(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "type_identifier" {
            name = context.base.node_text(&child).to_string();
            Break
        } else {
            Continue
        }
    });

    if !name.is_empty() {
        create_tag(name.clone(), "c", node, context, None);
        Some((ScopeType::Class, name))
    } else {
        None
    }
}

fn process_interface_declaration(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "type_identifier" {
            name = context.base.node_text(&child).to_string();
            Break
        } else {
            Continue
        }
    });

    if !name.is_empty() {
        create_tag(name.clone(), "i", node, context, None);
        Some((ScopeType::Interface, name))
    } else {
        None
    }
}

fn process_enum_declaration(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "identifier" {
            name = context.base.node_text(&child).to_string();
            Break
        } else {
            Continue
        }
    });

    if !name.is_empty() {
        create_tag(name.clone(), "g", node, context, None);
        Some((ScopeType::Enum, name))
    } else {
        None
    }
}

fn process_module(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "identifier" || child.kind() == "string" {
            name = context.base.node_text(&child).to_string();
            if name.starts_with('"') || name.starts_with('\'') {
                name = name[1..name.len() - 1].to_string();
            }
            Break
        } else {
            Continue
        }
    });

    if !name.is_empty() {
        create_tag(name.clone(), "n", node, context, None);
        Some((ScopeType::Module, name))
    } else {
        None
    }
}

fn process_method_definition(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut access = "public";

    iterate_children!(cursor, |child| {
        match child.kind() {
            "property_identifier" | "number" | "string" => {
                name = context.base.node_text(&child).to_string();
                if name.starts_with('"') || name.starts_with('\'') {
                    name = name[1..name.len() - 1].to_string();
                }
                Continue
            }
            "accessibility_modifier" => {
                access = context.base.node_text(&child);
                Continue
            }
            _ => Continue,
        }
    });

    if !name.is_empty() {
        let mut extras = IndexMap::new();
        if context
            .base
            .user_config
            .fields_config
            .is_field_enabled("access")
        {
            extras.insert("access".to_string(), access.to_string());
        }

        create_tag(name.clone(), "m", node, context, Some(extras));

        Some((ScopeType::Function, name))
    } else {
        None
    }
}

fn process_method_signature(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let access = "public";

    iterate_children!(cursor, |child| {
        if child.kind() == "property_identifier" || child.kind() == "string" {
            name = context.base.node_text(&child).to_string();
            if name.starts_with('"') || name.starts_with('\'') {
                name = name[1..name.len() - 1].to_string();
            }
            Break
        } else {
            Continue
        }
    });

    if !name.is_empty() {
        let mut extras = IndexMap::new();
        if context
            .base
            .user_config
            .fields_config
            .is_field_enabled("access")
        {
            extras.insert("access".to_string(), access.to_string());
        }

        create_tag(name, "m", node, context, Some(extras));
    }
    None
}

fn process_variable_declarator(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut is_function = false;

    iterate_children!(cursor, |child| {
        if cursor.field_name() == Some("name") {
            name = context.base.node_text(&child).to_string();
            Continue
        } else if cursor.field_name() == Some("value") {
            match child.kind() {
                "arrow_function" | "function_expression" => {
                    is_function = true;
                }
                _ => {}
            }
            Continue
        } else {
            Continue
        }
    });

    if !name.is_empty() {
        let kind = if is_function {
            "f"
        } else {
            let is_local = context
                .scope_stack
                .iter()
                .any(|(s, _)| matches!(s, ScopeType::Function));

            if is_local {
                "l"
            } else {
                let mut is_const = false;
                if let Some(_parent) = node.parent() {
                    let mut parent_cursor = cursor.clone();
                    parent_cursor.goto_parent(); // Go to variable_declarator's parent (lexical_declaration)
                    if parent_cursor.goto_first_child() {
                        if context.base.node_text(&parent_cursor.node()) == "const" {
                            is_const = true;
                        }
                    }
                }

                if is_const {
                    "C"
                } else {
                    "v"
                }
            }
        };

        create_tag(name.clone(), kind, node, context, None);

        if is_function {
            return Some((ScopeType::Function, name));
        }
    }

    None
}

fn process_type_alias_declaration(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "type_identifier" {
            name = context.base.node_text(&child).to_string();
            Break
        } else {
            Continue
        }
    });

    if !name.is_empty() {
        create_tag(name, "a", node, context, None);
    }
    None
}

fn process_parameter(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut access = String::new();

    iterate_children!(cursor, |child| {
        match child.kind() {
            "identifier" => name = context.base.node_text(&child).to_string(),
            "accessibility_modifier" => access = context.base.node_text(&child).to_string(),
            _ => {}
        }
        Continue
    });

    if !name.is_empty() && !access.is_empty() {
        if !access.is_empty() {
            let mut extras = IndexMap::new();
            if context
                .base
                .user_config
                .fields_config
                .is_field_enabled("access")
            {
                extras.insert("access".to_string(), access);
            }
            create_tag(name, "p", node, context, Some(extras));
        } else {
            create_tag(name, "z", node, context, None);
        }
    }
    None
}

fn process_public_field_definition(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut access = "public";

    iterate_children!(cursor, |child| {
        match child.kind() {
            "property_identifier" | "string" => {
                name = context.base.node_text(&child).to_string();
                if name.starts_with('"') || name.starts_with('\'') {
                    name = name[1..name.len() - 1].to_string();
                }
                Continue
            }
            "accessibility_modifier" => {
                access = context.base.node_text(&child);
                Continue
            }
            _ => Continue,
        }
    });

    if !name.is_empty() {
        let mut extras = IndexMap::new();
        if context
            .base
            .user_config
            .fields_config
            .is_field_enabled("access")
        {
            extras.insert("access".to_string(), access.to_string());
        }
        create_tag(name, "p", node, context, Some(extras));
    }
    None
}

fn process_property_signature(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "property_identifier" || child.kind() == "string" {
            name = context.base.node_text(&child).to_string();
            if name.starts_with('"') || name.starts_with('\'') {
                name = name[1..name.len() - 1].to_string();
            }
            Break
        } else {
            Continue
        }
    });

    if !name.is_empty() {
        let mut extras = IndexMap::new();
        if context
            .base
            .user_config
            .fields_config
            .is_field_enabled("access")
        {
            extras.insert("access".to_string(), "public".to_string());
        }
        create_tag(name, "p", node, context, Some(extras));
    }
    None
}

fn process_enum_body(
    cursor: &mut TreeCursor,
    context: &mut TypeScriptContext,
) -> Option<(ScopeType, String)> {
    iterate_children!(cursor, |child| {
        if child.kind() == "property_identifier" || child.kind() == "identifier" {
            let name = context.base.node_text(&child).to_string();
            create_tag(name, "e", child, context, None);
        }
        Continue
    });
    None
}
