use crate::config::Config;
use crate::language_parser::LanguageParserRegistry;
use crate::tag::Tag;
use std::fs;
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::thread;

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

            let Some(extension) = file_path.extension().and_then(|e| e.to_str()) else {
                continue;
            };

            let Some(lp) = registry.for_extension(extension) else {
                continue;
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

            local_tags.append(&mut tags);
        }

        local_tags
    }
}
