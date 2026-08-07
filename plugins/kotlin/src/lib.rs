wit_bindgen::generate!({
    world: "plugin-world",
    path: "../../wit",
});

use exports::treetags::plugin::plugin::{Guest, Request, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};
use treetags_plugin_common::{walk_tree, TagKindConfig, WalkContext};

struct KotlinPlugin;

impl Guest for KotlinPlugin {
    fn generate(req: Request, source: Vec<u8>) -> Result<Vec<Tag>, String> {
        let mut parser = TsParser::new();
        let language: tree_sitter::Language = tree_sitter_kotlin_sg::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|e| format!("set_language: {e}"))?;
        generate_tags(&mut parser, &req, &source)
    }
}

export!(KotlinPlugin);

const KOTLIN_DEFAULT_KINDS: &[(&[&str], &str)] = &[
    (&["C", "constant"], "C"),
    (&["T", "typealias"], "T"),
    (&["c", "class"], "c"),
    (&["i", "interface"], "i"),
    (&["m", "method"], "m"),
    (&["o", "object"], "o"),
    (&["p", "package"], "p"),
    (&["v", "variable"], "v"),
];

const KOTLIN_OPTIONAL_KINDS: &[(&[&str], &str)] = &[];

#[derive(Clone, Copy)]
enum ScopeKind {
    Class,
    Interface,
    Object,
    Method,
    PendingLambda,
}

impl ScopeKind {
    fn key(self) -> &'static str {
        match self {
            ScopeKind::Class => "class",
            ScopeKind::Interface => "interface",
            ScopeKind::Object => "object",
            ScopeKind::Method | ScopeKind::PendingLambda => "method",
        }
    }
}

struct KotlinWalker<'src> {
    source: &'src [u8],
    package: Option<String>,
    scope_stack: Vec<(ScopeKind, String)>,
    kinds: TagKindConfig,
    tags: Vec<Tag>,
}

impl WalkContext for KotlinWalker<'_> {
    fn process_node(&mut self, cursor: &TreeCursor) -> bool {
        let source = self.source;
        process_node_inner(source, cursor, self)
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }
}

impl KotlinWalker<'_> {
    /// Build the `key:value` scope string for the current position.
    fn current_scope(&self) -> Option<String> {
        if self.scope_stack.is_empty() {
            let pkg = self.package.as_deref()?;
            return Some(format!("package:{pkg}"));
        }
        let key = self.scope_stack.last().unwrap().0.key();
        let mut names: Vec<&str> = Vec::new();
        if let Some(pkg) = self.package.as_deref() {
            names.push(pkg);
        }
        for (_, n) in &self.scope_stack {
            names.push(n.as_str());
        }
        Some(format!("{}:{}", key, names.join(".")))
    }
}

fn node_text<'a>(node: Node<'a>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// First direct child whose kind matches one of `kinds`.
fn child_of_kind<'a>(node: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if kinds.contains(&child.kind()) {
                return Some(child);
            }
        }
    }
    None
}

/// True if `node` has a direct child of the given kind.
fn has_child_kind(node: Node, kind: &str) -> bool {
    for i in 0..node.child_count() as u32 {
        if let Some(child) = node.child(i) {
            if child.kind() == kind {
                return true;
            }
        }
    }
    false
}

/// Name of a declaration: first `type_identifier`/`simple_identifier` child.
fn decl_name(node: Node, source: &[u8]) -> Option<(String, u32)> {
    let name_node = child_of_kind(node, &["type_identifier", "simple_identifier"])?;
    Some((node_text(name_node, source).to_string(), line_of(name_node)))
}

/// "val"/"var" from a `binding_pattern_kind` child, if present.
fn binding_kind<'a>(node: Node, source: &'a [u8]) -> Option<&'a str> {
    let bpk = child_of_kind(node, &["binding_pattern_kind"])?;
    match node_text(bpk, source).trim() {
        "val" => Some("C"),
        "var" => Some("v"),
        _ => None,
    }
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

/// Emit tags for the identifiers bound by a `variable_declaration` or
/// `multi_variable_declaration` node, all with the given kind and scope.
fn emit_binding_names(
    walker: &mut KotlinWalker<'_>,
    source: &[u8],
    binding: Node,
    kind: &str,
    scope: &Option<String>,
) {
    match binding.kind() {
        "variable_declaration" => {
            if let Some(id) = child_of_kind(binding, &["simple_identifier"]) {
                let name = node_text(id, source).to_string();
                walker
                    .tags
                    .push(make_tag(name, line_of(id), kind, scope.clone()));
            }
        }
        "multi_variable_declaration" => {
            for i in 0..binding.child_count() as u32 {
                if let Some(child) = binding.child(i) {
                    if child.kind() == "variable_declaration" {
                        if let Some(id) = child_of_kind(child, &["simple_identifier"]) {
                            let name = node_text(id, source).to_string();
                            walker
                                .tags
                                .push(make_tag(name, line_of(id), kind, scope.clone()));
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn process_node_inner(source: &[u8], cursor: &TreeCursor, walker: &mut KotlinWalker<'_>) -> bool {
    let node = cursor.node();
    let line = line_of(node);

    match node.kind() {
        "package_header" => {
            if let Some(id) = child_of_kind(node, &["identifier"]) {
                let name = node_text(id, source).to_string();
                if !name.is_empty() {
                    walker.package = Some(name.clone());
                    if walker.kinds.is_enabled("p") {
                        walker.tags.push(make_tag(name, line, "p", None));
                    }
                }
            }
            false
        }
        "class_declaration" => {
            if let Some((name, _)) = decl_name(node, source) {
                let scope = walker.current_scope();
                let is_interface = has_child_kind(node, "interface");
                let (kind, scope_kind) = if is_interface {
                    ("i", ScopeKind::Interface)
                } else {
                    ("c", ScopeKind::Class)
                };
                if walker.kinds.is_enabled(kind) {
                    walker.tags.push(make_tag(name.clone(), line, kind, scope));
                }
                walker.scope_stack.push((scope_kind, name));
                return true;
            }
            false
        }
        "object_declaration" => {
            if let Some((name, _)) = decl_name(node, source) {
                let scope = walker.current_scope();
                if walker.kinds.is_enabled("o") {
                    walker.tags.push(make_tag(name.clone(), line, "o", scope));
                }
                walker.scope_stack.push((ScopeKind::Object, name));
                return true;
            }
            false
        }
        "function_declaration" => {
            if let Some((name, name_line)) = decl_name(node, source) {
                let scope = walker.current_scope();
                if walker.kinds.is_enabled("m") {
                    walker
                        .tags
                        .push(make_tag(name.clone(), name_line, "m", scope));
                }
                walker.scope_stack.push((ScopeKind::Method, name));
                return true;
            }
            false
        }
        "property_declaration" => {
            let kind = binding_kind(node, source).unwrap_or("C");
            if walker.kinds.is_enabled(kind) {
                let scope = walker.current_scope();
                if let Some(binding) = child_of_kind(
                    node,
                    &["variable_declaration", "multi_variable_declaration"],
                ) {
                    emit_binding_names(walker, source, binding, kind, &scope);
                }
            }
            if child_of_kind(node, &["lambda_literal", "anonymous_function"]).is_some() {
                if let Some(binding) = child_of_kind(node, &["variable_declaration"]) {
                    if let Some(id) = child_of_kind(binding, &["simple_identifier"]) {
                        let name = node_text(id, source).to_string();
                        walker.scope_stack.push((ScopeKind::PendingLambda, name));
                        return true;
                    }
                }
            }
            false
        }
        "class_parameter" => {
            if let Some(kind) = binding_kind(node, source) {
                if walker.kinds.is_enabled(kind) {
                    if let Some(id) = child_of_kind(node, &["simple_identifier"]) {
                        let scope = walker.current_scope();
                        let name = node_text(id, source).to_string();
                        walker.tags.push(make_tag(name, line_of(id), kind, scope));
                    }
                }
            }
            false
        }
        "for_statement" => {
            if let Some(binding) = child_of_kind(
                node,
                &["variable_declaration", "multi_variable_declaration"],
            ) {
                let scope = walker.current_scope();
                let kind = if binding.kind() == "multi_variable_declaration" {
                    "C"
                } else {
                    "m"
                };
                if walker.kinds.is_enabled(kind) {
                    emit_binding_names(walker, source, binding, kind, &scope);
                }
            }
            false
        }
        "lambda_literal" | "anonymous_function" => {
            let claimed = matches!(
                walker.scope_stack.last(),
                Some((ScopeKind::PendingLambda, _))
            );

            let pushed = if claimed {
                if let Some(top) = walker.scope_stack.last_mut() {
                    top.0 = ScopeKind::Method;
                }
                false
            } else {
                let scope = walker.current_scope();
                if walker.kinds.is_enabled("m") {
                    walker
                        .tags
                        .push(make_tag("<lambda>".to_string(), line, "m", scope));
                }
                walker
                    .scope_stack
                    .push((ScopeKind::Method, "<lambda>".to_string()));
                true
            };

            if let Some(params) = child_of_kind(node, &["lambda_parameters"]) {
                if walker.kinds.is_enabled("m") {
                    let inner_scope = walker.current_scope();
                    for i in 0..params.child_count() as u32 {
                        if let Some(child) = params.child(i) {
                            if child.kind() == "variable_declaration" {
                                if let Some(id) = child_of_kind(child, &["simple_identifier"]) {
                                    let name = node_text(id, source).to_string();
                                    walker.tags.push(make_tag(
                                        name,
                                        line_of(id),
                                        "m",
                                        inner_scope.clone(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            pushed
        }
        "type_alias" => {
            if walker.kinds.is_enabled("T") {
                if let Some((name, name_line)) = decl_name(node, source) {
                    let scope = walker.current_scope();
                    walker.tags.push(make_tag(name, name_line, "T", scope));
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

    let mut walker = KotlinWalker {
        source,
        package: None,
        scope_stack: Vec::new(),
        kinds: TagKindConfig::parse(&req.kinds, KOTLIN_DEFAULT_KINDS, KOTLIN_OPTIONAL_KINDS),
        tags: Vec::new(),
    };

    let mut cursor = tree.walk();
    if cursor.goto_first_child() {
        walk_tree(&mut cursor, &mut walker);
    }

    Ok(walker.tags)
}
