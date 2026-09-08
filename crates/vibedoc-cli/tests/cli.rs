use std::fs;
use std::process::{Command, Output};

fn run(directory: &std::path::Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vibedoc"))
        .current_dir(directory)
        .args(arguments)
        .output()
        .expect("run vibedoc")
}

#[test]
fn init_detects_project_and_refuses_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("README.md"), "# Project\n").unwrap();
    fs::write(directory.path().join("tsconfig.json"), "{}\n").unwrap();

    let first = run(directory.path(), &["init"]);
    assert_eq!(first.status.code(), Some(0));
    let config = fs::read_to_string(directory.path().join("vibedoc.toml")).unwrap();
    assert!(config.contains("projects = [\"tsconfig.json\"]"));
    assert!(config.contains("include = [\"README.md\"]"));

    let second = run(directory.path(), &["init"]);
    assert_eq!(second.status.code(), Some(2));
    assert!(
        String::from_utf8(second.stderr)
            .unwrap()
            .contains("already exists")
    );

    let docs_only = tempfile::tempdir().unwrap();
    fs::write(docs_only.path().join("README.md"), "# Project\n").unwrap();
    let initialized = run(docs_only.path(), &["init"]);
    assert_eq!(initialized.status.code(), Some(0));
    let config = fs::read_to_string(docs_only.path().join("vibedoc.toml")).unwrap();
    assert!(!config.contains("\n[adapters.typescript]"));
}

#[test]
fn check_output_is_deterministic_and_deny_warnings_changes_exit_status() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("README.md"),
        "# Project\n\nThe service handles things.\n",
    )
    .unwrap();

    let output = run(directory.path(), &["check"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        include_str!("snapshots/check-warnings.txt")
    );

    let denied = run(directory.path(), &["check", "--deny-warnings"]);
    assert_eq!(denied.status.code(), Some(1));
}

#[test]
fn json_commands_use_versioned_envelopes() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("README.md"), "# Project\n").unwrap();

    let check = run(directory.path(), &["check", "--format", "json"]);
    assert_eq!(check.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(check.stdout.clone()).unwrap(),
        include_str!("snapshots/check-clean.json")
    );
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["tool"]["name"], "vibedoc");
    assert_eq!(report["status"], "pass");
    assert!(report["verification"].is_object());
    assert!(report["summary"].is_object());

    let explain = run(
        directory.path(),
        &["explain", "VDOC-G006", "--format", "json"],
    );
    assert_eq!(explain.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&explain.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["tool"]["name"], "vibedoc");
    assert_eq!(report["rule"]["id"], "VDOC-G006");
}

#[test]
fn operational_json_error_uses_exit_code_two() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["check", "--format", "json"]);
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["status"], "error");
    assert!(
        report["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no Markdown")
    );
}
