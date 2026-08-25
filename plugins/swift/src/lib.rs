wit_bindgen::generate!({
    world: "plugin-world",
    path: "../../wit",
});

use exports::treetags::plugin::plugin::{Guest, Request, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};
use treetags_plugin_common::{
    child_ident, for_each_child, has_child, line_of, node_text, walk_tree, ScopeKey, ScopeStack,
    TagKindConfig, WalkContext,
};

struct SwiftPlugin;

impl Guest for SwiftPlugin {
    fn generate(req: Request, source: Vec<u8>) -> Result<Vec<Tag>, String> {
        let mut parser = TsParser::new();
        let language: tree_sitter::Language = tree_sitter_swift::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|e| format!("set_language: {e}"))?;
        generate_tags(&mut parser, &req, &source)
    }
}

export!(SwiftPlugin);

// This table must stay in sync with plugin.toml.
const SWIFT_DEFAULT_KINDS: &[(&[&str], &str)] = &[
    (&["c", "class"], "c"),
    (&["s", "struct"], "s"),
    (&["P", "protocol"], "P"),
    (&["g", "enum"], "g"),
    (&["e", "enumerator"], "e"),
    (&["x", "extension"], "x"),
    (&["a", "actor"], "a"),
    (&["f", "function"], "f"),
    (&["m", "method"], "m"),
    (&["p", "property"], "p"),
    (&["v", "variable"], "v"),
    (&["t", "typealias"], "t"),
    (&["A", "associatedtype"], "A"),
    (&["o", "operator"], "o"),
];

const SWIFT_OPTIONAL_KINDS: &[(&[&str], &str)] =
    &[(&["l", "local"], "l"), (&["z", "parameter"], "z")];

#[derive(Clone, Copy, PartialEq)]
enum ScopeKind {
    Class,
    Struct,
    Enum,
    Actor,
    Extension,
    Protocol,
    Function,
}

impl ScopeKey for ScopeKind {
    fn key(self) -> &'static str {
        match self {
            ScopeKind::Class => "class",
            ScopeKind::Struct => "struct",
            ScopeKind::Enum => "enum",
            ScopeKind::Actor => "actor",
            ScopeKind::Extension => "extension",
            ScopeKind::Protocol => "protocol",
            ScopeKind::Function => "function",
        }
    }
}

impl ScopeKind {
    /// Whether members declared directly inside this scope are type members
    /// (method/property) rather than locals.
    fn is_type(self) -> bool {
        !matches!(self, ScopeKind::Function)
    }
}

struct SwiftWalker<'src> {
    source: &'src [u8],
    scopes: ScopeStack<ScopeKind>,
    kinds: TagKindConfig,
    tags: Vec<Tag>,
}

impl WalkContext for SwiftWalker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        process_node_inner(self.source, cursor, self)
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

fn make_tag(name: String, line: u32, kind: &str, scope: Option<(&str, &str)>) -> Tag {
    let mut ext = Vec::new();
    if let Some((key, value)) = scope {
        ext.push((key.to_string(), value.to_string()));
    }
    Tag {
        name,
        line,
        kind: kind.to_string(),
        end_line: None,
        extension_fields: ext,
    }
}

fn add_field(tag: &mut Tag, key: &str, value: Option<String>) {
    if let Some(value) = value {
        tag.extension_fields.push((key.to_string(), value));
    }
}

/// 1-based line of the node's last line, for the `end` field.
fn end_line(node: Node) -> Option<u32> {
    Some(node.end_position().row as u32 + 1)
}

/// The base identifier of a type name node: `Foo` for `Foo` or `Foo<Bar>`.
fn type_name(node: Node, source: &[u8]) -> String {
    if node.kind() == "user_type" {
        if let Some(id) = node.named_child(0) {
            return node_text(id, source).to_string();
        }
    }
    node_text(node, source).to_string()
}

/// Explicit access level (`public`, `private`, …) if one is written.
fn access_of(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut access = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "modifiers" {
            access = child_ident(cursor, source, &["visibility_modifier"]).map(|(text, _)| text);
            break;
        }
    });
    access
}

/// Comma-joined superclass/protocol conformances from `inheritance_specifier`
/// children (also covers an enum's raw-value type, e.g. `enum E: Int`)
fn inherits_of(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut names = Vec::new();
    for_each_child!(cursor, {
        if cursor.node().kind() == "inheritance_specifier" {
            let spec = cursor.node();
            let ty = spec.child_by_field_name("inherits_from").unwrap_or(spec);
            names.push(node_text(ty, source).to_string());
        }
    });
    (!names.is_empty()).then(|| names.join(","))
}

fn signature_of(cursor: &mut TreeCursor, source: &[u8]) -> String {
    let mut parts = Vec::new();
    for_each_child!(cursor, {
        if cursor.node().kind() == "parameter" {
            parts.push(node_text(cursor.node(), source).to_string());
        }
    });
    format!("({})", parts.join(", "))
}

fn return_typeref(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("return_type")
        .map(|rt| format!("typename:{}", node_text(rt, source)))
}

fn annotation_typeref(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut typeref = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "type_annotation" {
            if let Some(ty) = cursor.node().child_by_field_name("type") {
                typeref = Some(format!("typename:{}", node_text(ty, source)));
            }
            break;
        }
    });
    typeref
}

fn process_node_inner(source: &[u8], cursor: &mut TreeCursor, w: &mut SwiftWalker) -> bool {
    match cursor.node().kind() {
        "class_declaration" => emit_type(cursor, source, w),
        "protocol_declaration" => emit_protocol(cursor, source, w),
        "function_declaration" | "protocol_function_declaration" => {
            emit_function(cursor, source, w)
        }
        "init_declaration" => emit_named_method(cursor, source, w, "init", true),
        "deinit_declaration" => emit_named_method(cursor, source, w, "deinit", false),
        "subscript_declaration" => emit_subscript(cursor, source, w),
        "property_declaration" => emit_property(cursor, source, w),
        "protocol_property_declaration" => {
            emit_protocol_property(cursor, source, w);
            false
        }
        "enum_entry" => {
            emit_enum_entry(cursor, source, w);
            false
        }
        "typealias_declaration" => {
            emit_typealias(cursor, source, w);
            false
        }
        "associatedtype_declaration" => {
            emit_associatedtype(cursor, source, w);
            false
        }
        "operator_declaration" => {
            emit_operator(cursor, source, w);
            false
        }
        "parameter" => {
            emit_parameter(cursor, source, w);
            false
        }
        "lambda_literal" => {
            w.scopes.push(ScopeKind::Function, "__closure");
            true
        }
        _ => false,
    }
}

fn emit_type(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) -> bool {
    let node = cursor.node();
    let dk = node
        .child_by_field_name("declaration_kind")
        .map(|n| node_text(n, source))
        .unwrap_or("");
    let (scope_kind, letter) = match dk {
        "struct" => (ScopeKind::Struct, "s"),
        "enum" => (ScopeKind::Enum, "g"),
        "actor" => (ScopeKind::Actor, "a"),
        "extension" => (ScopeKind::Extension, "x"),
        _ => (ScopeKind::Class, "c"),
    };
    let name_node = node.child_by_field_name("name");
    let name = name_node.map(|n| type_name(n, source)).unwrap_or_default();
    let line = name_node.map(line_of).unwrap_or_else(|| line_of(node));

    if w.kinds.is_enabled(letter) {
        let mut tag = make_tag(name.clone(), line, letter, w.scopes.current_field());
        add_field(&mut tag, "access", access_of(cursor, source));
        add_field(&mut tag, "inherits", inherits_of(cursor, source));
        tag.end_line = end_line(node);
        w.tags.push(tag);
    }
    w.scopes.push(scope_kind, &name);
    true
}

fn emit_protocol(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) -> bool {
    let node = cursor.node();
    let name_node = node.child_by_field_name("name");
    let name = name_node
        .map(|n| node_text(n, source).to_string())
        .unwrap_or_default();
    let line = name_node.map(line_of).unwrap_or_else(|| line_of(node));

    if w.kinds.is_enabled("P") {
        let mut tag = make_tag(name.clone(), line, "P", w.scopes.current_field());
        add_field(&mut tag, "access", access_of(cursor, source));
        add_field(&mut tag, "inherits", inherits_of(cursor, source));
        tag.end_line = end_line(node);
        w.tags.push(tag);
    }
    w.scopes.push(ScopeKind::Protocol, &name);
    true
}

fn emit_function(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) -> bool {
    let node = cursor.node();
    let name_node = node.child_by_field_name("name");
    let name = name_node
        .map(|n| node_text(n, source).to_string())
        .unwrap_or_default();
    let line = name_node.map(line_of).unwrap_or_else(|| line_of(node));

    let letter = if w.scopes.last_key().is_some_and(ScopeKind::is_type) {
        "m"
    } else {
        "f"
    };

    if w.kinds.is_enabled(letter) {
        let mut tag = make_tag(name.clone(), line, letter, w.scopes.current_field());
        add_field(&mut tag, "access", access_of(cursor, source));
        tag.extension_fields
            .push(("signature".to_string(), signature_of(cursor, source)));
        add_field(&mut tag, "typeref", return_typeref(node, source));
        tag.end_line = end_line(node);
        w.tags.push(tag);
    }
    w.scopes.push(ScopeKind::Function, &name);
    true
}

fn emit_named_method(
    cursor: &mut TreeCursor,
    source: &[u8],
    w: &mut SwiftWalker,
    name: &str,
    with_signature: bool,
) -> bool {
    let node = cursor.node();
    if w.kinds.is_enabled("m") {
        let mut tag = make_tag(
            name.to_string(),
            line_of(node),
            "m",
            w.scopes.current_field(),
        );
        add_field(&mut tag, "access", access_of(cursor, source));
        if with_signature {
            tag.extension_fields
                .push(("signature".to_string(), signature_of(cursor, source)));
        }
        tag.end_line = end_line(node);
        w.tags.push(tag);
    }
    w.scopes.push(ScopeKind::Function, name);
    true
}

fn emit_subscript(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) -> bool {
    let node = cursor.node();
    if w.kinds.is_enabled("m") {
        let mut tag = make_tag(
            "subscript".to_string(),
            line_of(node),
            "m",
            w.scopes.current_field(),
        );
        add_field(&mut tag, "access", access_of(cursor, source));
        tag.extension_fields
            .push(("signature".to_string(), signature_of(cursor, source)));
        add_field(&mut tag, "typeref", return_typeref(node, source));
        tag.end_line = end_line(node);
        w.tags.push(tag);
    }
    w.scopes.push(ScopeKind::Function, "subscript");
    true
}

fn emit_property(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) -> bool {
    let letter = match w.scopes.last_key() {
        Some(s) if s.is_type() => "p",
        Some(_) => "l",
        None => "v",
    };

    let access = access_of(cursor, source);
    let typeref = annotation_typeref(cursor, source);

    let mut first_name = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "pattern" {
            if let Some((name, line)) = child_ident(cursor, source, &["simple_identifier"]) {
                if first_name.is_none() {
                    first_name = Some(name.clone());
                }
                if w.kinds.is_enabled(letter) {
                    let mut tag = make_tag(name, line, letter, w.scopes.current_field());
                    add_field(&mut tag, "access", access.clone());
                    add_field(&mut tag, "typeref", typeref.clone());
                    w.tags.push(tag);
                }
            }
        }
    });

    if has_child(cursor, &["computed_property", "willset_didset_block"]) {
        w.scopes
            .push(ScopeKind::Function, first_name.as_deref().unwrap_or("_"));
        true
    } else {
        false
    }
}

fn emit_protocol_property(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.kinds.is_enabled("p") {
        return;
    }
    let access = access_of(cursor, source);
    let typeref = annotation_typeref(cursor, source);

    let mut name_line = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "pattern" {
            name_line = child_ident(cursor, source, &["simple_identifier"]);
            break;
        }
    });

    if let Some((name, line)) = name_line {
        let mut tag = make_tag(name, line, "p", w.scopes.current_field());
        add_field(&mut tag, "access", access);
        add_field(&mut tag, "typeref", typeref);
        w.tags.push(tag);
    }
}

fn emit_enum_entry(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.kinds.is_enabled("e") {
        return;
    }
    for_each_child!(cursor, {
        if cursor.node().kind() == "simple_identifier" {
            let n = cursor.node();
            let tag = make_tag(
                node_text(n, source).to_string(),
                line_of(n),
                "e",
                w.scopes.current_field(),
            );
            w.tags.push(tag);
        }
    });
}

fn emit_typealias(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.kinds.is_enabled("t") {
        return;
    }
    let node = cursor.node();
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let mut tag = make_tag(
        type_name(name_node, source),
        line_of(name_node),
        "t",
        w.scopes.current_field(),
    );
    add_field(&mut tag, "access", access_of(cursor, source));
    if let Some(value) = node.child_by_field_name("value") {
        add_field(
            &mut tag,
            "typeref",
            Some(format!("typename:{}", node_text(value, source))),
        );
    }
    w.tags.push(tag);
}

fn emit_associatedtype(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.kinds.is_enabled("A") {
        return;
    }
    let node = cursor.node();
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let tag = make_tag(
        type_name(name_node, source),
        line_of(name_node),
        "A",
        w.scopes.current_field(),
    );
    w.tags.push(tag);
}

fn emit_operator(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.kinds.is_enabled("o") {
        return;
    }
    if let Some((name, line)) = child_ident(cursor, source, &["custom_operator", "bang"]) {
        let tag = make_tag(name, line, "o", w.scopes.current_field());
        w.tags.push(tag);
    }
}

fn emit_parameter(cursor: &mut TreeCursor, source: &[u8], w: &mut SwiftWalker) {
    if !w.kinds.is_enabled("z") {
        return;
    }
    let node = cursor.node();
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let mut tag = make_tag(
        node_text(name_node, source).to_string(),
        line_of(name_node),
        "z",
        w.scopes.current_field(),
    );
    if let Some(ty) = node.child_by_field_name("type") {
        add_field(
            &mut tag,
            "typeref",
            Some(format!("typename:{}", node_text(ty, source))),
        );
    }
    w.tags.push(tag);
}

fn generate_tags(parser: &mut TsParser, req: &Request, source: &[u8]) -> Result<Vec<Tag>, String> {
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "parse failed".to_string())?;

    let mut walker = SwiftWalker {
        source,
        scopes: ScopeStack::new(),
        kinds: TagKindConfig::parse(&req.kinds, SWIFT_DEFAULT_KINDS, SWIFT_OPTIONAL_KINDS),
        tags: Vec::new(),
    };

    let mut cursor = tree.walk();
    if cursor.goto_first_child() {
        walk_tree(&mut cursor, &mut walker);
    }

    Ok(walker.tags)
}
