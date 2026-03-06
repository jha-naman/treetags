use indexmap::IndexMap;
use std::collections::HashSet;
use tree_sitter::{Node, Parser, TreeCursor};
use treetags_common::helper::IterationControl;
use treetags_common::iterate_children;
use treetags_common::tree_walker::{walk_generic, LanguageContext};

#[cfg(feature = "guest")]
use crate::exports::treetags::plugin::tag_generator::{Config, Guest as TagGeneratorGuest, Tag};
#[cfg(feature = "guest")]
use wit_bindgen::generate;

#[cfg(feature = "guest")]
generate!({
    world: "plugin",
    path: "../../wit/treetags.wit",
});

#[cfg(feature = "guest")]
struct TreetagsPlugin;

#[cfg(feature = "guest")]
impl TagGeneratorGuest for TreetagsPlugin {
    fn supported_extensions() -> Vec<String> {
        vec!["ts".to_string(), "tsx".to_string()]
    }

    fn generate(source: String, _cfg: Config) -> Result<Vec<Tag>, String> {
        let enabled_kinds: HashSet<String> = _cfg.enabled_kinds.into_iter().collect();
        let extras: HashSet<String> = _cfg.extras.into_iter().collect();

        use treetags_common::tag::Tag as CoreTag;

        // Call the shared logic
        let core_tags = generate_tags(&source, &enabled_kinds, &extras)?;

        // Map Core tags to WASM tags
        Ok(core_tags
            .into_iter()
            .map(|t: CoreTag| {
                let mut extensions = Vec::new();
                if let Some(ext) = t.extension_fields {
                    for (k, v) in ext {
                        extensions.push((k, v));
                    }
                }

                Tag {
                    name: t.name,
                    address: t.address,
                    kind: t.kind.unwrap_or_default(),
                    extension_fields: extensions,
                }
            })
            .collect())
    }
}

pub fn generate_tags(
    source: &str,
    enabled_kinds: &HashSet<String>,
    extras: &HashSet<String>,
) -> Result<Vec<treetags_common::tag::Tag>, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .map_err(|_| "Failed to load TypeScript grammar")?;

    let tree = parser.parse(source, None).ok_or("Failed to parse source")?;
    let root_node = tree.root_node();

    let lines: Vec<Vec<u8>> = source
        .split('\n')
        .map(|line| line.as_bytes().to_vec())
        .collect();

    let mut context = Context {
        source: source.as_bytes(),
        lines,
        tags: Vec::new(),
        scope_stack: Vec::new(),
        enabled_kinds,
        extras,
    };

    let mut cursor = root_node.walk();
    walk_generic(&mut cursor, &mut context);

    Ok(context.tags)
}

pub struct Context<'a> {
    source: &'a [u8],
    lines: Vec<Vec<u8>>,
    tags: Vec<treetags_common::tag::Tag>,
    scope_stack: Vec<(ScopeType, String)>,
    enabled_kinds: &'a HashSet<String>,
    extras: &'a HashSet<String>,
}

impl<'a> Context<'a> {
    fn node_text(&self, node: &Node) -> String {
        node.utf8_text(self.source).unwrap_or("").to_string()
    }
}

#[derive(Debug)]
pub enum ScopeType {
    Class,
    Interface,
    Enum,
    Module,
    Function,
}

impl<'a> LanguageContext for Context<'a> {
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

fn process_node(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
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

fn address_string_from_line(row: usize, context: &Context) -> String {
    if row >= context.lines.len() {
        return format!("/^{}$/;", row + 1);
    }
    let line_bytes = &context.lines[row];
    let escaped = String::from_utf8_lossy(line_bytes)
        .replace('\\', "\\\\")
        .replace('/', "\\/")
        .replace('^', "\\^")
        .replace('$', "\\$");
    format!("/^{}$/;\"", escaped)
}

fn create_tag(
    name: String,
    kind: &str,
    node: Node,
    context: &mut Context,
    extra_fields: Option<IndexMap<String, String>>,
) {
    if !context.enabled_kinds.is_empty() && !context.enabled_kinds.contains(kind) {
        return;
    }

    let row = node.start_position().row;
    let address = address_string_from_line(row, context);
    let mut extension_fields = IndexMap::new();

    if context.extras.contains("kind") {
        extension_fields.insert("kind".to_string(), kind.to_string());
    }
    if context.extras.contains("line") {
        extension_fields.insert("line".to_string(), (row + 1).to_string());
    }
    if context.extras.contains("roles") {
        extension_fields.insert("roles".to_string(), "def".to_string());
    }
    if let Some(extras) = extra_fields {
        for (k, v) in extras {
            extension_fields.insert(k, v);
        }
    }
    if context.extras.contains("scope") {
        if let Some((scope_type, scope_name)) = context.scope_stack.last() {
            let scope_key = match scope_type {
                ScopeType::Class => "class",
                ScopeType::Interface => "interface",
                ScopeType::Enum => "enum",
                ScopeType::Module => "module",
                ScopeType::Function => "function",
            };
            extension_fields.insert(scope_key.to_string(), scope_name.clone());
        }
    }
    if context.extras.contains("end") {
        extension_fields.insert("end".to_string(), (node.end_position().row + 1).to_string());
    }

    context.tags.push(treetags_common::tag::Tag {
        name,
        file_name: String::new(),
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
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "identifier" {
            name = context.node_text(&child);
            IterationControl::Break
        } else {
            IterationControl::Continue
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
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "type_identifier" {
            name = context.node_text(&child);
            IterationControl::Break
        } else {
            IterationControl::Continue
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
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "type_identifier" {
            name = context.node_text(&child);
            IterationControl::Break
        } else {
            IterationControl::Continue
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
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "identifier" {
            name = context.node_text(&child);
            IterationControl::Break
        } else {
            IterationControl::Continue
        }
    });

    if !name.is_empty() {
        create_tag(name.clone(), "g", node, context, None);
        Some((ScopeType::Enum, name))
    } else {
        None
    }
}

fn process_module(cursor: &mut TreeCursor, context: &mut Context) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "identifier" || child.kind() == "string" {
            name = context.node_text(&child);
            if name.starts_with('"') || name.starts_with('\'') {
                name = name[1..name.len() - 1].to_string();
            }
            IterationControl::Break
        } else {
            IterationControl::Continue
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
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut access = "public".to_string();

    iterate_children!(cursor, |child| {
        match child.kind() {
            "property_identifier" | "number" | "string" => {
                name = context.node_text(&child);
                if name.starts_with('"') || name.starts_with('\'') {
                    name = name[1..name.len() - 1].to_string();
                }
                IterationControl::Continue
            }
            "accessibility_modifier" => {
                access = context.node_text(&child);
                IterationControl::Continue
            }
            _ => IterationControl::Continue,
        }
    });

    if !name.is_empty() {
        let mut extras = IndexMap::new();
        if context.extras.contains("access") {
            extras.insert("access".to_string(), access);
        }
        create_tag(name.clone(), "m", node, context, Some(extras));
        Some((ScopeType::Function, name))
    } else {
        None
    }
}

fn process_method_signature(
    cursor: &mut TreeCursor,
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let access = "public";

    iterate_children!(cursor, |child| {
        if child.kind() == "property_identifier" || child.kind() == "string" {
            name = context.node_text(&child);
            if name.starts_with('"') || name.starts_with('\'') {
                name = name[1..name.len() - 1].to_string();
            }
            IterationControl::Break
        } else {
            IterationControl::Continue
        }
    });

    if !name.is_empty() {
        let mut extras = IndexMap::new();
        if context.extras.contains("access") {
            extras.insert("access".to_string(), access.to_string());
        }
        create_tag(name, "m", node, context, Some(extras));
    }
    None
}

fn process_variable_declarator(
    cursor: &mut TreeCursor,
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut is_function = false;

    iterate_children!(cursor, |child| {
        if cursor.field_name() == Some("name") {
            name = context.node_text(&child);
            IterationControl::Continue
        } else if cursor.field_name() == Some("value") {
            match child.kind() {
                "arrow_function" | "function_expression" => is_function = true,
                _ => {}
            }
            IterationControl::Continue
        } else {
            IterationControl::Continue
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
                    parent_cursor.goto_parent();
                    if parent_cursor.goto_first_child() {
                        if context.node_text(&parent_cursor.node()) == "const" {
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
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "type_identifier" {
            name = context.node_text(&child);
            IterationControl::Break
        } else {
            IterationControl::Continue
        }
    });

    if !name.is_empty() {
        create_tag(name, "a", node, context, None);
    }
    None
}

fn process_parameter(
    cursor: &mut TreeCursor,
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut access = String::new();

    iterate_children!(cursor, |child| {
        match child.kind() {
            "identifier" => name = context.node_text(&child),
            "accessibility_modifier" => access = context.node_text(&child),
            _ => {}
        }
        IterationControl::Continue
    });

    if !name.is_empty() && !access.is_empty() {
        let mut extras = IndexMap::new();
        if context.extras.contains("access") {
            extras.insert("access".to_string(), access);
        }
        create_tag(name, "p", node, context, Some(extras));
    } else if !name.is_empty() {
        create_tag(name, "z", node, context, None);
    }
    None
}

fn process_public_field_definition(
    cursor: &mut TreeCursor,
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();
    let mut access = "public".to_string();

    iterate_children!(cursor, |child| {
        match child.kind() {
            "property_identifier" | "string" => {
                name = context.node_text(&child);
                if name.starts_with('"') || name.starts_with('\'') {
                    name = name[1..name.len() - 1].to_string();
                }
                IterationControl::Continue
            }
            "accessibility_modifier" => {
                access = context.node_text(&child);
                IterationControl::Continue
            }
            _ => IterationControl::Continue,
        }
    });

    if !name.is_empty() {
        let mut extras = IndexMap::new();
        if context.extras.contains("access") {
            extras.insert("access".to_string(), access);
        }
        create_tag(name, "p", node, context, Some(extras));
    }
    None
}

fn process_property_signature(
    cursor: &mut TreeCursor,
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    let node = cursor.node();
    let mut name = String::new();

    iterate_children!(cursor, |child| {
        if child.kind() == "property_identifier" || child.kind() == "string" {
            name = context.node_text(&child);
            if name.starts_with('"') || name.starts_with('\'') {
                name = name[1..name.len() - 1].to_string();
            }
            IterationControl::Break
        } else {
            IterationControl::Continue
        }
    });

    if !name.is_empty() {
        let mut extras = IndexMap::new();
        if context.extras.contains("access") {
            extras.insert("access".to_string(), "public".to_string());
        }
        create_tag(name, "p", node, context, Some(extras));
    }
    None
}

fn process_enum_body(
    cursor: &mut TreeCursor,
    context: &mut Context,
) -> Option<(ScopeType, String)> {
    iterate_children!(cursor, |child| {
        if child.kind() == "property_identifier" || child.kind() == "identifier" {
            let name = context.node_text(&child);
            create_tag(name, "e", child, context, None);
        }
        IterationControl::Continue
    });
    None
}

#[cfg(feature = "guest")]
export!(TreetagsPlugin);
