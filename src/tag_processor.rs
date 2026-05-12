use crate::config::Config;
use crate::language_parser::LanguageParserRegistry;
use crate::tag::Tag;
use std::fs;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
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
        let tags_lock = Arc::new(Mutex::new(Vec::new()));
        let mut threads = Vec::with_capacity(self.workers);
        let mut senders = Vec::with_capacity(self.workers);

        // Build registry once; share Arc across threads.
        // LanguageParserRegistry::new also JIT-compiles WASM plugins once.
        let lang_registry = Arc::new(LanguageParserRegistry::new(&self.config));

        for _ in 0..self.workers {
            let (sender, receiver) = mpsc::channel::<String>();
            let tags_lock = Arc::clone(&tags_lock);
            let tag_file_path = self.tag_file_path.clone();
            let config = self.config.clone();
            let registry = Arc::clone(&lang_registry);

            let thread = thread::spawn(move || {
                Self::worker(receiver, tags_lock, tag_file_path, config, registry);
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

        for thread in threads {
            if let Err(e) = thread.join() {
                eprintln!("Worker thread panicked: {:?}", e);
            }
        }

        let result = {
            let lock_result = tags_lock.lock();
            match lock_result {
                Ok(guard) => guard.clone(),
                Err(poisoned) => {
                    eprintln!("Lock was poisoned: mutex poisoned error");
                    poisoned.into_inner().clone()
                }
            }
        };

        result
    }

    fn worker(
        file_names_rx: mpsc::Receiver<String>,
        tags_lock: Arc<Mutex<Vec<Tag>>>,
        tag_file_path: String,
        config: Config,
        registry: Arc<LanguageParserRegistry>,
    ) {
        // One Parser per thread — holds mutable parse state (ts_parser, tags_context, etc.)
        let mut parser = registry.create_parser();

        let tag_file_dir = if tag_file_path == "-" {
            std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())
        } else {
            let p = Path::new(&tag_file_path);
            p.parent().unwrap_or(Path::new("")).to_path_buf()
        };

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

            let mut tags = lp.generate_tags(&mut parser, &code, &file_path_relative, &config);

            if let Ok(mut guard) = tags_lock.lock() {
                guard.append(&mut tags);
            }
        }
    }
}
