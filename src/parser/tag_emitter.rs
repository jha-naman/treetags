#![allow(dead_code)]
use super::linear::{BlockMap, HookInput, Tok};
use crate::tag::{ExtensionFields, Tag};
use std::{borrow::Cow, sync::Arc};

pub(crate) enum TextValue<'a> {
    Span(u32, u32),
    Borrowed(&'a str),
    Owned(String),
}
impl<'a> TextValue<'a> {
    fn get<'s>(&'s self, source: &'a str) -> Cow<'s, str> {
        match self {
            Self::Span(a, b) => Cow::Borrowed(&source[*a as usize..*b as usize]),
            Self::Borrowed(v) => Cow::Borrowed(v),
            Self::Owned(v) => Cow::Borrowed(v),
        }
    }
}
impl From<String> for TextValue<'_> {
    fn from(v: String) -> Self {
        Self::Owned(v)
    }
}
impl<'a> From<&'a str> for TextValue<'a> {
    fn from(v: &'a str) -> Self {
        Self::Borrowed(v)
    }
}
impl From<Tok> for TextValue<'_> {
    fn from(v: Tok) -> Self {
        Self::Span(v.start, v.end)
    }
}

pub(crate) struct TagEmitter<'a> {
    input: HookInput<'a>,
    tags: &'a mut Vec<Tag>,
    blocks: BlockMap,
    /// C retains the innermost enclosing scope of each kind.
    inherit_all_scopes: bool,
    /// Innermost-last stack of enclosing scope frames. A tag with no explicit
    /// `.scope(...)` inherits the top frame, so hooks push a frame once instead
    /// of re-attaching the same scope to every declaration inside it.
    scopes: Vec<(&'static str, String)>,
}
impl<'a> TagEmitter<'a> {
    pub fn new(input: HookInput<'a>, tags: &'a mut Vec<Tag>, blocks: BlockMap) -> Self {
        Self {
            input,
            tags,
            blocks,
            scopes: Vec::new(),
            inherit_all_scopes: false,
        }
    }
    /// Retains the innermost scope of each kind, as required by the C oracle.
    pub fn inherit_all_scopes(&mut self) {
        self.inherit_all_scopes = true;
    }

    /// Pushes an enclosing scope that later `.tag(...)` calls inherit until the
    /// matching [`leave_scope`](Self::leave_scope).
    pub fn enter_scope(&mut self, kind: &'static str, name: String) {
        self.scopes.push((kind, name));
    }
    pub fn leave_scope(&mut self) {
        self.scopes.pop();
    }
    pub fn tag<'e>(
        &'e mut self,
        kind: &'static str,
        name: impl Into<TextValue<'a>>,
        declaration: (Tok, Tok),
    ) -> TagBuilder<'e, 'a> {
        TagBuilder {
            emitter: self,
            kind,
            name: name.into(),
            declaration,
            scope: None,
            signature: None,
            typeref: None,
            access: None,
            end_row: None,
        }
    }
}

pub(crate) struct TagBuilder<'e, 'a> {
    emitter: &'e mut TagEmitter<'a>,
    kind: &'static str,
    name: TextValue<'a>,
    declaration: (Tok, Tok),
    scope: Option<(&'static str, TextValue<'a>)>,
    signature: Option<TextValue<'a>>,
    typeref: Option<(&'static str, TextValue<'a>)>,
    access: Option<TextValue<'a>>,
    end_row: Option<u32>,
}
impl<'e, 'a> TagBuilder<'e, 'a> {
    pub fn scope(mut self, kind: &'static str, value: impl Into<TextValue<'a>>) -> Self {
        self.scope = Some((kind, value.into()));
        self
    }
    pub fn signature(mut self, value: impl Into<TextValue<'a>>) -> Self {
        self.signature = Some(value.into());
        self
    }
    /// `typeref:typename:<value>`.
    pub fn typeref(self, value: impl Into<TextValue<'a>>) -> Self {
        self.typeref_as("typename", value)
    }
    /// `typeref:<kind>:<value>` — e.g. `struct:` for a struct-typed reference.
    pub fn typeref_as(mut self, kind: &'static str, value: impl Into<TextValue<'a>>) -> Self {
        self.typeref = Some((kind, value.into()));
        self
    }
    /// A complete typeref value, for backends with an unprefixed oracle spelling.
    pub fn typeref_raw(mut self, value: impl Into<TextValue<'a>>) -> Self {
        self.typeref = Some(("", value.into()));
        self
    }
    pub fn access(mut self, value: impl Into<TextValue<'a>>) -> Self {
        self.access = Some(value.into());
        self
    }
    pub fn end(mut self, row: u32) -> Self {
        self.end_row = Some(row);
        self
    }
    /// Sets `end:` to the line that closes `open`'s balanced delimiter, using
    /// the emitter's precomputed [`BlockMap`]. A no-op if `open` is unmatched.
    pub fn body(mut self, open: Tok) -> Self {
        self.end_row = self.emitter.blocks.close_row(open);
        self
    }
    pub fn emit(self) -> Option<usize> {
        let options = self.emitter.input.options;
        if !options.tag_config.is_kind_enabled(self.kind) {
            return None;
        }
        let name = self.name.get(self.emitter.input.source);
        if name.is_empty() || name == "_" {
            return None;
        }
        let row = self.declaration.0.row as usize;
        let source = self.emitter.input.source;
        let start = self.emitter.input.line_starts[row] as usize;
        let finish = self
            .emitter
            .input
            .line_starts
            .get(row + 1)
            .map(|x| *x as usize - 1)
            .unwrap_or(source.len());
        let mut address = "/^".to_string();
        Tag::escape_address_into(source[start..finish].trim_end_matches('\r'), &mut address);
        address.push_str("$/;\"");
        let mut fields = ExtensionFields::new();
        if options.kind {
            fields.insert("kind", self.kind)
        }
        if options.line {
            fields.insert("line", (row + 1).to_string())
        }
        if options.file {
            fields.insert("file", self.emitter.input.path.to_string())
        }
        if self.emitter.inherit_all_scopes && (options.scope || options.qualified) {
            for (kind, value) in &self.emitter.scopes {
                fields.insert(*kind, value.clone());
            }
        }
        let scope = match &self.scope {
            Some((kind, value)) => Some((*kind, value.get(source).into_owned())),
            None => self
                .emitter
                .scopes
                .last()
                .map(|(kind, value)| (*kind, value.clone())),
        };
        if let Some((kind, value)) = scope {
            if options.scope || options.qualified {
                fields.insert(kind, value)
            }
        }
        if let Some((kind, v)) = self.typeref {
            if options.typeref {
                fields.insert(
                    "typeref",
                    if kind.is_empty() {
                        v.get(source).into_owned()
                    } else {
                        format!("{}:{}", kind, v.get(source))
                    },
                )
            }
        }
        if let Some(v) = self.signature {
            if options.signature {
                fields.insert("signature", v.get(source).into_owned())
            }
        }
        if let Some(v) = self.access {
            if options.access {
                fields.insert("access", v.get(source).into_owned())
            }
        }
        if let Some(end) = self.end_row {
            if options.end && end > row as u32 {
                fields.insert("end", (end + 1).to_string())
            }
        }
        let handle = self.emitter.tags.len();
        self.emitter.tags.push(Tag {
            name: name.into_owned(),
            file_name: Arc::from(self.emitter.input.path),
            address,
            kind: Some(self.kind.into()),
            extension_fields: (!fields.is_empty()).then_some(fields),
        });
        Some(handle)
    }
}
