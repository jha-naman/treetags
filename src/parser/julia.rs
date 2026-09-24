//! Julia definitions and import references from the bundled tree-sitter grammar.
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

pub(crate) const KIND_DEFAULTS: &[(&[&str], &str)] = &[
    (&["Y", "unknown"], "Y"),
    (&["c", "constant"], "c"),
    (&["f", "function"], "f"),
    (&["g", "field"], "g"),
    (&["m", "macro"], "m"),
    (&["n", "module"], "n"),
    (&["s", "struct"], "s"),
    (&["t", "type"], "t"),
];
pub(crate) const KIND_OPTIONALS: &[(&[&str], &str)] = &[];

#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Module,
    Struct,
}
impl ScopeKey for Scope {
    fn key(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Struct => "struct",
        }
    }
}

struct Walker<'a> {
    base: Context<'a>,
    scopes: ScopeStack<Scope>,
}

struct Name {
    value: String,
    end_byte: usize,
}

fn first_named<T>(
    cursor: &mut TreeCursor,
    action: impl FnOnce(&mut TreeCursor) -> Option<T>,
) -> Option<T> {
    let mut result = None;
    let mut action = Some(action);
    for_each_child!(cursor, {
        if cursor.node().is_named() {
            result = action.take().unwrap()(cursor);
            break;
        }
    });
    result
}

fn child_of_kind<T>(
    cursor: &mut TreeCursor,
    kind: &str,
    action: impl FnOnce(&mut TreeCursor) -> Option<T>,
) -> Option<T> {
    let mut result = None;
    let mut action = Some(action);
    for_each_child!(cursor, {
        if cursor.node().kind() == kind {
            result = action.take().unwrap()(cursor);
            break;
        }
    });
    result
}

// A type head may contain parameters and a supertype. The name is always on
// the left; the parameter list, if present, follows that name.
fn type_name(cursor: &mut TreeCursor, source: &[u8]) -> Option<(Name, Option<String>)> {
    let node = cursor.node();
    match node.kind() {
        "identifier" => Some((
            Name {
                value: node_text(node, source).to_owned(),
                end_byte: node.end_byte(),
            },
            None,
        )),
        "type_head" | "binary_expression" | "where_expression" => {
            first_named(cursor, |cursor| type_name(cursor, source))
        }
        "curly_expression" | "parametrized_type_expression" => {
            let name = first_named(cursor, |cursor| type_name(cursor, source))?.0;
            let signature = std::str::from_utf8(source.get(name.end_byte..node.end_byte())?)
                .ok()
                .map(str::to_owned);
            Some((name, signature))
        }
        _ => None,
    }
}

// Return a callable's name and the part of its signature following the name.
fn callable(cursor: &mut TreeCursor, source: &[u8]) -> Option<(Name, Option<String>)> {
    let node = cursor.node();
    match node.kind() {
        "signature" | "where_expression" | "typed_expression" | "parenthesized_expression" => {
            let (name, _) = first_named(cursor, |cursor| callable(cursor, source))?;
            let suffix = std::str::from_utf8(source.get(name.end_byte..node.end_byte())?)
                .ok()?
                .to_owned();
            Some((name, Some(suffix)))
        }
        "call_expression" => {
            let name = first_named(cursor, |cursor| callable_name(cursor, source))?;
            let suffix = std::str::from_utf8(source.get(name.end_byte..node.end_byte())?)
                .ok()?
                .to_owned();
            Some((name, Some(suffix)))
        }
        "identifier" | "operator" => Some((
            Name {
                value: node_text(node, source).to_owned(),
                end_byte: node.end_byte(),
            },
            None,
        )),
        _ => None,
    }
}

fn callable_name(cursor: &mut TreeCursor, source: &[u8]) -> Option<Name> {
    let node = cursor.node();
    match node.kind() {
        "identifier" | "operator" => Some(Name {
            value: node_text(node, source).to_owned(),
            end_byte: node.end_byte(),
        }),
        "scoped_identifier" => {
            let mut result = None;
            for_each_child!(cursor, {
                if cursor.node().is_named() {
                    result = callable_name(cursor, source);
                }
            });
            result
        }
        _ => None,
    }
}

fn has_call_head(cursor: &mut TreeCursor) -> bool {
    match cursor.node().kind() {
        "call_expression" => true,
        "signature" | "where_expression" | "typed_expression" | "parenthesized_expression" => {
            first_named(cursor, |cursor| Some(has_call_head(cursor))).unwrap_or(false)
        }
        _ => false,
    }
}

impl Walker<'_> {
    fn emit(
        &mut self,
        name: String,
        kind: &'static str,
        node: Node,
        signature: Option<String>,
        role: Option<&'static str>,
        scope: Option<(&'static str, String)>,
    ) {
        if name.is_empty() || !self.base.tag_config.is_kind_enabled(kind) {
            return;
        }
        if role.is_some() && !self.base.user_config.extras_config.reference {
            return;
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
            fields.insert("language", "Julia");
        }
        let inherited_scope = self.scopes.current_field().map(|(k, v)| (k, v.to_owned()));
        if config.is_field_enabled("scope") || self.base.user_config.extras_config.qualified {
            if let Some((key, value)) = scope.or(inherited_scope) {
                fields.insert(key, value);
            }
        }
        if let Some(signature) = signature.filter(|s| !s.is_empty()) {
            if config.is_field_enabled("signature") {
                fields.insert("signature", signature);
            }
        }
        if config.is_field_enabled("roles") {
            fields.insert("roles", role.unwrap_or("def"));
        }
        let line = line_of(node) as usize;
        self.base.tags.push(Tag {
            name,
            file_name: self.base.file_name.clone(),
            address: Tag::address_from_line(self.base.lines.get(line - 1).copied().unwrap_or(b"")),
            kind: Some(kind.into()),
            extension_fields: (!fields.is_empty()).then_some(fields),
        });
    }

    fn import_item(
        &mut self,
        cursor: &mut TreeCursor,
        statement: Node,
        role: &'static str,
        module: Option<&str>,
    ) {
        let kind = if module.is_some() { "Y" } else { "n" };
        if cursor.node().kind() == "import_alias" {
            for_each_child!(cursor, {
                if cursor.node().is_named() {
                    self.import_item(cursor, statement, role, module);
                }
            });
        } else if cursor.node().kind() != "operator" {
            let name = node_text(cursor.node(), self.base.source_code.as_bytes())
                .trim_start_matches('.')
                .to_owned();
            self.emit(
                name,
                kind,
                statement,
                None,
                Some(role),
                module.map(|m| ("module", m.to_owned())),
            );
        }
    }

    fn process_import(&mut self, cursor: &mut TreeCursor) {
        if !self.base.user_config.extras_config.reference {
            return;
        }
        let statement = cursor.node();
        let role = if statement.kind() == "using_statement" {
            "used"
        } else {
            "imported"
        };
        for_each_child!(cursor, {
            match cursor.node().kind() {
                "selected_import" => {
                    let module = first_named(cursor, |cursor| {
                        Some(
                            node_text(cursor.node(), self.base.source_code.as_bytes())
                                .trim_start_matches('.')
                                .to_owned(),
                        )
                    });
                    if let Some(module) = module {
                        self.emit(
                            module.clone(),
                            "n",
                            statement,
                            None,
                            Some("namespace"),
                            None,
                        );
                        let mut seen_base = false;
                        for_each_child!(cursor, {
                            if cursor.node().is_named() {
                                if seen_base {
                                    self.import_item(cursor, statement, role, Some(&module));
                                } else {
                                    seen_base = true;
                                }
                            }
                        });
                    }
                }
                "identifier" | "scoped_identifier" | "import_path" | "import_alias" => {
                    self.import_item(cursor, statement, role, None)
                }
                _ => {}
            }
        });
    }
}

impl WalkContext for Walker<'_> {
    fn process_node(&mut self, cursor: &mut TreeCursor) -> bool {
        let node = cursor.node();
        let source = self.base.source_code.as_bytes();
        match node.kind() {
            "module_definition" => {
                let mut name = None;
                for_each_child!(cursor, {
                    if cursor.field_name() == Some("name") {
                        name = Some(node_text(cursor.node(), source).to_owned());
                        break;
                    }
                });
                let Some(name) = name else {
                    return false;
                };
                self.emit(name.clone(), "n", node, None, None, None);
                self.scopes.push(Scope::Module, &name);
                true
            }
            "struct_definition" | "abstract_definition" | "primitive_definition" => {
                let Some((name, signature)) =
                    child_of_kind(cursor, "type_head", |cursor| type_name(cursor, source))
                else {
                    return false;
                };
                let is_struct = node.kind() == "struct_definition";
                self.emit(
                    name.value.clone(),
                    if is_struct { "s" } else { "t" },
                    node,
                    signature,
                    None,
                    None,
                );
                if is_struct {
                    self.scopes.push(Scope::Struct, &name.value);
                    for_each_child!(cursor, {
                        let field = cursor.node();
                        match field.kind() {
                            "identifier" => self.emit(
                                node_text(field, source).to_owned(),
                                "g",
                                field,
                                None,
                                None,
                                None,
                            ),
                            "typed_expression" => {
                                let field_name = first_named(cursor, |cursor| {
                                    (cursor.node().kind() == "identifier").then(|| Name {
                                        value: node_text(cursor.node(), source).to_owned(),
                                        end_byte: cursor.node().end_byte(),
                                    })
                                });
                                if let Some(field_name) = field_name {
                                    let sig = std::str::from_utf8(
                                        source
                                            .get(field_name.end_byte..field.end_byte())
                                            .unwrap_or(b""),
                                    )
                                    .ok()
                                    .map(str::to_owned);
                                    self.emit(field_name.value, "g", field, sig, None, None);
                                }
                            }
                            _ => {}
                        }
                    });
                }
                is_struct
            }
            "function_definition" | "macro_definition" => {
                if let Some((name, sig)) =
                    child_of_kind(cursor, "signature", |cursor| callable(cursor, source))
                {
                    self.emit(
                        name.value,
                        if node.kind() == "macro_definition" {
                            "m"
                        } else {
                            "f"
                        },
                        node,
                        sig,
                        None,
                        None,
                    );
                }
                false
            }
            "assignment" => {
                let tag = first_named(cursor, |cursor| {
                    has_call_head(cursor)
                        .then(|| callable(cursor, source))
                        .flatten()
                });
                if let Some((name, signature)) = tag {
                    self.emit(name.value, "f", node, signature, None, None);
                }
                false
            }
            "const_statement" => {
                let name = child_of_kind(cursor, "assignment", |cursor| {
                    first_named(cursor, |cursor| {
                        (cursor.node().kind() == "identifier")
                            .then(|| node_text(cursor.node(), source).to_owned())
                    })
                });
                if let Some(name) = name {
                    self.emit(name, "c", node, None, None, None);
                }
                false
            }
            "import_statement" | "using_statement" => {
                self.process_import(cursor);
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
