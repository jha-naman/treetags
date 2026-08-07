//! A reusable, low-allocation scope stack for building ctags scope fields.
//!
//! A plugin pushes a frame when it enters a named scope (class, method, …) and
//! pops it on the way out. [`ScopeStack::current_field`] hands back the scope
//! extension field — its key (e.g. `"class"`) and the dotted-path value (e.g.
//! `"com.example.Foo"`) — for the tag being emitted.

/// A scope kind that knows its ctags scope-field key.
///
/// Implement this on your plugin's scope enum so a [`ScopeStack`] can label the
/// current scope. `key` is the field name written to the tags file, e.g.
/// `"class"`, `"interface"`, `"method"`.
pub trait ScopeKey: Copy {
    fn key(self) -> &'static str;
}

struct Frame<K> {
    key: K,
    /// Length of `path` before this frame's name (and separator) were appended,
    /// so [`ScopeStack::pop`] can restore it in O(1).
    path_len_before: usize,
}

/// A stack of nested scopes plus an optional outermost package/namespace.
pub struct ScopeStack<K: ScopeKey> {
    has_package: bool,
    frames: Vec<Frame<K>>,
    /// The dotted path of the package (if any) followed by every frame name.
    path: String,
}

impl<K: ScopeKey> Default for ScopeStack<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: ScopeKey> ScopeStack<K> {
    pub fn new() -> Self {
        Self {
            has_package: false,
            frames: Vec::new(),
            path: String::new(),
        }
    }

    /// Set the outermost package/namespace. Call before pushing any frame; it
    /// forms the root of the dotted path and is reported with the `"package"`
    /// key when no frames are on the stack.
    pub fn set_package(&mut self, name: &str) {
        debug_assert!(
            self.frames.is_empty() && !self.has_package,
            "set_package must be called once, before any scope is pushed"
        );
        self.path.push_str(name);
        self.has_package = true;
    }

    /// Enter a scope named `name` of kind `key`.
    pub fn push(&mut self, key: K, name: &str) {
        let path_len_before = self.path.len();
        if !self.path.is_empty() {
            self.path.push('.');
        }
        self.path.push_str(name);
        self.frames.push(Frame {
            key,
            path_len_before,
        });
    }

    /// Leave the innermost scope.
    pub fn pop(&mut self) {
        if let Some(frame) = self.frames.pop() {
            self.path.truncate(frame.path_len_before);
        }
    }

    /// The kind of the innermost scope, if any.
    pub fn last_key(&self) -> Option<K> {
        self.frames.last().map(|f| f.key)
    }

    /// Replace the kind of the innermost scope (e.g. promoting a pending lambda
    /// to a method once it is claimed). No-op if the stack is empty.
    pub fn set_last_key(&mut self, key: K) {
        if let Some(frame) = self.frames.last_mut() {
            frame.key = key;
        }
    }

    /// The scope extension field for a tag emitted at the current position:
    /// `(key, dotted_path)`. `None` when there is no enclosing scope or package.
    pub fn current_field(&self) -> Option<(&'static str, &str)> {
        let key = match self.frames.last() {
            Some(frame) => frame.key.key(),
            None if self.has_package => "package",
            None => return None,
        };
        Some((key, self.path.as_str()))
    }
}
