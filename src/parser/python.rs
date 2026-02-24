use super::helper::{self, iterate_children, Break, Continue, LanguageContext, TagKindConfig};
use super::Parser;
use indexmap::IndexMap;
use tree_sitter::{Node, TreeCursor};

use crate::tag;

#[derive(Debug)]
enum ScopeType {
    Class,
    Function,
}

struct PythonContext<'a> {
    base: helper::Context<'a>,
    scope_stack: Vec<(ScopeType, String)>,
}

impl<'a> PythonContext<'a> {
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

impl<'a> LanguageContext for PythonContext<'a> {
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
    pub fn generate_python_tags_with_full_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        tag_config: &helper::TagKindConfig,
        user_config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        helper::generate_tags_with_config(
            &mut self.ts_parser,
            tree_sitter_python::LANGUAGE.into(),
            code,
            file_path_relative_to_tag_file,
            |source_code, lines, cursor, tags| {
                let mut context = PythonContext::new(
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
    context: &mut PythonContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    match node.kind() {
        "class_definition" => process_class_definition(cursor, context),
        "function_definition" => process_function_definition(cursor, context),
        "assignment" => process_assignment(cursor, context),
        "decorated_definition" => process_decorated_definition(cursor, context),
        "import_from_statement" => process_import_from_statement(cursor, context),
        _ => None,
    }
}

fn get_access_level(name: &str) -> &'static str {
    if name.starts_with('_') && !name.ends_with("__") {
        "protected"
    } else {
        "public"
    }
}

fn create_tag(
    name: String,
    kind: &str,
    node: Node,
    context: &mut PythonContext,
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

    // Access
    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("access")
    {
        let access = if kind == "l" {
            "private"
        } else {
            get_access_level(&name)
        };
        extension_fields.insert("access".to_string(), access.to_string());
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
        if kind == "m" {
            // Prefer class scope for members
            if let Some((_, name)) = context
                .scope_stack
                .iter()
                .rev()
                .find(|(t, _)| matches!(t, ScopeType::Class))
            {
                extension_fields.insert("class".to_string(), name.clone());
            }
        } else if let Some((scope_type, scope_name)) = context.scope_stack.last() {
            match scope_type {
                ScopeType::Class => {
                    extension_fields.insert("class".to_string(), scope_name.clone());
                }
                ScopeType::Function => {
                    extension_fields.insert("function".to_string(), scope_name.clone());
                }
            }
        }
    }

    // File field
    if context.base.user_config.extras_config.file_scope {
        extension_fields.insert("file".to_string(), "".to_string());
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

fn process_class_definition(
    cursor: &mut TreeCursor,
    context: &mut PythonContext,
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
        create_tag(name.clone(), "c", node, context, None);
        Some((ScopeType::Class, name))
    } else {
        None
    }
}

fn process_function_definition(
    cursor: &mut TreeCursor,
    context: &mut PythonContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut params_signature = String::new();
    let mut return_type = String::new();

    iterate_children!(cursor, |child| {
        match child.kind() {
            "identifier" => {
                name = context.base.node_text(&child).to_string();
                Continue
            }
            "parameters" => {
                params_signature = context.base.node_text(&child).to_string();
                Continue
            }
            "type" => {
                return_type = context.base.node_text(&child).to_string();
                Continue
            }
            _ => Continue,
        }
    });

    if !name.is_empty() {
        let is_method = context.scope_stack.last().map_or(false, |(scope_type, _)| {
            matches!(scope_type, ScopeType::Class)
        });

        let kind = if is_method { "m" } else { "f" };
        let mut extras = IndexMap::new();

        if context
            .base
            .user_config
            .fields_config
            .is_field_enabled("signature")
            && !params_signature.is_empty()
        {
            extras.insert("signature".to_string(), params_signature);
        }

        if !return_type.is_empty() {
            extras.insert("typeref".to_string(), format!("typename:{}", return_type));
        }

        create_tag(name.clone(), kind, node, context, Some(extras));
        Some((ScopeType::Function, name))
    } else {
        None
    }
}

fn process_assignment(
    cursor: &mut TreeCursor,
    context: &mut PythonContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut right_node = None;
    let mut type_node = None;

    iterate_children!(cursor, |child| {
        match cursor.field_name() {
            Some("right") => {
                right_node = Some(child);
                Continue
            }
            Some("type") => {
                type_node = Some(child);
                Continue
            }
            _ => Continue,
        }
    });

    iterate_children!(cursor, |_child| {
        if cursor.field_name() == Some("left") {
            process_assignment_target(cursor, context, node, right_node, type_node);
        }
        Continue
    });

    None
}

fn process_assignment_target(
    cursor: &mut TreeCursor,
    context: &mut PythonContext,
    assignment_node: Node,
    value_node: Option<Node>,
    type_node: Option<Node>,
) {
    let target_node = cursor.node();
    match target_node.kind() {
        "identifier" => {
            let name = context.base.node_text(&target_node).to_string();
            let is_in_function = context
                .scope_stack
                .iter()
                .any(|(st, _)| matches!(st, ScopeType::Function));

            let kind = if is_in_function { "l" } else { "v" };

            let mut extras = IndexMap::new();
            if let Some(tn) = type_node {
                extras.insert(
                    "typeref".to_string(),
                    format!("typename:{}", context.base.node_text(&tn)),
                );
            }

            create_tag(name, kind, assignment_node, context, Some(extras));
        }
        "pattern_list" => {
            iterate_children!(cursor, |_child| {
                process_assignment_target(cursor, context, assignment_node, None, None);
                Continue
            });
        }
        _ => {}
    }

    if let Some(val) = value_node {
        if val.kind() == "lambda" {
            if target_node.kind() == "identifier" {
                let name = context.base.node_text(&target_node).to_string();

                // Remove the variable tag we just added
                if let Some(last_tag) = context.base.tags.last() {
                    if last_tag.name == name {
                        context.base.tags.pop();
                    }
                }

                let is_method = context
                    .scope_stack
                    .iter()
                    .any(|(scope_type, _)| matches!(scope_type, ScopeType::Class));

                let kind = if is_method { "m" } else { "f" };
                let mut extras = IndexMap::new();

                if let Some(params) = val.child_by_field_name("parameters") {
                    let params_text = context.base.node_text(&params);
                    extras.insert("signature".to_string(), format!("({})", params_text));
                }

                create_tag(name, kind, assignment_node, context, Some(extras));
            }
        }
    }
}

fn process_decorated_definition(
    cursor: &mut TreeCursor,
    context: &mut PythonContext,
) -> Option<(ScopeType, String)> {
    let mut definition_node = None;

    iterate_children!(cursor, |child| {
        if cursor.field_name() == Some("definition") {
            definition_node = Some(child);
            Break
        } else {
            Continue
        }
    });

    if definition_node.is_some() {
        if cursor.goto_first_child() {
            loop {
                if cursor.field_name() == Some("definition") {
                    let result = process_node(cursor, context);
                    cursor.goto_parent();
                    return result;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }
    None
}

fn process_import_from_statement(
    cursor: &mut TreeCursor,
    context: &mut PythonContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut module_name = String::new();

    iterate_children!(cursor, |child| {
        match cursor.field_name() {
            Some("module_name") => {
                module_name = context.base.node_text(&child).to_string();
                Continue
            }
            _ => {
                if child.kind() == "aliased_import" {
                    let mut alias = String::new();
                    let mut original_name = String::new();

                    if let Some(alias_node) = child.child_by_field_name("alias") {
                        alias = context.base.node_text(&alias_node).to_string();
                    }
                    if let Some(name_node) = child.child_by_field_name("name") {
                        original_name = context.base.node_text(&name_node).to_string();
                    }

                    if !alias.is_empty() {
                        let mut extras = IndexMap::new();

                        let nameref = if module_name.is_empty() || module_name == "." {
                            format!("unknown:{}", original_name)
                        } else {
                            format!("module:{}.{}", module_name, original_name)
                        };

                        extras.insert("nameref".to_string(), nameref);

                        create_tag(alias, "Y", node, context, Some(extras));
                    }
                }
                Continue
            }
        }
    });

    None
}
