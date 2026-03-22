use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(unix)]
use std::{fs::Permissions, os::unix::fs::PermissionsExt};

fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("yore-mcp-test-{}-{}", label, nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_cmd(mut cmd: Command) -> (bool, String) {
    let output = cmd.output().expect("command failed to start");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output.status.success(), stdout)
}

fn long_auth_doc() -> String {
    let mut doc = String::from(
        "# Authentication Overview\n\n\
Authentication flow validates credentials against the identity store and issues a session token.\n\
Every successful login records an audit event and includes the actor, scope, and timestamp.\n\
If validation fails, the service records a denial event and returns an access failure.\n\n",
    );

    for idx in 0..12 {
        doc.push_str(&format!(
            "Authentication step {} keeps the audit trail consistent and explains why session revocation happens after suspicious activity.\n",
            idx + 1
        ));
    }

    doc
}

fn write_docs(root: &Path) {
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();

    let auth = long_auth_doc();
    fs::write(docs.join("aa-auth.md"), &auth).unwrap();
    fs::write(docs.join("zz-auth-copy.md"), &auth).unwrap();
    fs::write(
        docs.join("ops.md"),
        "# Operations\n\nDeployment runbook for maintenance windows.\n",
    )
    .unwrap();
}

fn build_index(root: &Path, index_dir: &Path) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(root)
        .args(["build", "docs", "--output"])
        .arg(index_dir);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "build failed: {}", stdout);
}

fn search_context(root: &Path, index_dir: &Path, args: &[&str]) -> Value {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(root)
        .args(["mcp", "search-context", "authentication", "--index"])
        .arg(index_dir)
        .args(args);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "search-context failed: {}", stdout);
    serde_json::from_str(stdout.trim()).unwrap()
}

#[test]
fn test_mcp_search_context_returns_handles_and_dedupes_duplicates() {
    let root = temp_dir("search-context");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    let value = search_context(
        &root,
        &index_dir,
        &[
            "--max-results",
            "3",
            "--max-tokens",
            "120",
            "--max-bytes",
            "600",
        ],
    );

    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["tool"], "search_context");
    assert_eq!(value["selection_mode"], "query");
    assert!(value["error"].is_null());

    let budget = value["budget"].as_object().unwrap();
    assert_eq!(budget["max_results"], 3);
    assert!(budget["deduped_hits"].as_u64().unwrap() >= 1);
    assert!(budget["estimated_tokens"].as_u64().unwrap() <= 120);
    assert!(budget["bytes"].as_u64().unwrap() <= 600);

    let results = value["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    let first = &results[0];
    assert!(first["handle"].as_str().unwrap().starts_with("ctx_"));
    assert_eq!(first["source"]["path"], "docs/aa-auth.md");
    assert_eq!(first["rank"], 1);
    assert!(first["preview"]
        .as_str()
        .unwrap()
        .contains("Authentication"));
}

#[test]
fn test_mcp_fetch_context_expands_handle_with_truncation_metadata() {
    let root = temp_dir("fetch-context");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    let search = search_context(
        &root,
        &index_dir,
        &[
            "--max-results",
            "2",
            "--max-tokens",
            "160",
            "--max-bytes",
            "700",
        ],
    );
    let handle = search["results"][0]["handle"].as_str().unwrap().to_string();

    let mut fetch = Command::new(env!("CARGO_BIN_EXE_yore"));
    fetch
        .current_dir(&root)
        .args([
            "mcp",
            "fetch-context",
            &handle,
            "--max-tokens",
            "40",
            "--max-bytes",
            "220",
            "--index",
        ])
        .arg(&index_dir);
    let (ok, stdout) = run_cmd(fetch);
    assert!(ok, "fetch-context failed: {}", stdout);

    let value: Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["tool"], "fetch_context");
    assert_eq!(value["handle"], handle);
    assert!(value["error"].is_null());
    assert_eq!(value["result"]["source"]["path"], "docs/aa-auth.md");
    assert!(value["pressure"]["truncated"].as_bool().unwrap());
    assert!(value["budget"]["estimated_tokens"].as_u64().unwrap() <= 40);
    assert!(value["budget"]["bytes"].as_u64().unwrap() <= 220);
    assert!(value["result"]["content"]
        .as_str()
        .unwrap()
        .contains("[truncated]"));
}

#[test]
fn test_mcp_search_context_works_from_different_cwd_after_relative_build() {
    let root = temp_dir("cross-cwd");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    let foreign_cwd = temp_dir("foreign-cwd");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(&foreign_cwd)
        .args(["mcp", "search-context", "authentication", "--index"])
        .arg(&index_dir);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "cross-cwd search-context failed: {}", stdout);

    let value: Value = serde_json::from_str(stdout.trim()).unwrap();
    assert!(value["error"].is_null());
    assert_eq!(value["results"][0]["source"]["path"], "docs/aa-auth.md");
}

#[cfg(unix)]
#[test]
fn test_mcp_search_context_falls_back_when_index_dir_is_read_only() {
    let root = temp_dir("readonly-index");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    fs::set_permissions(&index_dir, Permissions::from_mode(0o555)).unwrap();

    let search = search_context(
        &root,
        &index_dir,
        &[
            "--max-results",
            "1",
            "--max-tokens",
            "120",
            "--max-bytes",
            "500",
        ],
    );

    assert!(search["error"].is_null());
    let handle = search["results"][0]["handle"].as_str().unwrap().to_string();

    let mut fetch = Command::new(env!("CARGO_BIN_EXE_yore"));
    fetch
        .current_dir(&root)
        .args(["mcp", "fetch-context", &handle, "--index"])
        .arg(&index_dir);
    let (ok, stdout) = run_cmd(fetch);
    assert!(
        ok,
        "fetch-context failed after read-only search: {}",
        stdout
    );

    let value: Value = serde_json::from_str(stdout.trim()).unwrap();
    assert!(value["error"].is_null());
    assert_eq!(value["handle"], handle);
}

#[test]
fn test_mcp_search_context_reports_preview_truncation_metadata() {
    let root = temp_dir("preview-truncation");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    let value = search_context(
        &root,
        &index_dir,
        &[
            "--max-results",
            "1",
            "--max-tokens",
            "40",
            "--max-bytes",
            "500",
        ],
    );

    let result = &value["results"][0];
    assert!(result["truncated"].as_bool().unwrap());
    assert!(value["pressure"]["truncated"].as_bool().unwrap());
    assert!(result["preview"].as_str().unwrap().contains("[truncated]"));
    let reasons = result["truncation_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    assert!(reasons.contains(&"token_cap"));
}
