//! Integration test for the `--append` flow.

use assert_cmd::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn treetags_cmd(working_dir: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("treetags").expect("treetags binary should build");
    cmd.current_dir(working_dir)
        .args(["--plugins-dir", env!("TREETAGS_TEST_EMPTY_PLUGINS_DIR")])
        .args([
            "--user-languages-config",
            env!("TREETAGS_TEST_EMPTY_USER_CONFIG"),
        ]);
    cmd
}

#[test]
fn test_append_merges_existing_tags_with_newly_generated() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    fs::write(path.join("source.py"), "def fresh_function():\n    pass\n").unwrap();

    let tag_file = path.join("tags");
    fs::write(
        &tag_file,
        "existing_symbol\texisting.py\t/^def existing_symbol():$/;\"\tf\n",
    )
    .unwrap();

    treetags_cmd(path)
        .args(["--append=yes", "--sort=yes", "-f", "tags", "source.py"])
        .assert()
        .success();

    let output = fs::read_to_string(&tag_file).unwrap();

    assert!(
        output.contains("fresh_function"),
        "expected newly generated tag in output:\n{output}"
    );
    assert!(
        output.contains("existing_symbol"),
        "expected pre-existing tag to be preserved via append:\n{output}"
    );
    assert!(
        output.contains("existing.py"),
        "expected pre-existing tag's file name to be preserved:\n{output}"
    );
}

#[test]
fn test_without_append_existing_tags_are_replaced() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    fs::write(path.join("source.py"), "def fresh_function():\n    pass\n").unwrap();

    let tag_file = path.join("tags");
    fs::write(
        &tag_file,
        "existing_symbol\texisting.py\t/^def existing_symbol():$/;\"\tf\n",
    )
    .unwrap();

    // Same run but without --append: the existing tags must be dropped.
    treetags_cmd(path)
        .args(["--sort=yes", "-f", "tags", "source.py"])
        .assert()
        .success();

    let output = fs::read_to_string(&tag_file).unwrap();

    assert!(
        output.contains("fresh_function"),
        "expected newly generated tag in output:\n{output}"
    );
    assert!(
        !output.contains("existing_symbol"),
        "expected pre-existing tag to be dropped without --append:\n{output}"
    );
}
