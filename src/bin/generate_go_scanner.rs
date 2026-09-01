use anyhow::{bail, Context, Result};
use regex_automata::{
    dfa::{dense, Automaton},
    Anchored, Input, MatchKind,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::PathBuf,
    process::Command,
};
const OUTPUT: &str = "src/parser/generated/go.rs";
const TREE_SITTER_VERSION: &str = "tree-sitter 0.25.10";
const NODE_VERSION: &str = "v22.17.0";
fn main() -> Result<()> {
    let check = env::args().skip(1).any(|x| x == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let grammar = fs::read(root.join("grammar/go/grammar.js"))?;
    let temp = env::temp_dir().join(format!("treetags-go-gen-{}", std::process::id()));
    if temp.exists() {
        fs::remove_dir_all(&temp)?
    }
    fs::create_dir_all(&temp)?;
    fs::write(temp.join("grammar.js"), &grammar)?;
    let node = env::var("TREETAGS_NODE").unwrap_or_else(|_| "node".into());
    let status = Command::new("tree-sitter")
        .current_dir(&temp)
        .args(["generate", "--abi", "14", "--js-runtime", &node])
        .status()
        .context("run pinned tree-sitter CLI")?;
    if !status.success() {
        bail!("tree-sitter could not evaluate grammar.js; set TREETAGS_NODE to an absolute pinned Node executable")
    }
    let json: Value = serde_json::from_slice(&fs::read(temp.join("src/grammar.json"))?)?;
    let (machines, strings, patterns) = compile_lexical_machines(&json)?;
    validate_pattern_semantics(&node, &temp, &patterns)?;
    let keywords: BTreeSet<_> = strings
        .iter()
        .filter(|s| s.chars().count() > 1 && s.chars().all(|c| c == '_' || c.is_alphabetic()))
        .cloned()
        .collect();
    let punctuation: BTreeSet<_> = strings
        .iter()
        .filter(|s| {
            !s.is_empty()
                && !keywords.contains(*s)
                && !s.chars().any(char::is_whitespace)
                && s.chars().any(|c| !c.is_alphanumeric() && c != '_')
                && !s.contains('\0')
        })
        .cloned()
        .collect();
    let hash = format!("{:x}", Sha256::digest(&grammar));
    let cli = String::from_utf8_lossy(
        &Command::new("tree-sitter")
            .arg("--version")
            .output()?
            .stdout,
    )
    .trim()
    .to_string();
    if cli != TREE_SITTER_VERSION {
        bail!("expected {TREE_SITTER_VERSION}, found {cli}")
    }
    let node_version =
        String::from_utf8_lossy(&Command::new(&node).arg("--version").output()?.stdout)
            .trim()
            .to_string();
    if node_version != NODE_VERSION {
        bail!("expected Node {NODE_VERSION}, found {node_version}")
    }
    let word = json["word"].as_str().context("missing word token")?;
    let externals = json["externals"].as_array().map_or(0, Vec::len);
    let mut generated = render(
        &hash,
        &format!("{cli}; node {node_version}"),
        word,
        externals,
        &keywords,
        &punctuation,
        &patterns,
        &machines,
    );
    let candidate = temp.join("generated.rs");
    fs::write(&candidate, &generated)?;
    let status = Command::new("rustfmt")
        .arg(&candidate)
        .status()
        .context("format generated scanner")?;
    if !status.success() {
        bail!("rustfmt failed on generated scanner")
    }
    generated = fs::read_to_string(candidate)?;
    let path = root.join(OUTPUT);
    if check {
        if fs::read_to_string(&path)? != generated {
            bail!("{OUTPUT} is stale")
        }
    } else {
        fs::write(path, generated)?
    }
    fs::remove_dir_all(temp).ok();
    Ok(())
}
struct Machine {
    name: String,
    dfa: Vec<u16>,
    classes: Vec<u8>,
    class_count: usize,
    accept: Vec<bool>,
    dead: Vec<bool>,
    skip: bool,
    priority: i32,
    recovery: Option<(u8, bool)>,
    recovery_prefixes: Vec<Vec<u8>>,
}
fn leading_string(node: &Value) -> Option<Vec<u8>> {
    let node = unwrap_node(node);
    match node["type"].as_str()? {
        "STRING" => Some(node["value"].as_str()?.as_bytes().to_vec()),
        "SEQ" => leading_string(node["members"].as_array()?.first()?),
        _ => None,
    }
}
fn recovery_prefixes(node: &Value, skip: bool) -> Vec<Vec<u8>> {
    if !skip {
        return Vec::new();
    }
    let node = unwrap_node(node);
    let branches = if node["type"] == "CHOICE" {
        node["members"].as_array().cloned().unwrap_or_default()
    } else {
        vec![node.clone()]
    };
    branches
        .iter()
        .filter_map(leading_string)
        .filter(|p| p.len() > 1)
        .collect()
}
fn unwrap_node(mut node: &Value) -> &Value {
    while matches!(
        node["type"].as_str(),
        Some("TOKEN" | "IMMEDIATE_TOKEN" | "PREC" | "PREC_LEFT" | "PREC_RIGHT" | "ALIAS")
    ) {
        node = &node["content"]
    }
    node
}
fn delimited_recovery(node: &Value, pattern: &str) -> Option<(u8, bool)> {
    let node = unwrap_node(node);
    let members = node["members"].as_array()?;
    let first = unwrap_node(members.first()?);
    let last = unwrap_node(members.last()?);
    let a = first["value"].as_str()?;
    let b = last["value"].as_str()?;
    if a != b || a.len() != 1 {
        return None;
    }
    let byte = a.as_bytes()[0];
    let sample = format!("{a}\n{b}");
    let newline = regex::Regex::new(&format!("^(?:{pattern})$"))
        .ok()?
        .is_match(&sample);
    Some((byte, newline))
}
fn ir_regex(
    node: &Value,
    rules: &serde_json::Map<String, Value>,
    seen: &mut BTreeSet<String>,
) -> Result<String> {
    let ty = node["type"]
        .as_str()
        .context("lexical IR node without type")?;
    Ok(match ty {
        "BLANK" => String::new(),
        "STRING" => regex::escape(node["value"].as_str().context("STRING without value")?),
        "PATTERN" => format!(
            "(?:{})",
            lower_js_pattern(node["value"].as_str().context("PATTERN without value")?)?
        ),
        "SEQ" => node["members"]
            .as_array()
            .context("SEQ without members")?
            .iter()
            .map(|n| ir_regex(n, rules, seen))
            .collect::<Result<String>>()?,
        "CHOICE" => format!(
            "(?:{})",
            node["members"]
                .as_array()
                .context("CHOICE without members")?
                .iter()
                .map(|n| ir_regex(n, rules, seen))
                .collect::<Result<Vec<_>>>()?
                .join("|")
        ),
        "REPEAT" => format!("(?:{})*", ir_regex(&node["content"], rules, seen)?),
        "REPEAT1" => format!("(?:{})+", ir_regex(&node["content"], rules, seen)?),
        "TOKEN" | "IMMEDIATE_TOKEN" | "PREC" | "PREC_LEFT" | "PREC_RIGHT" | "ALIAS" => {
            ir_regex(&node["content"], rules, seen)?
        }
        "SYMBOL" => {
            let name = node["name"].as_str().context("SYMBOL without name")?;
            if !seen.insert(name.into()) {
                bail!("recursive lexical symbol {name}")
            }
            let result = ir_regex(
                rules
                    .get(name)
                    .with_context(|| format!("unknown lexical symbol {name}"))?,
                rules,
                seen,
            )?;
            seen.remove(name);
            format!("(?:{result})")
        }
        other => bail!("unsupported lexical IR node {other}: {node}"),
    })
}
fn lower_js_pattern(pattern: &str) -> Result<String> {
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut out = String::new();
    let mut i = 0;
    let mut class = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '[' => {
                class = true;
                out.push(c);
                i += 1
            }
            ']' => {
                class = false;
                out.push(c);
                i += 1
            }
            '.' if !class => {
                out.push_str("[^\\n\\r\\u{2028}\\u{2029}]");
                i += 1
            }
            '\\' => {
                let next = *chars.get(i + 1).context("trailing pattern escape")?;
                match next{
            'd'=>out.push_str("[0-9]"),'D'=>out.push_str("[^0-9]"),
            'w'=>out.push_str("[A-Za-z0-9_]"),'W'=>out.push_str("[^A-Za-z0-9_]"),
            's'=>out.push_str("[\\x09-\\x0D\\x20\\u{A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}]"),
            'S'=>out.push_str("[^\\x09-\\x0D\\x20\\u{A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}]"),
            'p'|'P'=>{out.push('\\');out.push(next);i+=2;if chars.get(i)!=Some(&'{'){bail!("unsupported Unicode property escape in {pattern}")}while i<chars.len(){let x=chars[i];out.push(x);i+=1;if x=='}'{break}}continue},
            'u'=>bail!("JavaScript \\u escapes require explicit lowering: {pattern}"),
            '1'..='9'=>bail!("backreferences are unsupported: {pattern}"),
            _=>{out.push('\\');out.push(next)}
        }
                i += 2
            }
            '(' if chars.get(i + 1) == Some(&'?') => {
                bail!("JavaScript group extensions are unsupported: {pattern}")
            }
            _ => {
                out.push(c);
                i += 1
            }
        }
    }
    if class {
        bail!("unterminated character class: {pattern}")
    }
    Ok(out)
}
fn validate_pattern_semantics(
    node: &str,
    temp: &std::path::Path,
    patterns: &BTreeSet<String>,
) -> Result<()> {
    let samples = [
        "",
        "0",
        "12",
        "123",
        "١",
        "A",
        "_",
        "π",
        "\n",
        "\r",
        "\u{2028}",
        "\u{2029}",
        "\u{a0}",
        "\u{feff}",
        "*",
        "**/",
        "abc",
        "x41",
        "u1234",
        "U00000041",
        "`",
        "\\n",
    ];
    let input = serde_json::json!({"patterns":patterns,"samples":samples});
    let script = temp.join("pattern_oracle.js");
    fs::write(
        &script,
        r#"const fs=require('fs');const x=JSON.parse(fs.readFileSync(0,'utf8'));process.stdout.write(JSON.stringify(x.patterns.map(p=>x.samples.map(s=>new RegExp('^(?:'+p+')$','u').test(s)))));"#,
    )?;
    let mut child = Command::new(node)
        .arg(script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&input)?.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!("JavaScript pattern oracle failed")
    };
    let expected: Vec<Vec<bool>> = serde_json::from_slice(&output.stdout)?;
    for ((pattern, want), index) in patterns.iter().zip(expected).zip(0..) {
        let lowered = lower_js_pattern(pattern)?;
        let rust = regex::Regex::new(&format!("^(?:{lowered})$"))
            .with_context(|| format!("lower pattern {pattern}"))?;
        for (sample, expected) in samples.iter().zip(want) {
            let got = rust.is_match(sample);
            if got != expected {
                bail!("pattern semantics differ for pattern #{index} {pattern:?}, sample {sample:?}: JS={expected}, generated={got}")
            }
        }
    }
    Ok(())
}
fn outer_prec(mut node: &Value) -> i32 {
    loop {
        match node["type"].as_str() {
            Some("ALIAS" | "TOKEN" | "IMMEDIATE_TOKEN") => node = &node["content"],
            Some("PREC" | "PREC_LEFT" | "PREC_RIGHT") => {
                return node["value"].as_i64().unwrap_or(0) as i32
            }
            _ => return 0,
        }
    }
}
fn nested_prec(node: &Value) -> i32 {
    let own = if node["type"] == "PREC" {
        node["value"].as_i64().unwrap_or(0) as i32
    } else {
        0
    };
    own.max(node.get("content").map(nested_prec).unwrap_or(0))
        .max(
            node.get("members")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(nested_prec).max().unwrap_or(0))
                .unwrap_or(0),
        )
}
fn compile_dfa(pattern: &str) -> Result<(Vec<u16>, Vec<u8>, usize, Vec<bool>, Vec<bool>)> {
    let dfa = dense::Builder::new()
        .configure(dense::Config::new().match_kind(MatchKind::All))
        .build(pattern)
        .with_context(|| format!("compile lexical expression {pattern}"))?;
    let start = dfa.start_state_forward(&Input::new("").anchored(Anchored::Yes))?;
    let mut ids = vec![start];
    let mut map = BTreeMap::new();
    map.insert(start.as_usize(), 0u16);
    let mut rows = Vec::new();
    let mut accept = Vec::new();
    let mut dead = Vec::new();
    let mut at = 0;
    while at < ids.len() {
        let state = ids[at];
        accept.push(dfa.is_match_state(dfa.next_eoi_state(state)));
        dead.push(dfa.is_dead_state(state));
        let mut row = [0u16; 256];
        for byte in 0..=255u8 {
            let next = dfa.next_state(state, byte);
            let key = next.as_usize();
            let id = if let Some(id) = map.get(&key) {
                *id
            } else {
                let id = ids.len() as u16;
                map.insert(key, id);
                ids.push(next);
                id
            };
            row[byte as usize] = id
        }
        rows.extend(row);
        at += 1
    }
    let states = accept.len();
    let mut representatives = Vec::<usize>::new();
    let mut signatures = BTreeMap::<Vec<u16>, u8>::new();
    let mut classes = vec![0u8; 256];
    for byte in 0..256 {
        let signature = (0..states)
            .map(|s| rows[s * 256 + byte])
            .collect::<Vec<_>>();
        let class = if let Some(class) = signatures.get(&signature) {
            *class
        } else {
            let class = representatives.len() as u8;
            representatives.push(byte);
            signatures.insert(signature, class);
            class
        };
        classes[byte] = class
    }
    let mut compact = Vec::with_capacity(states * representatives.len());
    for state in 0..states {
        for &byte in &representatives {
            compact.push(rows[state * 256 + byte])
        }
    }
    Ok((compact, classes, representatives.len(), accept, dead))
}
fn structurally_delimited(node: &Value) -> bool {
    let n = unwrap_node(node);
    let Some(ms) = n["members"].as_array() else {
        return false;
    };
    let (Some(a), Some(b)) = (
        ms.first().and_then(|x| unwrap_node(x)["value"].as_str()),
        ms.last().and_then(|x| unwrap_node(x)["value"].as_str()),
    ) else {
        return false;
    };
    a == b && !a.is_empty()
}
fn lexical_candidate(node: &Value) -> bool {
    matches!(
        node["type"].as_str(),
        Some("PATTERN" | "TOKEN" | "IMMEDIATE_TOKEN")
    ) || structurally_delimited(node)
}
fn references(node: &Value, out: &mut BTreeSet<String>) {
    if node["type"] == "SYMBOL" {
        if let Some(n) = node["name"].as_str() {
            out.insert(n.into());
        }
    }
    if let Some(c) = node.get("content") {
        references(c, out)
    }
    if let Some(ms) = node.get("members").and_then(Value::as_array) {
        for m in ms {
            references(m, out)
        }
    }
}
fn collect_terminals(
    node: &Value,
    rules: &serde_json::Map<String, Value>,
    machines: &BTreeSet<String>,
    strings: &mut BTreeSet<String>,
    patterns: &mut BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) {
    match node["type"].as_str() {
        Some("STRING") => {
            strings.insert(node["value"].as_str().unwrap().into());
        }
        Some("PATTERN") => {
            patterns.insert(node["value"].as_str().unwrap().into());
        }
        Some("SYMBOL") => {
            let n = node["name"].as_str().unwrap();
            if !machines.contains(n) && seen.insert(n.into()) {
                if let Some(rule) = rules.get(n) {
                    collect_terminals(rule, rules, machines, strings, patterns, seen)
                }
                seen.remove(n);
            }
        }
        _ => {
            if let Some(c) = node.get("content") {
                collect_terminals(c, rules, machines, strings, patterns, seen)
            }
            if let Some(ms) = node.get("members").and_then(Value::as_array) {
                for m in ms {
                    collect_terminals(m, rules, machines, strings, patterns, seen)
                }
            }
        }
    }
}
fn compile_lexical_machines(
    json: &Value,
) -> Result<(Vec<Machine>, BTreeSet<String>, BTreeSet<String>)> {
    let rules = json["rules"].as_object().context("grammar rules missing")?;
    let candidates = rules
        .iter()
        .filter(|(_, n)| lexical_candidate(n))
        .map(|(n, _)| n.clone())
        .collect::<BTreeSet<_>>();
    let mut selected = BTreeSet::new();
    for (name, node) in rules {
        if candidates.contains(name) {
            continue;
        }
        let mut refs = BTreeSet::new();
        references(node, &mut refs);
        selected.extend(refs.into_iter().filter(|r| candidates.contains(r)))
    }
    let word = json["word"].as_str().context("word token missing")?;
    selected.insert(word.into());
    let mut skip_names = BTreeSet::<String>::new();
    for extra in json["extras"]
        .as_array()
        .context("grammar extras missing")?
    {
        if extra["type"] == "SYMBOL" {
            let n = extra["name"]
                .as_str()
                .context("extra symbol missing name")?;
            selected.insert(n.into());
            skip_names.insert(n.into());
        }
    }
    let mut out = Vec::new();
    for name in &selected {
        let node = rules
            .get(name)
            .with_context(|| format!("missing lexical rule {name}"))?;
        let skip = skip_names.contains(name);
        let pattern = ir_regex(node, rules, &mut BTreeSet::new())?;
        if nested_prec(node) != outer_prec(node) {
            let prefix=leading_string(node).filter(|p|!p.is_empty()).with_context(||format!("branch-local lexical precedence in non-delimited rule {name} is unsupported"))?;
            let competitors = selected
                .iter()
                .filter(|other| {
                    *other != name
                        && rules.get(*other).and_then(leading_string).as_ref() == Some(&prefix)
                })
                .count();
            if !structurally_delimited(node) || competitors != 0 {
                bail!("branch-local precedence for {name} is not isolated by a unique delimiter")
            }
        }
        let recovery = delimited_recovery(node, &pattern);
        let (dfa, classes, class_count, accept, dead) = compile_dfa(&pattern)?;
        out.push(Machine {
            name: name.into(),
            dfa,
            classes,
            class_count,
            accept,
            dead,
            skip,
            priority: outer_prec(node),
            recovery,
            recovery_prefixes: recovery_prefixes(node, skip),
        })
    }
    for (index, extra) in json["extras"]
        .as_array()
        .context("grammar extras missing")?
        .iter()
        .enumerate()
    {
        if extra["type"] == "SYMBOL" {
            continue;
        }
        let pattern = ir_regex(extra, rules, &mut BTreeSet::new())?;
        let (dfa, classes, class_count, accept, dead) = compile_dfa(&pattern)?;
        out.push(Machine {
            name: format!("extra_{index}"),
            dfa,
            classes,
            class_count,
            accept,
            dead,
            skip: true,
            priority: outer_prec(extra),
            recovery: None,
            recovery_prefixes: Vec::new(),
        })
    }
    let mut strings = BTreeSet::new();
    let mut patterns = BTreeSet::new();
    let roots = rules.keys().cloned().collect::<Vec<_>>();
    for name in roots {
        if !selected.contains(&name) {
            collect_terminals(
                &rules[&name],
                rules,
                &selected,
                &mut strings,
                &mut patterns,
                &mut BTreeSet::new(),
            )
        }
    }
    for name in &selected {
        collect(&rules[name], &mut BTreeSet::new(), &mut patterns)
    }
    for extra in json["extras"].as_array().unwrap() {
        collect(extra, &mut BTreeSet::new(), &mut patterns)
    }
    Ok((out, strings, patterns))
}
fn collect(v: &Value, s: &mut BTreeSet<String>, p: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            if let (Some(t), Some(x)) = (
                m.get("type").and_then(Value::as_str),
                m.get("value").and_then(Value::as_str),
            ) {
                if t == "STRING" {
                    s.insert(x.into());
                } else if t == "PATTERN" {
                    p.insert(x.into());
                }
            }
            for x in m.values() {
                collect(x, s, p)
            }
        }
        Value::Array(a) => {
            for x in a {
                collect(x, s, p)
            }
        }
        _ => {}
    }
}
fn list(v: &BTreeSet<String>) -> String {
    v.iter().map(|x| format!("    {x:?},\n")).collect()
}
fn token_name(prefix: &str, text: &str) -> String {
    if prefix == "KW" {
        format!("KW_{}", text.to_ascii_uppercase())
    } else {
        format!(
            "PUNCT_{}",
            text.as_bytes()
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<Vec<_>>()
                .join("_")
        )
    }
}
fn token_constants(k: &BTreeSet<String>, p: &BTreeSet<String>) -> String {
    k.iter()
        .map(|text| ("KW", text))
        .chain(p.iter().map(|text| ("PUNCT", text)))
        .enumerate()
        .map(|(index, (prefix, text))| {
            format!(
                "pub const {}:TokenKind=TokenKind({});\n",
                token_name(prefix, text),
                index + 4
            )
        })
        .collect()
}
fn keyword_match(values: &BTreeSet<String>) -> String {
    values
        .iter()
        .map(|text| format!("            {text:?}=>{},\n", token_name("KW", text)))
        .collect()
}
fn punctuation_match(values: &BTreeSet<String>) -> String {
    let mut groups = BTreeMap::<u8, Vec<&str>>::new();
    for text in values {
        groups.entry(text.as_bytes()[0]).or_default().push(text);
    }
    groups
        .into_iter()
        .map(|(first, mut values)| {
            values.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
            let checks: String = values
                .into_iter()
                .map(|text| {
                    format!(
                        "                if remainder.starts_with({text:?}){{return Some(({},{}));}}\n",
                        text.len(),
                        token_name("PUNCT", text)
                    )
                })
                .collect();
            format!("            0x{first:02X}=>{{\n{checks}            }}\n")
        })
        .collect()
}
fn render(
    hash: &str,
    cli: &str,
    word: &str,
    externals: usize,
    k: &BTreeSet<String>,
    p: &BTreeSet<String>,
    r: &BTreeSet<String>,
    machines: &[Machine],
) -> String {
    let mut machine_defs = String::new();
    let mut machine_rows = String::new();
    for (i, m) in machines.iter().enumerate() {
        let transitions = m
            .dfa
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let accepts = m
            .accept
            .iter()
            .map(bool::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let dead = m
            .dead
            .iter()
            .map(bool::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let classes = m
            .classes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",");
        machine_defs.push_str(&format!("const TRANS_{i}:&[u16]=&[{transitions}];\nconst CLASS_{i}:&[u8;256]=&[{classes}];\nconst ACCEPT_{i}:&[bool]=&[{accepts}];\nconst DEAD_{i}:&[bool]=&[{dead}];\n"));
        let kind = if m.name == word {
            "IDENTIFIER"
        } else {
            "LITERAL"
        };
        let recovery = m
            .recovery
            .map_or("None".into(), |(b, n)| format!("Some(({b},{n}))"));
        let prefixes = m
            .recovery_prefixes
            .iter()
            .map(|p| format!("&{:?}", p))
            .collect::<Vec<_>>()
            .join(",");
        machine_rows.push_str(&format!("Machine{{trans:TRANS_{i},classes:CLASS_{i},class_count:{},accept:ACCEPT_{i},dead:DEAD_{i},kind:{kind},skip:{},priority:{},recovery:{recovery},recovery_prefixes:&[{prefixes}]}},\n",m.class_count,m.skip,m.priority));
    }
    format!(
        r#"// @generated by `cargo run --bin generate-go-scanner`; do not edit.
// generator: treetags-linear-scanner-v4
// grammar.js sha256: {hash}
// evaluated with: {cli}

use crate::parser::linear::{{ExternalLexer,GeneratedLexeme,GeneratedLexicon,TokenKind,TokenStream}};
pub const IDENTIFIER:TokenKind=TokenKind(1);pub const LITERAL:TokenKind=TokenKind(2);pub const UNKNOWN:TokenKind=TokenKind(3);
{}
pub const WORD_TOKEN_RULE:&str={word:?};pub const DECLARED_EXTERNAL_COUNT:usize={externals};
pub const LEXICAL_PATTERNS:&[&str]=&[
{}];
pub struct Lexicon;
impl GeneratedLexicon for Lexicon{{
    const UNKNOWN:TokenKind=UNKNOWN;
    fn lex(source:&str,offset:usize)->GeneratedLexeme{{lex(source,offset)}}
}}
struct Machine{{trans:&'static[u16],classes:&'static[u8;256],class_count:usize,accept:&'static[bool],dead:&'static[bool],kind:TokenKind,skip:bool,priority:i32,recovery:Option<(u8,bool)>,recovery_prefixes:&'static[&'static[u8]]}}
{machine_defs}
const MACHINES:&[Machine]=&[{machine_rows}];
fn keyword(text:&str)->TokenKind{{
        match text{{
{}            _=>IDENTIFIER,
        }}
}}
fn punctuation(remainder:&str)->Option<(usize,TokenKind)>{{
        match remainder.as_bytes().first().copied()?{{
{}            _=>{{}}
        }}
        None
}}
fn item(len:usize,kind:TokenKind,skip:bool,error:bool)->GeneratedLexeme{{GeneratedLexeme{{len,kind,skip,error}}}}
fn lex(source:&str,at:usize)->GeneratedLexeme{{
 let rest=&source[at..];let bytes=rest.as_bytes();let mut best:(usize,i32,TokenKind,bool)=(0,i32::MIN,UNKNOWN,false);
 for m in MACHINES{{
  let mut state=0usize;
  for (n,&byte) in bytes.iter().enumerate(){{
   state=m.trans[state*m.class_count+m.classes[byte as usize]as usize]as usize;
   if m.dead[state]{{break}}
   if m.accept[state]{{let len=n+1;if len>best.0||(len==best.0&&m.priority>best.1){{best=(len,m.priority,m.kind,m.skip)}}}}
  }}
 }}
 if best.0==0{{for m in MACHINES{{if let Some((delimiter,newline))=m.recovery{{if bytes.first()==Some(&delimiter){{let mut n=1;while n<bytes.len()&&bytes[n]!=delimiter&&(newline||bytes[n]!=b'\n'){{n+=1}}if n<bytes.len()&&bytes[n]==delimiter{{n+=1}}return item(n,m.kind,m.skip,true)}}}}}}}}
 if best.0==0{{for m in MACHINES{{if m.recovery_prefixes.iter().any(|prefix|bytes.starts_with(prefix)){{return item(bytes.len(),m.kind,m.skip,true)}}}}}}
 if let Some((len,kind))=punctuation(rest){{if len>best.0{{best=(len,0,kind,false)}}}}
 if best.0>0{{let kind=if best.2==IDENTIFIER{{keyword(&rest[..best.0])}}else{{best.2}};return item(best.0,kind,best.3,false)}}
 let ch=rest.chars().next().unwrap();item(ch.len_utf8(),UNKNOWN,false,true)
}}
pub fn scan<E:ExternalLexer>(source:&str)->Result<TokenStream,String>{{crate::parser::linear_scanner::scan::<E,Lexicon>(source)}}
"#,
        token_constants(k, p),
        list(r),
        keyword_match(k),
        punctuation_match(p)
    )
}
