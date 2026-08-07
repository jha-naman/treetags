wit_bindgen::generate!({
    world: "plugin-world",
    path: "../../wit",
});

use exports::treetags::plugin::plugin::{Guest, Request, Tag};
use tree_sitter::{Parser as TsParser, TreeCursor};
use treetags_plugin_common::{
    child_ident, for_each_child, has_child, line_of, node_text, walk_tree, ScopeKey, ScopeStack,
    TagKindConfig, WalkContext,
};

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

impl ScopeKey for ScopeKind {
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
    scopes: ScopeStack<ScopeKind>,
    kinds: TagKindConfig,
    tags: Vec<Tag>,
}

impl WalkContext for KotlinWalker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        let source = self.source;
        process_node_inner(source, cursor, self)
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

/// `"C"` for `val` / `"v"` for `var`, read from a `binding_pattern_kind` child.
/// Cursor restored.
fn binding_kind(cursor: &mut TreeCursor, source: &[u8]) -> Option<&'static str> {
    let mut result = None;
    for_each_child!(cursor, {
        let child = cursor.node();
        if child.kind() == "binding_pattern_kind" {
            result = match node_text(child, source).trim() {
                "val" => Some("C"),
                "var" => Some("v"),
                _ => None,
            };
            break;
        }
    });
    result
}

/// Collect `(name, line)` for every identifier bound by the first
/// `variable_declaration`/`multi_variable_declaration` child of the current node,
/// along with whether it was a destructuring (`multi_*`). Cursor restored.
fn collect_bindings(cursor: &mut TreeCursor, source: &[u8]) -> Option<(bool, Vec<(String, u32)>)> {
    let mut result = None;
    for_each_child!(cursor, {
        match cursor.node().kind() {
            "variable_declaration" => {
                let mut names = Vec::new();
                if let Some(id) = child_ident(cursor, source, &["simple_identifier"]) {
                    names.push(id);
                }
                result = Some((false, names));
                break;
            }
            "multi_variable_declaration" => {
                let mut names = Vec::new();
                for_each_child!(cursor, {
                    if cursor.node().kind() == "variable_declaration" {
                        if let Some(id) = child_ident(cursor, source, &["simple_identifier"]) {
                            names.push(id);
                        }
                    }
                });
                result = Some((true, names));
                break;
            }
            _ => {}
        }
    });
    result
}

/// Collect `(name, line)` for the parameters of the lambda the cursor is on.
/// Cursor restored.
fn collect_lambda_params(cursor: &mut TreeCursor, source: &[u8]) -> Vec<(String, u32)> {
    let mut params = Vec::new();
    for_each_child!(cursor, {
        if cursor.node().kind() == "lambda_parameters" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "variable_declaration" {
                    if let Some(id) = child_ident(cursor, source, &["simple_identifier"]) {
                        params.push(id);
                    }
                }
            });
            break;
        }
    });
    params
}

/// Name + line of a declaration: its first `type_identifier`/`simple_identifier`
/// direct child. Cursor restored.
fn decl_name(cursor: &mut TreeCursor, source: &[u8]) -> Option<(String, u32)> {
    child_ident(cursor, source, &["type_identifier", "simple_identifier"])
}

fn make_tag(name: String, line: u32, kind: &str, scope: Option<(&str, &str)>) -> Tag {
    let mut ext = vec![];
    if let Some((scope_key, scope_value)) = scope {
        ext.push((scope_key.to_string(), scope_value.to_string()));
    }
    Tag {
        name,
        line,
        kind: kind.to_string(),
        end_line: None,
        extension_fields: ext,
    }
}

fn process_node_inner(
    source: &[u8],
    cursor: &mut TreeCursor,
    walker: &mut KotlinWalker<'_>,
) -> bool {
    let node = cursor.node();
    let line = line_of(node);

    match node.kind() {
        "package_header" => {
            if let Some((name, _)) = child_ident(cursor, source, &["identifier"]) {
                if !name.is_empty() {
                    walker.scopes.set_package(&name);
                    if walker.kinds.is_enabled("p") {
                        walker.tags.push(make_tag(name, line, "p", None));
                    }
                }
            }
            false
        }
        "class_declaration" => {
            if let Some((name, _)) = decl_name(cursor, source) {
                let (kind, scope_kind) = if has_child(cursor, &["interface"]) {
                    ("i", ScopeKind::Interface)
                } else {
                    ("c", ScopeKind::Class)
                };
                if walker.kinds.is_enabled(kind) {
                    let scope = walker.scopes.current_field();
                    walker.tags.push(make_tag(name.clone(), line, kind, scope));
                }
                walker.scopes.push(scope_kind, &name);
                return true;
            }
            false
        }
        "object_declaration" => {
            if let Some((name, _)) = decl_name(cursor, source) {
                if walker.kinds.is_enabled("o") {
                    let scope = walker.scopes.current_field();
                    walker.tags.push(make_tag(name.clone(), line, "o", scope));
                }
                walker.scopes.push(ScopeKind::Object, &name);
                return true;
            }
            false
        }
        "function_declaration" => {
            if let Some((name, name_line)) = decl_name(cursor, source) {
                if walker.kinds.is_enabled("m") {
                    let scope = walker.scopes.current_field();
                    walker
                        .tags
                        .push(make_tag(name.clone(), name_line, "m", scope));
                }
                walker.scopes.push(ScopeKind::Method, &name);
                return true;
            }
            false
        }
        "property_declaration" => {
            let kind = binding_kind(cursor, source).unwrap_or("C");
            let bindings = collect_bindings(cursor, source);
            if walker.kinds.is_enabled(kind) {
                let scope = walker.scopes.current_field();
                if let Some((_, names)) = &bindings {
                    for (name, name_line) in names {
                        walker
                            .tags
                            .push(make_tag(name.clone(), *name_line, kind, scope));
                    }
                }
            }
            if has_child(cursor, &["lambda_literal", "anonymous_function"]) {
                if let Some((_, names)) = &bindings {
                    if let Some((name, _)) = names.first() {
                        walker.scopes.push(ScopeKind::PendingLambda, name);
                        return true;
                    }
                }
            }
            false
        }
        "class_parameter" => {
            if let Some(kind) = binding_kind(cursor, source) {
                if walker.kinds.is_enabled(kind) {
                    if let Some((name, name_line)) =
                        child_ident(cursor, source, &["simple_identifier"])
                    {
                        let scope = walker.scopes.current_field();
                        walker.tags.push(make_tag(name, name_line, kind, scope));
                    }
                }
            }
            false
        }
        "for_statement" => {
            if let Some((is_multi, names)) = collect_bindings(cursor, source) {
                let kind = if is_multi { "C" } else { "m" };
                if walker.kinds.is_enabled(kind) {
                    let scope = walker.scopes.current_field();
                    for (name, name_line) in names {
                        walker.tags.push(make_tag(name, name_line, kind, scope));
                    }
                }
            }
            false
        }
        "lambda_literal" | "anonymous_function" => {
            let claimed = matches!(walker.scopes.last_key(), Some(ScopeKind::PendingLambda));

            let pushed = if claimed {
                walker.scopes.set_last_key(ScopeKind::Method);
                false
            } else {
                if walker.kinds.is_enabled("m") {
                    let scope = walker.scopes.current_field();
                    walker
                        .tags
                        .push(make_tag("<lambda>".to_string(), line, "m", scope));
                }
                walker.scopes.push(ScopeKind::Method, "<lambda>");
                true
            };

            // Lambda parameters, scoped to the current (named or `<lambda>`) scope.
            if walker.kinds.is_enabled("m") {
                let params = collect_lambda_params(cursor, source);
                let inner_scope = walker.scopes.current_field();
                for (name, name_line) in params {
                    walker
                        .tags
                        .push(make_tag(name, name_line, "m", inner_scope));
                }
            }
            pushed
        }
        "type_alias" => {
            if walker.kinds.is_enabled("T") {
                if let Some((name, name_line)) = decl_name(cursor, source) {
                    let scope = walker.scopes.current_field();
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
        scopes: ScopeStack::new(),
        kinds: TagKindConfig::parse(&req.kinds, KOTLIN_DEFAULT_KINDS, KOTLIN_OPTIONAL_KINDS),
        tags: Vec::new(),
    };

    let mut cursor = tree.walk();
    if cursor.goto_first_child() {
        walk_tree(&mut cursor, &mut walker);
    }

    Ok(walker.tags)
}
