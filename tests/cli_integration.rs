use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── Shared helpers ──────────────────────────────────────────────────

fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("yore-cli-{label}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn yore(args: &[&str], index: &Path) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.args(args).arg("--index").arg(index);
    let output = cmd.output().expect("yore failed to start");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn yore_at(root: &Path, args: &[&str], index: &Path) -> (bool, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(root).args(args).arg("--index").arg(index);
    let output = cmd.output().expect("yore failed to start");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn build_index(root: &Path, docs_dir: &str, index: &Path) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(root)
        .args(["build", docs_dir, "--output"])
        .arg(index);
    let output = cmd.output().expect("yore build failed to start");
    assert!(
        output.status.success(),
        "yore build failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// Create a realistic documentation fixture with cross-links, duplicates,
/// orphans, and varied structure for comprehensive CLI testing.
fn write_fixture(root: &Path) {
    let docs = root.join("docs");
    let guides = docs.join("guides");
    let adr = docs.join("adr");
    fs::create_dir_all(&guides).unwrap();
    fs::create_dir_all(&adr).unwrap();

    fs::write(
        docs.join("README.md"),
        "\
# Project Documentation

Welcome to the project. See [Architecture](architecture.md) for the
system overview and [Getting Started](guides/getting-started.md) to
begin development.

## Quick Links

- [API Reference](api-reference.md)
- [Deployment Guide](guides/deployment.md)
",
    )
    .unwrap();

    fs::write(
        docs.join("architecture.md"),
        "\
# Architecture

## Overview

The system uses a layered approach per ADR-001.

## Components

### API Layer

See [API Reference](api-reference.md#endpoints) for endpoint details.

### Data Layer

PostgreSQL as primary store, per ADR-002.
",
    )
    .unwrap();

    fs::write(
        docs.join("api-reference.md"),
        "\
# API Reference

## Endpoints

All endpoints follow REST conventions.
See [Architecture](architecture.md#components) for context.

### GET /users

Returns a list of users.

### POST /users

Creates a new user.
",
    )
    .unwrap();

    fs::write(
        guides.join("getting-started.md"),
        "\
# Getting Started

## Prerequisites

Install Rust and PostgreSQL.

## Setup

Clone the repo and run `cargo build`.
See [Architecture](../architecture.md) for system context.
",
    )
    .unwrap();

    // Deployment: links to architecture, has broken link to runbook.md
    fs::write(
        guides.join("deployment.md"),
        "\
# Deployment Guide

## Docker

Build with `docker build .` and push to registry.

## Kubernetes

Apply manifests from `k8s/` directory.
See [Architecture](../architecture.md) for infrastructure overview.
See [Runbook](../runbook.md) for troubleshooting.
",
    )
    .unwrap();

    // Orphan: no inbound links from anywhere
    fs::write(
        docs.join("orphan-notes.md"),
        "\
# Orphan Notes

This document is not linked from anywhere.

## Scratch

Random development notes.
",
    )
    .unwrap();

    // Near-duplicate of architecture (nearly identical content to ensure
    // high MinHash similarity regardless of platform hash seeds)
    fs::write(
        docs.join("architecture-v2.md"),
        "\
# Architecture

## Overview

The system uses a layered approach per ADR-001.

## Components

### API Layer

See [API Reference](api-reference.md#endpoints) for endpoint details.

### Data Layer

PostgreSQL as primary store, per ADR-002.

### Cache Layer

Redis for session caching added in v2.
",
    )
    .unwrap();

    fs::write(
        adr.join("ADR-001.md"),
        "\
# ADR-001: Layered Architecture

## Status

Accepted

## Decision

Use a layered architecture with API, business logic, and data layers.
",
    )
    .unwrap();

    fs::write(
        adr.join("ADR-002.md"),
        "\
# ADR-002: PostgreSQL as Primary Store

## Status

Accepted

## Decision

Use PostgreSQL for all persistent data.
",
    )
    .unwrap();
}

// ── stats ───────────────────────────────────────────────────────────

#[test]
fn test_stats_human_output() {
    let root = temp_dir("stats-human");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(&["stats"], &index);
    assert!(ok, "stats failed: {stdout}");
    // Human output uses "Total files:" not "Files indexed:"
    assert!(stdout.contains("Total files"), "expected file count");
    assert!(stdout.contains("Unique keywords"), "expected keyword count");
}

#[test]
fn test_stats_json_output() {
    let root = temp_dir("stats-json");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(&["stats", "--json"], &index);
    assert!(ok, "stats --json failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    // JSON key is "total_files" not "files_indexed"
    assert!(v["total_files"].as_u64().unwrap() >= 9);
    assert!(v["unique_keywords"].as_u64().unwrap() > 0);
    assert!(v["top_keywords"].is_array());
}

// ── check-links ─────────────────────────────────────────────────────

#[test]
fn test_check_links_finds_broken_link() {
    let root = temp_dir("check-links");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (_, stdout, _) = yore_at(&root, &["check-links", "--json"], &index);
    let v: Value = serde_json::from_str(&stdout).unwrap();

    // JSON shape: { broken_links: N, broken: [...] }
    assert!(
        v["broken_links"].as_u64().unwrap() >= 1,
        "expected at least one broken link (runbook.md)"
    );
    let broken = v["broken"].as_array().unwrap();
    let has_runbook = broken
        .iter()
        .any(|b| b["link_target"].as_str().unwrap_or("").contains("runbook"));
    assert!(has_runbook, "expected broken link to runbook.md");
}

#[test]
fn test_check_links_summary_only() {
    let root = temp_dir("check-links-summary");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (_, stdout, _) = yore_at(&root, &["check-links", "--summary-only"], &index);
    assert!(
        stdout.contains("broken") || stdout.contains("Broken") || stdout.contains("Summary"),
        "expected summary output, got: {stdout}"
    );
}

// ── backlinks ───────────────────────────────────────────────────────

#[test]
fn test_backlinks_finds_inbound_links() {
    let root = temp_dir("backlinks");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    // Must use the full indexed path: docs/architecture.md
    let (ok, stdout, _) = yore(&["backlinks", "docs/architecture.md", "--json"], &index);
    assert!(ok, "backlinks failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    let backlinks = v["backlinks"].as_array().unwrap();
    assert!(
        backlinks.len() >= 3,
        "expected >=3 backlinks to architecture.md, got {}",
        backlinks.len()
    );
}

// ── orphans ─────────────────────────────────────────────────────────

#[test]
fn test_orphans_finds_unlinked_docs() {
    let root = temp_dir("orphans");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(&["orphans", "--json"], &index);
    assert!(ok, "orphans failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    let orphans = v["orphans"].as_array().unwrap();

    let orphan_files: Vec<&str> = orphans
        .iter()
        .map(|o| o["file"].as_str().unwrap())
        .collect();
    assert!(
        orphan_files.iter().any(|f| f.contains("orphan-notes")),
        "expected orphan-notes.md in orphans, got: {orphan_files:?}"
    );
}

// ── canonicality ────────────────────────────────────────────────────

#[test]
fn test_canonicality_scores_all_files() {
    let root = temp_dir("canonicality");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(&["canonicality", "--json"], &index);
    assert!(ok, "canonicality failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    let files = v["files"].as_array().unwrap();
    assert!(
        files.len() >= 9,
        "expected all files scored, got {}",
        files.len()
    );

    let readme = files
        .iter()
        .find(|f| f["file"].as_str().unwrap_or("").contains("README"));
    assert!(readme.is_some(), "README should appear in canonicality");
    let score = readme.unwrap()["score"].as_f64().unwrap();
    assert!(
        score > 0.5,
        "README should have high canonicality, got {score}"
    );
}

// ── canonical-orphans ───────────────────────────────────────────────

#[test]
fn test_canonical_orphans_json() {
    let root = temp_dir("canonical-orphans");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(
        &["canonical-orphans", "--threshold", "0.3", "--json"],
        &index,
    );
    assert!(ok, "canonical-orphans failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert!(v["orphans"].is_array());
}

// ── similar ─────────────────────────────────────────────────────────

#[test]
fn test_similar_finds_related_docs() {
    let root = temp_dir("similar");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    // similar returns a bare JSON array, not { results: [...] }
    let (ok, stdout, _) = yore(
        &[
            "similar",
            "docs/architecture.md",
            "--json",
            "--threshold",
            "0.1",
        ],
        &index,
    );
    assert!(ok, "similar failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    let results = v.as_array().unwrap();
    assert!(!results.is_empty(), "expected at least one similar doc");

    // architecture-v2 should be the top match
    let top_path = results[0]["path"].as_str().unwrap_or("");
    assert!(
        top_path.contains("architecture-v2"),
        "expected architecture-v2 as top similar, got {top_path}"
    );
}

// ── dupes ───────────────────────────────────────────────────────────

#[test]
fn test_dupes_detects_near_duplicates() {
    let root = temp_dir("dupes");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    // dupes returns a bare JSON array of pair objects
    let (ok, stdout, _) = yore(&["dupes", "--json", "--threshold", "0.1"], &index);
    assert!(ok, "dupes failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    let pairs = v.as_array().unwrap();

    let has_arch_pair = pairs.iter().any(|p| {
        let f1 = p["file1"].as_str().unwrap_or("");
        let f2 = p["file2"].as_str().unwrap_or("");
        (f1.contains("architecture.md") && f2.contains("architecture-v2"))
            || (f1.contains("architecture-v2") && f2.contains("architecture.md"))
    });
    assert!(
        has_arch_pair,
        "expected architecture.md <-> architecture-v2.md pair"
    );
}

#[test]
fn test_dupes_group_mode() {
    let root = temp_dir("dupes-group");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(&["dupes", "--group", "--threshold", "0.1"], &index);
    assert!(ok, "dupes --group failed: {stdout}");
    assert!(
        stdout.contains("architecture"),
        "expected grouped output mentioning architecture"
    );
}

// ── diff ────────────────────────────────────────────────────────────

#[test]
fn test_diff_between_similar_files() {
    let root = temp_dir("diff");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(
        &[
            "diff",
            "docs/architecture.md",
            "docs/architecture-v2.md",
            "--json",
        ],
        &index,
    );
    assert!(ok, "diff failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    // similarity is an object: { combined, jaccard, simhash }
    let sim = v["similarity"]["combined"].as_f64().unwrap();
    assert!(sim > 0.5, "expected high similarity, got {sim}");
    assert!(v["shared_keywords"].is_array());
}

// ── export-graph ────────────────────────────────────────────────────

#[test]
fn test_export_graph_json() {
    let root = temp_dir("export-graph-json");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(&["export-graph", "--format", "json"], &index);
    assert!(ok, "export-graph json failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert!(v["nodes"].is_array());
    assert!(v["edges"].is_array());
    let nodes = v["nodes"].as_array().unwrap();
    assert!(nodes.len() >= 9, "expected all docs as nodes");
}

#[test]
fn test_export_graph_dot() {
    let root = temp_dir("export-graph-dot");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(&["export-graph", "--format", "dot"], &index);
    assert!(ok, "export-graph dot failed: {stdout}");
    assert!(stdout.contains("digraph"), "expected DOT digraph output");
    assert!(stdout.contains("->"), "expected edges in DOT output");
}

// ── assemble (standalone) ───────────────────────────────────────────

#[test]
fn test_assemble_produces_context_digest() {
    let root = temp_dir("assemble");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(&["assemble", "architecture layered"], &index);
    assert!(ok, "assemble failed: {stdout}");
    assert!(
        stdout.contains("Context Digest"),
        "expected Context Digest header"
    );
}

// ── check (combined) ────────────────────────────────────────────────

#[test]
fn test_check_links_flag() {
    let root = temp_dir("check-links-flag");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (_, stdout, _) = yore_at(&root, &["check", "--links", "--ci"], &index);
    assert!(
        stdout.contains("broken") || stdout.contains("ok") || stdout.contains("Link"),
        "expected link check output, got: {stdout}"
    );
}

#[test]
fn test_check_dupes_flag() {
    let root = temp_dir("check-dupes-flag");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (_, stdout, _) = yore_at(&root, &["check", "--dupes", "--ci"], &index);
    assert!(
        !stdout.is_empty(),
        "expected some output from check --dupes"
    );
}

// ── suggest-consolidation ───────────────────────────────────────────

#[test]
fn test_suggest_consolidation_json() {
    let root = temp_dir("suggest-consolidation");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(
        &["suggest-consolidation", "--json", "--threshold", "0.3"],
        &index,
    );
    assert!(ok, "suggest-consolidation failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    // JSON shape: { total_groups: N, groups: [...] }
    assert!(v["groups"].is_array(), "expected groups array in JSON");
    assert!(
        v["total_groups"].as_u64().unwrap() >= 1,
        "expected at least one consolidation group"
    );
}

// ── eval ────────────────────────────────────────────────────────────

#[test]
fn test_eval_runs_questions() {
    let root = temp_dir("eval");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    // Eval expects JSONL with {id: usize, q: str, expect: [str]}
    let questions = root.join("questions.jsonl");
    fs::write(
        &questions,
        "{\"id\": 1, \"q\": \"what is the architecture?\", \"expect\": [\"layered\"]}\n",
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.args(["eval", "--questions"])
        .arg(&questions)
        .arg("--json")
        .arg("--index")
        .arg(&index);
    let output = cmd.output().expect("eval failed");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "eval failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert!(v["results"].is_array(), "expected results array");
}

// ── build JSON output ───────────────────────────────────────────────

#[test]
fn test_build_json_output() {
    let root = temp_dir("build-json");
    write_fixture(&root);
    let index = root.join(".yore");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(&root)
        .args(["build", "docs", "--json", "--output"])
        .arg(&index);
    let output = cmd.output().expect("build failed");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "build --json failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert!(v["files_indexed"].as_u64().unwrap() >= 9);
    assert!(v["unique_keywords"].as_u64().unwrap() > 0);
    assert!(v["total_relations"].as_u64().is_some());
}

// ── policy ──────────────────────────────────────────────────────────

#[test]
fn test_policy_enforces_rules() {
    let root = temp_dir("policy");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    // Policy config is YAML with { rules: [{ pattern, must_contain, ... }] }
    let policy = root.join(".yore-policy.yaml");
    fs::write(
        &policy,
        "\
rules:
  - pattern: \"**/adr/*.md\"
    name: adr-must-have-status
    must_contain:
      - \"Status\"
      - \"Decision\"
",
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(&root)
        .args(["policy", "--config"])
        .arg(&policy)
        .arg("--json")
        .arg("--index")
        .arg(&index);
    let output = cmd.output().expect("policy failed");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "policy failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    // ADR files contain both "Status" and "Decision", so 0 violations
    assert_eq!(v["total_violations"].as_u64().unwrap(), 0);
    assert!(v["violations"].is_array());
}

// ── stale ───────────────────────────────────────────────────────────

#[test]
fn test_stale_runs_without_error() {
    let root = temp_dir("stale");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    let (ok, stdout, _) = yore(&["stale", "--days", "0", "--json"], &index);
    assert!(ok, "stale failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert!(v["files"].is_array());
}

// ── dupes-sections ──────────────────────────────────────────────────

#[test]
fn test_dupes_sections_json() {
    let root = temp_dir("dupes-sections");
    write_fixture(&root);
    let index = root.join(".yore");
    build_index(&root, "docs", &index);

    // dupes-sections returns a bare array, not { groups: [...] }
    let (ok, stdout, _) = yore(&["dupes-sections", "--json", "--threshold", "0.5"], &index);
    assert!(ok, "dupes-sections failed: {stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    let groups = v.as_array().unwrap();
    assert!(
        !groups.is_empty(),
        "expected at least one duplicate section group"
    );
}
