use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("yore-vocab-it-{label}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_cmd(mut cmd: Command) -> (bool, String) {
    let output = cmd.output().expect("command failed to start");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output.status.success(), stdout)
}

fn write_vocabulary_fixture(index_dir: &Path) {
    fs::create_dir_all(index_dir).unwrap();

    let reverse_index = r#"{
        "keywords": {
            "yore": [
                {"file":"docs/a.md","line":1,"heading":"Yore", "level":1},
                {"file":"docs/b.md","line":1,"heading":"Yore", "level":1}
            ],
            "alpha": [
                {"file":"docs/a.md","line":4,"heading":"Alpha Guide", "level":1}
            ],
            "and": [
                {"file":"docs/a.md","line":2,"heading":"And", "level":1}
            ]
        }
    }"#;

    let forward_index = r#"{
        "files": {
            "docs/a.md": {
                "path":"docs/a.md",
                "size_bytes":64,
                "line_count":8,
                "headings":[{"line":1,"level":1,"text":"Yore Guide"},{"line":2,"level":1,"text":"And"},{"line":4,"level":1,"text":"Alpha Guide"}],
                "keywords":["yore","alpha"],
                "body_keywords":["alpha","guide","notes"],
                "links":[],
                "simhash":0,
                "term_frequencies":{"yore":3,"alpha":2,"guide":1},
                "doc_length":6,
                "minhash":[1, 2, 3],
                "section_fingerprints":[]
            },
            "docs/b.md": {
                "path":"docs/b.md",
                "size_bytes":64,
                "line_count":8,
                "headings":[{"line":1,"level":1,"text":"Yore Notes"}],
                "keywords":["yore"],
                "body_keywords":["yore","notes"],
                "links":[],
                "simhash":0,
                "term_frequencies":{"yore":2,"notes":1},
                "doc_length":3,
                "minhash":[4, 5, 6],
                "section_fingerprints":[]
            }
        },
        "indexed_at":"now",
        "version":1
    }"#;

    fs::write(index_dir.join("reverse_index.json"), reverse_index).unwrap();
    fs::write(index_dir.join("forward_index.json"), forward_index).unwrap();
}

fn run_vocabulary(index_dir: &Path, args: &[&str]) -> (bool, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    let mut full_args = vec!["vocabulary", "--index"];
    full_args.extend_from_slice(std::slice::from_ref(
        &index_dir.as_os_str().to_str().unwrap(),
    ));
    for arg in args {
        full_args.push(arg);
    }
    cmd.current_dir(index_dir.parent().unwrap()).args(full_args);
    run_cmd(cmd)
}

#[test]
fn test_vocabulary_help_mentions_command() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.arg("--help");
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "yore --help failed");
    assert!(stdout.contains("vocabulary"));
}

#[test]
fn test_vocabulary_subcommand_help_guidance() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.args(["vocabulary", "--help"]);
    let (ok, stdout) = run_cmd(cmd);
    assert!(ok, "yore vocabulary --help failed");
    assert!(stdout.contains("Derive a deterministic vocabulary list"));
    assert!(stdout.contains("Output formats"));
    assert!(stdout.contains("Usage guidance"));
    assert!(stdout.contains("--format prompt"));
    assert!(stdout.contains("--stopwords"));
    assert!(stdout.contains("--format json"));
    assert!(stdout.contains("--include-stemming"));
    assert!(stdout.contains("--no-default-stopwords"));
    assert!(stdout.contains("--common-terms"));
}

#[test]
fn test_vocabulary_formats_are_consistent_and_limited() {
    let root = temp_dir("formats");
    let index_dir = root.join(".yore-vocab-test");
    write_vocabulary_fixture(&index_dir);

    let (ok_lines, lines) = run_vocabulary(&index_dir, &["--format", "lines", "--limit", "2"]);
    assert!(ok_lines, "vocabulary lines failed");
    let term_lines: Vec<&str> = lines.lines().collect();
    assert_eq!(term_lines, vec!["yore", "alpha"]);

    let (ok_json, stdout_json) = run_vocabulary(&index_dir, &["--format", "json", "--limit", "2"]);
    assert!(ok_json, "vocabulary json failed");
    let json_value: Value = serde_json::from_str(stdout_json.trim()).unwrap();
    assert_eq!(json_value["format"], "json");
    assert_eq!(json_value["limit"], 2);
    assert_eq!(json_value["total"], 2);
    assert_eq!(json_value["terms"][0]["term"], "yore");
    assert_eq!(json_value["terms"][1]["term"], "alpha");
    assert_eq!(json_value["terms"].as_array().unwrap().len(), 2);

    let (ok_prompt, prompt) = run_vocabulary(&index_dir, &["--format", "prompt", "--limit", "2"]);
    assert!(ok_prompt, "vocabulary prompt failed");
    assert_eq!(prompt.trim(), "yore, alpha");
}

#[test]
fn test_vocabulary_custom_stopwords_and_json_alias() {
    let root = temp_dir("stopwords");
    let index_dir = root.join(".yore-vocab-test");
    write_vocabulary_fixture(&index_dir);

    let stopwords = root.join("stopwords.txt");
    fs::write(&stopwords, "yore\n").unwrap();

    let stop_path = stopwords.as_os_str().to_str().unwrap();
    let (ok_lines, lines) = run_vocabulary(
        &index_dir,
        &[
            "--stopwords",
            stop_path,
            "--format",
            "lines",
            "--limit",
            "2",
        ],
    );
    assert!(ok_lines, "vocabulary with stopwords failed");
    assert!(!lines.contains("yore"));
    assert!(lines.contains("alpha"));

    let (ok_alias, alias_stdout) = run_vocabulary(
        &index_dir,
        &[
            "--stopwords",
            stop_path,
            "--json",
            "--format",
            "lines",
            "--limit",
            "1",
        ],
    );
    assert!(ok_alias, "vocabulary --json alias failed");
    let alias_json: Value = serde_json::from_str(alias_stdout.trim()).unwrap();
    assert_eq!(alias_json["format"], "json");
    assert_eq!(alias_json["terms"][0]["term"], "alpha");
    assert_eq!(alias_json["terms"].as_array().unwrap().len(), 1);
}

#[test]
fn test_vocabulary_common_terms_auto_filters_common_tokens() {
    let root = temp_dir("common-terms");
    let index_dir = root.join(".yore-vocab-test");
    write_vocabulary_fixture(&index_dir);

    let stopwords = root.join("stopwords.txt");
    fs::write(&stopwords, "and\n").unwrap();

    let (ok_json, stdout_json) = run_vocabulary(
        &index_dir,
        &[
            "--common-terms",
            "1",
            "--no-default-stopwords",
            "--stopwords",
            stopwords.as_os_str().to_str().unwrap(),
            "--format",
            "json",
            "--limit",
            "10",
        ],
    );
    assert!(ok_json, "vocabulary with common-terms failed");

    let json_value: Value = serde_json::from_str(stdout_json.trim()).unwrap();
    assert_eq!(json_value["auto_common_terms"], 1);
    let terms = json_value["terms"]
        .as_array()
        .expect("terms should be an array")
        .iter()
        .map(|t| t["term"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(!terms.contains(&"yore"));
    assert!(terms.contains(&"alpha"));
}
