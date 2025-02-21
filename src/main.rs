use regex::RegexSet;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use treetags::{parse_tag_file, Config, Parser, Tag};
use walkdir::WalkDir;

mod shell_to_regex;

fn main() {
    let config = Config::new();
    let tags_lock = Arc::new(Mutex::new(Vec::new()));
    let tag_file_path_str = if config.append {
        match find_tag_file(&config.tag_file) {
            Some(path_str) => path_str,
            None => {
                eprintln!("Could not find the tag file: {}", config.tag_file);
                std::process::exit(1)
            }
        }
    } else {
        std::env::current_dir()
            .unwrap()
            .join(config.tag_file)
            .to_string_lossy()
            .into_owned()
    };
    let tag_file_path = Path::new(&tag_file_path_str);

    let file_names = if config.append {
        config.file_names
    } else {
        get_files_from_dir(
            tag_file_path
                .parent()
                .expect("Failed to access tag file parent"),
            &config.exclude,
        )
    };

    let num_workers: usize = config.workers;
    let mut threads = Vec::with_capacity(num_workers);
    let mut senders = Vec::with_capacity(num_workers);

    for _id in 1..=num_workers {
        let (file_names_sender, file_names_receiver) = mpsc::channel::<String>();
        let mut tags_lock = Arc::clone(&tags_lock);
        let tag_file_path_str = tag_file_path_str.clone();
        threads.push(thread::spawn(move || {
            worker(file_names_receiver, &mut tags_lock, tag_file_path_str);
        }));
        senders.push(file_names_sender);
    }

    for chunk in file_names.chunks(num_workers) {
        for (index, file_name) in chunk.iter().enumerate() {
            let _ = senders[index].send(file_name.to_string());
        }
    }

    for sender in senders {
        drop(sender);
    }

    for thread in threads {
        thread.join().unwrap();
    }

    let tags_lock = Arc::clone(&tags_lock);
    let mut tags = tags_lock.lock().unwrap();

    if config.append {
        tags.extend(parse_tag_file(&PathBuf::from(&tag_file_path_str)));
    }

    let tag_file = File::create(tag_file_path_str).expect("Expected to be able to open tags file");
    let mut tag_file_writer = BufWriter::new(tag_file);

    tags.sort_by(|a, b| a.name.cmp(&b.name));

    for tag in &*tags {
        tag_file_writer
            .write_all(&tag.into_bytes())
            .expect("Failed to write to the tag file");
    }
}

fn find_tag_file(filename: &str) -> Option<String> {
    let mut current_dir = std::env::current_dir().unwrap();

    if let Ok(_file) = File::open(current_dir.join(filename)) {
        return Some(current_dir.join(filename).to_string_lossy().into_owned());
    }

    while let Some(parent) = current_dir.parent() {
        current_dir = parent.to_path_buf();
        if let Ok(_file) = File::open(current_dir.join(filename)) {
            return Some(current_dir.join(filename).to_string_lossy().into_owned());
        }
    }

    None
}

fn get_files_from_dir(dir_path: &Path, exclude_patterns: &Vec<String>) -> Vec<String> {
    let mut file_names = Vec::new();
    let exclude_patterns = exclude_patterns
        .into_iter()
        .map(|pattern| shell_to_regex::shell_to_regex(&pattern));
    let exclude_patterns = RegexSet::new(exclude_patterns).unwrap();
    let walker = WalkDir::new(dir_path).into_iter();

    for entry in walker.filter_entry(|e| !exclude_patterns.is_match(e.path().to_str().unwrap())) {
        if let Ok(entry) = entry {
            if !entry.file_type().is_file() {
                continue;
            }

            file_names.push(entry.path().to_str().unwrap().to_string());
        }
    }

    file_names
}

fn worker(
    file_names_rx: mpsc::Receiver<String>,
    tags_lock: &mut Arc<Mutex<Vec<Tag>>>,
    tag_file_path_str: String,
) {
    let mut parser = Parser::new();
    let tag_file_path = Path::new(&tag_file_path_str);

    for file_name in file_names_rx {
        let file_path = std::env::current_dir().unwrap().join(&file_name);
        let file_path_relative_to_tag_file = file_path
            .strip_prefix(tag_file_path.parent().unwrap())
            .unwrap()
            .to_string_lossy()
            .into_owned();

        match file_path.extension() {
            Some(raw_extension) => match raw_extension.to_str() {
                Some(extension) => parser.parse_file(
                    tags_lock,
                    &file_path_relative_to_tag_file,
                    &file_path.to_string_lossy().into_owned(),
                    extension,
                ),
                None => (),
            },
            None => (),
        }
    }
}
