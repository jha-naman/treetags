use tree_sitter::{Node, Parser, TreeCursor};
use wit_bindgen::generate;

generate!({
    world: "plugin",
    path: "../../wit/treetags.wit",
});

struct TreetagsPlugin;

impl Guest for TreetagsPlugin {
    fn supported_extensions() -> Vec<String> {
        vec!["ts".to_string(), "tsx".to_string()]
    }

    fn generate(source: String, _cfg: Config) -> Result<Vec<Tag>, String> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .map_err(|_| "Failed to load TypeScript grammar")?;

        let tree = parser
            .parse(&source, None)
            .ok_or("Failed to parse source")?;
        let root_node = tree.root_node();

        let mut context = Context {
            source: source.as_bytes(),
            tags: Vec::new(),
            scope_stack: Vec::new(),
        };

        let mut cursor = root_node.walk();
        walk_tree(&mut cursor, &mut context);

        Ok(context.tags)
    }
}

struct Context<'a> {
    source: &'a [u8],
    tags: Vec<Tag>,
    scope_stack: Vec<(String, String)>, // (Type, Name)
}

fn walk_tree(cursor: &mut TreeCursor, context: &mut Context) {
    let pushed_scope = process_node(cursor, context);

    if cursor.goto_first_child() {
        loop {
            walk_tree(cursor, context);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }

    if pushed_scope {
        context.scope_stack.pop();
    }
}

fn process_node(cursor: &mut TreeCursor, context: &mut Context) -> bool {
    let node = cursor.node();
    let kind = node.kind();

    match kind {
        "function_declaration" | "generator_function_declaration" => {
            if let Some(name) = get_child_text(node, "identifier", context.source) {
                let tag_kind = if kind == "generator_function_declaration" {
                    "G"
                } else {
                    "f"
                };
                add_tag(name.clone(), tag_kind, node, context);
                context.scope_stack.push(("function".to_string(), name));
                return true;
            }
        }
        "class_declaration" => {
            if let Some(name) = get_child_text(node, "type_identifier", context.source) {
                add_tag(name.clone(), "c", node, context);
                context.scope_stack.push(("class".to_string(), name));
                return true;
            }
        }
        "interface_declaration" => {
            if let Some(name) = get_child_text(node, "type_identifier", context.source) {
                add_tag(name.clone(), "i", node, context);
                context.scope_stack.push(("interface".to_string(), name));
                return true;
            }
        }
        "enum_declaration" => {
            if let Some(name) = get_child_text(node, "identifier", context.source) {
                add_tag(name.clone(), "g", node, context);
                context.scope_stack.push(("enum".to_string(), name));
                return true;
            }
        }
        "module" => {
            // Modules can have identifier or string name
            let mut name = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" || child.kind() == "string" {
                    let text = child.utf8_text(context.source).unwrap_or("").to_string();
                    name = Some(if text.starts_with('"') || text.starts_with('\'') {
                        text[1..text.len() - 1].to_string()
                    } else {
                        text
                    });
                    break;
                }
            }

            if let Some(n) = name {
                add_tag(n.clone(), "n", node, context);
                context.scope_stack.push(("module".to_string(), n));
                return true;
            }
        }
        "method_definition" => {
            // Find name
            let mut name = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "property_identifier" | "number" | "string") {
                    let text = child.utf8_text(context.source).unwrap_or("").to_string();
                    name = Some(if text.starts_with('"') || text.starts_with('\'') {
                        text[1..text.len() - 1].to_string()
                    } else {
                        text
                    });
                    break;
                }
            }

            if let Some(n) = name {
                // Check access
                let mut access = "public".to_string();
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "accessibility_modifier" {
                        access = child
                            .utf8_text(context.source)
                            .unwrap_or("public")
                            .to_string();
                    }
                }

                add_tag_with_access(n.clone(), "m", node, context, &access);
                context.scope_stack.push(("function".to_string(), n));
                return true;
            }
        }
        "variable_declarator" => {
            // Simplified variable handling
            if let Some(name) = get_child_text(node, "identifier", context.source) {
                // Check if it's a function (arrow or expression)
                let mut is_func = false;
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if matches!(child.kind(), "arrow_function" | "function_expression") {
                        is_func = true;
                        break;
                    }
                }

                let kind = if is_func { "f" } else { "v" };
                add_tag(name.clone(), kind, node, context);

                if is_func {
                    context.scope_stack.push(("function".to_string(), name));
                    return true;
                }
            }
        }
        "type_alias_declaration" => {
            if let Some(name) = get_child_text(node, "type_identifier", context.source) {
                add_tag(name, "a", node, context);
            }
        }
        _ => {}
    }
    false
}

fn add_tag(name: String, kind: &str, node: Node, context: &mut Context) {
    add_tag_with_access(name, kind, node, context, "")
}

fn add_tag_with_access(name: String, kind: &str, node: Node, context: &mut Context, access: &str) {
    let mut extensions = Vec::new();

    // Add scope information if available
    if let Some((scope_type, scope_name)) = context.scope_stack.last() {
        extensions.push((scope_type.clone(), scope_name.clone()));
    }

    if !access.is_empty() {
        extensions.push(("access".to_string(), access.to_string()));
    }

    context.tags.push(Tag {
        name,
        line: (node.start_position().row + 1) as u64,
        kind: kind.to_string(),
        extension_fields: extensions,
    });
}

fn get_child_text(node: Node, child_kind: &str, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == child_kind {
            return child.utf8_text(source).ok().map(|s| s.to_string());
        }
    }
    None
}

export!(TreetagsPlugin);
