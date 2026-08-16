wit_bindgen::generate!({
    world: "plugin-world",
    path: "../../wit",
});

use exports::treetags::plugin::plugin::{Guest, Request, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};
use treetags_plugin_common::{
    child_ident, for_each_child, line_of, node_text, walk_tree, ScopeKey, ScopeStack,
    TagKindConfig, WalkContext,
};

struct ObjcPlugin;

impl Guest for ObjcPlugin {
    fn generate(req: Request, source: Vec<u8>) -> Result<Vec<Tag>, String> {
        let mut parser = TsParser::new();
        let language: tree_sitter::Language = tree_sitter_objc::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|e| format!("set_language: {e}"))?;
        generate_tags(&mut parser, &req, &source)
    }
}

export!(ObjcPlugin);

const OBJC_DEFAULT_KINDS: &[(&[&str], &str)] = &[
    (&["C", "category"], "C"),
    (&["E", "field"], "E"),
    (&["I", "implementation"], "I"),
    (&["M", "macro"], "M"),
    (&["P", "protocol"], "P"),
    (&["c", "class"], "c"),
    (&["e", "enum"], "e"),
    (&["f", "function"], "f"),
    (&["i", "interface"], "i"),
    (&["m", "method"], "m"),
    (&["p", "property"], "p"),
    (&["s", "struct"], "s"),
    (&["t", "typedef"], "t"),
    (&["v", "var"], "v"),
];

const OBJC_OPTIONAL_KINDS: &[(&[&str], &str)] = &[];

#[derive(Clone, Copy)]
enum ScopeKind {
    Interface,
    Implementation,
    Protocol,
    Struct,
}

impl ScopeKey for ScopeKind {
    fn key(self) -> &'static str {
        match self {
            ScopeKind::Interface => "interface",
            ScopeKind::Implementation => "implementation",
            ScopeKind::Protocol => "protocol",
            ScopeKind::Struct => "struct",
        }
    }
}

/// One undo action per `process_node` that returned `true`, reversed in LIFO
/// order by `pop_scope`.
enum Open {
    /// Pop a scope frame and restore the previous `current_category`.
    ScopeWithCategory(Option<String>),
    /// Leave a `{ ... }` body: decrement the body-depth suppressor.
    Body,
}

struct ObjcWalker<'src> {
    source: &'src [u8],
    scopes: ScopeStack<ScopeKind>,
    kinds: TagKindConfig,
    tags: Vec<Tag>,
    /// Category name of the enclosing `@interface Foo (Cat)` /
    /// `@implementation Foo (Cat)`, added as a `category:` field to members.
    current_category: Option<String>,
    /// Depth inside function/method bodies; when > 0, in-body declarations are
    /// not tagged (locals are not ctags-visible for Objective-C).
    in_body: u32,
    opens: Vec<Open>,
}

impl WalkContext for ObjcWalker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        let source = self.source;
        process_node_inner(source, cursor, self)
    }

    fn pop_scope(&mut self) {
        match self.opens.pop() {
            Some(Open::ScopeWithCategory(prev)) => {
                self.scopes.pop();
                self.current_category = prev;
            }
            Some(Open::Body) => {
                self.in_body = self.in_body.saturating_sub(1);
            }
            None => self.scopes.pop(),
        }
    }
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

/// Resolve the declared name of a (possibly nested) declarator, following the
/// `declarator` field so parameter names in function/function-pointer
/// declarators are skipped. Returns `(name, is_function_declarator)`.
fn declarator_name(node: Node, source: &[u8]) -> Option<(String, bool)> {
    declarator_name_inner(node, source, false)
}

fn declarator_name_inner(node: Node, source: &[u8], is_func: bool) -> Option<(String, bool)> {
    if matches!(
        node.kind(),
        "identifier" | "type_identifier" | "field_identifier"
    ) {
        return Some((node_text(node, source).to_string(), is_func));
    }
    let func = is_func || node.kind() == "function_declarator";
    if let Some(d) = node.child_by_field_name("declarator") {
        return declarator_name_inner(d, source, func);
    }
    // No `declarator` field: descend into an identifier-like child, or recurse
    // through a nested `*_declarator` that isn't attached via that field
    // (e.g. `parenthesized_declarator` -> `pointer_declarator`).
    let mut result = None;
    let mut cursor = node.walk();
    for_each_child!(cursor, {
        let child = cursor.node();
        let k = child.kind();
        if matches!(k, "identifier" | "field_identifier" | "type_identifier") {
            result = Some((node_text(child, source).to_string(), func));
            break;
        }
        if k.ends_with("_declarator") {
            if let Some(r) = declarator_name_inner(child, source, func) {
                result = Some(r);
                break;
            }
        }
    });
    result
}

/// Comma-joined adopted protocols of an `@interface`. On a class interface the
/// `<Proto1, Proto2>` list is a `parameterized_arguments` child whose
/// `type_name` grandchildren name the protocols.
fn interface_protocols(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut names = Vec::new();
    for_each_child!(cursor, {
        if cursor.node().kind() == "parameterized_arguments" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "type_name" {
                    names.push(node_text(cursor.node(), source).trim().to_string());
                }
            });
            break;
        }
    });
    (!names.is_empty()).then(|| names.join(","))
}

/// Comma-joined adopted protocols of an `@protocol`, taken from its
/// `protocol_reference_list` (`<NSObject>`).
fn protocol_reference_list(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut names = Vec::new();
    for_each_child!(cursor, {
        if cursor.node().kind() == "protocol_reference_list" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "identifier" {
                    names.push(node_text(cursor.node(), source).to_string());
                }
            });
            break;
        }
    });
    (!names.is_empty()).then(|| names.join(","))
}

/// Build an Objective-C selector from a method_declaration/method_definition.
/// Selector labels are the direct `identifier` children; each labelled part of
/// a keyword selector is followed by a `method_parameter`. A no-argument method
/// has a single `identifier` and no `method_parameter`.
fn method_selector(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut labels: Vec<String> = Vec::new();
    let mut has_param = false;
    for_each_child!(cursor, {
        match cursor.node().kind() {
            "identifier" => labels.push(node_text(cursor.node(), source).to_string()),
            "method_parameter" => has_param = true,
            "keyword_declarator" => {
                if let Some((label, _)) = child_ident(cursor, source, &["identifier"]) {
                    labels.push(label);
                }
                has_param = true;
            }
            _ => {}
        }
    });
    if labels.is_empty() {
        return None;
    }
    if has_param {
        Some(labels.iter().map(|l| format!("{l}:")).collect())
    } else {
        Some(labels.join(""))
    }
}

/// Emit a tag of `kind` for each `struct_declarator` inside the current node's
/// `struct_declaration` child (used for instance variables and `@property`s).
fn emit_struct_declarators(
    cursor: &mut TreeCursor,
    source: &[u8],
    line: u32,
    kind: &str,
    scope: Option<(&str, &str)>,
    tags: &mut Vec<Tag>,
) {
    for_each_child!(cursor, {
        if cursor.node().kind() == "struct_declaration" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "struct_declarator" {
                    if let Some((name, _)) = declarator_name(cursor.node(), source) {
                        tags.push(make_tag(name, line, kind, scope));
                    }
                }
            });
        }
    });
}

/// Handle `@interface`/`@implementation` (with optional `(Category)`): emit the
/// container tag, an optional category tag, and push the container scope.
fn handle_container(
    cursor: &mut TreeCursor,
    source: &[u8],
    walker: &mut ObjcWalker<'_>,
    kind: &str,
    scope_kind: ScopeKind,
) -> bool {
    let node = cursor.node();
    let line = line_of(node);
    let name = match child_ident(cursor, source, &["identifier"]) {
        Some((n, _)) => n,
        None => return false,
    };
    let category = node
        .child_by_field_name("category")
        .map(|n| node_text(n, source).to_string());
    let protocols = if category.is_none() {
        interface_protocols(cursor, source)
    } else {
        None
    };

    if walker.kinds.is_enabled(kind) {
        let scope = walker.scopes.current_field();
        let mut tag = make_tag(name.clone(), line, kind, scope);
        if let Some(cat) = &category {
            tag.extension_fields
                .push(("category".to_string(), cat.clone()));
        } else if let Some(p) = &protocols {
            tag.extension_fields
                .push(("protocols".to_string(), p.clone()));
        }
        walker.tags.push(tag);
    }

    let prev_cat = walker.current_category.clone();
    walker.scopes.push(scope_kind, &name);
    walker.opens.push(Open::ScopeWithCategory(prev_cat));

    if let Some(cat) = category {
        if walker.kinds.is_enabled("C") {
            let scope = walker.scopes.current_field();
            walker.tags.push(make_tag(cat.clone(), line, "C", scope));
        }
        walker.current_category = Some(cat);
    }
    true
}

fn process_node_inner(source: &[u8], cursor: &mut TreeCursor, walker: &mut ObjcWalker<'_>) -> bool {
    let node = cursor.node();
    let line = line_of(node);

    match node.kind() {
        "preproc_def" | "preproc_function_def" => {
            if walker.kinds.is_enabled("M") {
                if let Some((name, _)) = child_ident(cursor, source, &["identifier"]) {
                    walker.tags.push(make_tag(name, line, "M", None));
                }
            }
            false
        }
        "class_interface" => handle_container(cursor, source, walker, "i", ScopeKind::Interface),
        "class_implementation" => {
            handle_container(cursor, source, walker, "I", ScopeKind::Implementation)
        }
        "protocol_declaration" => {
            let name = match child_ident(cursor, source, &["identifier"]) {
                Some((n, _)) => n,
                None => return false,
            };
            let protocols = protocol_reference_list(cursor, source);
            if walker.kinds.is_enabled("P") {
                let scope = walker.scopes.current_field();
                let mut tag = make_tag(name.clone(), line, "P", scope);
                if let Some(p) = &protocols {
                    tag.extension_fields
                        .push(("protocols".to_string(), p.clone()));
                }
                walker.tags.push(tag);
            }
            let prev_cat = walker.current_category.clone();
            walker.scopes.push(ScopeKind::Protocol, &name);
            walker.opens.push(Open::ScopeWithCategory(prev_cat));
            true
        }
        "method_declaration" | "method_definition" => {
            let is_class = node_text(node, source).trim_start().starts_with('+');
            let kind = if is_class { "c" } else { "m" };
            if walker.kinds.is_enabled(kind) {
                if let Some(name) = method_selector(cursor, source) {
                    let scope = walker.scopes.current_field();
                    let mut tag = make_tag(name, line, kind, scope);
                    if let Some(cat) = &walker.current_category {
                        tag.extension_fields
                            .push(("category".to_string(), cat.clone()));
                    }
                    walker.tags.push(tag);
                }
            }
            false
        }
        "property_declaration" => {
            if walker.kinds.is_enabled("p") {
                let scope = walker.scopes.current_field();
                emit_struct_declarators(cursor, source, line, "p", scope, &mut walker.tags);
            }
            false
        }
        "instance_variable" => {
            if walker.kinds.is_enabled("E") {
                let scope = walker.scopes.current_field();
                emit_struct_declarators(cursor, source, line, "E", scope, &mut walker.tags);
            }
            false
        }
        "field_declaration" => {
            if walker.kinds.is_enabled("E") {
                let scope = walker.scopes.current_field();
                for_each_child!(cursor, {
                    if cursor.field_name() == Some("declarator") {
                        if let Some((name, is_func)) = declarator_name(cursor.node(), source) {
                            if !is_func {
                                walker.tags.push(make_tag(name, line, "E", scope));
                            }
                        }
                    }
                });
            }
            false
        }
        "struct_specifier" | "union_specifier" => {
            if let Some(nm) = node.child_by_field_name("name") {
                if node.child_by_field_name("body").is_some() {
                    let name = node_text(nm, source).to_string();
                    if walker.kinds.is_enabled("s") {
                        let scope = walker.scopes.current_field();
                        walker.tags.push(make_tag(name.clone(), line, "s", scope));
                    }
                    let prev_cat = walker.current_category.clone();
                    walker.scopes.push(ScopeKind::Struct, &name);
                    walker.opens.push(Open::ScopeWithCategory(prev_cat));
                    return true;
                }
            }
            false
        }
        "enum_specifier" => {
            if let Some(nm) = node.child_by_field_name("name") {
                if walker.kinds.is_enabled("e") {
                    let scope = walker.scopes.current_field();
                    walker.tags.push(make_tag(
                        node_text(nm, source).to_string(),
                        line,
                        "e",
                        scope,
                    ));
                }
            }
            false
        }
        "type_definition" => {
            // `typedef NS_ENUM(BaseType, EnumName) { ... }` — the macro carries
            // the enum name; emit it as an enum rather than a bogus typedef.
            if let Some(tn) = node.child_by_field_name("type") {
                if tn.kind() == "macro_type_specifier" {
                    let macro_name = tn
                        .child_by_field_name("name")
                        .map(|n| node_text(n, source))
                        .unwrap_or_default();
                    if macro_name == "NS_ENUM" || macro_name == "NS_OPTIONS" {
                        if walker.kinds.is_enabled("e") {
                            if let Some(ty) = tn.child_by_field_name("type") {
                                let enum_name = node_text(ty, source).trim().to_string();
                                if !enum_name.is_empty() {
                                    let scope = walker.scopes.current_field();
                                    walker.tags.push(make_tag(enum_name, line, "e", scope));
                                }
                            }
                        }
                        return false;
                    }
                }
            }

            let declarator = node.child_by_field_name("declarator");
            let name = declarator
                .and_then(|d| declarator_name(d, source))
                .map(|(n, _)| n);
            // Point the typedef at the line where its name is written (e.g. the
            // `} KlassCoordinate;` line of a typedef'd anonymous struct).
            let name_line = declarator.map(line_of).unwrap_or(line);
            if let Some(name) = name {
                if walker.kinds.is_enabled("t") {
                    let scope = walker.scopes.current_field();
                    walker
                        .tags
                        .push(make_tag(name.clone(), name_line, "t", scope));
                }
                // Anonymous struct/union body: scope its members under the alias.
                if let Some(tn) = node.child_by_field_name("type") {
                    if matches!(tn.kind(), "struct_specifier" | "union_specifier")
                        && tn.child_by_field_name("name").is_none()
                        && tn.child_by_field_name("body").is_some()
                    {
                        let prev_cat = walker.current_category.clone();
                        walker.scopes.push(ScopeKind::Struct, &name);
                        walker.opens.push(Open::ScopeWithCategory(prev_cat));
                        return true;
                    }
                }
            }
            false
        }
        "function_definition" => {
            if walker.in_body == 0 && walker.kinds.is_enabled("f") {
                if let Some(decl) = node.child_by_field_name("declarator") {
                    if let Some((name, _)) = declarator_name(decl, source) {
                        let scope = walker.scopes.current_field();
                        walker.tags.push(make_tag(name, line, "f", scope));
                    }
                }
            }
            false
        }
        "declaration" => {
            if walker.in_body == 0
                && walker.scopes.current_field().is_none()
                && walker.kinds.is_enabled("v")
            {
                for_each_child!(cursor, {
                    if cursor.field_name() == Some("declarator") {
                        if let Some((name, is_func)) = declarator_name(cursor.node(), source) {
                            if !is_func {
                                walker.tags.push(make_tag(name, line, "v", None));
                            }
                        }
                    }
                });
            }
            false
        }
        "compound_statement" => {
            walker.in_body += 1;
            walker.opens.push(Open::Body);
            true
        }
        _ => false,
    }
}

fn generate_tags(parser: &mut TsParser, req: &Request, source: &[u8]) -> Result<Vec<Tag>, String> {
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "parse failed".to_string())?;

    let mut walker = ObjcWalker {
        source,
        scopes: ScopeStack::new(),
        kinds: TagKindConfig::parse(&req.kinds, OBJC_DEFAULT_KINDS, OBJC_OPTIONAL_KINDS),
        tags: Vec::new(),
        current_category: None,
        in_body: 0,
        opens: Vec::new(),
    };

    let mut cursor = tree.walk();
    if cursor.goto_first_child() {
        walk_tree(&mut cursor, &mut walker);
    }

    Ok(walker.tags)
}
