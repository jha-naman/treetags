// src/tag_processor.rs

//! Module for processing source files and generating tags.
//!
//! This module handles the multithreaded processing of source files,
//! extracting tag information and coordinating the results.

use crate::config::Config;
use crate::parser::Parser;
use crate::tag::Tag;
use indexmap::IndexMap;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use wasmtime::component::{bindgen, Component, Linker};
use wasmtime::{Engine, Store};

bindgen!({
    world: "plugin",
    path: "wit/treetags.wit",
});

use exports::treetags::plugin::tag_generator::Config as WasmConfig;
use exports::treetags::plugin::tag_generator::Tag as WasmTag;

/// A structure for processing files and generating tags.
///
/// TagProcessor dispatches file processing tasks to multiple worker
/// threads and collects the resulting tags.
pub struct TagProcessor {
    /// Path to the tag file, used for calculating relative paths
    tag_file_path: String,

    /// Number of worker threads to use for processing
    workers: usize,

    /// Configuration for tag generation
    config: Config,
}

struct WasmPlugin {
    store: Store<()>,
    bindings: Plugin,
    extensions: Vec<String>,
}

impl WasmPlugin {
    fn new(path: &Path, engine: &Engine) -> Result<Self, Box<dyn std::error::Error>> {
        let component = Component::from_file(engine, path)?;
        let linker = Linker::new(engine);
        let mut store = Store::new(engine, ());

        let bindings = Plugin::instantiate(&mut store, &component, &linker)?;
        let extensions = bindings
            .treetags_plugin_tag_generator()
            .call_supported_extensions(&mut store)?;

        Ok(Self {
            store,
            bindings,
            extensions,
        })
    }

    fn generate_tags(
        &mut self,
        source: &str,
        file_path: &str,
        config: &Config,
    ) -> Result<Vec<Tag>, String> {
        let wasm_config = WasmConfig {
            file_path: file_path.to_string(),
            enabled_kinds: vec![], // TODO: Populate based on file extension and config
            extras: config.extras.split_whitespace().map(String::from).collect(),
        };

        let result = self
            .bindings
            .treetags_plugin_tag_generator()
            .call_generate(&mut self.store, source, &wasm_config)
            .map_err(|e| e.to_string())?;

        match result {
            Ok(wasm_tags) => Ok(wasm_tags.into_iter().map(Self::convert_tag).collect()),
            Err(e) => Err(e),
        }
    }

    fn convert_tag(wasm_tag: WasmTag) -> Tag {
        let mut extension_fields = IndexMap::new();
        for (key, value) in wasm_tag.extension_fields {
            extension_fields.insert(key, value);
        }
        // Assuming address is line number based for now as per WIT definition
        let address = format!("{}", wasm_tag.line);

        Tag {
            name: wasm_tag.name,
            file_name: String::new(), // Will be filled by caller or adjusted
            address,
            kind: Some(wasm_tag.kind),
            extension_fields: Some(extension_fields),
        }
    }
}

impl TagProcessor {
    /// Creates a new TagProcessor instance.
    ///
    /// # Arguments
    ///
    /// * `tag_file_path` - Path to the tag file
    /// * `workers` - Number of worker threads to use
    /// * `config` - Configuration for tag generation
    ///
    /// # Returns
    ///
    /// A new TagProcessor instance
    pub fn new(tag_file_path: String, workers: usize, config: Config) -> Self {
        Self {
            tag_file_path,
            workers,
            config,
        }
    }

    /// Processes a list of files and generates tags.
    ///
    /// This method distributes the work among multiple threads and
    /// collects the results.
    ///
    /// # Arguments
    ///
    /// * `file_names` - List of file paths to process
    ///
    /// # Returns
    ///
    /// A vector of generated tags
    pub fn process_files(&self, file_names: Vec<String>) -> Vec<Tag> {
        let tags_lock = Arc::new(Mutex::new(Vec::new()));
        let mut threads = Vec::with_capacity(self.workers);
        let mut senders = Vec::with_capacity(self.workers);

        // Create worker threads
        for _ in 0..self.workers {
            let (sender, receiver) = mpsc::channel::<String>();
            let tags_lock = Arc::clone(&tags_lock);
            let tag_file_path = self.tag_file_path.clone();
            let config = self.config.clone();

            let thread = thread::spawn(move || {
                Self::worker(receiver, tags_lock, tag_file_path, config);
            });

            threads.push(thread);
            senders.push(sender);
        }

        // Distribute files to workers
        for chunk in file_names.chunks(self.workers) {
            for (index, file_name) in chunk.iter().enumerate() {
                if let Err(e) = senders[index].send(file_name.clone()) {
                    eprintln!("Failed to send file to worker: {}", e);
                }
            }
        }

        // Close all senders
        drop(senders);

        // Wait for all threads to complete
        for thread in threads {
            if let Err(e) = thread.join() {
                eprintln!("Worker thread panicked: {:?}", e);
            }
        }

        // Extract tags from the lock - Fixed the lifetime issue
        let result = {
            let lock_result = tags_lock.lock();
            match lock_result {
                Ok(guard) => guard.clone(),
                Err(poisoned) => {
                    eprintln!("Lock was poisoned: mutex poisoned error");
                    // Recover the data even if the mutex is poisoned
                    poisoned.into_inner().clone()
                }
            }
        };

        result
    }

    /// Worker function executed by each thread.
    ///
    /// Receives files to process, generates tags, and adds them to the
    /// shared tag collection.
    ///
    /// # Arguments
    ///
    /// * `file_names_rx` - Channel receiver for file names
    /// * `tags_lock` - Shared mutex for the tag collection
    /// * `tag_file_path` - Path to the tag file for relative path calculations
    /// * `config` - Configuration for tag generation
    fn worker(
        file_names_rx: mpsc::Receiver<String>,
        tags_lock: Arc<Mutex<Vec<Tag>>>,
        tag_file_path: String,
        config: Config,
    ) {
        let mut parser = Parser::new(&config);

        // Initialize WASM engine and plugins
        let engine = Engine::default();
        let mut plugins: Vec<WasmPlugin> = config
            .wasm_plugins
            .iter()
            .filter_map(|p| match WasmPlugin::new(p, &engine) {
                Ok(plugin) => Some(plugin),
                Err(e) => {
                    eprintln!("Failed to load WASM plugin {:?}: {}", p, e);
                    None
                }
            })
            .collect();

        let tag_file_dir = if tag_file_path == "-" {
            // If writing to stdout, use current directory as the base
            std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())
        } else {
            let tag_file_path = Path::new(&tag_file_path);
            tag_file_path
                .parent()
                .unwrap_or(Path::new(""))
                .to_path_buf()
        };

        // Process each file
        while let Ok(file_name) = file_names_rx.recv() {
            let file_path = std::env::current_dir().unwrap().join(&file_name);

            // Get relative path to tag file
            let file_path_relative = match file_path.strip_prefix(&tag_file_dir) {
                Ok(path) => path.to_string_lossy().into_owned(),
                Err(_) => file_name.clone(),
            };

            // Parse file if it has a recognizable extension
            if let Some(extension) = file_path.extension().and_then(|e| e.to_str()) {
                // Check if any WASM plugin supports this extension
                if let Some(plugin) = plugins
                    .iter_mut()
                    .find(|p| p.extensions.contains(&extension.to_string()))
                {
                    if let Ok(source) = std::fs::read_to_string(&file_path) {
                        match plugin.generate_tags(&source, &file_path_relative, &config) {
                            Ok(mut tags) => {
                                for tag in &mut tags {
                                    // Ensure file name is set correctly
                                    if tag.file_name.is_empty() {
                                        tag.file_name = file_path_relative.clone();
                                    }
                                }
                                if let Ok(mut tags_guard) = tags_lock.lock() {
                                    tags_guard.append(&mut tags);
                                }
                            }
                            Err(e) => eprintln!("Error in WASM plugin for {}: {}", file_name, e),
                        }
                    }
                    continue;
                }

                match parser.parse_file_with_config(
                    &file_path_relative,
                    &file_path.to_string_lossy(),
                    extension,
                    &config,
                ) {
                    Ok(mut tags) => {
                        // Add tags to the shared collection
                        if let Ok(mut tags_guard) = tags_lock.lock() {
                            tags_guard.append(&mut tags);
                        }
                    }
                    Err(error) => eprintln!("{}", error),
                }
            }
        }
    }
}
