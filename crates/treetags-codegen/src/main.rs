use clap::Parser;
use std::{fs, path::PathBuf, process::ExitCode};
use treetags_codegen::{generate, generate_shared, GenerationOptions, NamedSource};

#[derive(Parser)]
#[command(about = "Compile treetags language data to deterministic Rust tables")]
struct Args {
    #[arg(long)]
    grammar: PathBuf,
    #[arg(long = "node-types")]
    node_types: PathBuf,
    #[arg(long)]
    query: PathBuf,
    #[arg(long)]
    kinds: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    module_name: String,
    /// Also emit the shared language-neutral structural-parser module here.
    #[arg(long)]
    shared_output: Option<PathBuf>,
    /// C-family parsing facts (parse.json); required with --shared-output.
    #[arg(long)]
    parse: Option<PathBuf>,
    /// Verify that output is current without writing it.
    #[arg(long)]
    check: bool,
}

fn read(path: &PathBuf) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    let grammar = read(&args.grammar)?;
    let nodes = read(&args.node_types)?;
    let query = read(&args.query)?;
    let kinds = read(&args.kinds)?;
    let parse_path = args.parse.as_ref().ok_or("--parse is required")?;
    let parse = read(parse_path)?;
    let output = generate(
        NamedSource::new(&args.grammar.display().to_string(), &grammar),
        NamedSource::new(&args.node_types.display().to_string(), &nodes),
        NamedSource::new(&args.query.display().to_string(), &query),
        NamedSource::new(&args.kinds.display().to_string(), &kinds),
        NamedSource::new(&parse_path.display().to_string(), &parse),
        &GenerationOptions {
            module_name: &args.module_name,
        },
    )
    .map_err(|errors| {
        errors
            .into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let mut outputs = vec![(args.output.clone(), output.rust_source)];
    if let Some(shared_path) = &args.shared_output {
        let shared = generate_shared(
            NamedSource::new(&args.grammar.display().to_string(), &grammar),
            NamedSource::new(&args.node_types.display().to_string(), &nodes),
            NamedSource::new(&args.kinds.display().to_string(), &kinds),
            NamedSource::new(&parse_path.display().to_string(), &parse),
        )
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        outputs.push((shared_path.clone(), shared.rust_source));
    }
    for (path, source) in outputs {
        if args.check {
            let current = read(&path)?;
            if current != source {
                return Err(format!("{} is stale; regenerate it", path.display()));
            }
        } else {
            fs::write(&path, source).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}
