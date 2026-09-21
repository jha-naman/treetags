//! Java tag generation with the bundled tree-sitter-java grammar.
use super::common::{
    cursor::{line_of, node_text},
    scope::{ScopeKey, ScopeStack},
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};

pub(crate) const LANG_NAME: &str = "java";
pub(crate) const LANG_EXTENSIONS: &[&str] = &["java"];
pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["a", "annotation"], "a"),
    (&["c", "class"], "c"),
    (&["e", "enumConstant"], "e"),
    (&["f", "field"], "f"),
    (&["g", "enum"], "g"),
    (&["i", "interface"], "i"),
    (&["m", "method"], "m"),
    (&["p", "package"], "p"),
];
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] = &[(&["l", "local"], "l")];

const FIELD_ORDER: &[&str] = &[
    "kind",
    "line",
    "annotation",
    "class",
    "enum",
    "file",
    "interface",
];

#[derive(Clone, Copy)]
enum ScopeKind {
    Class,
    Interface,
    Enum,
    Annotation,
}

impl ScopeKey for ScopeKind {
    fn key(self) -> &'static str {
        match self {
            ScopeKind::Class => "class",
            ScopeKind::Interface => "interface",
            ScopeKind::Enum => "enum",
            ScopeKind::Annotation => "annotation",
        }
    }
}

struct JavaWalker<'src> {
    base: Context<'src>,
    scopes: ScopeStack<ScopeKind>,
}

impl WalkContext for JavaWalker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        process_node_inner(self.base.source_code.as_bytes(), cursor, self)
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

fn emit_tag(
    w: &mut JavaWalker,
    name: String,
    line: u32,
    kind: &'static str,
    file: bool,
    scoped: bool,
) {
    if name.is_empty() || name == "_" || !w.base.tag_config.is_kind_enabled(kind) {
        return;
    }
    let mut fields = ExtensionFields::new();
    fields.insert("kind", kind);
    fields.insert("line", line.to_string());
    if scoped {
        if let Some((key, value)) = w.scopes.current_field() {
            fields.insert(key, value.to_string());
        }
    }
    if file {
        fields.insert("file", "");
    }
    let mut raw: Vec<_> = fields.into_iter().collect();
    let mut enabled_fields = ExtensionFields::new();
    for &key in FIELD_ORDER {
        let Some(pos) = raw.iter().position(|(field, _)| field.as_ref() == key) else {
            continue;
        };
        let enabled = match key {
            "kind" | "line" => w.base.user_config.fields_config.is_field_enabled(key),
            "annotation" | "class" | "enum" | "interface" => {
                w.base.user_config.fields_config.is_field_enabled("scope")
                    || w.base.user_config.extras_config.qualified
            }
            _ => true,
        };
        let (key, value) = raw.swap_remove(pos);
        if enabled {
            enabled_fields.insert(key, value);
        }
    }
    debug_assert!(
        raw.is_empty(),
        "java emit_tag: field(s) missing from FIELD_ORDER: {raw:?}"
    );
    w.base.tags.push(Tag {
        name,
        file_name: w.base.file_name.clone(),
        address: Tag::address_from_line(
            w.base
                .lines
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

fn name_of(node: Node, source: &[u8]) -> Option<String> {
    Some(node_text(node.child_by_field_name("name")?, source).to_string())
}

fn is_private(cursor: &mut TreeCursor) -> bool {
    let mut private = false;
    for_each_child!(cursor, {
        if cursor.node().kind() == "modifiers" {
            for_each_child!(cursor, {
                if cursor.node().kind() == "private" {
                    private = true;
                    break;
                }
            });
            break;
        }
    });
    private
}

fn has_default(cursor: &mut TreeCursor) -> bool {
    let mut found = false;
    for_each_child!(cursor, {
        if cursor.node().kind() == "default" {
            found = true;
            break;
        }
    });
    found
}

fn process_node_inner(source: &[u8], cursor: &mut TreeCursor, w: &mut JavaWalker) -> bool {
    let node = cursor.node();
    let line = line_of(node);
    match node.kind() {
        "class_declaration"
        | "record_declaration"
        | "interface_declaration"
        | "enum_declaration"
        | "annotation_type_declaration" => {
            let Some(name) = name_of(node, source) else {
                return false;
            };
            let (kind, scope) = match node.kind() {
                "interface_declaration" => ("i", ScopeKind::Interface),
                "enum_declaration" => ("g", ScopeKind::Enum),
                "annotation_type_declaration" => ("a", ScopeKind::Annotation),
                _ => ("c", ScopeKind::Class),
            };
            emit_tag(w, name.clone(), line, kind, false, true);
            w.scopes.push(scope, &name);
            true
        }
        "method_declaration" | "constructor_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                emit_tag(
                    w,
                    node_text(name_node, source).to_string(),
                    line_of(name_node),
                    "m",
                    false,
                    true,
                );
            }
            false
        }
        "field_declaration" | "local_variable_declaration" => {
            let field = node.kind() == "field_declaration";
            let private = field && is_private(cursor);
            for_each_child!(cursor, {
                if cursor.node().kind() == "variable_declarator" {
                    if let Some(name) = name_of(cursor.node(), source) {
                        emit_tag(w, name, line, if field { "f" } else { "l" }, private, field);
                    }
                }
            });
            false
        }
        "enum_constant" => {
            if let Some(name) = name_of(node, source) {
                emit_tag(w, name, line, "e", true, true);
            }
            false
        }
        "annotation_type_element_declaration" => {
            if let Some(name) = name_of(node, source) {
                if has_default(cursor) {
                    emit_tag(w, name.clone(), line, "f", false, true);
                }
                emit_tag(w, name, line, "m", false, true);
            }
            false
        }
        "package_declaration" => {
            for_each_child!(cursor, {
                if matches!(cursor.node().kind(), "identifier" | "scoped_identifier") {
                    emit_tag(
                        w,
                        node_text(cursor.node(), source).to_string(),
                        line,
                        "p",
                        false,
                        false,
                    );
                    break;
                }
            });
            false
        }
        _ => false,
    }
}

pub(crate) fn generate(
    parser: &mut TsParser,
    language: tree_sitter::Language,
    source: &[u8],
    path: &str,
    kinds: &TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<Tag>> {
    generate_tags_with_config(
        parser,
        language,
        source,
        path,
        |source_code, lines, cursor, tags| {
            let mut walker = JavaWalker {
                base: Context {
                    source_code,
                    lines,
                    file_name: path.into(),
                    tags,
                    tag_config: kinds,
                    user_config: config,
                },
                scopes: ScopeStack::new(),
            };
            walk_tree(cursor, &mut walker);
        },
    )
}
