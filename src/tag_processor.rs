use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use treetags::{Parser, Tag};

pub struct TagProcessor {
    tag_file_path: String,
    workers: usize,
}

impl TagProcessor {
    pub fn new(tag_file_path: String, workers: usize) -> Self {
        Self {
            tag_file_path,
            workers,
        }
    }

    pub fn process_files(&self, file_names: Vec<String>) -> Vec<Tag> {
        let tags_lock = Arc::new(Mutex::new(Vec::new()));
        let mut threads = Vec::with_capacity(self.workers);
        let mut senders = Vec::with_capacity(self.workers);

        // Create worker threads
        for _ in 0..self.workers {
            let (sender, receiver) = mpsc::channel::<String>();
            let tags_lock = Arc::clone(&tags_lock);
            let tag_file_path = self.tag_file_path.clone();

            let thread = thread::spawn(move || {
                Self::worker(receiver, tags_lock, tag_file_path);
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

    fn worker(
        file_names_rx: mpsc::Receiver<String>,
        tags_lock: Arc<Mutex<Vec<Tag>>>,
        tag_file_path: String,
    ) {
        let mut parser = Parser::new();
        let tag_file_path = Path::new(&tag_file_path);
        let tag_file_dir = tag_file_path.parent().unwrap_or(Path::new(""));

        // Process each file
        while let Ok(file_name) = file_names_rx.recv() {
            let file_path = std::path::PathBuf::from(&file_name);

            // Get relative path to tag file
            let file_path_relative = match file_path.strip_prefix(tag_file_dir) {
                Ok(path) => path.to_string_lossy().into_owned(),
                Err(_) => file_name.clone(),
            };

            // Parse file if it has a recognizable extension
            if let Some(extension) = file_path.extension().and_then(|e| e.to_str()) {
                let mut tags =
                    parser.parse_file(&file_path_relative, &file_path.to_string_lossy(), extension);

                // Add tags to the shared collection
                if let Ok(mut tags_guard) = tags_lock.lock() {
                    tags_guard.append(&mut tags);
                }
            }
        }
    }
}
