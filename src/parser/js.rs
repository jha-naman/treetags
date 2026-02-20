use super::helper::{self, iterate_children, Break, Continue, LanguageContext, TagKindConfig};
use super::Parser;
use indexmap::IndexMap;
use tree_sitter::{Node, TreeCursor};

use crate::tag;

#[derive(Debug)]
enum ScopeType {
    Class,
    Function,
    Property,
}

struct JsContext<'a> {
    base: helper::Context<'a>,
    scope_stack: Vec<(ScopeType, String)>,
    sequence_counter: u16,
    filename_hash: String,
}

impl<'a> JsContext<'a> {
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

    fn calculate_filename_hash(filename: &str) -> String {
        let mut hash: u32 = 5381;
        for byte in filename.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
        }
        format!("{:08x}", hash)
    }

    fn generate_anonymous_name(&mut self) -> String {
        let name = format!(
            "anonymousFunction{}{:02x}",
            self.filename_hash, self.sequence_counter
        );
        self.sequence_counter += 1;
        name
    }

    fn create_extension_fields(&self) -> IndexMap<String, String> {
        let mut fields = IndexMap::new();

        for (scope_type, name) in &self.scope_stack {
            match scope_type {
                ScopeType::Class => {
                    fields.insert(String::from("class"), name.clone());
                }
                ScopeType::Function => {
                    fields.insert(String::from("function"), name.clone());
                }
                ScopeType::Property => {
                    fields.insert(String::from("property"), name.clone());
                }
            }
        }

        fields
    }
}

impl<'a> LanguageContext for JsContext<'a> {
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
    pub fn generate_js_tags_with_full_config(
        &mut self,
        code: &[u8],
        file_path_relative_to_tag_file: &str,
        tag_config: &helper::TagKindConfig,
        user_config: &crate::config::Config,
    ) -> Option<Vec<tag::Tag>> {
        helper::generate_tags_with_config(
            &mut self.ts_parser,
            tree_sitter_javascript::LANGUAGE.into(),
            code,
            file_path_relative_to_tag_file,
            |source_code, lines, cursor, tags| {
                let mut context = JsContext::new(
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

fn process_node(cursor: &mut TreeCursor, context: &mut JsContext) -> Option<(ScopeType, String)> {
    let node = cursor.node();

    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            process_function_declaration(cursor, context)
        }
        "class_declaration" => process_class_declaration(cursor, context),
        "variable_declarator" => process_variable_declarator(cursor, context),
        "method_definition" => process_method_definition(cursor, context),
        "field_definition" | "class_static_block" => process_field_definition(cursor, context),
        "pair" => process_pair(cursor, context),
        "expression_statement" => process_expression_statement(cursor, context),
        "call_expression" => process_call_expression(cursor, context),
        _ => None,
    }
}

fn create_tag(
    name: String,
    kind_char: &str,
    node: Node,
    context: &mut JsContext,
    extra_fields: Option<IndexMap<String, String>>,
) {
    if name.is_empty() {
        return;
    }

    if !context.base.tag_config.is_kind_enabled(kind_char) {
        return;
    }

    let row = node.start_position().row;
    let address = helper::address_string_from_line(row, &context.base);
    let mut extension_fields = IndexMap::new();

    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("kind")
    {
        extension_fields.insert(String::from("kind"), kind_char.to_string());
    }

    if context
        .base
        .user_config
        .fields_config
        .is_field_enabled("line")
    {
        extension_fields.insert(String::from("line"), (row + 1).to_string());
    }

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

    if let Some(extras) = extra_fields {
        for (key, value) in extras {
            if context
                .base
                .user_config
                .fields_config
                .is_field_enabled("scope")
            {
                extension_fields.insert(key, value);
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

fn process_function_declaration(
    cursor: &mut TreeCursor,
    context: &mut JsContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let is_generator = node.kind() == "generator_function_declaration";
    let tag_kind = if is_generator { "g" } else { "f" };

    if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
        create_tag(name.clone(), tag_kind, node, context, None);
        return Some((ScopeType::Function, name));
    }
    None
}

fn process_class_declaration(
    cursor: &mut TreeCursor,
    context: &mut JsContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    if let Some(name) = helper::get_node_name(cursor, &context.base, &["identifier"]) {
        create_tag(name.clone(), "c", node, context, None);
        return Some((ScopeType::Class, name));
    }
    None
}

fn process_variable_declarator(
    cursor: &mut TreeCursor,
    context: &mut JsContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut is_function = false;
    let mut is_arrow_function = false;
    let mut is_constant = false;

    let parent = node.parent();
    if let Some(parent_node) = parent {
        if parent_node.kind() == "lexical_declaration" {
            let decl_text = context.base.node_text(&parent_node);
            if decl_text.trim_start().starts_with("const") {
                is_constant = true;
            }
        }
    }

    iterate_children!(cursor, |child| {
        match child.kind() {
            "identifier" => {
                name = context.base.node_text(&child).to_string();
                Continue
            }
            "function_expression" => {
                is_function = true;
                Continue
            }
            "arrow_function" => {
                is_arrow_function = true;
                Continue
            }
            _ => Continue,
        }
    });

    if !name.is_empty() {
        if is_function || is_arrow_function {
            create_tag(name.clone(), "f", node, context, None);
            return Some((ScopeType::Function, name));
        } else if is_constant {
            create_tag(name, "C", node, context, None);
        } else {
            create_tag(name, "v", node, context, None);
        }
    }

    None
}

fn process_method_definition(
    cursor: &mut TreeCursor,
    context: &mut JsContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut kind_tag = "m";

    iterate_children!(cursor, |child| {
        match child.kind() {
            "property_identifier" | "identifier" => {
                name = context.base.node_text(&child).to_string();
                Continue
            }
            "get" => {
                kind_tag = "G";
                Continue
            }
            "set" => {
                kind_tag = "S";
                Continue
            }
            _ => Continue,
        }
    });

    if !name.is_empty() {
        create_tag(name.clone(), kind_tag, node, context, None);
        return Some((ScopeType::Function, name));
    }
    None
}

fn process_field_definition(
    cursor: &mut TreeCursor,
    context: &mut JsContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    let mut is_method = false;
    iterate_children!(cursor, |child| {
        match child.kind() {
            "property_identifier" | "private_property_identifier" => {
                name = context.base.node_text(&child).to_string();
                Continue
            }
            _ => {
                if cursor.field_name() == Some("value") {
                    match child.kind() {
                        "function_expression" | "arrow_function" => is_method = true,
                        _ => {}
                    }
                }
                Continue
            }
        }
    });

    if !name.is_empty() {
        let tag_kind = if is_method { "m" } else { "M" };
        create_tag(name.clone(), tag_kind, node, context, None);
        if is_method {
            return Some((ScopeType::Function, name));
        }
    }
    None
}

fn process_pair(cursor: &mut TreeCursor, context: &mut JsContext) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut key_name = String::new();
    let mut is_func = false;

    iterate_children!(cursor, |child| {
        if cursor.field_name() == Some("key") {
            key_name = context.base.node_text(&child).to_string();
        } else if cursor.field_name() == Some("value") {
            match child.kind() {
                "function_expression" | "arrow_function" => is_func = true,
                _ => {}
            }
        }
        Continue
    });

    if !key_name.is_empty() {
        let tag_kind = if is_func { "m" } else { "p" };
        create_tag(key_name.clone(), tag_kind, node, context, None);
        if is_func {
            return Some((ScopeType::Function, key_name));
        }
    }

    None
}

fn process_expression_statement(
    cursor: &mut TreeCursor,
    context: &mut JsContext,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();

    let mut left_node = None;
    let mut right_node = None;

    iterate_children!(cursor, |child| {
        if child.kind() == "assignment_expression" {
            if cursor.goto_first_child() {
                loop {
                    match cursor.field_name() {
                        Some("left") => left_node = Some(cursor.node()),
                        Some("right") => right_node = Some(cursor.node()),
                        _ => {}
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
                cursor.goto_parent();
            }
            Break
        } else {
            Continue
        }
    });

    if let (Some(left), Some(right)) = (left_node, right_node) {
        if left.kind() == "member_expression" {
            let full_name = context.base.node_text(&left);

            let parts: Vec<&str> = full_name.split('.').collect();
            if parts.len() >= 2 {
                let name = parts.last().unwrap().to_string();

                let mut kind;
                let mut extra = IndexMap::new();

                match right.kind() {
                    "function_expression" | "arrow_function" => {
                        kind = "m";
                        if parts.contains(&"prototype") {
                            if let Some(class_idx) = parts.iter().position(|&x| x == "prototype") {
                                if class_idx > 0 {
                                    extra.insert(
                                        "class".to_string(),
                                        parts[0..class_idx].join(".").to_string(),
                                    );
                                }
                            }
                        } else {
                            extra.insert(
                                "property".to_string(),
                                parts[0..parts.len() - 1].join("."),
                            );
                        }
                    }
                    "object" => {
                        kind = "p";
                    }
                    _ => {
                        kind = "p";
                    }
                }

                if parts.len() == 2 && right.kind() == "object" {
                    kind = "p";
                }

                if full_name.contains(".prototype.") {
                    let class_name = full_name.split(".prototype.").next().unwrap();
                    extra.insert("class".to_string(), class_name.to_string());
                }

                create_tag(name.clone(), kind, node, context, Some(extra));

                if right.kind() == "object" {
                    return Some((ScopeType::Property, full_name.to_string()));
                }
                if kind == "m" {
                    return Some((ScopeType::Function, name));
                }
            }
        }
    }
    None
}

fn process_call_expression(
    cursor: &mut TreeCursor,
    context: &mut JsContext,
) -> Option<(ScopeType, String)> {
    let mut result = None;
    iterate_children!(cursor, |child| {
        if cursor.field_name() == Some("function") {
            if child.kind() == "function_expression" || child.kind() == "arrow_function" {
                let name = context.generate_anonymous_name();
                create_tag(name.clone(), "f", child, context, None);
                result = Some((ScopeType::Function, name));
            } else if child.kind() == "parenthesized_expression" {
                if cursor.goto_first_child() {
                    loop {
                        let inner = cursor.node();
                        if inner.kind() == "function_expression" || inner.kind() == "arrow_function"
                        {
                            let name = context.generate_anonymous_name();
                            create_tag(name.clone(), "f", inner, context, None);
                            result = Some((ScopeType::Function, name));
                            break;
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                    cursor.goto_parent();
                }
            }
        }
        Continue
    });

    result
}
