use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("yore-test-{}-{}", label, nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_cmd(mut cmd: Command) -> (bool, String) {
    let output = cmd.output().expect("command failed to start");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output.status.success(), stdout)
}

fn write_docs(root: &Path) {
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(
        docs.join("a.md"),
        "# Async Migration Plan\n\nKubernetes deployment steps.\n",
    )
    .unwrap();
    fs::write(docs.join("b.md"), "# Notes\n\nDeployment guide.\n").unwrap();
}

fn build_index(root: &Path, index_dir: &Path) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(root)
        .args(["build", "docs", "--output"])
        .arg(index_dir);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "build failed: {}", stdout);
}

#[test]
fn test_query_multi_term_returns_results() {
    let root = temp_dir("query-multi-term");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    let mut single = Command::new(env!("CARGO_BIN_EXE_yore"));
    single
        .current_dir(&root)
        .args(["query", "kubernetes", "--json", "--index"])
        .arg(&index_dir);
    let (ok_single, stdout_single) = run_cmd(single);
    assert!(ok_single, "single-term query failed");
    let single_json: Value = serde_json::from_str(&stdout_single).unwrap();
    assert!(!single_json.as_array().unwrap().is_empty());

    let mut multi = Command::new(env!("CARGO_BIN_EXE_yore"));
    multi
        .current_dir(&root)
        .args(["query", "kubernetes", "deployment", "--json", "--index"])
        .arg(&index_dir);
    let (ok_multi, stdout_multi) = run_cmd(multi);
    assert!(ok_multi, "multi-term query failed");
    let multi_json: Value = serde_json::from_str(&stdout_multi).unwrap();
    assert!(!multi_json.as_array().unwrap().is_empty());
}

#[test]
fn test_query_empty_results_explain_diagnostics() {
    let root = temp_dir("query-empty");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(&root)
        .args(["query", "--query", "nonesuchterm", "--explain", "--index"])
        .arg(&index_dir);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "query failed");
    assert!(stdout.contains("No results found."));
    assert!(stdout.contains("Diagnostics:"));
    assert!(stdout.contains("No data to explain."));
}

#[test]
fn test_query_empty_results_json_explain() {
    let root = temp_dir("query-empty-json");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(&root)
        .args([
            "query",
            "--query",
            "nonesuchterm",
            "--json",
            "--explain",
            "--index",
        ])
        .arg(&index_dir);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "query failed");
    let value: Value = serde_json::from_str(&stdout).unwrap();
    let results = value.get("results").and_then(|v| v.as_array()).unwrap();
    assert!(results.is_empty());
    let diagnostics = value
        .get("diagnostics")
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(diagnostics.contains_key("tokens"));
    assert!(diagnostics.contains_key("notice"));
}
