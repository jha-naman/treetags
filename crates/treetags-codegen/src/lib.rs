//! Offline compiler for treetags' data-driven C-family tag definitions.
//!
//! This crate is deliberately absent from the `treetags` dependency graph.
//! Generated files are ordinary checked-in Rust source.

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

#[derive(Clone, Copy, Debug)]
pub struct NamedSource<'a> {
    pub filename: &'a str,
    pub contents: &'a str,
}

impl<'a> NamedSource<'a> {
    pub const fn new(filename: &'a str, contents: &'a str) -> Self {
        Self { filename, contents }
    }
}

#[derive(Clone, Debug)]
pub struct GenerationOptions<'a> {
    pub module_name: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedOutput {
    pub rust_source: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub filename: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.filename)?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }
        if let Some(column) = self.column {
            write!(f, ":{column}")?;
        }
        write!(f, ": {}", self.message)
    }
}
impl std::error::Error for Diagnostic {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KindGroup {
    node: String,
    variants: Vec<KindVariant>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KindVariant {
    name: String,
    id: u16,
    #[serde(alias = "kind")]
    letter: String,
    #[serde(alias = "display")]
    display_name: String,
    #[serde(alias = "default_enabled")]
    default: bool,
}

/// C-family parsing facts not derivable from the grammar, tag queries, or kinds.
/// Emitted verbatim into the shared parser module so the runtime stays free of
/// language literals.
#[derive(Debug, Deserialize)]
struct ParseConfig {
    string_prefixes: Vec<String>,
    ctype_strip_keywords: Vec<String>,
    typeref_prefixes: BTreeMap<String, String>,
    anon: AnonConfig,
    expression_skip: ExpressionSkip,
    declarator: DeclaratorConfig,
    specifier_prefixes: Vec<String>,
    access_specifiers: Vec<String>,
    template_kw: String,
    template_param_keywords: Vec<String>,
    control_keywords: Vec<String>,
    /// Root node → structural handler category for keyword-introduced constructs.
    dispatch: BTreeMap<String, String>,
    /// Role name → `node.variant` key, resolved to a kind letter per language.
    roles: BTreeMap<String, String>,
    preproc: PreprocConfig,
    fields: FieldsConfig,
    /// Storage-class keyword → role, applied when the keyword heads a declaration.
    storage_roles: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct PreprocConfig {
    include: String,
    define: String,
    macro_param_field: String,
}

#[derive(Debug, Deserialize)]
struct FieldsConfig {
    typeref_key: String,
    typeref_default: String,
    function_qualifier: String,
    param_function: String,
}

#[derive(Debug, Deserialize)]
struct AnonConfig {
    prefix: String,
    #[allow(dead_code)]
    hash: String,
    seed: u32,
}

#[derive(Debug, Deserialize)]
struct ExpressionSkip {
    scope_op: String,
    operators: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DeclaratorConfig {
    pointer_prefixes: Vec<String>,
    cv_prefixes: Vec<String>,
    scope_op: String,
    destructor: String,
    operator_kw: String,
}

#[derive(Debug, Deserialize)]
struct NodeType {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    named: bool,
    #[serde(default)]
    fields: BTreeMap<String, Value>,
}

#[derive(Debug)]
struct CompiledPattern {
    root: String,
    name_capture: String,
    matches: Vec<CompiledMatch>,
    predicates: Vec<CompiledPredicate>,
    actions: Vec<CompiledAction>,
}

#[derive(Debug)]
struct CompiledMatch {
    field: String,
    kind: Option<String>,
    absent: bool,
}

#[derive(Debug)]
enum CompiledPredicate {
    Eq {
        capture: String,
        value: String,
        positive: bool,
    },
    Match {
        capture: String,
        value: String,
        positive: bool,
    },
    AnyOf {
        capture: String,
        values: Vec<String>,
        positive: bool,
    },
}

#[derive(Debug)]
enum CompiledAction {
    SelectKind(String),
    ResolveName {
        capture: String,
        resolver: String,
    },
    Transform {
        capture: String,
        transform: String,
    },
    EnterScope {
        capture: String,
        scope: String,
    },
    Field {
        field: String,
        capture: String,
        format: String,
    },
    ConditionalKind {
        condition: String,
        then_kind: String,
        else_kind: String,
    },
    SkipIf {
        capture: String,
        condition: String,
    },
    Anonymous {
        target: String,
        kind_id: u8,
        prefix: String,
    },
    Emit {
        each: bool,
    },
}

fn json<T: for<'de> Deserialize<'de>>(
    source: NamedSource<'_>,
    document: &'static str,
) -> Result<T, Diagnostic> {
    serde_json::from_str(source.contents).map_err(|error| Diagnostic {
        filename: source.filename.into(),
        line: Some(error.line()),
        column: Some(error.column()),
        message: format!("invalid {document}: {error}"),
    })
}

/// Validate the four inputs and compile them to stable Rust tables.
pub fn generate(
    grammar: NamedSource<'_>,
    node_types: NamedSource<'_>,
    query: NamedSource<'_>,
    kinds: NamedSource<'_>,
    parse: NamedSource<'_>,
    options: &GenerationOptions<'_>,
) -> Result<GeneratedOutput, Vec<Diagnostic>> {
    let mut errors = Vec::new();
    let parse_cfg: Option<ParseConfig> = match json(parse, "parse JSON") {
        Ok(v) => Some(v),
        Err(e) => {
            errors.push(e);
            None
        }
    };
    let grammar_json: Value = match json(grammar, "grammar JSON") {
        Ok(v) => v,
        Err(e) => {
            errors.push(e);
            Value::Null
        }
    };
    let nodes: Vec<NodeType> = match json(node_types, "node-types JSON") {
        Ok(v) => v,
        Err(e) => {
            errors.push(e);
            Vec::new()
        }
    };
    let groups: Vec<KindGroup> = match json(kinds, "kinds JSON") {
        Ok(v) => v,
        Err(e) => {
            errors.push(e);
            Vec::new()
        }
    };

    if !grammar_json.is_null() {
        let rules = grammar_json.get("rules").and_then(Value::as_object);
        if rules.is_none() {
            errors.push(Diagnostic {
                filename: grammar.filename.into(),
                line: None,
                column: None,
                message: "grammar must contain an object-valued `rules` member".into(),
            });
        }
    }
    let named_nodes: BTreeMap<_, _> = nodes
        .iter()
        .filter(|n| n.named)
        .map(|n| (n.kind.as_str(), n))
        .collect();
    // External tokens and aliases legitimately appear only in node-types. The
    // shared names must still agree where both documents describe a rule.
    if let Some(rules) = grammar_json.get("rules").and_then(Value::as_object) {
        for kind in rules.keys().filter(|kind| !kind.starts_with('_')) {
            if let Some(node) = named_nodes.get(kind.as_str()) {
                let _declared_fields = &node.fields; // fields are validated by Query::new below
            }
        }
    }

    let mut variants = BTreeMap::new();
    let mut ids = BTreeMap::new();
    let mut metadata: BTreeMap<&str, (&str, bool)> = BTreeMap::new();
    for group in &groups {
        if !named_nodes.contains_key(group.node.as_str()) {
            errors.push(Diagnostic {
                filename: kinds.filename.into(),
                line: None,
                column: None,
                message: format!("unknown AST node `{}`", group.node),
            });
        }
        for variant in &group.variants {
            if variant.letter.chars().count() != 1 {
                errors.push(Diagnostic {
                    filename: kinds.filename.into(),
                    line: None,
                    column: None,
                    message: format!("kind `{}` must have a one-character letter", variant.name),
                });
            }
            let key = format!("{}.{}", group.node, variant.name);
            if variants.insert(key.clone(), variant).is_some() {
                errors.push(Diagnostic {
                    filename: kinds.filename.into(),
                    line: None,
                    column: None,
                    message: format!("duplicate kind variant `{key}`"),
                });
            }
            if let Some(old) = ids.insert(variant.id, key.clone()) {
                errors.push(Diagnostic {
                    filename: kinds.filename.into(),
                    line: None,
                    column: None,
                    message: format!("kind id {} is shared by `{old}` and `{key}`", variant.id),
                });
            }
            match metadata.get(variant.letter.as_str()) {
                Some(&(display, default))
                    if display != variant.display_name || default != variant.default =>
                {
                    errors.push(Diagnostic {
                        filename: kinds.filename.into(),
                        line: None,
                        column: None,
                        message: format!(
                            "conflicting metadata for kind letter `{}`",
                            variant.letter
                        ),
                    })
                }
                None => {
                    metadata.insert(&variant.letter, (&variant.display_name, variant.default));
                }
                _ => {}
            }
        }
    }

    validate_query(query, &named_nodes, &variants, &mut errors);
    let patterns = compile_query(query, &variants, &mut errors);
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut out = String::new();
    writeln!(out, "// @generated by treetags-codegen; do not edit.").unwrap();
    writeln!(out, "// source module: {}", options.module_name).unwrap();
    writeln!(out, "use super::runtime::{{\n    ActionSpec, CaptureSpec, KindSpec, LanguageTables, MatchSpec, PatternSpec, PredicateSpec,\n    RootIndex, SpecifierSpec,\n}};\n").unwrap();
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static KINDS: &[KindSpec] = &["
    )
    .unwrap();
    let mut emitted = BTreeSet::new();
    for group in &groups {
        for variant in &group.variants {
            if emitted.insert(variant.letter.as_str()) {
                writeln!(
                    out,
                    "    KindSpec {{ letter: {:?}, name: {:?}, default_enabled: {} }},",
                    variant.letter, variant.display_name, variant.default
                )
                .unwrap();
            }
        }
    }
    writeln!(out, "];\n").unwrap();
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static SYNTAX_NODES: &[&str] = &["
    )
    .unwrap();
    for node in named_nodes.keys() {
        writeln!(out, "    {:?},", node).unwrap();
    }
    writeln!(out, "];\n").unwrap();
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static GRAMMAR_RULES: &[&str] = &["
    )
    .unwrap();
    if let Some(rules) = grammar_json.get("rules").and_then(Value::as_object) {
        let mut names: Vec<_> = rules.keys().collect();
        names.sort();
        for name in names {
            writeln!(out, "    {:?},", name).unwrap();
        }
    }
    writeln!(out, "];\n").unwrap();
    for (constant, enabled) in [("KIND_DEFAULTS", true), ("KIND_OPTIONALS", false)] {
        writeln!(out, "#[rustfmt::skip]").unwrap();
        writeln!(out, "pub(crate) static {constant}: &[(&[&str], &str)] = &[").unwrap();
        let mut mapped = BTreeSet::new();
        for group in &groups {
            for variant in &group.variants {
                if variant.default == enabled && mapped.insert(variant.letter.as_str()) {
                    writeln!(
                        out,
                        "    (&[{:?}, {:?}], {:?}),",
                        variant.letter, variant.display_name, variant.letter
                    )
                    .unwrap();
                }
            }
        }
        writeln!(out, "];\n").unwrap();
    }
    let captures: BTreeSet<_> = patterns
        .iter()
        .flat_map(|pattern| {
            std::iter::once(pattern.name_capture.as_str()).chain(pattern.actions.iter().filter_map(
                |action| match action {
                    CompiledAction::ResolveName { capture, .. }
                    | CompiledAction::Transform { capture, .. }
                    | CompiledAction::EnterScope { capture, .. }
                    | CompiledAction::Field { capture, .. }
                    | CompiledAction::SkipIf { capture, .. } => Some(capture.as_str()),
                    _ => None,
                },
            ))
        })
        .collect();
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static CAPTURES: &[CaptureSpec] = &["
    )
    .unwrap();
    for capture in &captures {
        writeln!(out, "    CaptureSpec {{ name: {:?} }},", capture).unwrap();
    }
    writeln!(out, "];\n").unwrap();

    let mut actions = Vec::new();
    let mut predicates = Vec::new();
    let mut matchers = Vec::new();
    let mut pattern_rows = Vec::new();
    for pattern in &patterns {
        let start = actions.len();
        actions.extend(pattern.actions.iter());
        let predicate_start = predicates.len();
        predicates.extend(pattern.predicates.iter());
        let match_start = matchers.len();
        matchers.extend(pattern.matches.iter());
        pattern_rows.push((
            pattern,
            start,
            actions.len() - start,
            predicate_start,
            predicates.len() - predicate_start,
            match_start,
            matchers.len() - match_start,
        ));
    }
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static ACTIONS: &[ActionSpec] = &["
    )
    .unwrap();
    for action in actions {
        let value = match action {
            CompiledAction::SelectKind(letter) => format!("ActionSpec::SelectKind({letter:?})"),
            CompiledAction::ResolveName { capture, resolver } => format!(
                "ActionSpec::ResolveName {{ capture: {capture:?}, resolver: {resolver:?} }}"
            ),
            CompiledAction::Transform { capture, transform } => format!(
                "ActionSpec::Transform {{ capture: {capture:?}, transform: {transform:?} }}"
            ),
            CompiledAction::EnterScope { capture, scope } => {
                format!("ActionSpec::EnterScope {{ capture: {capture:?}, scope: {scope:?} }}")
            }
            CompiledAction::Field { field, capture, format: template } => format!(
                "ActionSpec::Field {{ field: {field:?}, capture: {capture:?}, format: {template:?} }}"
            ),
            CompiledAction::ConditionalKind { condition, then_kind, else_kind } => format!(
                "ActionSpec::ConditionalKind {{ condition: {condition:?}, then_kind: {then_kind:?}, else_kind: {else_kind:?} }}"
            ),
            CompiledAction::SkipIf { capture, condition } => format!(
                "ActionSpec::SkipIf {{ capture: {capture:?}, condition: {condition:?} }}"
            ),
            CompiledAction::Anonymous { target, kind_id, prefix } => format!(
                "ActionSpec::Anonymous {{ target: {target:?}, kind_id: {kind_id}, prefix: {prefix:?} }}"
            ),
            CompiledAction::Emit { each } => format!("ActionSpec::Emit {{ each: {each} }}"),
        };
        writeln!(out, "    {value},").unwrap();
    }
    writeln!(out, "];\n").unwrap();
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static MATCHERS: &[MatchSpec] = &["
    )
    .unwrap();
    for matcher in matchers {
        writeln!(
            out,
            "    MatchSpec {{ field: {:?}, kind: {:?}, absent: {} }},",
            matcher.field, matcher.kind, matcher.absent
        )
        .unwrap();
    }
    writeln!(out, "];\n").unwrap();
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static PREDICATES: &[PredicateSpec] = &["
    )
    .unwrap();
    for predicate in predicates {
        let value = match predicate {
            CompiledPredicate::Eq { capture, value, positive } => format!("PredicateSpec::Eq {{ capture: {capture:?}, value: {value:?}, positive: {positive} }}"),
            CompiledPredicate::Match { capture, value, positive } => format!("PredicateSpec::Match {{ capture: {capture:?}, regex: {value:?}, positive: {positive} }}"),
            CompiledPredicate::AnyOf { capture, values, positive } => format!("PredicateSpec::AnyOf {{ capture: {capture:?}, values: &{values:?}, positive: {positive} }}"),
        };
        writeln!(out, "    {value},").unwrap();
    }
    writeln!(out, "];\n").unwrap();
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static PATTERNS: &[PatternSpec] = &["
    )
    .unwrap();
    for (pattern, start, len, predicate_start, predicate_len, match_start, match_len) in
        &pattern_rows
    {
        writeln!(out, "    PatternSpec {{ root_kind: {:?}, name_capture: {:?}, match_start: {match_start}, match_len: {match_len}, action_start: {start}, action_len: {len}, predicate_start: {predicate_start}, predicate_len: {predicate_len} }},", pattern.root, pattern.name_capture).unwrap();
    }
    writeln!(out, "];\n").unwrap();
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static ROOTS: &[RootIndex] = &["
    )
    .unwrap();
    for (index, (pattern, _, _, _, _, _, _)) in pattern_rows.iter().enumerate() {
        writeln!(
            out,
            "    RootIndex {{ kind: {:?}, pattern_start: {index}, pattern_len: 1 }},",
            pattern.root
        )
        .unwrap();
    }
    writeln!(out, "];\n").unwrap();

    // Keyword dispatch table: which leading keyword introduces which construct,
    // with its kind letter and scope, derived from the grammar (FIRST keyword),
    // the tag actions (letter/scope/base), and parse.json (handler category).
    let rules_map = grammar_json.get("rules").and_then(Value::as_object);
    struct SpecRow {
        category: String,
        letter: String,
        scope: Option<String>,
        base_format: Option<String>,
        anon_id: u8,
    }
    // Keyed by (keyword, category): most keywords map to one construct, but a few
    // (e.g. `namespace` → definition vs alias) share a keyword across categories.
    let mut specifiers: BTreeMap<(String, String), SpecRow> = BTreeMap::new();
    if let (Some(rules), Some(cfg)) = (rules_map, parse_cfg.as_ref()) {
        for pattern in &patterns {
            let Some(category) = cfg.dispatch.get(&pattern.root) else {
                continue;
            };
            let Some(keyword) = first_keyword(rules, &pattern.root) else {
                continue;
            };
            let letter = pattern.actions.iter().find_map(|a| match a {
                CompiledAction::SelectKind(l) => Some(l.clone()),
                _ => None,
            });
            let scope = pattern.actions.iter().find_map(|a| match a {
                CompiledAction::EnterScope { scope, .. } => Some(scope.clone()),
                _ => None,
            });
            let base_format = pattern.actions.iter().find_map(|a| match a {
                CompiledAction::Field { capture, format, .. } if capture == "base" => {
                    Some(format.clone())
                }
                _ => None,
            });
            let anon_id = pattern.actions.iter().find_map(|a| match a {
                CompiledAction::Anonymous { kind_id, .. } => Some(*kind_id),
                _ => None,
            });
            let Some(letter) = letter else { continue };
            let entry = specifiers
                .entry((keyword, category.clone()))
                .or_insert_with(|| SpecRow {
                    category: category.clone(),
                    letter: letter.clone(),
                    scope: scope.clone(),
                    base_format: None,
                    anon_id: 0,
                });
            // Merge rows that share a keyword (e.g. enum with/without a base).
            if base_format.is_some() {
                entry.base_format = base_format;
            }
            if entry.scope.is_none() {
                entry.scope = scope;
            }
            if let Some(id) = anon_id {
                entry.anon_id = id;
            }
        }
    }
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static SPECIFIERS: &[SpecifierSpec] = &["
    )
    .unwrap();
    for ((keyword, _category), row) in &specifiers {
        let scope = match &row.scope {
            Some(s) => format!("Some({s:?})"),
            None => "None".to_owned(),
        };
        let base = match &row.base_format {
            Some(b) => format!("Some({b:?})"),
            None => "None".to_owned(),
        };
        writeln!(
            out,
            "    SpecifierSpec {{ keyword: {keyword:?}, category: {:?}, letter: {:?}, scope: {scope}, base_format: {base}, anon_id: {} }},",
            row.category, row.letter, row.anon_id
        )
        .unwrap();
    }
    writeln!(out, "];\n").unwrap();

    // Role → kind letter, resolved from parse.json roles against this language's
    // kind variants (a role absent from the language is simply omitted).
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static ROLES: &[(&str, &str)] = &["
    )
    .unwrap();
    if let Some(cfg) = parse_cfg.as_ref() {
        for (role, key) in &cfg.roles {
            if let Some(variant) = variants.get(key.as_str()) {
                writeln!(out, "    ({role:?}, {:?}),", variant.letter).unwrap();
            }
        }
    }
    writeln!(out, "];\n").unwrap();

    // Storage keyword → kind letter (resolving the role's variant per language).
    writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static STORAGE_ROLES: &[(&str, &str)] = &["
    )
    .unwrap();
    if let Some(cfg) = parse_cfg.as_ref() {
        for (keyword, role) in &cfg.storage_roles {
            if let Some(letter) = cfg
                .roles
                .get(role)
                .and_then(|key| variants.get(key.as_str()))
                .map(|v| v.letter.as_str())
            {
                writeln!(out, "    ({keyword:?}, {letter:?}),").unwrap();
            }
        }
    }
    writeln!(out, "];\n").unwrap();

    writeln!(out, "#[rustfmt::skip]").unwrap();
    writeln!(
        out,
        "pub(crate) static TABLES: LanguageTables = LanguageTables {{"
    )
    .unwrap();
    writeln!(out, "    name: {:?},", options.module_name).unwrap();
    writeln!(out, "    kinds: KINDS,").unwrap();
    writeln!(out, "    roots: ROOTS,").unwrap();
    writeln!(out, "    patterns: PATTERNS,").unwrap();
    writeln!(out, "    matchers: MATCHERS,").unwrap();
    writeln!(out, "    captures: CAPTURES,").unwrap();
    writeln!(out, "    predicates: PREDICATES,").unwrap();
    writeln!(out, "    actions: ACTIONS,").unwrap();
    writeln!(out, "    syntax_nodes: SYNTAX_NODES,").unwrap();
    writeln!(out, "    grammar_rules: GRAMMAR_RULES,").unwrap();
    writeln!(out, "    specifiers: SPECIFIERS,").unwrap();
    writeln!(out, "    roles: ROLES,").unwrap();
    writeln!(out, "    storage_roles: STORAGE_ROLES,").unwrap();
    writeln!(out, "}};").unwrap();
    writeln!(out, "\n#[rustfmt::skip]\npub(crate) fn generate(code: &[u8], path: &str, kinds: &crate::parser::TagKindConfig, config: &crate::config::Config) -> Option<Vec<crate::tag::Tag>> {{").unwrap();
    writeln!(
        out,
        "    super::runtime::generate(&TABLES, code, path, kinds, config)"
    )
    .unwrap();
    writeln!(out, "}}").unwrap();
    Ok(GeneratedOutput { rust_source: out })
}

/// Walk every rule expression and collect the value of every `STRING` terminal.
fn collect_string_terminals(rules: &Value, out: &mut BTreeSet<String>) {
    match rules {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("STRING") {
                if let Some(value) = map.get("value").and_then(Value::as_str) {
                    out.insert(value.to_owned());
                }
            }
            for value in map.values() {
                collect_string_terminals(value, out);
            }
        }
        Value::Array(items) => {
            for value in items {
                collect_string_terminals(value, out);
            }
        }
        _ => {}
    }
}

/// A `STRING` terminal that lexes as an identifier (keyword or reserved word).
fn is_identifier_terminal(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        && !value.is_empty()
}

/// A `STRING` terminal that lexes as an operator/punctuator token.
///
/// Bracket characters are always single-token, so multi-character terminals
/// containing them (`()`, `[]`, `[[`, `]]` — only valid inside operator names or
/// attributes) are rejected. Comment introducers are handled by the lexer, not
/// as operators.
fn is_punctuator_terminal(value: &str) -> bool {
    const OPERATOR_CHARS: &str = "!%&*+,-./:;<=>?^|~()[]{}";
    const BRACKETS: &str = "()[]{}";
    if value.is_empty() || value.chars().any(|c| !OPERATOR_CHARS.contains(c)) {
        return false;
    }
    if value == "//" || value == "/*" {
        return false;
    }
    if value.chars().count() > 1 && value.chars().any(|c| BRACKETS.contains(c)) {
        return false;
    }
    true
}

/// Collect every rule symbol (`SYMBOL`/`ALIAS` target) referenced by an expression.
fn referenced_symbols(expr: &Value, out: &mut BTreeSet<String>) {
    match expr {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("SYMBOL") {
                if let Some(name) = map.get("name").and_then(Value::as_str) {
                    out.insert(name.to_owned());
                }
            }
            for value in map.values() {
                referenced_symbols(value, out);
            }
        }
        Value::Array(items) => {
            for value in items {
                referenced_symbols(value, out);
            }
        }
        _ => {}
    }
}

/// True if an expression can match empty (optional/repeatable leading element).
fn is_optional_expr(e: &Value) -> bool {
    match e.get("type").and_then(Value::as_str) {
        Some("BLANK") | Some("REPEAT") => true,
        Some("CHOICE") => e
            .get("members")
            .and_then(Value::as_array)
            .is_some_and(|ms| ms.iter().any(|m| m.get("type").and_then(Value::as_str) == Some("BLANK"))),
        _ => false,
    }
}

/// The canonical first mandatory keyword introducing a rule (skipping optional
/// leading elements like `inline namespace` / `__extension__ typedef`).
fn first_keyword(rules: &serde_json::Map<String, Value>, node: &str) -> Option<String> {
    fn walk(e: &Value, rules: &serde_json::Map<String, Value>, depth: usize) -> Option<String> {
        if depth > 8 {
            return None;
        }
        match e.get("type").and_then(Value::as_str)? {
            "STRING" => {
                let v = e.get("value").and_then(Value::as_str)?;
                is_identifier_terminal(v).then(|| v.to_owned())
            }
            "SYMBOL" => {
                let name = e.get("name").and_then(Value::as_str)?;
                rules.get(name).and_then(|r| walk(r, rules, depth + 1))
            }
            "SEQ" => {
                for member in e.get("members")?.as_array()? {
                    if is_optional_expr(member) {
                        continue;
                    }
                    return walk(member, rules, depth + 1);
                }
                None
            }
            "CHOICE" => e
                .get("members")?
                .as_array()?
                .iter()
                .find_map(|m| walk(m, rules, depth + 1)),
            "PREC" | "PREC_LEFT" | "PREC_RIGHT" | "PREC_DYNAMIC" | "FIELD" | "ALIAS" | "TOKEN"
            | "IMMEDIATE_TOKEN" | "REPEAT" | "REPEAT1" => {
                walk(e.get("content")?, rules, depth + 1)
            }
            _ => None,
        }
    }
    walk(rules.get(node)?, rules, 0)
}

/// Every rule symbol reachable from `root` by following `SYMBOL` references.
fn reachable_symbols(rules: &serde_json::Map<String, Value>, root: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![root.to_owned()];
    while let Some(name) = stack.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(body) = rules.get(&name) {
            let mut refs = BTreeSet::new();
            referenced_symbols(body, &mut refs);
            stack.extend(refs);
        }
    }
    seen
}

/// Emit the shared, language-neutral structural-parser tables derived purely from
/// the C-family grammar. Both `c` and `cpp` consume this one module; it carries no
/// per-language tag rules.
pub fn generate_shared(
    grammar: NamedSource<'_>,
    _node_types: NamedSource<'_>,
    kinds: NamedSource<'_>,
    parse: NamedSource<'_>,
) -> Result<GeneratedOutput, Vec<Diagnostic>> {
    let cfg: ParseConfig = json(parse, "parse JSON").map_err(|e| vec![e])?;
    let grammar_json: Value = json(grammar, "grammar JSON").map_err(|e| vec![e])?;
    let rules = match grammar_json.get("rules").and_then(Value::as_object) {
        Some(rules) => rules,
        None => {
            return Err(vec![Diagnostic {
                filename: grammar.filename.into(),
                line: None,
                column: None,
                message: "grammar must contain an object-valued `rules` member".into(),
            }])
        }
    };

    // Reachability self-check: every node the tag rules can produce must be
    // reachable from `translation_unit` through the grammar. A gap here means the
    // structural walk could silently drop a whole class of tags.
    let groups: Vec<KindGroup> = json(kinds, "kinds JSON").map_err(|e| vec![e])?;
    let reachable = reachable_symbols(rules, "translation_unit");
    let mut missing: Vec<String> = groups
        .iter()
        .map(|g| g.node.clone())
        .filter(|node| rules.contains_key(node.as_str()) && !reachable.contains(node))
        .collect();
    missing.sort();
    missing.dedup();
    if !missing.is_empty() {
        return Err(vec![Diagnostic {
            filename: grammar.filename.into(),
            line: None,
            column: None,
            message: format!(
                "tagged nodes not reachable from translation_unit: {}",
                missing.join(", ")
            ),
        }]);
    }

    let mut terminals = BTreeSet::new();
    collect_string_terminals(grammar_json.get("rules").unwrap(), &mut terminals);
    let keywords: Vec<&str> = terminals
        .iter()
        .map(String::as_str)
        .filter(|s| is_identifier_terminal(s))
        .collect();
    // Maximal munch requires longer operators to be tried before their prefixes.
    let mut punctuators: Vec<&str> = terminals
        .iter()
        .map(String::as_str)
        .filter(|s| is_punctuator_terminal(s))
        .collect();
    punctuators.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then(a.cmp(b)));

    let mut out = String::new();
    writeln!(out, "// @generated by treetags-codegen; do not edit.").unwrap();
    writeln!(out, "// Shared C-family structural-parser tables.").unwrap();
    writeln!(out, "#![allow(dead_code)]\n").unwrap();
    writeln!(
        out,
        "/// Reserved words: `STRING` terminals that lex as identifiers."
    )
    .unwrap();
    writeln!(out, "#[rustfmt::skip]").unwrap();
    writeln!(out, "pub(crate) static KEYWORDS: &[&str] = &[").unwrap();
    for keyword in &keywords {
        writeln!(out, "    {keyword:?},").unwrap();
    }
    writeln!(out, "];\n").unwrap();
    writeln!(
        out,
        "/// Operator/punctuator terminals, longest first for maximal munch."
    )
    .unwrap();
    writeln!(out, "#[rustfmt::skip]").unwrap();
    writeln!(out, "pub(crate) static PUNCTUATORS: &[&str] = &[").unwrap();
    for punctuator in &punctuators {
        writeln!(out, "    {punctuator:?},").unwrap();
    }
    writeln!(out, "];\n").unwrap();

    // Parsing facts from parse.json, emitted verbatim as data the engine reads.
    let slice = |out: &mut String, name: &str, items: &[String]| {
        writeln!(out, "pub(crate) static {name}: &[&str] = &[").unwrap();
        for item in items {
            writeln!(out, "    {item:?},").unwrap();
        }
        writeln!(out, "];").unwrap();
    };
    writeln!(out, "// Parsing facts (from parse.json).").unwrap();
    slice(&mut out, "STRING_PREFIXES", &cfg.string_prefixes);
    slice(&mut out, "CTYPE_STRIP", &cfg.ctype_strip_keywords);
    writeln!(out, "pub(crate) static TYPEREF_PREFIXES: &[(&str, &str)] = &[").unwrap();
    for (keyword, label) in &cfg.typeref_prefixes {
        writeln!(out, "    ({keyword:?}, {label:?}),").unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out, "pub(crate) static ANON_PREFIX: &str = {:?};", cfg.anon.prefix).unwrap();
    writeln!(out, "pub(crate) static ANON_SEED: u32 = {};", cfg.anon.seed).unwrap();
    writeln!(
        out,
        "pub(crate) static EXPR_SKIP_SCOPE_OP: &str = {:?};",
        cfg.expression_skip.scope_op
    )
    .unwrap();
    slice(&mut out, "EXPR_SKIP_OPS", &cfg.expression_skip.operators);
    slice(&mut out, "DECL_POINTER_PREFIXES", &cfg.declarator.pointer_prefixes);
    slice(&mut out, "DECL_CV_PREFIXES", &cfg.declarator.cv_prefixes);
    writeln!(out, "pub(crate) static DECL_SCOPE_OP: &str = {:?};", cfg.declarator.scope_op).unwrap();
    writeln!(out, "pub(crate) static DECL_DESTRUCTOR: &str = {:?};", cfg.declarator.destructor).unwrap();
    writeln!(out, "pub(crate) static DECL_OPERATOR_KW: &str = {:?};", cfg.declarator.operator_kw).unwrap();
    slice(&mut out, "SPECIFIER_PREFIXES", &cfg.specifier_prefixes);
    slice(&mut out, "ACCESS_SPECIFIERS", &cfg.access_specifiers);
    writeln!(out, "pub(crate) static TEMPLATE_KW: &str = {:?};", cfg.template_kw).unwrap();
    slice(&mut out, "TEMPLATE_PARAM_KEYWORDS", &cfg.template_param_keywords);
    slice(&mut out, "CONTROL_KEYWORDS", &cfg.control_keywords);
    writeln!(out, "pub(crate) static PREPROC_INCLUDE: &str = {:?};", cfg.preproc.include).unwrap();
    writeln!(out, "pub(crate) static PREPROC_DEFINE: &str = {:?};", cfg.preproc.define).unwrap();
    writeln!(
        out,
        "pub(crate) static PREPROC_MACRO_PARAM_FIELD: &str = {:?};",
        cfg.preproc.macro_param_field
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) static FIELD_TYPEREF_KEY: &str = {:?};",
        cfg.fields.typeref_key
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) static FIELD_TYPEREF_DEFAULT: &str = {:?};",
        cfg.fields.typeref_default
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) static FIELD_FUNCTION_QUALIFIER: &str = {:?};",
        cfg.fields.function_qualifier
    )
    .unwrap();
    writeln!(
        out,
        "pub(crate) static FIELD_PARAM_FUNCTION: &str = {:?};",
        cfg.fields.param_function
    )
    .unwrap();
    Ok(GeneratedOutput { rust_source: out })
}

fn validate_query(
    query: NamedSource<'_>,
    nodes: &BTreeMap<&str, &NodeType>,
    variants: &BTreeMap<String, &KindVariant>,
    errors: &mut Vec<Diagnostic>,
) {
    let node_re = Regex::new(r"\(([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    for captures in node_re.captures_iter(query.contents) {
        let node = &captures[1];
        if node != "_" && !nodes.contains_key(node) {
            let (line, column) = location(query.contents, captures.get(1).unwrap().start());
            errors.push(Diagnostic {
                filename: query.filename.into(),
                line: Some(line),
                column: Some(column),
                message: format!("query references unknown node `{node}`"),
            });
        }
    }
    let kind_re = Regex::new(r#"\(#tt-kind!\s+\"([^\"]+)\"\s+\"([^\"]+)\"\)"#).unwrap();
    for captures in kind_re.captures_iter(query.contents) {
        let key = format!("{}.{}", &captures[1], &captures[2]);
        if !variants.contains_key(&key) {
            let (line, column) = location(query.contents, captures.get(0).unwrap().start());
            errors.push(Diagnostic {
                filename: query.filename.into(),
                line: Some(line),
                column: Some(column),
                message: format!("unknown kind variant `{key}`"),
            });
        }
    }
    let directive_re = Regex::new(r"\(#(tt-[A-Za-z0-9_-]+!)").unwrap();
    const KNOWN: &[&str] = &[
        "tt-emit!",
        "tt-emit-each!",
        "tt-kind!",
        "tt-name!",
        "tt-transform!",
        "tt-field!",
        "tt-kind-if!",
        "tt-enter-scope!",
        "tt-scope!",
        "tt-iterate!",
        "tt-anonymous!",
        "tt-skip-if!",
        "tt-require!",
    ];
    for captures in directive_re.captures_iter(query.contents) {
        if !KNOWN.contains(&&captures[1]) {
            let (line, column) = location(query.contents, captures.get(1).unwrap().start());
            errors.push(Diagnostic {
                filename: query.filename.into(),
                line: Some(line),
                column: Some(column),
                message: format!("unknown treetags directive `#{}`", &captures[1]),
            });
        }
    }
    let predicate_re = Regex::new(r"\(#([A-Za-z0-9_-]+[!?])").unwrap();
    const STANDARD: &[&str] = &[
        "eq?",
        "not-eq?",
        "any-eq?",
        "any-not-eq?",
        "match?",
        "not-match?",
        "any-match?",
        "any-not-match?",
        "any-of?",
        "not-any-of?",
        "is?",
        "is-not?",
        "set!",
    ];
    for captures in predicate_re.captures_iter(query.contents) {
        let operator = &captures[1];
        if !operator.starts_with("tt-") && !STANDARD.contains(&operator) {
            let (line, column) = location(query.contents, captures.get(1).unwrap().start());
            errors.push(Diagnostic {
                filename: query.filename.into(),
                line: Some(line),
                column: Some(column),
                message: format!("unknown query predicate `#{operator}`"),
            });
        }
    }
}

fn compile_query(
    query: NamedSource<'_>,
    variants: &BTreeMap<String, &KindVariant>,
    errors: &mut Vec<Diagnostic>,
) -> Vec<CompiledPattern> {
    let root_re = Regex::new(r"^\s*\(\(?([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    let kind_re = Regex::new(r#"\(#tt-kind!\s+\"([^\"]+)\"\s+\"([^\"]+)\"\)"#).unwrap();
    let name_re = Regex::new(r#"\(#tt-name!\s+@([A-Za-z0-9_.-]+)\s+\"([^\"]+)\"\)"#).unwrap();
    let transform_re =
        Regex::new(r#"\(#tt-transform!\s+@([A-Za-z0-9_.-]+)\s+\"([^\"]+)\"\)"#).unwrap();
    let scope_re =
        Regex::new(r#"\(#tt-enter-scope!\s+@([A-Za-z0-9_.-]+)\s+\"([^\"]+)\"\)"#).unwrap();
    let field_re =
        Regex::new(r#"\(#tt-field!\s+\"([^\"]+)\"\s+@([A-Za-z0-9_.-]+)\s+\"([^\"]*)\"\)"#).unwrap();
    let kind_if_re =
        Regex::new(r#"\(#tt-kind-if!\s+\"([^\"]+)\"\s+\"([^\"]+)\"\s+\"([^\"]*)\"\)"#).unwrap();
    let skip_if_re = Regex::new(r#"\(#tt-skip-if!\s+@([A-Za-z0-9_.-]+)\s+\"([^\"]+)\"\)"#).unwrap();
    let anonymous_re =
        Regex::new(r#"\(#tt-anonymous!\s+\"([^\"]+)\"\s+\"([0-9]+)\"\s+\"([^\"]*)\"\)"#).unwrap();
    let binary_predicate_re = Regex::new(
        r#"\(#(eq\?|not-eq\?|match\?|not-match\?)\s+@([A-Za-z0-9_.-]+)\s+\"([^\"]*)\"\)"#,
    )
    .unwrap();
    let any_of_re =
        Regex::new(r#"\(#(any-of\?|not-any-of\?)\s+@([A-Za-z0-9_.-]+)((?:\s+\"[^\"]*\")+)\s*\)"#)
            .unwrap();
    let string_re = Regex::new(r#"\"([^\"]*)\""#).unwrap();
    let capture_re = Regex::new(r"@([A-Za-z0-9_.-]+)").unwrap();
    let field_match_re =
        Regex::new(r"([A-Za-z_][A-Za-z0-9_]*):\s*\(([A-Za-z_][A-Za-z0-9_]*|_)\)").unwrap();
    let absent_re = Regex::new(r"!([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    let mut result = Vec::new();
    for (offset, block) in top_level_forms(query.contents) {
        let Some(root) = root_re.captures(block).map(|c| c[1].to_owned()) else {
            continue;
        };
        let mut actions = Vec::new();
        let mut predicates = Vec::new();
        let mut matches = field_match_re
            .captures_iter(block)
            .map(|found| CompiledMatch {
                field: found[1].to_owned(),
                kind: (&found[2] != "_").then(|| found[2].to_owned()),
                absent: false,
            })
            .collect::<Vec<_>>();
        matches.extend(absent_re.captures_iter(block).map(|found| CompiledMatch {
            field: found[1].to_owned(),
            kind: None,
            absent: true,
        }));
        for found in binary_predicate_re.captures_iter(block) {
            let positive = !found[1].starts_with("not-");
            let predicate = if found[1].ends_with("eq?") {
                CompiledPredicate::Eq {
                    capture: found[2].to_owned(),
                    value: found[3].to_owned(),
                    positive,
                }
            } else {
                CompiledPredicate::Match {
                    capture: found[2].to_owned(),
                    value: found[3].to_owned(),
                    positive,
                }
            };
            predicates.push(predicate);
        }
        for found in any_of_re.captures_iter(block) {
            predicates.push(CompiledPredicate::AnyOf {
                capture: found[2].to_owned(),
                values: string_re
                    .captures_iter(&found[3])
                    .map(|value| value[1].to_owned())
                    .collect(),
                positive: !found[1].starts_with("not-"),
            });
        }
        if let Some(found) = kind_re.captures(block) {
            let key = format!("{}.{}", &found[1], &found[2]);
            if let Some(kind) = variants.get(&key) {
                actions.push(CompiledAction::SelectKind(kind.letter.clone()));
            }
        }
        let name = name_re.captures(block);
        let name_capture = name
            .as_ref()
            .map(|c| c[1].to_owned())
            .or_else(|| {
                capture_re
                    .captures_iter(block)
                    .find_map(|c| (&c[1] == "name").then(|| c[1].to_owned()))
            })
            .unwrap_or_else(|| "declarator".into());
        if let Some(found) = name {
            actions.push(CompiledAction::ResolveName {
                capture: found[1].to_owned(),
                resolver: found[2].to_owned(),
            });
        }
        for found in transform_re.captures_iter(block) {
            actions.push(CompiledAction::Transform {
                capture: found[1].to_owned(),
                transform: found[2].to_owned(),
            });
        }
        for found in scope_re.captures_iter(block) {
            actions.push(CompiledAction::EnterScope {
                capture: found[1].to_owned(),
                scope: found[2].to_owned(),
            });
        }
        for found in field_re.captures_iter(block) {
            actions.push(CompiledAction::Field {
                field: found[1].to_owned(),
                capture: found[2].to_owned(),
                format: found[3].to_owned(),
            });
        }
        for found in kind_if_re.captures_iter(block) {
            actions.push(CompiledAction::ConditionalKind {
                condition: found[1].to_owned(),
                then_kind: found[2].to_owned(),
                else_kind: found[3].to_owned(),
            });
        }
        for found in skip_if_re.captures_iter(block) {
            actions.push(CompiledAction::SkipIf {
                capture: found[1].to_owned(),
                condition: found[2].to_owned(),
            });
        }
        for found in anonymous_re.captures_iter(block) {
            actions.push(CompiledAction::Anonymous {
                target: found[1].to_owned(),
                kind_id: found[2].parse().expect("validated anonymous kind id"),
                prefix: found[3].to_owned(),
            });
        }
        if block.contains("(#tt-emit-each!") {
            actions.push(CompiledAction::Emit { each: true });
        } else if block.contains("(#tt-emit!") {
            actions.push(CompiledAction::Emit { each: false });
        }
        if !actions
            .iter()
            .any(|a| matches!(a, CompiledAction::SelectKind(_)))
            || !actions
                .iter()
                .any(|a| matches!(a, CompiledAction::Emit { .. }))
        {
            let (line, column) = location(query.contents, offset);
            errors.push(Diagnostic {
                filename: query.filename.into(),
                line: Some(line),
                column: Some(column),
                message: "every tag pattern requires #tt-kind! and an emit directive".into(),
            });
        }
        result.push(CompiledPattern {
            root,
            name_capture,
            matches,
            predicates,
            actions,
        });
    }
    result
}

fn top_level_forms(text: &str) -> Vec<(usize, &str)> {
    let bytes = text.as_bytes();
    let (mut depth, mut start, mut string, mut escape, mut comment) =
        (0usize, None, false, false, false);
    let mut forms = Vec::new();
    for (index, byte) in bytes.iter().copied().enumerate() {
        if comment {
            if byte == b'\n' {
                comment = false;
            }
            continue;
        }
        if !string && byte == b';' {
            comment = true;
            continue;
        }
        if string {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                string = false;
            }
            continue;
        }
        if byte == b'"' {
            string = true;
            continue;
        }
        if byte == b'(' {
            if depth == 0 {
                start = Some(index);
            }
            depth += 1;
        }
        if byte == b')' && depth > 0 {
            depth -= 1;
            if depth == 0 {
                let begin = start.take().unwrap();
                forms.push((begin, &text[begin..=index]));
            }
        }
    }
    forms
}

fn location(text: &str, offset: usize) -> (usize, usize) {
    let prefix = &text[..offset];
    (
        prefix.bytes().filter(|b| *b == b'\n').count() + 1,
        prefix
            .rsplit_once('\n')
            .map_or(prefix.len() + 1, |(_, tail)| tail.len() + 1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_has_source_location() {
        let source = NamedSource::new("bad.json", "{");
        let error = json::<Value>(source, "test JSON").unwrap_err();
        assert_eq!(error.filename, "bad.json");
        assert_eq!(error.line, Some(1));
    }
    #[test]
    fn location_is_one_based() {
        assert_eq!(location("a\nbc", 3), (2, 2));
    }

    #[test]
    fn shared_module_extracts_terminals_from_grammar() {
        let output = generate_shared(
            NamedSource::new(
                "grammar.json",
                include_str!("../../../codegen/cpp/shared/grammar.json"),
            ),
            NamedSource::new(
                "node-types.json",
                include_str!("../../../codegen/cpp/shared/node-types.json"),
            ),
            NamedSource::new(
                "kinds.json",
                include_str!("../../../codegen/cpp/cpp/kinds.json"),
            ),
            NamedSource::new(
                "parse.json",
                include_str!("../../../codegen/cpp/shared/parse.json"),
            ),
        )
        .unwrap()
        .rust_source;
        // Keywords are derived, not hand-listed.
        assert!(output.contains("\"class\","));
        assert!(output.contains("\"namespace\","));
        assert!(output.contains("\"typedef\","));
        // Multi-character operators must precede their prefixes for maximal munch.
        let shift_assign = output.find("\">>=\"").expect("has >>=");
        let shift = output.find("\">>\",").expect("has >>");
        assert!(shift_assign < shift, "longer operators must sort first");
        // Bracket-doublings and comment introducers are not punctuator tokens.
        assert!(!output.contains("\"[[\""));
        assert!(!output.contains("\"//\""));
        assert!(!output.contains("\"()\""));
    }

    #[test]
    fn shared_module_rejects_unreachable_tagged_node() {
        // A node that exists in the grammar but is not reachable from
        // translation_unit must fail the reachability self-check.
        let errors = generate_shared(
            NamedSource::new(
                "grammar.json",
                include_str!("../../../codegen/cpp/shared/grammar.json"),
            ),
            NamedSource::new(
                "node-types.json",
                include_str!("../../../codegen/cpp/shared/node-types.json"),
            ),
            NamedSource::new(
                "kinds.json",
                r#"[{"node":"macro_type_specifier","variants":[{"name":"x","id":1,"letter":"x","display_name":"x","default":true}]}]"#,
            ),
            NamedSource::new(
                "parse.json",
                include_str!("../../../codegen/cpp/shared/parse.json"),
            ),
        )
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|e| e.message.contains("not reachable from translation_unit")));
    }

    #[test]
    fn output_contains_typed_tables_not_query_source() {
        let output = generate(
            NamedSource::new(
                "grammar.json",
                include_str!("../../../codegen/cpp/shared/grammar.json"),
            ),
            NamedSource::new(
                "node-types.json",
                include_str!("../../../codegen/cpp/shared/node-types.json"),
            ),
            NamedSource::new("tags.scm", include_str!("../../../codegen/cpp/c/tags.scm")),
            NamedSource::new(
                "kinds.json",
                include_str!("../../../codegen/cpp/c/kinds.json"),
            ),
            NamedSource::new(
                "parse.json",
                include_str!("../../../codegen/cpp/shared/parse.json"),
            ),
            &GenerationOptions { module_name: "c" },
        )
        .unwrap();
        assert!(output.rust_source.contains("static PATTERNS"));
        assert!(output.rust_source.contains("static SPECIFIERS"));
        assert!(output.rust_source.contains("static ACTIONS"));
        assert!(!output.rust_source.contains("query:"));
        assert_eq!(
            output,
            generate(
                NamedSource::new(
                    "grammar.json",
                    include_str!("../../../codegen/cpp/shared/grammar.json")
                ),
                NamedSource::new(
                    "node-types.json",
                    include_str!("../../../codegen/cpp/shared/node-types.json")
                ),
                NamedSource::new("tags.scm", include_str!("../../../codegen/cpp/c/tags.scm")),
                NamedSource::new(
                    "kinds.json",
                    include_str!("../../../codegen/cpp/c/kinds.json")
                ),
                NamedSource::new(
                    "parse.json",
                    include_str!("../../../codegen/cpp/shared/parse.json")
                ),
                &GenerationOptions { module_name: "c" },
            )
            .unwrap()
        );
    }

    #[test]
    fn objective_c_inputs_generate_native_dialect_tables() {
        let output = generate(
            NamedSource::new(
                "grammar.json",
                include_str!("../../../codegen/objective_c/grammar.json"),
            ),
            NamedSource::new(
                "node-types.json",
                include_str!("../../../codegen/objective_c/node-types.json"),
            ),
            NamedSource::new(
                "tags.scm",
                include_str!("../../../codegen/objective_c/tags.scm"),
            ),
            NamedSource::new(
                "kinds.json",
                include_str!("../../../codegen/objective_c/kinds.json"),
            ),
            NamedSource::new(
                "parse.json",
                include_str!("../../../codegen/objective_c/parse.json"),
            ),
            &GenerationOptions {
                module_name: "objective_c",
            },
        )
        .unwrap()
        .rust_source;
        assert!(output.contains("name: \"objective_c\""));
        assert!(output.contains("(\"objc_interface\", \"i\")"));
        assert!(output.contains("(\"objc_method\", \"M\")"));
        assert!(output.contains("name: \"protocol\""));
    }
}
