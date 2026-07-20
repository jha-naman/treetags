use crate::config::Config;
use crate::language_parser::{LangId, LanguageParserRegistry, NameResolution};
use crate::tag::Tag;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::thread;

/// Bytes read from the head of a file to inspect its `#!` shebang line.
const SHEBANG_PREFIX_BYTES: u64 = 256;

/// Bytes read from the head of a file for content-based selector heuristics
/// (e.g. C vs C++ for `.h`).
const SELECTOR_PREFIX_BYTES: u64 = 8192;

/// Bytes read from each of the head and tail of a file to inspect editor
/// modelines.
const MODELINE_WINDOW_BYTES: u64 = 4096;

/// Resolves the language for a file through the full ladder: name (force /
/// pattern / extension), then content-based disambiguation for ambiguous
/// names, then a gated `#!` shebang fallback. Reads file content only when the
/// name is ambiguous or unresolved, so the common case does no IO here.
///
/// A resolved language, plus any file content already read while resolving it,
/// so the caller can avoid re-reading the file.
pub(crate) struct Selection {
    pub lang: LangId,
    /// The full file content when it was read during resolution (ambiguous
    /// names), else `None`.
    pub content: Option<Vec<u8>>,
}

/// Shared by the tag-generation worker and `--print-language`.
pub(crate) fn select_language(
    registry: &LanguageParserRegistry,
    config: &Config,
    path: &Path,
) -> Option<Selection> {
    match registry.resolve_by_name(path) {
        NameResolution::Unique(id) => Some(Selection {
            lang: id,
            content: None,
        }),
        NameResolution::Ambiguous(ids) => {
            // The file will be parsed whichever candidate wins, so read it in
            // full once here and hand it back for reuse instead of reading a
            // prefix now and the whole file again for parsing.
            match fs::read(path) {
                Ok(content) => {
                    let cap = (SELECTOR_PREFIX_BYTES as usize).min(content.len());
                    let lang = registry
                        .disambiguate(&ids, &content[..cap])
                        .unwrap_or(ids[0]);
                    Some(Selection {
                        lang,
                        content: Some(content),
                    })
                }
                // Let the caller surface the read error uniformly.
                Err(_) => Some(Selection {
                    lang: ids[0],
                    content: None,
                }),
            }
        }
        NameResolution::None => {
            // Content guessing (matching ctags): shebang runs for executable
            // files or under -G; editor modelines run only under -G. Only
            // bounded prefixes are read so unmatched files stay cheap to skip.
            if config.guess_language_eagerly || is_executable(path) {
                if let Some(id) = read_prefix(path, SHEBANG_PREFIX_BYTES)
                    .ok()
                    .and_then(|prefix| registry.resolve_by_shebang(&prefix))
                {
                    return Some(Selection {
                        lang: id,
                        content: None,
                    });
                }
            }
            if config.guess_language_eagerly {
                if let Some(id) = read_head_and_tail(path, MODELINE_WINDOW_BYTES)
                    .ok()
                    .and_then(|(head, tail)| registry.resolve_by_modeline(&head, &tail))
                {
                    return Some(Selection {
                        lang: id,
                        content: None,
                    });
                }
            }
            None
        }
    }
}

/// Reads up to `max` bytes from the start of `path`.
fn read_prefix(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    fs::File::open(path)?.take(max).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Reads up to `window` bytes from the head and, for larger files, up to
/// `window` bytes from the tail. The tail is empty when the whole file already
/// fits in the head window.
fn read_head_and_tail(path: &Path, window: u64) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let mut file = fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut head = Vec::new();
    (&mut file).take(window).read_to_end(&mut head)?;
    if len <= window {
        return Ok((head, Vec::new()));
    }
    let mut tail = Vec::new();
    file.seek(SeekFrom::Start(len - window))?;
    file.take(window).read_to_end(&mut tail)?;
    Ok((head, tail))
}

/// Whether `path` has the executable bit set. Always `false` on non-Unix, where
/// there is no executable bit, so shebang detection there requires `-G`.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    false
}

pub struct TagProcessor {
    tag_file_path: String,
    workers: usize,
    config: Config,
}

impl TagProcessor {
    pub fn new(tag_file_path: String, workers: usize, config: Config) -> Self {
        Self {
            tag_file_path,
            workers,
            config,
        }
    }

    pub fn process_files(&self, file_names: Vec<String>) -> Vec<Tag> {
        let mut threads = Vec::with_capacity(self.workers);
        let mut senders = Vec::with_capacity(self.workers);

        // Build registry once; share Arc across threads.
        // LanguageParserRegistry::new also JIT-compiles WASM plugins once.
        let lang_registry = Arc::new(LanguageParserRegistry::new(&self.config));

        for _ in 0..self.workers {
            let (sender, receiver) = mpsc::channel::<String>();
            let tag_file_path = self.tag_file_path.clone();
            let config = self.config.clone();
            let registry = Arc::clone(&lang_registry);

            let thread = thread::spawn(move || -> Vec<Tag> {
                Self::worker(receiver, tag_file_path, config, registry)
            });

            threads.push(thread);
            senders.push(sender);
        }

        for chunk in file_names.chunks(self.workers) {
            for (index, file_name) in chunk.iter().enumerate() {
                if let Err(e) = senders[index].send(file_name.clone()) {
                    eprintln!("Failed to send file to worker: {}", e);
                }
            }
        }

        drop(senders);

        let mut all_tags = Vec::new();
        for thread in threads {
            match thread.join() {
                Ok(tags) => all_tags.extend(tags),
                Err(e) => eprintln!("Worker thread panicked: {:?}", e),
            }
        }

        all_tags
    }

    fn worker(
        file_names_rx: mpsc::Receiver<String>,
        tag_file_path: String,
        config: Config,
        registry: Arc<LanguageParserRegistry>,
    ) -> Vec<Tag> {
        // One Parser per thread — holds mutable parse state (ts_parser, tags_context, etc.)
        let mut parser = registry.create_parser();

        let tag_file_dir = if tag_file_path == "-" {
            std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())
        } else {
            let p = Path::new(&tag_file_path);
            p.parent().unwrap_or(Path::new("")).to_path_buf()
        };

        let mut local_tags = Vec::new();

        while let Ok(file_name) = file_names_rx.recv() {
            let file_path = std::env::current_dir().unwrap().join(&file_name);

            let file_path_relative = match file_path.strip_prefix(&tag_file_dir) {
                Ok(path) => path.to_string_lossy().into_owned(),
                Err(_) => file_name.clone(),
            };

            let selection = match select_language(&registry, &config, &file_path) {
                Some(selection) => selection,
                None => continue,
            };
            let lp = registry.parser(selection.lang);

            // Reuse content already read during resolution (ambiguous names)
            // instead of reading the file a second time.
            let code = match selection.content {
                Some(content) => content,
                None => match fs::read(&file_path) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("{}", e);
                        continue;
                    }
                },
            };

            let mut tags =
                lp.generate_tags(&mut parser, &code, &file_path_relative, &config, &file_path);

            if config.sort {
                tags.sort_unstable_by(|a, b| a.sort_cmp(b));
            }

            local_tags.append(&mut tags);
        }

        local_tags
    }
}
