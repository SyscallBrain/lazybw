//! Snapshots of the CLI output (`themes`, `keys`, `config`).

use std::process::Command;

fn run(args: &[&str]) -> String {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let output = Command::new(env!("CARGO_BIN_EXE_lazybw"))
        .args(args)
        .env("XDG_CONFIG_HOME", dir.path().join("config"))
        .env("XDG_STATE_HOME", dir.path().join("state"))
        .env("NO_COLOR", "1")
        .output()
        .expect("run lazybw");
    assert!(
        output.status.success(),
        "lazybw {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).replace(&dir.path().display().to_string(), "<TMP>")
}

#[test]
fn themes_output_snapshot() {
    insta::assert_snapshot!("cli_themes", run(&["themes"]));
}

#[test]
fn keys_list_output_snapshot() {
    insta::assert_snapshot!("cli_keys_list", run(&["keys", "--context", "list", "--defaults"]));
}

#[test]
fn keys_check_output_snapshot() {
    insta::assert_snapshot!("cli_keys_check", run(&["keys", "--defaults", "--check"]));
}

#[test]
fn config_path_output_snapshot() {
    insta::assert_snapshot!("cli_config_path", run(&["config", "--path"]));
}
