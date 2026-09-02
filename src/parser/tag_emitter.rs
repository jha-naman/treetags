#![allow(dead_code)]
use super::linear::{HookInput, Tok};
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
}
impl<'a> TagEmitter<'a> {
    pub fn new(input: HookInput<'a>, tags: &'a mut Vec<Tag>) -> Self {
        Self { input, tags }
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
    typeref: Option<TextValue<'a>>,
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
    pub fn typeref(mut self, value: impl Into<TextValue<'a>>) -> Self {
        self.typeref = Some(value.into());
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
        if let Some((kind, value)) = self.scope {
            if options.scope || options.qualified {
                fields.insert(kind, value.get(source).into_owned())
            }
        }
        if let Some(v) = self.typeref {
            if options.typeref {
                fields.insert("typeref", format!("typename:{}", v.get(source)))
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

impl TagEmitter<'_> {
    pub fn set_end(&mut self, handle: usize, start_row: u32, end_row: u32) {
        if !self.input.options.end || end_row <= start_row {
            return;
        }
        if let Some(tag) = self.tags.get_mut(handle) {
            let fields = tag
                .extension_fields
                .get_or_insert_with(ExtensionFields::new);
            fields.insert("end", (end_row + 1).to_string());
        }
    }
}
