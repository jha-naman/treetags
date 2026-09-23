//! Release-pinned grammar distribution. Only explicit installation uses the network.
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};

use crate::config::{Config, GrammarCommands};
use crate::wasm_grammars::{WasmGrammar, WasmGrammars, GRAMMARS};

const MAX_DOWNLOAD: u64 = 32 * 1024 * 1024;

#[derive(Debug, PartialEq)]
enum Status {
    Missing,
    Pinned,
    Different,
}

fn checksum(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn status(grammar: &WasmGrammar, root: &Path) -> Result<Status> {
    let path = grammar.path(root);
    match fs::read(&path) {
        Ok(bytes) if checksum(&bytes) == grammar.sha256 => Ok(Status::Pinned),
        Ok(_) => Ok(Status::Different),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Status::Missing),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

fn resolve(names: &[String]) -> Result<Vec<&'static WasmGrammar>> {
    names
        .iter()
        .map(|name| name.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>()
        .iter()
        .map(|name| {
            GRAMMARS.iter().copied().find(|g| g.name == name).ok_or_else(|| {
                anyhow!("unknown grammar '{name}'; run `treetags grammar available` for supported names")
            })
        })
        .collect()
}

pub(crate) fn handle(action: &GrammarCommands, config: &Config) -> Result<()> {
    let root = &config.wasm_grammars_dir;
    match action {
        GrammarCommands::Available => list(root, false),
        GrammarCommands::Installed => list(root, true),
        GrammarCommands::Install {
            names,
            configured,
            force,
        } => {
            let names = if *configured {
                &config.wasm_grammar_languages
            } else {
                names
            };
            let grammars = resolve(names)?;
            if grammars.is_empty() {
                println!("No WASM grammars configured in [wasm_grammars].languages.");
                return Ok(());
            }
            install_all(&grammars, root, *force, &mut download)
        }
        GrammarCommands::Uninstall { names } => {
            let grammars = resolve(names)?;
            batch(&grammars, |grammar| {
                let path = grammar.path(root);
                match fs::remove_file(&path) {
                    Ok(()) => println!("Uninstalled '{}' from {}", grammar.name, path.display()),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                        println!("'{}' is not installed.", grammar.name);
                    }
                    Err(err) => {
                        return Err(err).with_context(|| format!("removing {}", path.display()))
                    }
                }
                Ok(())
            })
        }
    }
}

fn list(root: &Path, installed_only: bool) -> Result<()> {
    println!("NAME\tPINNED VERSION\tABI\tSTATUS\tPATH");
    batch(GRAMMARS, |grammar| {
        let status = status(grammar, root)?;
        if installed_only && status == Status::Missing {
            return Ok(());
        }
        let label = match status {
            Status::Missing => "missing",
            Status::Pinned => "matches pinned version",
            Status::Different => "different from pinned version",
        };
        println!(
            "{}\t{}\t{}\t{}\t{}",
            grammar.name,
            grammar.version,
            grammar.abi,
            label,
            grammar.path(root).display()
        );
        Ok(())
    })
}

fn batch(
    grammars: &[&WasmGrammar],
    mut operation: impl FnMut(&WasmGrammar) -> Result<()>,
) -> Result<()> {
    let mut failures = 0;
    for grammar in grammars {
        if let Err(err) = operation(grammar) {
            eprintln!("grammar '{}': {err:#}", grammar.name);
            failures += 1;
        }
    }
    if failures != 0 {
        bail!("{failures} grammar operation(s) failed");
    }
    Ok(())
}

fn install_all(
    grammars: &[&WasmGrammar],
    root: &Path,
    force: bool,
    fetch: &mut impl FnMut(&str) -> Result<Vec<u8>>,
) -> Result<()> {
    batch(grammars, |grammar| install(grammar, root, force, fetch))
}

fn install(
    grammar: &WasmGrammar,
    root: &Path,
    force: bool,
    fetch: &mut impl FnMut(&str) -> Result<Vec<u8>>,
) -> Result<()> {
    if !force {
        match status(grammar, root)? {
            Status::Pinned => {
                println!(
                    "'{}' {} is already installed.",
                    grammar.name, grammar.version
                );
                return Ok(());
            }
            Status::Different => {
                bail!("existing file differs from the pinned version; use --force to replace it")
            }
            Status::Missing => {}
        }
    }
    println!(
        "Downloading '{}' {} from {}...",
        grammar.name, grammar.version, grammar.url
    );
    let bytes = fetch(grammar.url).with_context(|| format!("downloading {}", grammar.url))?;
    if checksum(&bytes) != grammar.sha256 {
        bail!("checksum mismatch; expected {}", grammar.sha256);
    }
    WasmGrammars::validate(grammar, &bytes).map_err(|err| anyhow!(err))?;
    publish(grammar, root, &bytes, force)?;
    println!(
        "Installed '{}' {} to {}",
        grammar.name,
        grammar.version,
        grammar.path(root).display()
    );
    Ok(())
}

fn publish(grammar: &WasmGrammar, root: &Path, bytes: &[u8], force: bool) -> Result<()> {
    let destination = grammar.path(root);
    let directory = destination.parent().unwrap();
    fs::create_dir_all(directory).with_context(|| format!("creating {}", directory.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if force {
        temporary
            .persist(&destination)
            .map_err(|err| anyhow!("installing {}: {err}", destination.display()))?;
    } else if let Err(err) = temporary.persist_noclobber(&destination) {
        // Another installer may have published the same pin while we downloaded.
        if err.error.kind() != std::io::ErrorKind::AlreadyExists
            || status(grammar, root)? != Status::Pinned
        {
            bail!(
                "installing {}: {err}; existing files require --force",
                destination.display()
            );
        }
    }
    Ok(())
}

fn download(url: &str) -> Result<Vec<u8>> {
    let agent = ureq::AgentBuilder::new()
        .https_only(true)
        .redirects(5)
        .timeout(Duration::from_secs(60))
        .build();
    download_with(&agent, url, MAX_DOWNLOAD)
}

fn download_with(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>> {
    let response = agent.get(url).call()?;
    if response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .is_some_and(|size| size > limit)
    {
        bail!("grammar exceeds download size limit ({limit} bytes)");
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("grammar exceeds download size limit ({limit} bytes)");
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_grammars::{OCAML, ZIG};
    use std::net::TcpListener;
    use std::thread;

    fn fixture(grammar: &WasmGrammar) -> Vec<u8> {
        fs::read(grammar.path(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/grammars/wasm")))
            .unwrap()
    }

    #[test]
    fn catalog_matches_fixture_provenance() {
        let provenance: serde_json::Value =
            serde_json::from_str(include_str!("../tests/grammars/wasm/provenance.json")).unwrap();
        assert_eq!(provenance.as_array().unwrap().len(), GRAMMARS.len());
        for grammar in GRAMMARS {
            let entry = provenance
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["language"] == grammar.name)
                .unwrap();
            assert_eq!(entry["grammar_version"], grammar.version);
            assert_eq!(entry["abi"], grammar.abi);
            assert_eq!(entry["url"], grammar.url);
            assert_eq!(entry["sha256"], grammar.sha256);
            assert_eq!(checksum(&fixture(grammar)), grammar.sha256);
        }
    }

    #[test]
    fn names_are_normalized_and_all_validated() {
        let names = [" ZIG ", "ocaml", "zig"].map(String::from);
        assert_eq!(
            resolve(&names)
                .unwrap()
                .iter()
                .map(|g| g.name)
                .collect::<Vec<_>>(),
            ["ocaml", "zig"]
        );
        assert!(resolve(&["zig".into(), "../other".into()]).is_err());
    }

    #[test]
    fn installation_is_idempotent_and_force_repairs_manual_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut count = 0;
        let mut fetch = |_: &str| {
            count += 1;
            Ok(fixture(&ZIG))
        };
        install(&ZIG, dir.path(), false, &mut fetch).unwrap();
        install(&ZIG, dir.path(), false, &mut fetch).unwrap();
        fs::write(ZIG.path(dir.path()), b"manual grammar").unwrap();
        assert!(install(&ZIG, dir.path(), false, &mut fetch).is_err());
        assert_eq!(fs::read(ZIG.path(dir.path())).unwrap(), b"manual grammar");
        install(&ZIG, dir.path(), true, &mut fetch).unwrap();
        assert_eq!(count, 2);
        assert_eq!(status(&ZIG, dir.path()).unwrap(), Status::Pinned);
        assert_eq!(fs::read_dir(dir.path().join("14")).unwrap().count(), 1);
    }

    #[test]
    fn failures_preserve_existing_files_and_batch_successes() {
        let dir = tempfile::tempdir().unwrap();
        publish(&ZIG, dir.path(), b"original", false).unwrap();
        for mut fetch in [
            (|_: &str| bail!("network failure")) as fn(&str) -> Result<Vec<u8>>,
            |_| Ok(b"bad checksum".to_vec()),
        ] {
            assert!(install(&ZIG, dir.path(), true, &mut fetch).is_err());
            assert_eq!(fs::read(ZIG.path(dir.path())).unwrap(), b"original");
        }
        let mut fetch = |url: &str| {
            if url == OCAML.url {
                bail!("network failure")
            }
            Ok(fixture(&ZIG))
        };
        assert!(install_all(&[&OCAML, &ZIG], dir.path(), true, &mut fetch).is_err());
        assert_eq!(status(&ZIG, dir.path()).unwrap(), Status::Pinned);
        assert_eq!(status(&OCAML, dir.path()).unwrap(), Status::Missing);
        assert_eq!(fs::read_dir(dir.path().join("14")).unwrap().count(), 1);
    }

    #[test]
    fn validation_failure_never_replaces_a_file() {
        let dir = tempfile::tempdir().unwrap();
        publish(&ZIG, dir.path(), b"original", false).unwrap();
        let incompatible = WasmGrammar {
            export_name: Some("nonexistent"),
            ..ZIG
        };
        assert!(install(&incompatible, dir.path(), true, &mut |_| Ok(fixture(&ZIG))).is_err());
        assert_eq!(fs::read(ZIG.path(dir.path())).unwrap(), b"original");
        let invalid = WasmGrammar {
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ..ZIG
        };
        assert!(install(&invalid, dir.path(), true, &mut |_| Ok(Vec::new())).is_err());
        assert_eq!(fs::read(ZIG.path(dir.path())).unwrap(), b"original");
    }

    #[test]
    fn concurrent_publication_is_atomic_and_does_not_clobber_manual_files() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = fixture(&ZIG);
        thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| publish(&ZIG, dir.path(), &bytes, false).unwrap());
            }
        });
        assert_eq!(fs::read(ZIG.path(dir.path())).unwrap(), bytes);
        fs::write(ZIG.path(dir.path()), b"manual").unwrap();
        assert!(publish(&ZIG, dir.path(), &bytes, false).is_err());
        assert_eq!(fs::read(ZIG.path(dir.path())).unwrap(), b"manual");
        assert_eq!(fs::read_dir(dir.path().join("14")).unwrap().count(), 1);
        fs::remove_file(ZIG.path(dir.path())).unwrap();
        fs::create_dir(ZIG.path(dir.path())).unwrap();
        assert!(publish(&ZIG, dir.path(), &bytes, true).is_err());
        assert_eq!(fs::read_dir(dir.path().join("14")).unwrap().count(), 1);
    }

    fn server(responses: Vec<Vec<u8>>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let _ = stream.read(&mut request);
                let _ = stream.write_all(&response);
            }
        });
        (url, handle)
    }

    #[test]
    fn http_success_redirects_and_response_failures() {
        let agent = ureq::AgentBuilder::new()
            .redirects(2)
            .timeout(Duration::from_secs(2))
            .build();
        let body = fixture(&ZIG);
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(&body);
        let (url, server) = server(vec![
            b"HTTP/1.1 302 Found\r\nLocation: /grammar\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            response,
        ]);
        assert_eq!(download_with(&agent, &url, MAX_DOWNLOAD).unwrap(), body);
        server.join().unwrap();
        for response in [
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n",
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n123456789",
            "HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nshort",
        ] {
            let (url, handle) = self::server(vec![response.as_bytes().to_vec()]);
            assert!(download_with(&agent, &url, 8).is_err(), "{response}");
            handle.join().unwrap();
        }
        assert!(download("http://127.0.0.1:1").is_err());
    }

    #[test]
    fn http_timeout_and_redirect_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(150));
        });
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_millis(50))
            .build();
        assert!(download_with(&agent, &url, MAX_DOWNLOAD).is_err());
        handle.join().unwrap();
        let redirect = b"HTTP/1.1 302 Found\r\nLocation: /again\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec();
        let (url, handle) = server(vec![redirect; 2]);
        let agent = ureq::AgentBuilder::new().redirects(2).build();
        assert!(download_with(&agent, &url, MAX_DOWNLOAD).is_err());
        handle.join().unwrap();
    }
}
