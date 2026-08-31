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

struct ZigPlugin;

impl Guest for ZigPlugin {
    fn generate(req: Request, source: Vec<u8>) -> Result<Vec<Tag>, String> {
        let mut parser = TsParser::new();
        let language: tree_sitter::Language = tree_sitter_zig::LANGUAGE.into();
        parser
            .set_language(&language)
            .map_err(|e| format!("set_language: {e}"))?;
        generate_tags(&mut parser, &req, &source)
    }
}

export!(ZigPlugin);

// These tables must stay in sync with plugin.toml.
const ZIG_DEFAULT_KINDS: &[(&[&str], &str)] = &[
    (&["s", "struct"], "s"),
    (&["u", "union"], "u"),
    (&["g", "enum"], "g"),
    (&["e", "enumerator"], "e"),
    (&["o", "opaque"], "o"),
    (&["r", "errorSet"], "r"),
    (&["E", "error"], "E"),
    (&["t", "typealias"], "t"),
    (&["f", "function"], "f"),
    (&["F", "field"], "F"),
    (&["C", "constant"], "C"),
    (&["v", "variable"], "v"),
    (&["n", "namespace"], "n"),
    (&["T", "test"], "T"),
];

const ZIG_OPTIONAL_KINDS: &[(&[&str], &str)] =
    &[(&["l", "local"], "l"), (&["z", "parameter"], "z")];

#[derive(Clone, Copy, PartialEq)]
enum ScopeKind {
    Struct,
    Union,
    Enum,
    Opaque,
    ErrorSet,
    Function,
    Test,
}

impl ScopeKey for ScopeKind {
    fn key(self) -> &'static str {
        match self {
            ScopeKind::Struct => "struct",
            ScopeKind::Union => "union",
            ScopeKind::Enum => "enum",
            ScopeKind::Opaque => "opaque",
            ScopeKind::ErrorSet => "errorSet",
            ScopeKind::Function => "function",
            ScopeKind::Test => "test",
        }
    }
}

struct ZigWalker<'src> {
    source: &'src [u8],
    scopes: ScopeStack<ScopeKind>,
    kinds: TagKindConfig,
    tags: Vec<Tag>,
}

impl WalkContext for ZigWalker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        process_node_inner(self.source, cursor, self)
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

fn make_tag(name: String, line: u32, kind: &str, scope: Option<(&str, &str)>) -> Tag {
    let mut extension_fields = Vec::new();
    if let Some((key, value)) = scope {
        extension_fields.push((key.to_string(), value.to_string()));
    }
    Tag {
        name,
        line,
        kind: kind.to_string(),
        end_line: None,
        extension_fields,
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

fn process_node_inner(source: &[u8], cursor: &mut TreeCursor, w: &mut ZigWalker) -> bool {
    match cursor.node().kind() {
        "variable_declaration" => emit_variable(cursor, source, w),
        "function_declaration" => emit_function(cursor, source, w),
        "test_declaration" => emit_test(cursor, source, w),
        "container_field" => {
            emit_container_field(cursor, source, w);
            false
        }
        "error_set_declaration" => {
            emit_errors(cursor, source, w);
            false
        }
        "parameter" => {
            emit_parameter(cursor, source, w);
            false
        }
        _ => false,
    }
}

fn is_local(w: &ZigWalker) -> bool {
    matches!(
        w.scopes.last_key(),
        Some(ScopeKind::Function | ScopeKind::Test)
    )
}

fn access_of(node: Node, source: &[u8]) -> String {
    if node_text(node, source).trim_start().starts_with("pub ") {
        "public".to_string()
    } else {
        "private".to_string()
    }
}

fn initializer_kind(cursor: &mut TreeCursor) -> Option<(&'static str, ScopeKind)> {
    let mut result = None;
    for_each_child!(cursor, {
        result = match cursor.node().kind() {
            "struct_declaration" => Some(("s", ScopeKind::Struct)),
            "union_declaration" => Some(("u", ScopeKind::Union)),
            "enum_declaration" => Some(("g", ScopeKind::Enum)),
            "opaque_declaration" => Some(("o", ScopeKind::Opaque)),
            "error_set_declaration" => Some(("r", ScopeKind::ErrorSet)),
            _ => result,
        };
        if result.is_some() {
            break;
        }
    });
    result
}

fn initializer_text(node: Node, source: &[u8]) -> Option<String> {
    let text = node_text(node, source);
    let (_, value) = text.split_once('=')?;
    Some(value.trim().trim_end_matches(';').trim().to_string())
}

fn declared_type(cursor: &mut TreeCursor, source: &[u8]) -> Option<String> {
    cursor
        .node()
        .child_by_field_name("type")
        .map(|node| node_text(node, source).to_string())
}

fn emit_variable(cursor: &mut TreeCursor, source: &[u8], w: &mut ZigWalker) -> bool {
    let node = cursor.node();
    let Some((name, line)) = child_ident(cursor, source, &["identifier"]) else {
        return false;
    };
    if name == "_" {
        return false;
    }
    let container = initializer_kind(cursor);
    let local = is_local(w);
    let initializer = initializer_text(node, source);

    let letter = if let Some((letter, _)) = container {
        letter
    } else if local {
        "l"
    } else if initializer
        .as_deref()
        .is_some_and(|value| value.starts_with("@import("))
    {
        "n"
    } else {
        let text = node_text(node, source).trim_start();
        let without_pub = text.strip_prefix("pub ").unwrap_or(text).trim_start();
        let without_linkage = without_pub
            .strip_prefix("export ")
            .or_else(|| without_pub.strip_prefix("threadlocal "))
            .unwrap_or(without_pub)
            .trim_start();
        if declared_type(cursor, source).as_deref() == Some("type") {
            "t"
        } else if without_linkage.starts_with("var ") || without_linkage.starts_with("extern ") {
            "v"
        } else {
            "C"
        }
    };

    if w.kinds.is_enabled(letter) {
        let mut tag = make_tag(name.clone(), line, letter, w.scopes.current_field());
        if !local {
            add_field(&mut tag, "access", Some(access_of(node, source)));
        }
        if container.is_none() {
            add_field(
                &mut tag,
                "typeref",
                if letter == "t" {
                    initializer.clone().map(|ty| format!("typename:{ty}"))
                } else {
                    declared_type(cursor, source).map(|ty| format!("typename:{ty}"))
                },
            );
        }
        if container.is_some() {
            tag.end_line = end_line(node);
        }
        w.tags.push(tag);
    }

    if let Some((_, scope_kind)) = container {
        w.scopes.push(scope_kind, &name);
        true
    } else {
        false
    }
}

fn emit_function(cursor: &mut TreeCursor, source: &[u8], w: &mut ZigWalker) -> bool {
    let node = cursor.node();
    let Some(name_node) = node.child_by_field_name("name") else {
        return false;
    };
    let name = node_text(name_node, source).to_string();

    if w.kinds.is_enabled("f") {
        let mut tag = make_tag(
            name.clone(),
            line_of(name_node),
            "f",
            w.scopes.current_field(),
        );
        add_field(&mut tag, "access", Some(access_of(node, source)));
        let mut signature = None;
        for_each_child!(cursor, {
            if cursor.node().kind() == "parameters" {
                signature = Some(node_text(cursor.node(), source).to_string());
                break;
            }
        });
        add_field(&mut tag, "signature", signature);
        add_field(
            &mut tag,
            "typeref",
            node.child_by_field_name("type")
                .map(|ty| format!("typename:{}", node_text(ty, source))),
        );
        add_field(&mut tag, "implementation", implementation_of(node, source));
        tag.end_line = end_line(node);
        w.tags.push(tag);
    }

    if node.child_by_field_name("body").is_some() {
        w.scopes.push(ScopeKind::Function, &name);
        true
    } else {
        false
    }
}

fn implementation_of(node: Node, source: &[u8]) -> Option<String> {
    let prefix = node_text(node, source).split("fn").next().unwrap_or("");
    ["extern", "export", "inline", "noinline"]
        .into_iter()
        .find(|modifier| prefix.split_whitespace().any(|word| word == *modifier))
        .map(str::to_string)
}

fn emit_test(cursor: &mut TreeCursor, source: &[u8], w: &mut ZigWalker) -> bool {
    let node = cursor.node();
    let mut name_line = None;
    for_each_child!(cursor, {
        if matches!(cursor.node().kind(), "string" | "identifier") {
            let name_node = cursor.node();
            let raw = node_text(name_node, source);
            let name = raw
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(raw)
                .to_string();
            name_line = Some((name, line_of(name_node)));
            break;
        }
    });
    let Some((name, line)) = name_line else {
        return false;
    };

    if w.kinds.is_enabled("T") {
        let mut tag = make_tag(name.clone(), line, "T", w.scopes.current_field());
        add_field(&mut tag, "access", Some(access_of(node, source)));
        tag.end_line = end_line(node);
        w.tags.push(tag);
    }
    w.scopes.push(ScopeKind::Test, &name);
    true
}

fn emit_container_field(cursor: &mut TreeCursor, source: &[u8], w: &mut ZigWalker) {
    let letter = match w.scopes.last_key() {
        Some(ScopeKind::Enum) => "e",
        Some(ScopeKind::Struct | ScopeKind::Union) => "F",
        _ => return,
    };
    if !w.kinds.is_enabled(letter) {
        return;
    }
    let node = cursor.node();
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(name_node, source).to_string();
    if name == "_" {
        return;
    }
    let mut tag = make_tag(name, line_of(name_node), letter, w.scopes.current_field());
    if letter == "F" {
        add_field(
            &mut tag,
            "typeref",
            node.child_by_field_name("type")
                .map(|ty| format!("typename:{}", node_text(ty, source))),
        );
    }
    w.tags.push(tag);
}

fn emit_errors(cursor: &mut TreeCursor, source: &[u8], w: &mut ZigWalker) {
    if !w.kinds.is_enabled("E") || w.scopes.last_key() != Some(ScopeKind::ErrorSet) {
        return;
    }
    for_each_child!(cursor, {
        if cursor.node().kind() == "identifier" {
            let name_node = cursor.node();
            w.tags.push(make_tag(
                node_text(name_node, source).to_string(),
                line_of(name_node),
                "E",
                w.scopes.current_field(),
            ));
        }
    });
}

fn emit_parameter(cursor: &mut TreeCursor, source: &[u8], w: &mut ZigWalker) {
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
    add_field(
        &mut tag,
        "typeref",
        node.child_by_field_name("type")
            .map(|ty| format!("typename:{}", node_text(ty, source))),
    );
    w.tags.push(tag);
}

fn generate_tags(parser: &mut TsParser, req: &Request, source: &[u8]) -> Result<Vec<Tag>, String> {
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "parse failed".to_string())?;
    let mut walker = ZigWalker {
        source,
        scopes: ScopeStack::new(),
        kinds: TagKindConfig::parse(&req.kinds, ZIG_DEFAULT_KINDS, ZIG_OPTIONAL_KINDS),
        tags: Vec::new(),
    };
    let mut cursor = tree.walk();
    if cursor.goto_first_child() {
        walk_tree(&mut cursor, &mut walker);
    }
    Ok(walker.tags)
}
