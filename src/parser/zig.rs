//! Zig tag generation, ported from the original Zig plugin.
use super::common::{
    cursor::{child_ident, line_of, node_text},
    scope::{ScopeKey, ScopeStack},
    scope_walker::{walk_tree, WalkContext},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use std::sync::Arc;
use tree_sitter::{Node, Parser as TsParser, TreeCursor};

pub(crate) const LANG_NAME: &'static str = "zig";
pub(crate) const LANG_EXTENSIONS: &'static [&'static str] = &["zig"];

pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["s", "struct"], "s"),
    (&["u", "union"], "u"),
    (&["g", "enum"], "g"),
    (&["e", "enumerator"], "e"),
    (&["o", "opaque"], "o"),
    (&["r", "errorSet"], "r"),
    (&["E", "error"], "E"),
    (&["f", "function"], "f"),
    (&["m", "method"], "m"),
    (&["F", "field"], "F"),
    (&["c", "constant"], "c"),
    (&["v", "variable"], "v"),
    (&["n", "namespace"], "n"),
    (&["t", "test"], "t"),
];

pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] =
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
    config: &'src crate::config::Config,
    source: &'src [u8],
    lines: Vec<&'src [u8]>,
    file_name: Arc<str>,
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

fn emit_tag(
    w: &mut ZigWalker,
    name: String,
    line: u32,
    kind: &'static str,
    extra_fields: impl FnOnce(&mut ExtensionFields),
) {
    if name.is_empty() || name == "_" || !w.kinds.is_kind_enabled(kind) {
        return;
    }
    let mut fields = ExtensionFields::new();
    fields.insert("kind", kind);
    fields.insert("line", line.to_string());
    if let Some((key, value)) = w.scopes.current_field() {
        fields.insert(key.to_string(), value.to_string());
    }
    extra_fields(&mut fields);
    let mut fields: Vec<_> = fields.into_iter().collect();
    // Keep standard fields first, followed by the language fields alphabetically.
    fields.sort_unstable_by(|a, b| {
        let order = |key: &str| match key {
            "kind" => 0,
            "line" => 1,
            "end" => 2,
            _ => 3,
        };
        order(&a.0).cmp(&order(&b.0)).then_with(|| a.0.cmp(&b.0))
    });
    let mut enabled_fields = ExtensionFields::new();
    for (key, value) in fields {
        let enabled = match key.as_ref() {
            "kind" | "line" | "end" | "access" | "signature" | "typeref" => {
                w.config.fields_config.is_field_enabled(&key)
            }
            "struct" | "union" | "enum" | "opaque" | "errorSet" | "function" | "test" => {
                w.config.fields_config.is_field_enabled("scope") || w.config.extras_config.qualified
            }
            _ => true,
        };
        if enabled {
            enabled_fields.insert(key, value);
        }
    }
    w.tags.push(Tag {
        name,
        file_name: w.file_name.clone(),
        address: Tag::address_from_line(
            w.lines
                .get(line.saturating_sub(1) as usize)
                .copied()
                .unwrap_or(b""),
        ),
        kind: Some(kind.into()),
        extension_fields: if enabled_fields.is_empty() {
            None
        } else {
            Some(enabled_fields)
        },
    });
}

fn add_field(fields: &mut ExtensionFields, key: &'static str, value: Option<String>) {
    if let Some(value) = value {
        fields.insert(key, value);
    }
}

fn add_end_line(fields: &mut ExtensionFields, node: Node) {
    add_field(
        fields,
        "end",
        Some((node.end_position().row + 1).to_string()),
    );
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
        if without_linkage.starts_with("var ") || without_linkage.starts_with("extern ") {
            "v"
        } else {
            "c"
        }
    };

    emit_tag(w, name.clone(), line, letter, |fields| {
        if !local {
            add_field(fields, "access", Some(access_of(node, source)));
        }
        if container.is_none() {
            add_field(
                fields,
                "typeref",
                declared_type(cursor, source).map(|ty| format!("typename:{ty}")),
            );
        }
        if container.is_some() {
            add_end_line(fields, node);
        }
    });

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

    let letter = if matches!(
        w.scopes.last_key(),
        Some(ScopeKind::Struct | ScopeKind::Enum | ScopeKind::Union | ScopeKind::Opaque)
    ) {
        "m"
    } else {
        "f"
    };

    emit_tag(w, name.clone(), line_of(name_node), letter, |fields| {
        add_field(fields, "access", Some(access_of(node, source)));
        let mut signature = None;
        for_each_child!(cursor, {
            if cursor.node().kind() == "parameters" {
                signature = Some(node_text(cursor.node(), source).to_string());
                break;
            }
        });
        add_field(fields, "signature", signature);
        add_field(
            fields,
            "typeref",
            node.child_by_field_name("type")
                .map(|ty| format!("typename:{}", node_text(ty, source))),
        );
        add_field(fields, "implementation", implementation_of(node, source));
        add_end_line(fields, node);
    });

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

    emit_tag(w, name.clone(), line, "t", |fields| {
        add_field(fields, "access", Some(access_of(node, source)));
        add_end_line(fields, node);
    });
    w.scopes.push(ScopeKind::Test, &name);
    true
}

fn emit_container_field(cursor: &mut TreeCursor, source: &[u8], w: &mut ZigWalker) {
    let letter = match w.scopes.last_key() {
        Some(ScopeKind::Enum) => "e",
        Some(ScopeKind::Struct | ScopeKind::Union) => "F",
        _ => return,
    };
    let node = cursor.node();
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(name_node, source).to_string();
    emit_tag(w, name, line_of(name_node), letter, |fields| {
        if letter == "F" {
            add_field(
                fields,
                "typeref",
                node.child_by_field_name("type")
                    .map(|ty| format!("typename:{}", node_text(ty, source))),
            );
        }
    });
}

fn emit_errors(cursor: &mut TreeCursor, source: &[u8], w: &mut ZigWalker) {
    if w.scopes.last_key() != Some(ScopeKind::ErrorSet) {
        return;
    }
    for_each_child!(cursor, {
        if cursor.node().kind() == "identifier" {
            let name_node = cursor.node();
            emit_tag(
                w,
                node_text(name_node, source).to_string(),
                line_of(name_node),
                "E",
                |_| {},
            );
        }
    });
}

fn emit_parameter(cursor: &mut TreeCursor, source: &[u8], w: &mut ZigWalker) {
    let node = cursor.node();
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    emit_tag(
        w,
        node_text(name_node, source).to_string(),
        line_of(name_node),
        "z",
        |fields| {
            add_field(
                fields,
                "typeref",
                node.child_by_field_name("type")
                    .map(|ty| format!("typename:{}", node_text(ty, source))),
            );
        },
    );
}

pub(crate) fn generate(
    parser: &mut TsParser,
    language: tree_sitter::Language,
    source: &[u8],
    path: &str,
    kinds: &TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<Tag>> {
    parser.set_language(&language).ok()?;
    let tree = parser.parse(source, None)?;
    let mut walker = ZigWalker {
        config,
        source,
        lines: crate::split_by_newlines::split_by_newlines(source),
        file_name: path.into(),
        scopes: ScopeStack::new(),
        kinds: kinds.clone(),
        tags: Vec::new(),
    };
    let mut cursor = tree.walk();
    if cursor.goto_first_child() {
        walk_tree(&mut cursor, &mut walker);
    }
    Some(walker.tags)
}
