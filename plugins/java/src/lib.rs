wit_bindgen::generate!({
    world: "plugin-world",
    path: "../../wit",
});

use exports::treetags::plugin::plugin::{Guest, Request, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};
use treetags_plugin_common::{walk_tree, TagKindConfig, WalkContext};

struct JavaPlugin;

impl Guest for JavaPlugin {
    fn generate(req: Request, source: Vec<u8>) -> Result<Vec<Tag>, String> {
        let mut parser = TsParser::new();
        let language: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|e| format!("set_language: {e}"))?;
        generate_tags(&mut parser, &req, &source)
    }
}

export!(JavaPlugin);

const JAVA_DEFAULT_KINDS: &[(&[&str], &str)] = &[
    (&["a", "annotation"], "a"),
    (&["c", "class"], "c"),
    (&["e", "enumConstant"], "e"),
    (&["f", "field"], "f"),
    (&["g", "enum"], "g"),
    (&["i", "interface"], "i"),
    (&["m", "method"], "m"),
    (&["p", "package"], "p"),
];

const JAVA_OPTIONAL_KINDS: &[(&[&str], &str)] = &[(&["l", "local"], "l")];

#[derive(Clone, Copy)]
enum ScopeKind {
    Class,
    Interface,
    Enum,
    Annotation,
}

impl ScopeKind {
    fn as_str(self) -> &'static str {
        match self {
            ScopeKind::Class => "class",
            ScopeKind::Interface => "interface",
            ScopeKind::Enum => "enum",
            ScopeKind::Annotation => "annotation",
        }
    }
}

struct JavaWalker<'src> {
    source: &'src [u8],
    scope_stack: Vec<(ScopeKind, String)>,
    kinds: TagKindConfig,
    tags: Vec<Tag>,
}

impl WalkContext for JavaWalker<'_> {
    fn process_node(&mut self, cursor: &TreeCursor) -> bool {
        let source = self.source;
        process_node_inner(source, cursor, self)
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }
}

impl JavaWalker<'_> {
    fn current_scope(&self) -> Option<String> {
        let (kind, _) = self.scope_stack.last()?;
        let type_str = kind.as_str();
        let names = self
            .scope_stack
            .iter()
            .map(|(_, n)| n.as_str())
            .collect::<Vec<_>>()
            .join(".");
        Some(format!("{}:{}", type_str, names))
    }
}

fn node_text<'a>(node: Node<'a>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn get_name_str(node: Node, source: &[u8]) -> Option<String> {
    let name_node = node.child_by_field_name("name")?;
    Some(node_text(name_node, source).to_string())
}

fn is_private(node: Node) -> bool {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if child.kind() == "modifiers" {
                for j in 0..child.child_count() as u32 {
                    if let Some(mod_child) = child.child(j) {
                        if mod_child.kind() == "private" {
                            return true;
                        }
                    }
                }
                return false;
            }
        }
    }
    false
}

fn has_default_value(node: Node) -> bool {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if child.kind() == "default" {
                return true;
            }
        }
    }
    false
}

fn make_tag(name: String, line: u32, kind: &str, scope: Option<String>) -> Tag {
    let mut ext = vec![];
    if let Some(scope_str) = scope {
        if let Some((scope_type, scope_name)) = scope_str.split_once(':') {
            ext.push((scope_type.to_string(), scope_name.to_string()));
        }
    }
    Tag {
        name,
        line,
        kind: kind.to_string(),
        end_line: None,
        extension_fields: ext,
    }
}

fn make_file_tag(name: String, line: u32, kind: &str, scope: Option<String>) -> Tag {
    let mut t = make_tag(name, line, kind, scope);
    t.extension_fields.push(("file".to_string(), String::new()));
    t
}

fn process_node_inner(source: &[u8], cursor: &TreeCursor, walker: &mut JavaWalker<'_>) -> bool {
    let node = cursor.node();
    let line = node.start_position().row as u32 + 1;

    match node.kind() {
        "class_declaration" => {
            if let Some(name) = get_name_str(node, source) {
                let scope = walker.current_scope();
                if walker.kinds.is_enabled("c") {
                    walker.tags.push(make_tag(name.clone(), line, "c", scope));
                }
                walker.scope_stack.push((ScopeKind::Class, name));
                return true;
            }
            false
        }
        "interface_declaration" => {
            if let Some(name) = get_name_str(node, source) {
                let scope = walker.current_scope();
                if walker.kinds.is_enabled("i") {
                    walker.tags.push(make_tag(name.clone(), line, "i", scope));
                }
                walker.scope_stack.push((ScopeKind::Interface, name));
                return true;
            }
            false
        }
        "enum_declaration" => {
            if let Some(name) = get_name_str(node, source) {
                let scope = walker.current_scope();
                if walker.kinds.is_enabled("g") {
                    walker.tags.push(make_tag(name.clone(), line, "g", scope));
                }
                walker.scope_stack.push((ScopeKind::Enum, name));
                return true;
            }
            false
        }
        "annotation_type_declaration" => {
            if let Some(name) = get_name_str(node, source) {
                let scope = walker.current_scope();
                if walker.kinds.is_enabled("a") {
                    walker.tags.push(make_tag(name.clone(), line, "a", scope));
                }
                walker.scope_stack.push((ScopeKind::Annotation, name));
                return true;
            }
            false
        }
        "record_declaration" => {
            if walker.kinds.is_enabled("c") {
                if let Some(name) = get_name_str(node, source) {
                    let scope = walker.current_scope();
                    walker.tags.push(make_tag(name.clone(), line, "c", scope));
                    walker.scope_stack.push((ScopeKind::Class, name));
                    return true;
                }
            }
            false
        }
        "method_declaration" | "constructor_declaration" => {
            if walker.kinds.is_enabled("m") {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = node_text(name_node, source).to_string();
                    let name_line = name_node.start_position().row as u32 + 1;
                    let scope = walker.current_scope();
                    walker.tags.push(make_tag(name, name_line, "m", scope));
                }
            }
            false
        }
        "field_declaration" => {
            if walker.kinds.is_enabled("f") {
                let scope = walker.current_scope();
                let private = is_private(node);
                let decl_line = node.start_position().row as u32 + 1;
                for i in 0..node.child_count() as u32 {
                    if let Some(child) = node.child(i) {
                        if child.kind() == "variable_declarator" {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let name = node_text(name_node, source).to_string();
                                if private {
                                    walker.tags.push(make_file_tag(
                                        name,
                                        decl_line,
                                        "f",
                                        scope.clone(),
                                    ));
                                } else {
                                    walker
                                        .tags
                                        .push(make_tag(name, decl_line, "f", scope.clone()));
                                }
                            }
                        }
                    }
                }
            }
            false
        }
        "local_variable_declaration" => {
            if walker.kinds.is_enabled("l") {
                let decl_line = node.start_position().row as u32 + 1;
                for i in 0..node.child_count() as u32 {
                    if let Some(child) = node.child(i) {
                        if child.kind() == "variable_declarator" {
                            if let Some(name_node) = child.child_by_field_name("name") {
                                let name = node_text(name_node, source).to_string();
                                walker.tags.push(make_tag(name, decl_line, "l", None));
                            }
                        }
                    }
                }
            }
            false
        }
        "enum_constant" => {
            if walker.kinds.is_enabled("e") {
                if let Some(name) = get_name_str(node, source) {
                    let scope = walker.current_scope();
                    walker.tags.push(make_file_tag(name, line, "e", scope));
                }
            }
            false
        }
        "annotation_type_element_declaration" => {
            if let Some(name) = get_name_str(node, source) {
                let scope = walker.current_scope();
                let with_default = has_default_value(node);
                if with_default && walker.kinds.is_enabled("f") {
                    walker
                        .tags
                        .push(make_tag(name.clone(), line, "f", scope.clone()));
                }
                if walker.kinds.is_enabled("m") {
                    walker.tags.push(make_tag(name, line, "m", scope));
                }
            }
            false
        }
        "package_declaration" => {
            if walker.kinds.is_enabled("p") {
                for i in 0..node.child_count() as u32 {
                    if let Some(child) = node.child(i) {
                        let k = child.kind();
                        if k == "identifier" || k == "scoped_identifier" {
                            let name = node_text(child, source).to_string();
                            walker.tags.push(make_tag(name, line, "p", None));
                            break;
                        }
                    }
                }
            }
            false
        }
        _ => false,
    }
}

fn generate_tags(parser: &mut TsParser, req: &Request, source: &[u8]) -> Result<Vec<Tag>, String> {
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "parse failed".to_string())?;

    let mut walker = JavaWalker {
        source,
        scope_stack: Vec::new(),
        kinds: TagKindConfig::parse(&req.kinds, JAVA_DEFAULT_KINDS, JAVA_OPTIONAL_KINDS),
        tags: Vec::new(),
    };

    let mut cursor = tree.walk();
    if cursor.goto_first_child() {
        walk_tree(&mut cursor, &mut walker);
    }

    Ok(walker.tags)
}
