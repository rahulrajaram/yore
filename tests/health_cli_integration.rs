use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("yore-health-test-{label}-{nanos}"));
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

    let noisy = r"# Build Plan

## Part 1
Step one

## Part 2
Step two

## Part 3
Step three

## Changelog
- Added one
- Added two

## Completed Work
line one
line two
line three
";
    fs::write(docs.join("noisy.md"), noisy).unwrap();
    fs::write(
        docs.join("healthy.md"),
        "# Healthy\n\n## Notes\nShort doc.\n",
    )
    .unwrap();
}

fn build_index(root: &Path, index_dir: &Path) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(root)
        .args(["build", "docs", "--output"])
        .arg(index_dir);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "build failed: {stdout}");
}

#[test]
fn test_health_all_json_reports_detected_issues() {
    let root = temp_dir("health-all-json");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(&root)
        .args([
            "health",
            "--all",
            "--json",
            "--max-lines",
            "10",
            "--max-part-sections",
            "3",
            "--max-completed-lines",
            "2",
            "--max-changelog-entries",
            "1",
            "--index",
        ])
        .arg(&index_dir);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "health failed: {stdout}");

    let value: Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["total_files"], 2);
    assert_eq!(value["unhealthy_files"], 1);
    assert_eq!(value["warning_files"], 0);

    let files = value["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert!(files[0]["file"]
        .as_str()
        .unwrap()
        .ends_with("docs/noisy.md"));
    assert_eq!(files[0]["status"], "unhealthy");

    let issues = files[0]["issues"].as_array().unwrap();
    let kinds: Vec<&str> = issues
        .iter()
        .map(|issue| issue["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"bloated-file"));
    assert!(kinds.contains(&"accumulator-pattern"));
    assert!(kinds.contains(&"stale-completed"));
    assert!(kinds.contains(&"changelog-bloat"));
}

#[test]
fn test_health_single_file_human_output_reports_healthy() {
    let root = temp_dir("health-single-human");
    write_docs(&root);
    let index_dir = root.join(".yore-test");
    build_index(&root, &index_dir);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(&root)
        .args(["health", "docs/healthy.md", "--index"])
        .arg(&index_dir);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "health failed: {stdout}");
    assert!(stdout.contains("docs/healthy.md"));
    assert!(stdout.contains("HEALTHY"));
}
