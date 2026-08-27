wit_bindgen::generate!({
    world: "plugin-world",
    path: "../../wit",
});

use exports::treetags::plugin::plugin::{Guest, Request, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};
use treetags_plugin_common::{
    for_each_child, line_of, node_text, walk_tree, ScopeKey, ScopeStack, TagKindConfig, WalkContext,
};

struct DartPlugin;

impl Guest for DartPlugin {
    fn generate(req: Request, source: Vec<u8>) -> Result<Vec<Tag>, String> {
        let mut parser = TsParser::new();
        let language: tree_sitter::Language = tree_sitter_dart::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|e| format!("set_language: {e}"))?;
        generate_tags(&mut parser, &req, &source)
    }
}

export!(DartPlugin);

// These tables must stay in sync with plugin.toml.
const DART_DEFAULT_KINDS: &[(&[&str], &str)] = &[
    (&["c", "class"], "c"),
    (&["M", "mixin"], "M"),
    (&["g", "enum"], "g"),
    (&["e", "enumerator"], "e"),
    (&["x", "extension"], "x"),
    (&["t", "typedef"], "t"),
    (&["f", "function"], "f"),
    (&["m", "method"], "m"),
    (&["p", "property"], "p"),
    (&["F", "field"], "F"),
    (&["v", "variable"], "v"),
];

const DART_OPTIONAL_KINDS: &[(&[&str], &str)] =
    &[(&["l", "local"], "l"), (&["z", "parameter"], "z")];

#[derive(Clone, Copy, PartialEq)]
enum ScopeKind {
    Class,
    Mixin,
    Enum,
    Extension,
    Function,
}

impl ScopeKey for ScopeKind {
    fn key(self) -> &'static str {
        match self {
            ScopeKind::Class => "class",
            ScopeKind::Mixin => "mixin",
            ScopeKind::Enum => "enum",
            ScopeKind::Extension => "extension",
            ScopeKind::Function => "function",
        }
    }
}

enum SigClass {
    /// a method or function.
    Callable,
    /// a property accessor.
    Property,
    /// any of the constructor signatures.
    Constructor,
}

fn classify_sig(kind: &str) -> Option<SigClass> {
    match kind {
        "function_signature" | "operator_signature" => Some(SigClass::Callable),
        "getter_signature" | "setter_signature" => Some(SigClass::Property),
        "constructor_signature"
        | "constant_constructor_signature"
        | "factory_constructor_signature"
        | "redirecting_factory_constructor_signature" => Some(SigClass::Constructor),
        _ => None,
    }
}

struct DartWalker<'src> {
    source: &'src [u8],
    scopes: ScopeStack<ScopeKind>,
    kinds: TagKindConfig,
    tags: Vec<Tag>,
}

impl WalkContext for DartWalker<'_> {
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

fn end_line(node: Node) -> Option<u32> {
    Some(node.end_position().row as u32 + 1)
}

fn access_of_name(name: &str) -> Option<String> {
    name.starts_with('_').then(|| "private".to_string())
}

fn type_base_name(node: Node, source: &[u8]) -> Option<String> {
    let first = node.named_child(0)?;
    (first.kind() == "type_identifier").then(|| node_text(first, source).to_string())
}

fn named_child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut i = 0;
    while let Some(child) = node.named_child(i) {
        if child.kind() == kind {
            return Some(child);
        }
        i += 1;
    }
    None
}

fn process_node_inner(source: &[u8], cursor: &mut TreeCursor, w: &mut DartWalker) -> bool {
    match cursor.node().kind() {
        "class_declaration" => emit_type(cursor, source, w, "c", ScopeKind::Class),
        "mixin_declaration" => emit_type(cursor, source, w, "M", ScopeKind::Mixin),
        "enum_declaration" => emit_type(cursor, source, w, "g", ScopeKind::Enum),
        "extension_declaration" => emit_extension(cursor, source, w),
        "extension_type_declaration" => emit_extension_type(cursor, source, w),
        "extension_type_representation" => {
            emit_representation(cursor, source, w);
            false
        }
        "enum_constant" => {
            emit_enum_constant(cursor, source, w);
            false
        }
        "type_alias" => {
            emit_typedef(cursor, source, w);
            false
        }
        "top_level_variable_declaration" => {
            emit_fields(cursor, source, w, "v");
            false
        }
        "function_declaration" => emit_signature_decl(cursor, source, w, "f"),
        "local_function_declaration" => emit_local_function(cursor, source, w),
        "getter_declaration" | "setter_declaration" => emit_signature_decl(cursor, source, w, "p"),
        "method_declaration" => emit_method(cursor, source, w),
        "declaration" => {
            emit_member_declaration(cursor, source, w);
            false
        }
        "local_variable_declaration" => {
            emit_locals(cursor, source, w);
            false
        }
        "formal_parameter" => {
            emit_parameter(cursor, source, w);
            false
        }
        _ => false,
    }
}

/// Emit a class / mixin / enum tag and open its scope.
fn emit_type(
    cursor: &mut TreeCursor,
    source: &[u8],
    w: &mut DartWalker,
    letter: &str,
    scope_kind: ScopeKind,
) -> bool {
    let node = cursor.node();
    let (name, line) = type_name_line(cursor, source);

    if w.kinds.is_enabled(letter) {
        let mut tag = make_tag(name.clone(), line, letter, w.scopes.current_field());
        add_field(&mut tag, "access", access_of_name(&name));
        add_field(&mut tag, "inherits", inherits_of(cursor, source));
        tag.end_line = end_line(node);
        w.tags.push(tag);
    }
    w.scopes.push(scope_kind, &name);
    true
}

/// The declared name + line of a class/mixin/enum. Handles the
/// `class Bar = Foo with M;` form, whose name sits inside a
/// `mixin_application_class` rather than the `name` field.
fn type_name_line(cursor: &mut TreeCursor, source: &[u8]) -> (String, u32) {
    let node = cursor.node();
    if let Some(nm) = node.child_by_field_name("name") {
        return (node_text(nm, source).to_string(), line_of(nm));
    }
    let mut result = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "mixin_application_class" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "identifier" {
                    let id = cursor.node();
                    result = Some((node_text(id, source).to_string(), line_of(id)));
                    break;
                }
            });
            break;
        }
    });
    result.unwrap_or_else(|| (String::new(), line_of(node)))
}

fn inherits_of(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    let mut names = Vec::new();
    for_each_child!(cursor, {
        match cursor.node().kind() {
            // `type` direct child = a mixin's `on` constraint.
            "type" => {
                if let Some(n) = type_base_name(cursor.node(), source) {
                    names.push(n);
                }
            }
            "superclass" => {
                for_each_child!(cursor, {
                    match cursor.node().kind() {
                        "type" => {
                            if let Some(n) = type_base_name(cursor.node(), source) {
                                names.push(n);
                            }
                        }
                        "mixins" => collect_types(cursor, source, &mut names),
                        _ => {}
                    }
                });
            }
            "mixins" | "interfaces" => collect_types(cursor, source, &mut names),
            "mixin_application_class" => {
                for_each_child!(cursor, {
                    if cursor.node().kind() == "mixin_application" {
                        for_each_child!(cursor, {
                            match cursor.node().kind() {
                                "type" => {
                                    if let Some(n) = type_base_name(cursor.node(), source) {
                                        names.push(n);
                                    }
                                }
                                "mixins" => collect_types(cursor, source, &mut names),
                                _ => {}
                            }
                        });
                    }
                });
            }
            _ => {}
        }
    });
    (!names.is_empty()).then(|| names.join(","))
}

fn collect_types(cursor: &mut TreeCursor, source: &[u8], names: &mut Vec<String>) {
    for_each_child!(cursor, {
        if cursor.node().kind() == "type" {
            if let Some(n) = type_base_name(cursor.node(), source) {
                names.push(n);
            }
        }
    });
}

fn emit_extension(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) -> bool {
    let node = cursor.node();
    let on_type = node
        .child_by_field_name("class")
        .and_then(|t| type_base_name(t, source));
    let (name, line) = match node.child_by_field_name("name") {
        Some(nm) => (node_text(nm, source).to_string(), line_of(nm)),
        None => (
            on_type.clone().unwrap_or_else(|| "extension".to_string()),
            line_of(node),
        ),
    };

    if w.kinds.is_enabled("x") {
        let mut tag = make_tag(name.clone(), line, "x", w.scopes.current_field());
        add_field(&mut tag, "access", access_of_name(&name));
        add_field(
            &mut tag,
            "typeref",
            on_type.map(|t| format!("typename:{t}")),
        );
        tag.end_line = end_line(node);
        w.tags.push(tag);
    }
    w.scopes.push(ScopeKind::Extension, &name);
    true
}

fn emit_extension_type(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) -> bool {
    let node = cursor.node();
    let name_node = node
        .child_by_field_name("name")
        .and_then(|n| named_child_of_kind(n, "identifier"));
    let (name, line) = match name_node {
        Some(id) => (node_text(id, source).to_string(), line_of(id)),
        None => (String::new(), line_of(node)),
    };
    let repr_type = node
        .child_by_field_name("representation")
        .and_then(|r| r.child_by_field_name("type"))
        .map(|t| format!("typename:{}", node_text(t, source)));

    if w.kinds.is_enabled("x") {
        let mut tag = make_tag(name.clone(), line, "x", w.scopes.current_field());
        add_field(&mut tag, "access", access_of_name(&name));
        add_field(&mut tag, "typeref", repr_type);
        add_field(&mut tag, "inherits", inherits_of(cursor, source));
        tag.end_line = end_line(node);
        w.tags.push(tag);
    }
    w.scopes.push(ScopeKind::Extension, &name);
    true
}

fn emit_representation(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    if !w.kinds.is_enabled("F") {
        return;
    }
    let node = cursor.node();
    let Some(nm) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(nm, source).to_string();
    let mut tag = make_tag(name.clone(), line_of(nm), "F", w.scopes.current_field());
    add_field(&mut tag, "access", access_of_name(&name));
    if let Some(t) = node.child_by_field_name("type") {
        add_field(
            &mut tag,
            "typeref",
            Some(format!("typename:{}", node_text(t, source))),
        );
    }
    w.tags.push(tag);
}

fn emit_enum_constant(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    if !w.kinds.is_enabled("e") {
        return;
    }
    if let Some(nm) = cursor.node().child_by_field_name("name") {
        let tag = make_tag(
            node_text(nm, source).to_string(),
            line_of(nm),
            "e",
            w.scopes.current_field(),
        );
        w.tags.push(tag);
    }
}

fn emit_typedef(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    if !w.kinds.is_enabled("t") {
        return;
    }
    let mut name_line = None;
    let mut value = None;
    for_each_child!(cursor, {
        match cursor.node().kind() {
            "type_identifier" if name_line.is_none() => {
                let id = cursor.node();
                name_line = Some((node_text(id, source).to_string(), line_of(id)));
            }
            "type" => value = Some(node_text(cursor.node(), source).to_string()),
            _ => {}
        }
    });
    if let Some((name, line)) = name_line {
        let mut tag = make_tag(name.clone(), line, "t", w.scopes.current_field());
        add_field(&mut tag, "access", access_of_name(&name));
        add_field(&mut tag, "typeref", value.map(|v| format!("typename:{v}")));
        w.tags.push(tag);
    }
}

fn emit_fields(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker, letter: &str) {
    if !w.kinds.is_enabled(letter) {
        return;
    }
    let mut typeref = None;
    for_each_child!(cursor, {
        match cursor.node().kind() {
            "type" => typeref = Some(format!("typename:{}", node_text(cursor.node(), source))),
            "initialized_identifier_list" | "static_final_declaration_list" => {
                for_each_child!(cursor, {
                    let entry = cursor.node();
                    if matches!(
                        entry.kind(),
                        "initialized_identifier" | "static_final_declaration"
                    ) {
                        if let Some(nm) = entry.child_by_field_name("name") {
                            let name = node_text(nm, source).to_string();
                            let mut tag = make_tag(
                                name.clone(),
                                line_of(nm),
                                letter,
                                w.scopes.current_field(),
                            );
                            add_field(&mut tag, "access", access_of_name(&name));
                            add_field(&mut tag, "typeref", typeref.clone());
                            w.tags.push(tag);
                        }
                    }
                });
            }
            _ => {}
        }
    });
}

fn emit_locals(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    if !w.kinds.is_enabled("l") {
        return;
    }
    for_each_child!(cursor, {
        if cursor.node().kind() == "initialized_variable_definition" {
            if let Some(nm) = cursor.node().child_by_field_name("name") {
                let tag = make_tag(
                    node_text(nm, source).to_string(),
                    line_of(nm),
                    "l",
                    w.scopes.current_field(),
                );
                w.tags.push(tag);
            }
        }
    });
}

fn emit_parameter(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    if !w.kinds.is_enabled("z") {
        return;
    }
    let node = cursor.node();
    if let Some(nm) = node.child_by_field_name("name") {
        let mut tag = make_tag(
            node_text(nm, source).to_string(),
            line_of(nm),
            "z",
            w.scopes.current_field(),
        );
        for_each_child!(cursor, {
            if cursor.node().kind() == "type" {
                add_field(
                    &mut tag,
                    "typeref",
                    Some(format!("typename:{}", node_text(cursor.node(), source))),
                );
                break;
            }
        });
        w.tags.push(tag);
    }
}

fn emit_signature_decl(
    cursor: &mut TreeCursor,
    source: &[u8],
    w: &mut DartWalker,
    letter: &str,
) -> bool {
    let node = cursor.node();
    let Some(sig) = node.child_by_field_name("signature") else {
        return false;
    };
    let (name, line) = callable_name_line(sig, source);
    callable_tag(w, source, node, sig, letter, name.clone(), line);
    push_body_scope(node, w, &name)
}

fn emit_local_function(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) -> bool {
    let node = cursor.node();
    let mut sig = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "function_signature" {
            sig = Some(cursor.node());
            break;
        }
    });
    let Some(sig) = sig else { return false };
    let (name, line) = callable_name_line(sig, source);
    callable_tag(w, source, node, sig, "f", name.clone(), line);
    push_body_scope(node, w, &name)
}

fn emit_method(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) -> bool {
    let node = cursor.node();
    let mut sig = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "method_signature" {
            for_each_child!(cursor, {
                if classify_sig(cursor.node().kind()).is_some() {
                    sig = Some(cursor.node());
                    break;
                }
            });
            break;
        }
    });
    let Some(sig) = sig else { return false };
    let letter = member_letter(&sig);
    let (name, line) = callable_name_line(sig, source);
    callable_tag(w, source, node, sig, letter, name.clone(), line);
    push_body_scope(node, w, &name)
}

fn emit_member_declaration(cursor: &mut TreeCursor, source: &[u8], w: &mut DartWalker) {
    let mut sig = None;
    for_each_child!(cursor, {
        if classify_sig(cursor.node().kind()).is_some() {
            sig = Some(cursor.node());
            break;
        }
    });
    match sig {
        Some(sig) => {
            let letter = member_letter(&sig);
            let (name, line) = callable_name_line(sig, source);
            callable_tag(w, source, cursor.node(), sig, letter, name, line);
        }
        None => emit_fields(cursor, source, w, "F"),
    }
}

fn member_letter(sig: &Node) -> &'static str {
    match classify_sig(sig.kind()) {
        Some(SigClass::Property) => "p",
        _ => "m",
    }
}

fn push_body_scope(node: Node, w: &mut DartWalker, name: &str) -> bool {
    if node.child_by_field_name("body").is_some() {
        w.scopes.push(ScopeKind::Function, name);
        true
    } else {
        false
    }
}

fn callable_name_line(sig: Node, source: &[u8]) -> (String, u32) {
    match sig.kind() {
        "operator_signature" => match sig.child_by_field_name("operator") {
            Some(op) => (format!("operator {}", node_text(op, source)), line_of(op)),
            None => ("operator".to_string(), line_of(sig)),
        },
        "constructor_signature"
        | "constant_constructor_signature"
        | "factory_constructor_signature"
        | "redirecting_factory_constructor_signature" => {
            (constructor_name(sig, source), line_of(sig))
        }
        _ => match sig.child_by_field_name("name") {
            Some(nm) => (node_text(nm, source).to_string(), line_of(nm)),
            None => (String::new(), line_of(sig)),
        },
    }
}

fn constructor_name(sig: Node, source: &[u8]) -> String {
    let text = node_text(sig, source);
    let head = text.split('(').next().unwrap_or(text);
    head.split_whitespace().last().unwrap_or("").to_string()
}

fn callable_tag(
    w: &mut DartWalker,
    source: &[u8],
    wrapper: Node,
    sig: Node,
    letter: &str,
    name: String,
    line: u32,
) {
    if !w.kinds.is_enabled(letter) {
        return;
    }
    let mut tag = make_tag(name.clone(), line, letter, w.scopes.current_field());
    add_field(&mut tag, "access", access_of_name(&name));
    if let Some(params) = named_child_of_kind(sig, "formal_parameter_list") {
        tag.extension_fields.push((
            "signature".to_string(),
            node_text(params, source).to_string(),
        ));
    }
    if let Some(rt) = sig.child_by_field_name("return_type") {
        add_field(
            &mut tag,
            "typeref",
            Some(format!("typename:{}", node_text(rt, source))),
        );
    }
    tag.end_line = end_line(wrapper);
    w.tags.push(tag);
}

fn generate_tags(parser: &mut TsParser, req: &Request, source: &[u8]) -> Result<Vec<Tag>, String> {
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "parse failed".to_string())?;

    let mut walker = DartWalker {
        source,
        scopes: ScopeStack::new(),
        kinds: TagKindConfig::parse(&req.kinds, DART_DEFAULT_KINDS, DART_OPTIONAL_KINDS),
        tags: Vec::new(),
    };

    let mut cursor = tree.walk();
    if cursor.goto_first_child() {
        walk_tree(&mut cursor, &mut walker);
    }

    Ok(walker.tags)
}
