//! Elixir definitions from the native tree-sitter grammar.
use super::common::{
    cursor::{field_child, line_of, node_text},
    scope::{ScopeKey, ScopeStack},
    scope_walker::{walk_tree, WalkContext},
    tree_walker::{generate_tags_with_config, Context},
};
use super::TagKindConfig;
use crate::for_each_child;
use crate::tag::{ExtensionFields, Tag};
use tree_sitter::{Node, Parser as TsParser, TreeCursor};

pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["a", "macro"], "a"),
    (&["c", "callback"], "c"),
    (&["d", "delegate"], "d"),
    (&["e", "exception"], "e"),
    (&["f", "function"], "f"),
    (&["g", "guard"], "g"),
    (&["i", "implementation"], "i"),
    (&["m", "module"], "m"),
    (&["o", "operator"], "o"),
    (&["p", "protocol"], "p"),
    (&["r", "record"], "r"),
    (&["t", "test"], "t"),
    (&["y", "type"], "y"),
];

#[derive(Clone, Copy)]
enum Scope {
    Module,
    Protocol,
}
impl ScopeKey for Scope {
    fn key(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Protocol => "protocol",
        }
    }
}

struct Walker<'a> {
    base: Context<'a>,
    scopes: ScopeStack<Scope>,
}

fn first_arg<'tree>(cursor: &mut TreeCursor<'tree>) -> Option<Node<'tree>> {
    let mut result = None;
    for_each_child!(cursor, {
        if cursor.node().kind() == "arguments" {
            for_each_child!(cursor, {
                if cursor.node().is_named() {
                    result = Some(cursor.node());
                    break;
                }
            });
            break;
        }
    });
    result
}

fn name_from_head(head: Node, source: &str) -> Option<(String, &'static str)> {
    match head.kind() {
        "identifier" | "alias" => Some((node_text(head, source.as_bytes()).to_owned(), "f")),
        "call" => {
            let target = head.child_by_field_name("target")?;
            Some((node_text(target, source.as_bytes()).to_owned(), "f"))
        }
        "binary_operator" => {
            let left = head.child_by_field_name("left")?;
            if left.kind() == "call" || left.kind() == "identifier" {
                if left.kind() == "identifier" && head.child_by_field_name("right").is_some() {
                    let right = head.child_by_field_name("right")?;
                    let middle = source.get(left.end_byte()..right.start_byte())?.trim();
                    if middle != "when" && middle != "::" && !middle.is_empty() {
                        return Some((middle.to_owned(), "o"));
                    }
                }
                name_from_head(left, source)
            } else {
                let right = head.child_by_field_name("right")?;
                let middle = source.get(left.end_byte()..right.start_byte())?.trim();
                (!middle.is_empty()).then(|| (middle.to_owned(), "o"))
            }
        }
        "atom" => Some((
            node_text(head, source.as_bytes())
                .trim_start_matches(':')
                .to_owned(),
            "r",
        )),
        "string" => Some((
            node_text(head, source.as_bytes())
                .trim_matches('"')
                .to_owned(),
            "t",
        )),
        _ => None,
    }
}

impl Walker<'_> {
    fn emit(&mut self, name: String, kind: &'static str, node: Node, end: Option<Node>) {
        if name.is_empty() || !self.base.tag_config.is_kind_enabled(kind) {
            return;
        }
        let config = &self.base.user_config.fields_config;
        let line = line_of(node) as usize;
        let mut fields = ExtensionFields::new();
        if config.is_field_enabled("kind") {
            fields.insert("kind", kind);
        }
        if config.is_field_enabled("line") {
            fields.insert("line", line.to_string());
        }
        if config.is_field_enabled("language") {
            fields.insert("language", "Elixir");
        }
        if let Some(end) = end {
            if config.is_field_enabled("end") {
                fields.insert("end", (end.end_position().row + 1).to_string());
            }
        }
        if let Some((key, value)) = self.scopes.current_field() {
            if config.is_field_enabled("scope") || self.base.user_config.extras_config.qualified {
                fields.insert(key, value.to_owned());
            }
        }
        self.base.tags.push(Tag {
            name,
            file_name: self.base.file_name.clone(),
            address: Tag::address_from_line(self.base.lines.get(line - 1).copied().unwrap_or(b"")),
            kind: Some(kind.into()),
            extension_fields: (!fields.is_empty()).then_some(fields),
        });
    }

    fn process_call(&mut self, cursor: &mut TreeCursor) -> bool {
        let node = cursor.node();
        let Some(target) = field_child(cursor, "target") else {
            return false;
        };
        let target_text = node_text(target, self.base.source_code.as_bytes());
        let (kind, scope) = match target_text {
            "defmodule" => ("m", Some(Scope::Module)),
            "defprotocol" => ("p", Some(Scope::Protocol)),
            "defimpl" => ("i", Some(Scope::Protocol)),
            "defexception" => ("e", None),
            "def" | "defp" | "defn" | "defnp" => ("f", None),
            "defmacro" | "defmacrop" => ("a", None),
            "defguard" | "defguardp" => ("g", None),
            "defdelegate" => ("d", None),
            "test" => ("t", None),
            "Record.defrecord" | "Record.defrecordp" | "defrecord" | "defrecordp" => ("r", None),
            _ => return false,
        };
        if kind == "e" {
            let name = self
                .scopes
                .current_field()
                .map(|(_, path)| path.rsplit('.').next().unwrap_or(path).to_owned())
                .unwrap_or_default();
            self.emit(name, kind, node, None);
            return false;
        }
        let Some(head) = first_arg(cursor) else {
            return false;
        };
        let head = if head.kind() == "binary_operator"
            && self
                .base
                .source_code
                .get(head.start_byte()..head.end_byte())
                .is_some_and(|s| s.contains(" when "))
        {
            head.child_by_field_name("left").unwrap_or(head)
        } else {
            head
        };
        let Some((name, head_kind)) = name_from_head(head, self.base.source_code) else {
            return false;
        };
        let actual_kind = if head_kind == "o" { "o" } else { kind };
        let mut end = None;
        if matches!(kind, "m" | "p") {
            for_each_child!(cursor, {
                if cursor.node().kind() == "do_block" {
                    end = Some(cursor.node());
                    break;
                }
            });
        }
        if kind == "i" {
            self.scopes.push(Scope::Protocol, &name);
            self.emit(name, actual_kind, node, end);
            return true;
        }
        self.emit(name.clone(), actual_kind, node, end);
        if let Some(scope) = scope {
            self.scopes.push(scope, &name);
            return true;
        }
        false
    }

    fn process_attribute(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();
        let Some(call) = field_child(cursor, "operand") else {
            return;
        };
        if call.kind() != "call" {
            return;
        }
        let mut kind = None;
        let mut arg = None;
        for_each_child!(cursor, {
            if cursor.node() == call {
                if let Some(target) = field_child(cursor, "target") {
                    kind = match node_text(target, self.base.source_code.as_bytes()) {
                        "type" | "typep" | "opaque" => Some("y"),
                        "callback" | "macrocallback" => Some("c"),
                        _ => None,
                    };
                }
                arg = first_arg(cursor);
                break;
            }
        });
        let Some(kind) = kind else {
            return;
        };
        let Some(arg) = arg else {
            return;
        };
        let head = if arg.kind() == "binary_operator" {
            arg.child_by_field_name("left").unwrap_or(arg)
        } else {
            arg
        };
        if let Some((name, _)) = name_from_head(head, self.base.source_code) {
            self.emit(name, kind, node, None);
        }
    }
}

impl WalkContext for Walker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        let node = cursor.node();
        match node.kind() {
            "call" => self.process_call(cursor),
            "unary_operator" => {
                self.process_attribute(cursor);
                false
            }
            _ => false,
        }
    }
    fn pop_scope(&mut self) {
        self.scopes.pop();
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
            let mut walker = Walker {
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
