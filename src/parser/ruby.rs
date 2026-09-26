//! Ruby definitions and library references, using the bundled grammar.
use super::common::{
    cursor::{field_child, line_of, node_text},
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use tree_sitter::{Node, TreeCursor};

pub(crate) const LANG_NAME: &str = "ruby";
pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["A", "accessor"], "A"),
    (&["C", "constant"], "C"),
    (&["L", "library"], "L"),
    (&["S", "singletonMethod"], "S"),
    (&["a", "alias"], "a"),
    (&["c", "class"], "c"),
    (&["f", "method"], "f"),
    (&["m", "module"], "m"),
];
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] = &[];

struct Frame {
    kind: &'static str,
    name: String,
    tag_index: Option<usize>,
    singleton: bool,
}

struct Walker<'a> {
    base: Context<'a>,
    frames: Vec<Frame>,
}

fn text<'a>(node: Node<'a>, source: &'a str) -> &'a str {
    node_text(node, source.as_bytes())
}

fn symbol_name(node: Node, source: &str) -> String {
    let value = text(node, source).trim();
    value
        .trim_start_matches(':')
        .trim_matches(['\'', '"'])
        .to_owned()
}

fn terminal_name(raw: &str) -> &str {
    raw.rsplit("::").next().unwrap_or(raw)
}

impl Walker<'_> {
    fn scope(&self) -> Option<(&'static str, String)> {
        let mut path = String::new();
        let mut kind = None;
        for frame in &self.frames {
            if frame.name.is_empty() {
                continue;
            }
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(&frame.name);
            kind = Some(frame.kind);
        }
        kind.map(|kind| (kind, path))
    }

    fn emit(
        &mut self,
        name: String,
        kind: &'static str,
        node: Node,
        end: Option<Node>,
        signature: Option<String>,
        role: &'static str,
    ) -> Option<usize> {
        if name.is_empty() || !self.base.tag_config.is_kind_enabled(kind) {
            return None;
        }
        if kind == "L" && !self.base.user_config.extras_config.reference {
            return None;
        }
        let config = &self.base.user_config.fields_config;
        let mut fields = ExtensionFields::new();
        if config.is_field_enabled("kind") {
            fields.insert("kind", kind);
        }
        if config.is_field_enabled("line") {
            fields.insert("line", line_of(node).to_string());
        }
        if config.is_field_enabled("language") {
            fields.insert("language", "Ruby");
        }
        if config.is_field_enabled("scope") || self.base.user_config.extras_config.qualified {
            if let Some((scope_kind, scope_name)) = self.scope() {
                fields.insert(scope_kind, scope_name);
            }
        }
        if config.is_field_enabled("signature") {
            if let Some(signature) = signature {
                fields.insert("signature", signature);
            }
        }
        if config.is_field_enabled("roles") {
            fields.insert("roles", role);
        }
        if config.is_field_enabled("end") {
            if let Some(end) = end {
                fields.insert("end", (end.end_position().row + 1).to_string());
            }
        }
        let line = line_of(node) as usize;
        let index = self.base.tags.len();
        self.base.tags.push(Tag {
            name,
            file_name: self.base.file_name.clone(),
            address: Tag::address_from_line(self.base.lines.get(line - 1).copied().unwrap_or(b"")),
            kind: Some(kind.into()),
            extension_fields: (!fields.is_empty()).then_some(fields),
        });
        Some(index)
    }

    fn declaration(&mut self, cursor: &mut TreeCursor, kind: &'static str) -> bool {
        let node = cursor.node();
        let Some(name_node) = field_child(cursor, "name") else {
            return false;
        };
        let raw = text(name_node, self.base.source_code);
        let name = terminal_name(raw).to_owned();
        // Explicit A::B declarations carry the lexical prefix on their own tag;
        // their body uses B as the current scope, as in Universal Ctags.
        let explicit_scope = raw.rsplit_once("::").map(|(prefix, _)| prefix.to_owned());
        if let Some(prefix) = &explicit_scope {
            self.frames.push(Frame {
                kind: "module",
                name: prefix.clone(),
                tag_index: None,
                singleton: false,
            });
        }
        let index = self.emit(name.clone(), kind, node, Some(node), None, "def");
        if explicit_scope.is_some() {
            self.frames.pop();
        }
        self.frames.push(Frame {
            kind: if kind == "c" { "class" } else { "module" },
            name,
            tag_index: index,
            singleton: false,
        });
        true
    }

    fn method(&mut self, cursor: &mut TreeCursor) -> bool {
        let node = cursor.node();
        let Some(name_node) = field_child(cursor, "name") else {
            return false;
        };
        let name = text(name_node, self.base.source_code).to_owned();
        let singleton =
            node.kind() == "singleton_method" || self.frames.iter().rev().any(|f| f.singleton);
        let kind = if singleton { "S" } else { "f" };
        let signature = field_child(cursor, "parameters")
            .map(|params| {
                let raw = text(params, self.base.source_code);
                if raw.starts_with('(') {
                    raw.to_owned()
                } else {
                    "()".to_owned()
                }
            })
            .or_else(|| Some("()".to_owned()));
        self.emit(name.clone(), kind, node, Some(node), signature, "def");
        self.frames.push(Frame {
            kind: "method",
            name,
            tag_index: None,
            singleton: false,
        });
        true
    }

    fn assignment(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();
        let Some(lhs) = field_child(cursor, "left") else {
            return;
        };
        if lhs.kind() == "constant" {
            self.emit(
                terminal_name(text(lhs, self.base.source_code)).to_owned(),
                "C",
                node,
                None,
                None,
                "def",
            );
        }
    }

    fn alias(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();
        if let Some(name) = field_child(cursor, "name") {
            self.emit(
                symbol_name(name, self.base.source_code),
                "a",
                node,
                None,
                None,
                "def",
            );
        }
    }

    fn call(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();
        let Some(method) = field_child(cursor, "method") else {
            return;
        };
        let method_name = text(method, self.base.source_code);
        if field_child(cursor, "receiver").is_some() {
            return;
        }
        if !matches!(
            method_name,
            "require"
                | "require_relative"
                | "load"
                | "attr_reader"
                | "attr_writer"
                | "attr_accessor"
                | "alias_method"
                | "include"
                | "prepend"
                | "extend"
        ) {
            return;
        }
        if field_child(cursor, "arguments").is_none() {
            return;
        }
        let mut args = Vec::new();
        for_each_child!(cursor, {
            if cursor.field_name() == Some("arguments") {
                for_each_child!(cursor, {
                    if cursor.node().is_named() {
                        args.push(cursor.node());
                    }
                });
                break;
            }
        });
        match method_name {
            "require" | "require_relative" | "load" => {
                let role = match method_name {
                    "require" => "required",
                    "require_relative" => "requiredRel",
                    _ => "loaded",
                };
                if let Some(arg) = args.first() {
                    if arg.kind() != "string" {
                        return;
                    }
                    let raw = text(*arg, self.base.source_code);
                    let name = raw.trim_matches(['\'', '"']);
                    if !name.contains("#{") {
                        self.emit(name.to_owned(), "L", node, None, None, role);
                    }
                }
            }
            "attr_reader" | "attr_writer" | "attr_accessor" => {
                for arg in args {
                    let name = symbol_name(arg, self.base.source_code);
                    if name.is_empty() || name.contains("#{") {
                        continue;
                    }
                    if method_name != "attr_writer" {
                        self.emit(name.clone(), "A", node, None, None, "def");
                    }
                    if method_name != "attr_reader" {
                        self.emit(format!("{name}="), "A", node, None, None, "def");
                    }
                }
            }
            "alias_method" => {
                if let Some(arg) = args.first() {
                    self.emit(
                        symbol_name(*arg, self.base.source_code),
                        "a",
                        node,
                        None,
                        None,
                        "def",
                    );
                }
            }
            "include" | "prepend" | "extend" => {
                if let Some(index) = self.frames.last().and_then(|f| f.tag_index) {
                    let mixin = args
                        .iter()
                        .map(|arg| format!("{method_name}:{}", text(*arg, self.base.source_code)))
                        .collect::<Vec<_>>()
                        .join(",");
                    if !mixin.is_empty() {
                        let tag = &mut self.base.tags[index];
                        let fields = tag
                            .extension_fields
                            .get_or_insert_with(ExtensionFields::new);
                        let value = match fields.get("mixin") {
                            Some(existing) => format!("{existing},{mixin}"),
                            None => mixin,
                        };
                        fields.insert("mixin", value);
                    }
                }
            }
            _ => {}
        }
    }
}

impl WalkContext for Walker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        let node = cursor.node();
        match node.kind() {
            "class" => self.declaration(cursor, "c"),
            "module" => self.declaration(cursor, "m"),
            "method" | "singleton_method" => self.method(cursor),
            "singleton_class" => {
                let singleton = field_child(cursor, "value")
                    .is_some_and(|value| text(value, self.base.source_code) == "self");
                self.frames.push(Frame {
                    kind: "class",
                    name: String::new(),
                    tag_index: None,
                    singleton,
                });
                true
            }
            "assignment" => {
                self.assignment(cursor);
                false
            }
            "alias" => {
                self.alias(cursor);
                false
            }
            "call" => {
                self.call(cursor);
                false
            }
            _ => false,
        }
    }

    fn pop_scope(&mut self) {
        self.frames.pop();
    }
}

pub(crate) fn generate(
    ts_parser: &mut tree_sitter::Parser,
    language: tree_sitter::Language,
    code: &[u8],
    path: &str,
    tag_config: &TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<Tag>> {
    generate_tags_with_config(
        ts_parser,
        language,
        code,
        path,
        |source_code, lines, cursor, tags| {
            let mut walker = Walker {
                base: Context {
                    source_code,
                    lines,
                    file_name: path.into(),
                    tags,
                    tag_config,
                    user_config: config,
                },
                frames: Vec::new(),
            };
            walk_tree(cursor, &mut walker);
        },
    )
}
