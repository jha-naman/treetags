use crate::config::Config;
use crate::language_parser::{LanguageParserRegistry, NameResolution};
use crate::tag::Tag;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::thread;

/// Bytes read from the head of a file to inspect its `#!` shebang line.
const SHEBANG_PREFIX_BYTES: u64 = 256;

/// Reads up to `max` bytes from the start of `path`.
fn read_prefix(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    fs::File::open(path)?.take(max).read_to_end(&mut buf)?;
    Ok(buf)
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

            let lp = match registry.resolve_by_name(&file_path) {
                NameResolution::Unique(id) => registry.parser(id),
                // No ambiguous extensions exist yet; fall back to the
                // highest-priority candidate until selectors land.
                NameResolution::Ambiguous(ids) => registry.parser(ids[0]),
                NameResolution::None => {
                    // Shebang fallback, gated (matching ctags) behind the
                    // executable bit or --guess-language-eagerly. Only a bounded
                    // prefix is read here, so non-script files are cheap to skip.
                    if !config.guess_language_eagerly && !is_executable(&file_path) {
                        continue;
                    }
                    match read_prefix(&file_path, SHEBANG_PREFIX_BYTES) {
                        Ok(prefix) => match registry.resolve_by_shebang(&prefix) {
                            Some(id) => registry.parser(id),
                            None => continue,
                        },
                        Err(_) => continue,
                    }
                }
            };

            let code = match fs::read(&file_path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{}", e);
                    continue;
                }
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
