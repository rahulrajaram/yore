use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("yore-rel-{}-{}", label, nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn yore_build(root: &Path, docs_dir: &str, index_dir: &Path) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_yore"));
    cmd.current_dir(root)
        .args(["build", docs_dir, "--output"])
        .arg(index_dir);
    let output = cmd.output().expect("yore build failed to start");
    assert!(
        output.status.success(),
        "yore build failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

// ── Test 1: Build persists relations.json ──────────────────────────

#[test]
fn test_build_persists_relations_json() {
    let root = temp_dir("persist");
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();

    fs::write(
        docs.join("a.md"),
        "# Alpha\n\nSee [Beta](b.md) for details.\n",
    )
    .unwrap();
    fs::write(docs.join("b.md"), "# Beta\n\nMore info here.\n").unwrap();

    let index = root.join(".yore");
    yore_build(&root, "docs", &index);

    let relations_path = index.join("relations.json");
    assert!(
        relations_path.exists(),
        "relations.json should be created by build"
    );

    let content = fs::read_to_string(&relations_path).unwrap();
    let rel: Value = serde_json::from_str(&content).unwrap();

    assert_eq!(rel["version"], 1);
    assert!(
        rel["total_edges"].as_u64().unwrap() > 0,
        "expected at least one edge"
    );
    assert!(rel["indexed_at"].is_string());
    assert!(rel["edges"].is_array());
}

// ── Test 2: Document-level LinksTo edges ───────────────────────────

#[test]
fn test_document_level_links_to_edges() {
    let root = temp_dir("links-to");
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();

    fs::write(
        docs.join("a.md"),
        "# Alpha\n\nSee [Beta](b.md) for details.\n",
    )
    .unwrap();
    fs::write(docs.join("b.md"), "# Beta\n\nContent here.\n").unwrap();

    let index = root.join(".yore");
    yore_build(&root, "docs", &index);

    let content = fs::read_to_string(index.join("relations.json")).unwrap();
    let rel: Value = serde_json::from_str(&content).unwrap();
    let edges = rel["edges"].as_array().unwrap();

    let links_to: Vec<&Value> = edges.iter().filter(|e| e["kind"] == "links_to").collect();

    assert!(!links_to.is_empty(), "expected at least one links_to edge");

    let edge = links_to
        .iter()
        .find(|e| {
            e["source"].as_str().unwrap().contains("a.md")
                && e["target"].as_str().unwrap().contains("b.md")
        })
        .expect("expected a.md -> b.md links_to edge");

    assert_eq!(edge["kind"], "links_to");
}

// ── Test 3: Section-level edges ────────────────────────────────────

#[test]
fn test_section_level_edges() {
    let root = temp_dir("section");
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();

    fs::write(
        docs.join("a.md"),
        "# Alpha\n\n## Getting Started\n\nSee [Beta setup](b.md#setup-guide) for more.\n",
    )
    .unwrap();
    fs::write(
        docs.join("b.md"),
        "# Beta\n\n## Setup Guide\n\nInstructions here.\n",
    )
    .unwrap();

    let index = root.join(".yore");
    yore_build(&root, "docs", &index);

    let content = fs::read_to_string(index.join("relations.json")).unwrap();
    let rel: Value = serde_json::from_str(&content).unwrap();
    let edges = rel["edges"].as_array().unwrap();

    let section_edges: Vec<&Value> = edges
        .iter()
        .filter(|e| e["kind"] == "section_links_to")
        .collect();

    assert!(
        !section_edges.is_empty(),
        "expected at least one section_links_to edge"
    );

    let edge = section_edges
        .iter()
        .find(|e| {
            e["source"].as_str().unwrap().contains("a.md")
                && e["target"].as_str().unwrap().contains("b.md")
        })
        .expect("expected a.md -> b.md section_links_to edge");

    assert!(
        edge["source_section"].is_object(),
        "source_section should be populated"
    );
}

// ── Test 4: ADR reference edges ────────────────────────────────────

#[test]
fn test_adr_reference_edges() {
    let root = temp_dir("adr-ref");
    let docs = root.join("docs");
    let adr_dir = docs.join("adr");
    fs::create_dir_all(&adr_dir).unwrap();

    fs::write(
        docs.join("guide.md"),
        "# Guide\n\nWe chose this approach per ADR-001.\n",
    )
    .unwrap();
    fs::write(
        adr_dir.join("ADR-001.md"),
        "# ADR-001: Use Postgres\n\nDecision: Use Postgres.\n",
    )
    .unwrap();

    let index = root.join(".yore");
    yore_build(&root, "docs", &index);

    let content = fs::read_to_string(index.join("relations.json")).unwrap();
    let rel: Value = serde_json::from_str(&content).unwrap();
    let edges = rel["edges"].as_array().unwrap();

    let adr_edges: Vec<&Value> = edges
        .iter()
        .filter(|e| e["kind"] == "adr_reference")
        .collect();

    assert!(
        !adr_edges.is_empty(),
        "expected at least one adr_reference edge"
    );

    let edge = adr_edges
        .iter()
        .find(|e| {
            e["source"].as_str().unwrap().contains("guide.md")
                && e["target"].as_str().unwrap().contains("ADR-001")
        })
        .expect("expected guide.md -> ADR-001.md adr_reference edge");

    assert_eq!(edge["kind"], "adr_reference");
    assert!(edge["raw_text"].is_string());
}

// ── Test 5: Deterministic ordering ─────────────────────────────────

#[test]
fn test_deterministic_ordering() {
    let root = temp_dir("deterministic");
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();

    fs::write(
        docs.join("a.md"),
        "# Alpha\n\nSee [Beta](b.md) and [Gamma](c.md).\n",
    )
    .unwrap();
    fs::write(docs.join("b.md"), "# Beta\n\nSee [Gamma](c.md).\n").unwrap();
    fs::write(docs.join("c.md"), "# Gamma\n\nEnd.\n").unwrap();

    // Build twice
    let index1 = root.join(".yore1");
    let index2 = root.join(".yore2");
    yore_build(&root, "docs", &index1);
    yore_build(&root, "docs", &index2);

    let content1 = fs::read_to_string(index1.join("relations.json")).unwrap();
    let content2 = fs::read_to_string(index2.join("relations.json")).unwrap();

    let rel1: Value = serde_json::from_str(&content1).unwrap();
    let rel2: Value = serde_json::from_str(&content2).unwrap();

    // Edges arrays should be identical (ignoring indexed_at timestamp)
    assert_eq!(
        rel1["edges"], rel2["edges"],
        "edges arrays must be identical across builds"
    );
    assert_eq!(rel1["total_edges"], rel2["total_edges"]);
}

// ── Test 6: Self-links excluded ────────────────────────────────────

#[test]
fn test_self_links_excluded() {
    let root = temp_dir("self-link");
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();

    fs::write(
        docs.join("a.md"),
        "# Alpha\n\nSee [self](a.md) and [other section](#beta).\n",
    )
    .unwrap();

    let index = root.join(".yore");
    yore_build(&root, "docs", &index);

    let content = fs::read_to_string(index.join("relations.json")).unwrap();
    let rel: Value = serde_json::from_str(&content).unwrap();
    let edges = rel["edges"].as_array().unwrap();

    for edge in edges {
        assert_ne!(
            edge["source"], edge["target"],
            "self-link edge found: {:?}",
            edge
        );
    }
}
