use super::*;

#[test]
fn test_jaccard_similarity() {
    let set1: HashSet<String> = ["foo", "bar", "baz"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let set2: HashSet<String> = ["bar", "baz", "qux"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    let sim = jaccard_similarity(&set1, &set2);
    // Intersection: {bar, baz} = 2
    // Union: {foo, bar, baz, qux} = 4
    // Jaccard: 2/4 = 0.5
    assert_eq!(sim, 0.5);

    // Empty sets
    let empty1: HashSet<String> = HashSet::new();
    let empty2: HashSet<String> = HashSet::new();
    assert_eq!(jaccard_similarity(&empty1, &empty2), 0.0);

    // Identical sets
    assert_eq!(jaccard_similarity(&set1, &set1), 1.0);
}

#[test]
fn test_simhash_similarity() {
    // Identical hashes
    assert_eq!(simhash_similarity(0x123456, 0x123456), 1.0);

    // Completely different (all bits flipped)
    let hash1 = 0x0000000000000000u64;
    let hash2 = 0xFFFFFFFFFFFFFFFFu64;
    assert_eq!(simhash_similarity(hash1, hash2), 0.0);

    // 1 bit different out of 64
    let hash_a = 0b0000000000000000u64;
    let hash_b = 0b0000000000000001u64;
    let sim = simhash_similarity(hash_a, hash_b);
    assert!((sim - (63.0 / 64.0)).abs() < 0.01);
}

#[test]
fn test_hamming_distance() {
    assert_eq!(hamming_distance(0b1010, 0b1010), 0);
    assert_eq!(hamming_distance(0b1010, 0b0101), 4);
    assert_eq!(hamming_distance(0b1111, 0b0000), 4);
    assert_eq!(hamming_distance(0b1100, 0b1010), 2);
}

#[test]
fn test_compute_simhash_stability() {
    let text1 = "The quick brown fox jumps over the lazy dog";
    let text2 = "The quick brown fox jumps over the lazy dog";

    let hash1 = compute_simhash(text1);
    let hash2 = compute_simhash(text2);

    // Identical text should produce identical hashes
    assert_eq!(hash1, hash2);
}

#[test]
fn test_compute_simhash_similarity() {
    let text1 = "machine learning algorithms";
    let text2 = "machine learning systems";
    let text3 = "completely different topic about cooking";

    let hash1 = compute_simhash(text1);
    let hash2 = compute_simhash(text2);
    let hash3 = compute_simhash(text3);

    // Similar texts should have high similarity
    let sim_similar = simhash_similarity(hash1, hash2);
    // Different texts should have lower similarity
    let sim_different = simhash_similarity(hash1, hash3);

    assert!(sim_similar > sim_different);
    assert!(sim_similar > 0.5); // Similar texts should be > 50% similar
}

#[test]
fn test_minhash_basic() {
    let keywords1 = vec!["foo".to_string(), "bar".to_string(), "baz".to_string()];
    let keywords2 = vec!["foo".to_string(), "bar".to_string(), "baz".to_string()];

    let mh1 = compute_minhash(&keywords1, 128);
    let mh2 = compute_minhash(&keywords2, 128);

    // Same keywords should produce same MinHash
    assert_eq!(mh1, mh2);
    assert_eq!(mh1.len(), 128);

    // Similarity should be 1.0
    assert_eq!(minhash_similarity(&mh1, &mh2), 1.0);
}

#[test]
fn test_minhash_similarity_estimation() {
    let keywords1 = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let keywords2 = vec!["b".to_string(), "c".to_string(), "d".to_string()];
    let keywords3 = vec!["x".to_string(), "y".to_string(), "z".to_string()];

    let mh1 = compute_minhash(&keywords1, 128);
    let mh2 = compute_minhash(&keywords2, 128);
    let mh3 = compute_minhash(&keywords3, 128);

    // keywords1 and keywords2 share 2 out of 4 unique items = 0.5 Jaccard
    let sim_similar = minhash_similarity(&mh1, &mh2);
    // keywords1 and keywords3 share 0 items
    let sim_different = minhash_similarity(&mh1, &mh3);

    // Similar sets should have higher MinHash similarity
    assert!(sim_similar > sim_different);
    // MinHash should approximate Jaccard (within reasonable error)
    assert!(sim_similar > 0.3 && sim_similar < 0.7); // Approximately 0.5
}

#[test]
fn test_lsh_buckets() {
    let mut files = HashMap::new();

    // Create 3 files with MinHash signatures
    let keywords1 = vec!["foo".to_string(), "bar".to_string(), "baz".to_string()];
    let keywords2 = vec!["foo".to_string(), "bar".to_string(), "baz".to_string()];
    let keywords3 = vec!["completely".to_string(), "different".to_string()];

    files.insert(
        "file1.md".to_string(),
        FileEntry {
            path: "file1.md".to_string(),
            size_bytes: 100,
            line_count: 10,
            headings: vec![],
            keywords: keywords1.clone(),
            body_keywords: vec![],
            links: vec![],
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: compute_minhash(&keywords1, 128),
            section_fingerprints: vec![],
            adr_references: vec![],
        },
    );

    files.insert(
        "file2.md".to_string(),
        FileEntry {
            path: "file2.md".to_string(),
            size_bytes: 100,
            line_count: 10,
            headings: vec![],
            keywords: keywords2.clone(),
            body_keywords: vec![],
            links: vec![],
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: compute_minhash(&keywords2, 128),
            section_fingerprints: vec![],
            adr_references: vec![],
        },
    );

    files.insert(
        "file3.md".to_string(),
        FileEntry {
            path: "file3.md".to_string(),
            size_bytes: 100,
            line_count: 10,
            headings: vec![],
            keywords: keywords3.clone(),
            body_keywords: vec![],
            links: vec![],
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: compute_minhash(&keywords3, 128),
            section_fingerprints: vec![],
            adr_references: vec![],
        },
    );

    let buckets = lsh_buckets(&files, 16);

    // Should create some buckets
    assert!(!buckets.is_empty());

    // file1 and file2 should likely be in the same bucket (identical MinHash)
    // Check if they appear together in any bucket
    let mut file1_file2_together = false;
    for paths in buckets.values() {
        if paths.contains(&"file1.md".to_string()) && paths.contains(&"file2.md".to_string()) {
            file1_file2_together = true;
            break;
        }
    }
    assert!(
        file1_file2_together,
        "Identical files should be in same LSH bucket"
    );
}

#[test]
fn test_bm25_score_basic() {
    let mut term_freq = HashMap::new();
    term_freq.insert("test".to_string(), 5);
    term_freq.insert("word".to_string(), 2);

    let doc = FileEntry {
        path: "test.md".to_string(),
        size_bytes: 100,
        line_count: 10,
        headings: vec![],
        keywords: vec![],
        body_keywords: vec![],
        links: vec![],
        simhash: 0,
        term_frequencies: term_freq,
        doc_length: 100,
        minhash: vec![],
        section_fingerprints: vec![],
        adr_references: vec![],
    };

    let mut idf_map = HashMap::new();
    idf_map.insert("test".to_string(), 2.5);
    idf_map.insert("word".to_string(), 1.8);

    let query = vec!["test".to_string()];
    let score = bm25_score(&query, &doc, 100.0, &idf_map);

    // Score should be > 0 for matching term
    assert!(score > 0.0);

    // Query with no matching terms should score 0
    let empty_query = vec!["nonexistent".to_string()];
    let zero_score = bm25_score(&empty_query, &doc, 100.0, &idf_map);
    assert_eq!(zero_score, 0.0);
}

#[test]
fn test_bm25_score_ordering() {
    // Document with high term frequency
    let mut tf_high = HashMap::new();
    tf_high.insert("test".to_string(), 10);

    let doc_high_tf = FileEntry {
        path: "high.md".to_string(),
        size_bytes: 100,
        line_count: 10,
        headings: vec![],
        keywords: vec![],
        body_keywords: vec![],
        links: vec![],
        simhash: 0,
        term_frequencies: tf_high,
        doc_length: 50,
        minhash: vec![],
        section_fingerprints: vec![],
        adr_references: vec![],
    };

    // Document with low term frequency
    let mut tf_low = HashMap::new();
    tf_low.insert("test".to_string(), 1);

    let doc_low_tf = FileEntry {
        path: "low.md".to_string(),
        size_bytes: 100,
        line_count: 10,
        headings: vec![],
        keywords: vec![],
        body_keywords: vec![],
        links: vec![],
        simhash: 0,
        term_frequencies: tf_low,
        doc_length: 50,
        minhash: vec![],
        section_fingerprints: vec![],
        adr_references: vec![],
    };

    let mut idf_map = HashMap::new();
    idf_map.insert("test".to_string(), 2.0);

    let query = vec!["test".to_string()];
    let score_high = bm25_score(&query, &doc_high_tf, 50.0, &idf_map);
    let score_low = bm25_score(&query, &doc_low_tf, 50.0, &idf_map);

    // Higher term frequency should yield higher BM25 score
    assert!(score_high > score_low);
}

#[test]
fn test_policy_rule_matching_and_violations() {
    // Build a simple policy with one rule
    let rule = PolicyRule {
        pattern: "agents/plans/*.md".to_string(),
        must_contain: vec!["## Objective".to_string()],
        must_not_contain: vec![],
        name: Some("plans-must-have-objective".to_string()),
        severity: Some("error".to_string()),
        ..Default::default()
    };

    let policy = PolicyConfig { rules: vec![rule] };

    // Compile glob and check that it matches only the agents/plans file
    let glob = Glob::new(&policy.rules[0].pattern).unwrap();
    let matcher = glob.compile_matcher();
    assert!(matcher.is_match("agents/plans/plan.md"));
    assert!(!matcher.is_match("docs/architecture/auth.md"));

    // Simulate a violation: empty content should trigger missing "## Objective"
    let rule_ref = &policy.rules[0];
    let file_path = "agents/plans/plan.md";
    let content = String::new();
    let violations = collect_policy_violations_for_content(rule_ref, file_path, &content);

    assert_eq!(violations.len(), 1);
    let v = &violations[0];
    assert_eq!(v.file, "agents/plans/plan.md");
    assert_eq!(v.rule, "plans-must-have-objective");
    assert_eq!(v.severity, "error");
    assert_eq!(v.kind, "policy_violation");
}

#[test]
fn test_policy_min_max_length_violations() {
    // Require 10–20 lines
    let rule = PolicyRule {
        pattern: "docs/*.md".to_string(),
        min_length: Some(10),
        max_length: Some(20),
        name: Some("length-bounds".to_string()),
        severity: Some("error".to_string()),
        ..Default::default()
    };

    // Too short: 3 lines
    let short_content = "line1\nline2\nline3\n";
    let short_violations =
        collect_policy_violations_for_content(&rule, "docs/short.md", short_content);
    assert!(
        short_violations
            .iter()
            .any(|v| v.message.contains("Document too short")),
        "Expected a 'Document too short' violation"
    );

    // Too long: 25 lines
    let long_content: String = (0..25).map(|i| format!("line{i}\n")).collect();
    let long_violations =
        collect_policy_violations_for_content(&rule, "docs/long.md", &long_content);
    assert!(
        long_violations
            .iter()
            .any(|v| v.message.contains("Document too long")),
        "Expected a 'Document too long' violation"
    );
}

#[test]
fn test_policy_required_and_forbidden_headings() {
    let rule = PolicyRule {
        pattern: "docs/*.md".to_string(),
        required_headings: vec!["Objective".to_string()],
        forbidden_headings: vec!["Deprecated".to_string()],
        name: Some("heading-rules".to_string()),
        severity: Some("error".to_string()),
        ..Default::default()
    };

    let content = r"
# Title

## Objective

Some content here.

## Deprecated
";

    let violations = collect_policy_violations_for_content(&rule, "docs/example.md", content);

    // Should not flag missing Objective (it exists)
    assert!(
        !violations
            .iter()
            .any(|v| v.message.contains("Missing required heading")),
        "Did not expect a missing required heading violation"
    );

    // Should flag forbidden Deprecated heading
    assert!(
        violations
            .iter()
            .any(|v| v.message.contains("Forbidden heading present")),
        "Expected a forbidden heading violation"
    );
}

#[test]
fn test_policy_section_length_violation() {
    let rule = PolicyRule {
        pattern: "docs/*.md".to_string(),
        max_section_length: Some(3),
        section_heading_regex: Some("^Async".to_string()),
        name: Some("status-section-length".to_string()),
        severity: Some("warn".to_string()),
        ..Default::default()
    };

    let content = r"
# Status

## Async Migration
line1
line2
line3
line4

## Other
ok
";

    let violations =
        collect_policy_violations_for_content(&rule, "docs/IMPLEMENTATION_STATUS.md", content);

    assert!(
        violations
            .iter()
            .any(|v| v.message.contains("Section too long")),
        "Expected a section-length violation"
    );
}

#[test]
fn test_policy_required_link() {
    let rule = PolicyRule {
        pattern: "docs/*.md".to_string(),
        must_link_to: vec!["docs/ASYNC_MIGRATION_COMPLETE_SUMMARY.md".to_string()],
        name: Some("status-requires-summary-link".to_string()),
        severity: Some("error".to_string()),
        ..Default::default()
    };

    let missing_link = r"
# Status
No links here.
";
    let violations =
        collect_policy_violations_for_content(&rule, "docs/IMPLEMENTATION_STATUS.md", missing_link);
    assert!(
        violations
            .iter()
            .any(|v| v.message.contains("Missing required link")),
        "Expected a missing required link violation"
    );

    let with_link = r"
# Status
See [summary](ASYNC_MIGRATION_COMPLETE_SUMMARY.md).
";
    let ok_violations =
        collect_policy_violations_for_content(&rule, "docs/IMPLEMENTATION_STATUS.md", with_link);
    assert!(
        ok_violations.is_empty(),
        "Did not expect violations when required link is present"
    );
}

#[test]
fn test_suggest_new_link_target_same_dir() {
    let mut available = HashSet::new();
    available.insert("docs/guide/auth.md".to_string());
    available.insert("docs/guide/other.md".to_string());

    // Source and target are in the same parent; filename matches exactly one file
    let suggested = suggest_new_link_target("docs/guide/README.md", "auth.md", &available);
    // Expect a simple relative path suggestion
    assert_eq!(suggested.as_deref(), Some("auth.md"));
}

#[test]
fn test_apply_reference_mapping_to_content() {
    let content = "See [auth](docs/old/auth.md) for details.";
    let updated = apply_reference_mapping_to_content(
        content,
        "docs/old/auth.md",
        "docs/architecture/AUTH.md",
    );
    assert_eq!(
        updated,
        "See [auth](docs/architecture/AUTH.md) for details."
    );
}

#[test]
fn test_build_consolidation_groups_basic() {
    // Minimal forward index with two files; we create a single duplicate pair
    let mut files = HashMap::new();

    files.insert(
        "docs/a.md".to_string(),
        FileEntry {
            path: "docs/a.md".to_string(),
            size_bytes: 0,
            line_count: 1,
            headings: vec![],
            keywords: vec!["foo".to_string()],
            body_keywords: vec![],
            links: vec![],
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: vec![],
            section_fingerprints: vec![],
            adr_references: vec![],
        },
    );
    files.insert(
        "docs/b.md".to_string(),
        FileEntry {
            path: "docs/b.md".to_string(),
            size_bytes: 0,
            line_count: 1,
            headings: vec![],
            keywords: vec!["foo".to_string()],
            body_keywords: vec![],
            links: vec![],
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: vec![],
            section_fingerprints: vec![],
            adr_references: vec![],
        },
    );

    let forward_index = ForwardIndex {
        files,
        indexed_at: chrono_now(),
        version: 3,
        source_root: String::new(),
        avg_doc_length: 0.0,
        idf_map: HashMap::new(),
    };

    let pairs = vec![("docs/a.md".to_string(), "docs/b.md".to_string(), 0.9_f64)];

    let result = build_consolidation_groups(&forward_index, &pairs);
    assert_eq!(result.total_groups, 1);
    let group = &result.groups[0];
    assert!(group.canonical == "docs/a.md" || group.canonical == "docs/b.md");
    assert_eq!(group.merge_into.len(), 1);
}

#[test]
fn test_compute_inbound_link_counts() {
    let mut files = HashMap::new();

    files.insert(
        "docs/a.md".to_string(),
        FileEntry {
            path: "docs/a.md".to_string(),
            size_bytes: 0,
            line_count: 1,
            headings: vec![],
            keywords: vec![],
            body_keywords: vec![],
            links: vec![Link {
                line: 1,
                text: "b".to_string(),
                target: "b.md".to_string(),
            }],
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: vec![],
            section_fingerprints: vec![],
            adr_references: vec![],
        },
    );
    files.insert(
        "docs/b.md".to_string(),
        FileEntry {
            path: "docs/b.md".to_string(),
            size_bytes: 0,
            line_count: 1,
            headings: vec![],
            keywords: vec![],
            body_keywords: vec![],
            links: vec![],
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: vec![],
            section_fingerprints: vec![],
            adr_references: vec![],
        },
    );

    let forward_index = ForwardIndex {
        files,
        indexed_at: "0".to_string(),
        version: 3,
        source_root: String::new(),
        avg_doc_length: 0.0,
        idf_map: HashMap::new(),
    };

    let counts = compute_inbound_link_counts(&forward_index);
    // a.md links to b.md, so b.md should have 1 inbound link
    assert_eq!(counts.get("docs/b.md"), Some(&1));
}

#[test]
fn test_index_sections() {
    let content = "# Introduction\nThis is the intro.\n\n## Details\nMore details here.\n\n## Summary\nFinal thoughts.";
    let headings = vec![
        Heading {
            line: 1,
            level: 1,
            text: "Introduction".to_string(),
        },
        Heading {
            line: 4,
            level: 2,
            text: "Details".to_string(),
        },
        Heading {
            line: 7,
            level: 2,
            text: "Summary".to_string(),
        },
    ];

    let sections = index_sections(content, &headings);

    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].heading, "Introduction");
    assert_eq!(sections[0].level, 1);
    assert_eq!(sections[0].line_start, 1);

    assert_eq!(sections[1].heading, "Details");
    assert_eq!(sections[1].level, 2);
    assert_eq!(sections[1].line_start, 4);

    assert_eq!(sections[2].heading, "Summary");
    assert_eq!(sections[2].level, 2);
}

#[test]
fn test_index_sections_similar_content() {
    let content1 = "## Testing\nRun the tests with:\n```\npytest\n```";
    let content2 = "## Testing\nRun the tests with:\n```\npytest\n```";
    let content3 = "## Testing\nCompletely different content about testing";

    let headings1 = vec![Heading {
        line: 1,
        level: 2,
        text: "Testing".to_string(),
    }];
    let headings2 = vec![Heading {
        line: 1,
        level: 2,
        text: "Testing".to_string(),
    }];
    let headings3 = vec![Heading {
        line: 1,
        level: 2,
        text: "Testing".to_string(),
    }];

    let sections1 = index_sections(content1, &headings1);
    let sections2 = index_sections(content2, &headings2);
    let sections3 = index_sections(content3, &headings3);

    // Identical content should produce identical SimHash
    assert_eq!(sections1[0].simhash, sections2[0].simhash);

    // Different content should produce different SimHash
    assert_ne!(sections1[0].simhash, sections3[0].simhash);

    // Identical sections should have 100% similarity
    let sim_identical = simhash_similarity(sections1[0].simhash, sections2[0].simhash);
    assert_eq!(sim_identical, 1.0);

    // Different sections should have < 100% similarity
    let sim_different = simhash_similarity(sections1[0].simhash, sections3[0].simhash);
    assert!(sim_different < 1.0);
}

#[test]
fn test_compute_document_metrics_captures_structure_signals() {
    let content = r"---
title: Demo
owner: Docs
---

# Overview
Intro paragraph.

## Part 1
- first
- second

## Changelog
- Added feature
- Fixed bug

## Completed Work
```rust
fn main() {}
```
";
    let lines: Vec<&str> = content.lines().collect();
    let headings = vec![
        Heading {
            line: 6,
            level: 1,
            text: "Overview".to_string(),
        },
        Heading {
            line: 9,
            level: 2,
            text: "Part 1".to_string(),
        },
        Heading {
            line: 13,
            level: 2,
            text: "Changelog".to_string(),
        },
        Heading {
            line: 17,
            level: 2,
            text: "Completed Work".to_string(),
        },
    ];
    let links = vec![Link {
        line: 7,
        text: "readme".to_string(),
        target: "README.md".to_string(),
    }];

    let metrics = compute_document_metrics("docs/demo.md", content, &lines, &headings, &links);

    assert_eq!(metrics.path, "docs/demo.md");
    assert_eq!(metrics.frontmatter_key_count, 2);
    assert_eq!(metrics.heading_count, 4);
    assert_eq!(metrics.section_count, 4);
    assert_eq!(metrics.h1_count, 1);
    assert_eq!(metrics.h2_count, 3);
    assert_eq!(metrics.part_heading_count, 1);
    assert_eq!(metrics.changelog_heading_count, 1);
    assert_eq!(metrics.completion_heading_count, 1);
    assert_eq!(metrics.changelog_entry_count, 2);
    assert_eq!(metrics.list_item_count, 4);
    assert_eq!(metrics.code_block_count, 1);
    assert!(metrics.longest_section_lines >= 3);
    assert!(metrics
        .sections
        .iter()
        .any(|section| section.looks_like_part));
    assert!(metrics
        .sections
        .iter()
        .any(|section| section.looks_like_changelog && section.list_item_count == 2));
}

#[test]
fn test_cmd_build_writes_document_metrics_index() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("yore-build-metrics-{unique}"));
    let docs_dir = root.join("docs");
    let index_dir = root.join(".yore");

    fs::create_dir_all(&docs_dir).unwrap();
    fs::write(
        docs_dir.join("guide.md"),
        "# Guide\n\n## Part 1\n- step one\n- step two\n",
    )
    .unwrap();

    cmd_build(&docs_dir, &index_dir, "md", &[], true, None, false, false).unwrap();

    let metrics_path = index_dir.join("document_metrics.json");
    assert!(metrics_path.exists());

    let metrics_index: DocumentMetricsIndex =
        serde_json::from_str(&fs::read_to_string(metrics_path).unwrap()).unwrap();
    assert_eq!(metrics_index.version, 1);
    assert_eq!(metrics_index.files.len(), 1);

    let metrics = metrics_index.files.values().next().unwrap();
    assert_eq!(metrics.heading_count, 2);
    assert_eq!(metrics.part_heading_count, 1);
    assert_eq!(metrics.list_item_count, 2);
    assert_eq!(metrics.section_count, 2);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn test_extract_keywords() {
    let text = "This is a TEST document with some KEYWORDS";
    let keywords = extract_keywords(text);

    // Should lowercase (but not stem - extract_keywords doesn't stem)
    assert!(keywords.contains(&"test".to_string()));
    assert!(keywords.contains(&"document".to_string()));
    assert!(keywords.contains(&"keywords".to_string())); // Note: not stemmed

    // Should not contain stop words
    assert!(!keywords.contains(&"this".to_string()));
    assert!(!keywords.contains(&"is".to_string()));
    // "a" and "with" are too short or stop words
    assert!(!keywords.contains(&"with".to_string()));
}

#[test]
fn test_stem_word() {
    // Test actual stemming behavior
    assert_eq!(stem_word("running"), "runn"); // Simple stemmer removes "ing"
    assert_eq!(stem_word("tests"), "test"); // Removes "s"
    assert_eq!(stem_word("testing"), "test"); // Removes "ing"
    assert_eq!(stem_word("keywords"), "keyword"); // Removes "s"

    // Short words should not be stemmed
    assert_eq!(stem_word("go"), "go");
    assert_eq!(stem_word("it"), "it");
}

#[test]
fn test_get_link_context_basic() {
    let path = "test_get_link_context_basic.md";
    fs::write(path, "first line\nsecond line with a link\nthird line\n").unwrap();

    let mut cache: HashMap<String, Vec<String>> = HashMap::new();
    let ctx = get_link_context(&mut cache, path, 2).unwrap();
    assert_eq!(ctx.as_deref(), Some("second line with a link"));

    // Out-of-range line number should yield None
    let ctx_out = get_link_context(&mut cache, path, 10).unwrap();
    assert!(ctx_out.is_none());

    fs::remove_file(path).unwrap();
}

#[test]
fn test_get_link_context_truncates_long_lines() {
    let path = "test_get_link_context_truncate.md";
    let long_line = "a".repeat(200);
    fs::write(path, format!("{long_line}\n")).unwrap();

    let mut cache: HashMap<String, Vec<String>> = HashMap::new();
    let ctx = get_link_context(&mut cache, path, 1)
        .unwrap()
        .expect("expected context");

    assert!(ctx.len() <= 160);
    assert!(ctx.ends_with("..."));

    fs::remove_file(path).unwrap();
}

#[test]
fn test_get_top_doc_terms_basic() {
    // Setup: doc with term frequencies and IDF map
    // Note: term_frequencies and idf_map use STEMMED keys
    // "docker" -> "dock", "nginx" -> "nginx", "helm" -> "helm"
    let mut term_frequencies = HashMap::new();
    term_frequencies.insert("dock".to_string(), 10); // stem of "docker"
    term_frequencies.insert("nginx".to_string(), 5);
    term_frequencies.insert("helm".to_string(), 3);

    let entry = FileEntry {
        path: "test.md".to_string(),
        size_bytes: 100,
        line_count: 10,
        headings: vec![],
        keywords: vec![],
        body_keywords: vec![
            "docker".to_string(),
            "nginx".to_string(),
            "helm".to_string(),
            "container".to_string(), // not in tf, will be excluded
        ],
        links: vec![],
        simhash: 0,
        term_frequencies,
        doc_length: 100,
        minhash: vec![],
        section_fingerprints: vec![],
        adr_references: vec![],
    };

    let mut idf_map = HashMap::new();
    idf_map.insert("dock".to_string(), 2.0); // stemmed
    idf_map.insert("nginx".to_string(), 1.5);
    idf_map.insert("helm".to_string(), 3.0);

    // Test: get top 2 terms, excluding nothing
    let terms = get_top_doc_terms(&entry, &idf_map, &[], 2);

    // docker: 10 * 2.0 = 20
    // helm: 3 * 3.0 = 9
    // nginx: 5 * 1.5 = 7.5
    assert_eq!(terms.len(), 2);
    assert_eq!(terms[0], "docker");
    assert_eq!(terms[1], "helm");
}

#[test]
fn test_get_top_doc_terms_excludes_query_terms() {
    // Note: term_frequencies and idf_map use STEMMED keys
    let mut term_frequencies = HashMap::new();
    term_frequencies.insert("kubernete".to_string(), 10); // stem of "kubernetes"
    term_frequencies.insert("dock".to_string(), 5); // stem of "docker"
    term_frequencies.insert("nginx".to_string(), 3);

    let entry = FileEntry {
        path: "test.md".to_string(),
        size_bytes: 100,
        line_count: 10,
        headings: vec![],
        keywords: vec![],
        body_keywords: vec![
            "kubernetes".to_string(),
            "docker".to_string(),
            "nginx".to_string(),
        ],
        links: vec![],
        simhash: 0,
        term_frequencies,
        doc_length: 100,
        minhash: vec![],
        section_fingerprints: vec![],
        adr_references: vec![],
    };

    let mut idf_map = HashMap::new();
    idf_map.insert("kubernete".to_string(), 2.0); // stemmed
    idf_map.insert("dock".to_string(), 1.5); // stemmed
    idf_map.insert("nginx".to_string(), 3.0);

    // Exclude "kubernetes" from results (different case, should still match after stemming)
    let exclude = vec!["Kubernetes".to_string()];
    let terms = get_top_doc_terms(&entry, &idf_map, &exclude, 3);

    assert_eq!(terms.len(), 2);
    assert!(!terms.contains(&"kubernetes".to_string()));
    assert_eq!(terms[0], "nginx"); // 3 * 3.0 = 9
    assert_eq!(terms[1], "docker"); // 5 * 1.5 = 7.5
}

#[test]
fn test_get_top_doc_terms_deduplicates_stems() {
    let mut term_frequencies = HashMap::new();
    term_frequencies.insert("run".to_string(), 10); // stem of running, runs, run

    let entry = FileEntry {
        path: "test.md".to_string(),
        size_bytes: 100,
        line_count: 10,
        headings: vec![],
        keywords: vec![],
        body_keywords: vec!["running".to_string(), "runs".to_string(), "run".to_string()],
        links: vec![],
        simhash: 0,
        term_frequencies,
        doc_length: 100,
        minhash: vec![],
        section_fingerprints: vec![],
        adr_references: vec![],
    };

    let mut idf_map = HashMap::new();
    idf_map.insert("run".to_string(), 1.0);

    let terms = get_top_doc_terms(&entry, &idf_map, &[], 5);

    // Should only return one term (first occurrence), not all three
    assert_eq!(terms.len(), 1);
}

#[test]
fn test_get_top_doc_terms_zero_returns_empty() {
    let entry = FileEntry {
        path: "test.md".to_string(),
        size_bytes: 100,
        line_count: 10,
        headings: vec![],
        keywords: vec!["test".to_string()],
        body_keywords: vec!["test".to_string()],
        links: vec![],
        simhash: 0,
        term_frequencies: HashMap::new(),
        doc_length: 100,
        minhash: vec![],
        section_fingerprints: vec![],
        adr_references: vec![],
    };

    let idf_map = HashMap::new();
    let terms = get_top_doc_terms(&entry, &idf_map, &[], 0);

    assert!(terms.is_empty());
}

#[test]
fn test_find_link_candidates_single_match() {
    let mut available = HashSet::new();
    available.insert("docs/guide/auth.md".to_string());
    available.insert("docs/guide/other.md".to_string());

    // Source and target are in the same parent; filename matches exactly one file
    let candidates = find_link_candidates("docs/guide/README.md", "auth.md", &available);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0], "auth.md");
}

#[test]
fn test_find_link_candidates_multiple_matches() {
    let mut available = HashSet::new();
    available.insert("docs/v1/auth.md".to_string());
    available.insert("docs/v2/auth.md".to_string());
    available.insert("docs/archive/auth.md".to_string());

    // Multiple files with same name - should return all
    let candidates = find_link_candidates("docs/README.md", "auth.md", &available);
    assert!(candidates.len() >= 2);
}

#[test]
fn test_find_link_candidates_no_match() {
    let mut available = HashSet::new();
    available.insert("docs/guide/other.md".to_string());

    // No file matches
    let candidates = find_link_candidates("docs/README.md", "nonexistent.md", &available);
    assert!(candidates.is_empty());
}

#[test]
fn test_link_fix_proposal_serialization() {
    let proposal = LinkFixProposal {
        source: "docs/README.md".to_string(),
        line: 42,
        broken_target: "../old/auth.md".to_string(),
        candidates: vec![
            "../archive/auth.md".to_string(),
            "../v2/auth.md".to_string(),
        ],
        decision: None,
    };

    let yaml = serde_yaml::to_string(&proposal).unwrap();
    assert!(yaml.contains("source: docs/README.md"));
    assert!(yaml.contains("line: 42"));
    assert!(yaml.contains("broken_target:"));
    assert!(yaml.contains("candidates:"));

    // Test deserialization
    let parsed: LinkFixProposal = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed.source, "docs/README.md");
    assert_eq!(parsed.line, 42);
    assert_eq!(parsed.candidates.len(), 2);
}

#[test]
fn test_link_fix_proposal_with_decision() {
    let yaml = r#"
source: docs/README.md
line: 42
broken_target: "../old/auth.md"
candidates:
  - "../archive/auth.md"
  - "../v2/auth.md"
decision: 1
"#;
    let proposal: LinkFixProposal = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(proposal.decision, Some(1));
    assert_eq!(proposal.candidates[1], "../v2/auth.md");
}

#[test]
fn test_diff_result_serialization() {
    let result = DiffResult {
        file1: "docs/a.md".to_string(),
        file2: "docs/b.md".to_string(),
        similarity: DiffSimilarity {
            combined: 0.75,
            jaccard: 0.6,
            simhash: 0.9,
        },
        shared_keywords: vec!["auth".to_string(), "login".to_string()],
        only_in_file1: vec!["oauth".to_string()],
        only_in_file2: vec!["jwt".to_string()],
        shared_headings: vec!["Introduction".to_string()],
    };

    let json = serde_json::to_string_pretty(&result).unwrap();
    assert!(json.contains("\"file1\": \"docs/a.md\""));
    assert!(json.contains("\"combined\": 0.75"));
    assert!(json.contains("\"shared_keywords\""));
}

#[test]
fn test_stats_result_serialization() {
    let result = StatsResult {
        total_files: 100,
        unique_keywords: 500,
        total_headings: 250,
        body_keywords: 1000,
        total_links: 300,
        index_version: 3,
        indexed_at: "2024-01-01T00:00:00Z".to_string(),
        top_keywords: vec![
            KeywordCount {
                keyword: "authentication".to_string(),
                count: 50,
            },
            KeywordCount {
                keyword: "kubernetes".to_string(),
                count: 40,
            },
        ],
    };

    let json = serde_json::to_string_pretty(&result).unwrap();
    assert!(json.contains("\"total_files\": 100"));
    assert!(json.contains("\"top_keywords\""));
    assert!(json.contains("\"authentication\""));
}

#[test]
fn test_mv_result_serialization() {
    let result = MvResult {
        from: "docs/old.md".to_string(),
        to: "docs/new.md".to_string(),
        moved: true,
        updated_files: vec!["docs/index.md".to_string(), "docs/guide.md".to_string()],
    };

    let json = serde_json::to_string_pretty(&result).unwrap();
    assert!(json.contains("\"from\": \"docs/old.md\""));
    assert!(json.contains("\"moved\": true"));
    assert!(json.contains("\"updated_files\""));
}

#[test]
fn test_fix_references_result_serialization() {
    let result = FixReferencesResult {
        mapping_file: "mappings.yaml".to_string(),
        mappings_count: 5,
        updated_files: vec!["docs/a.md".to_string()],
        applied: false,
    };

    let json = serde_json::to_string_pretty(&result).unwrap();
    assert!(json.contains("\"mapping_file\": \"mappings.yaml\""));
    assert!(json.contains("\"mappings_count\": 5"));
    assert!(json.contains("\"applied\": false"));
}

#[test]
fn test_yore_config_basic_parsing() {
    let toml = r#"
[index.docs]
roots = ["docs/"]
types = ["md"]
output = ".yore"
"#;
    let config: YoreConfig = toml::from_str(toml).unwrap();
    assert!(config.index.contains_key("docs"));
    let docs = config.index.get("docs").unwrap();
    assert_eq!(docs.roots, vec!["docs/"]);
    assert_eq!(docs.types, vec!["md"]);
}

#[test]
fn test_yore_config_link_check_section() {
    let toml = r#"
[link-check]
exclude = ["archive/**", "deprecated/**"]

[[link-check.severity-overrides]]
pattern = "archive/**"
severity = "warn"

[[link-check.severity-overrides]]
pattern = "deprecated/**"
severity = "info"
"#;
    let config: YoreConfig = toml::from_str(toml).unwrap();
    let link_check = config.link_check.unwrap();
    assert_eq!(link_check.exclude.len(), 2);
    assert_eq!(link_check.severity_overrides.len(), 2);
    assert_eq!(link_check.severity_overrides[0].pattern, "archive/**");
    assert_eq!(link_check.severity_overrides[0].severity, "warn");
}

#[test]
fn test_yore_config_external_repos() {
    let toml = r#"
[[external.repos]]
path = "../runtime/docs"
prefix = "runtime"

[[external.repos]]
path = "../api-docs"
"#;
    let config: YoreConfig = toml::from_str(toml).unwrap();
    let external = config.external.unwrap();
    assert_eq!(external.repos.len(), 2);
    assert_eq!(external.repos[0].path, "../runtime/docs");
    assert_eq!(external.repos[0].prefix, Some("runtime".to_string()));
    assert_eq!(external.repos[1].prefix, None);
}

#[test]
fn test_yore_config_policy_section() {
    let toml = r#"
[policy]
rules-file = ".yore-policy.yaml"
"#;
    let config: YoreConfig = toml::from_str(toml).unwrap();
    let policy = config.policy.unwrap();
    assert_eq!(policy.rules_file, Some(".yore-policy.yaml".to_string()));
}

#[test]
fn test_yore_config_full_example() {
    let toml = r#"
[index.docs]
roots = ["docs/"]
types = ["md", "txt"]
output = ".yore"

[index.all]
roots = ["docs/", "specs/"]
types = ["md"]

[link-check]
exclude = ["archive/**"]

[[link-check.severity-overrides]]
pattern = "deprecated/**"
severity = "info"

[policy]
rules-file = ".yore-policy.yaml"

[[external.repos]]
path = "../runtime/docs"
prefix = "runtime"
"#;
    let config: YoreConfig = toml::from_str(toml).unwrap();

    // Index profiles
    assert_eq!(config.index.len(), 2);
    assert!(config.index.contains_key("docs"));
    assert!(config.index.contains_key("all"));

    // Link check
    let link_check = config.link_check.unwrap();
    assert_eq!(link_check.exclude.len(), 1);
    assert_eq!(link_check.severity_overrides.len(), 1);

    // Policy
    let policy = config.policy.unwrap();
    assert!(policy.rules_file.is_some());

    // External
    let external = config.external.unwrap();
    assert_eq!(external.repos.len(), 1);
}

#[test]
fn test_yore_config_empty_is_valid() {
    let toml = "";
    let config: YoreConfig = toml::from_str(toml).unwrap();
    assert!(config.index.is_empty());
    assert!(config.link_check.is_none());
    assert!(config.policy.is_none());
    assert!(config.external.is_none());
}

#[test]
fn test_build_result_serialization() {
    let result = BuildResult {
        index_path: ".yore".to_string(),
        files_indexed: 150,
        total_headings: 450,
        total_links: 200,
        unique_keywords: 800,
        duration_ms: 1234,
        renames_tracked: None,
        total_relations: None,
    };

    let json = serde_json::to_string_pretty(&result).unwrap();
    assert!(json.contains("\"index_path\": \".yore\""));
    // renames_tracked should be absent when None due to skip_serializing_if
    assert!(!json.contains("renames_tracked"));
    assert!(json.contains("\"files_indexed\": 150"));
    assert!(json.contains("\"total_headings\": 450"));
    assert!(json.contains("\"total_links\": 200"));
    assert!(json.contains("\"unique_keywords\": 800"));
    assert!(json.contains("\"duration_ms\": 1234"));
}

#[test]
fn test_eval_json_result_serialization() {
    let result = EvalJsonResult {
        questions_file: "questions.jsonl".to_string(),
        total_questions: 10,
        passed: 8,
        failed: 2,
        pass_rate: 80.0,
        results: vec![
            EvalQuestionResult {
                question: "How do I authenticate?".to_string(),
                passed: true,
                expected: vec!["auth.md".to_string()],
                found: vec!["auth.md".to_string()],
                missing: vec![],
                ranking: None,
            },
            EvalQuestionResult {
                question: "What is the API endpoint?".to_string(),
                passed: false,
                expected: vec!["api.md".to_string()],
                found: vec![],
                missing: vec!["api.md".to_string()],
                ranking: Some(RankingMetrics {
                    precision_at_k: vec![MetricAtK { k: 5, value: 0.4 }],
                    recall_at_k: vec![MetricAtK { k: 5, value: 1.0 }],
                    mrr: 0.5,
                    ndcg_at_k: vec![MetricAtK { k: 5, value: 0.8 }],
                }),
            },
        ],
        ranking_metrics: None,
    };

    let json = serde_json::to_string_pretty(&result).unwrap();
    assert!(json.contains("\"questions_file\": \"questions.jsonl\""));
    assert!(json.contains("\"total_questions\": 10"));
    assert!(json.contains("\"passed\": 8"));
    assert!(json.contains("\"failed\": 2"));
    assert!(json.contains("\"pass_rate\": 80.0"));
    assert!(json.contains("\"results\""));
    assert!(json.contains("How do I authenticate?"));
    assert!(json.contains("\"missing\": []"));
    // ranking is omitted when None
    assert!(!json.contains("\"ranking\":\n") || json.contains("\"ranking\":"));
    // ranking_metrics omitted at top level when None
    assert!(!json.contains("\"ranking_metrics\""));
    // The second question has ranking data
    assert!(json.contains("\"mrr\": 0.5"));
    assert!(json.contains("\"precision_at_k\""));
}

#[test]
fn test_unique_doc_ranking() {
    let sections = vec![
        SectionMatch {
            doc_path: "docs/a.md".to_string(),
            heading: "H1".to_string(),
            line_start: 1,
            line_end: 10,
            bm25_score: 5.0,
            content: "content a".to_string(),
            canonicality: 0.5,
        },
        SectionMatch {
            doc_path: "docs/b.md".to_string(),
            heading: "H2".to_string(),
            line_start: 1,
            line_end: 10,
            bm25_score: 4.0,
            content: "content b".to_string(),
            canonicality: 0.5,
        },
        SectionMatch {
            doc_path: "docs/a.md".to_string(),
            heading: "H3".to_string(),
            line_start: 11,
            line_end: 20,
            bm25_score: 3.0,
            content: "content a2".to_string(),
            canonicality: 0.5,
        },
        SectionMatch {
            doc_path: "docs/c.md".to_string(),
            heading: "H4".to_string(),
            line_start: 1,
            line_end: 10,
            bm25_score: 2.0,
            content: "content c".to_string(),
            canonicality: 0.5,
        },
    ];
    let ranking = unique_doc_ranking(&sections);
    assert_eq!(ranking, vec!["docs/a.md", "docs/b.md", "docs/c.md"]);
}

#[test]
fn test_precision_at_k() {
    let ranked = vec![
        "a.md".to_string(),
        "b.md".to_string(),
        "c.md".to_string(),
        "d.md".to_string(),
        "e.md".to_string(),
    ];
    let relevant: HashSet<String> = ["a.md", "c.md"].iter().map(|s| s.to_string()).collect();

    assert_eq!(precision_at_k(&ranked, &relevant, 5), 0.4);
    assert_eq!(precision_at_k(&ranked, &relevant, 1), 1.0);
    assert_eq!(precision_at_k(&ranked, &relevant, 2), 0.5);
    assert_eq!(precision_at_k(&ranked, &relevant, 0), 0.0);
}

#[test]
fn test_recall_at_k() {
    let ranked = vec!["a.md".to_string(), "b.md".to_string(), "c.md".to_string()];
    let relevant: HashSet<String> = ["a.md", "c.md"].iter().map(|s| s.to_string()).collect();

    assert_eq!(recall_at_k(&ranked, &relevant, 3), 1.0);
    assert_eq!(recall_at_k(&ranked, &relevant, 1), 0.5);
    assert_eq!(recall_at_k(&ranked, &relevant, 0), 0.0);

    // Empty relevant set
    let empty: HashSet<String> = HashSet::new();
    assert_eq!(recall_at_k(&ranked, &empty, 3), 0.0);
}

#[test]
fn test_reciprocal_rank() {
    let ranked = vec!["a.md".to_string(), "b.md".to_string(), "c.md".to_string()];

    // Relevant at rank 1
    let rel1: HashSet<String> = ["a.md"].iter().map(|s| s.to_string()).collect();
    assert_eq!(reciprocal_rank(&ranked, &rel1), 1.0);

    // Relevant at rank 2
    let rel2: HashSet<String> = ["b.md"].iter().map(|s| s.to_string()).collect();
    assert_eq!(reciprocal_rank(&ranked, &rel2), 0.5);

    // No match
    let no_match: HashSet<String> = ["z.md"].iter().map(|s| s.to_string()).collect();
    assert_eq!(reciprocal_rank(&ranked, &no_match), 0.0);
}

#[test]
fn test_ndcg_at_k() {
    // Perfect ranking: both relevant docs at positions 0 and 1
    let perfect = vec!["a.md".to_string(), "b.md".to_string(), "c.md".to_string()];
    let relevant: HashSet<String> = ["a.md", "b.md"].iter().map(|s| s.to_string()).collect();
    assert_eq!(ndcg_at_k(&perfect, &relevant, 3), 1.0);

    // Bad ranking: relevant docs at end
    let bad = vec![
        "c.md".to_string(),
        "d.md".to_string(),
        "a.md".to_string(),
        "b.md".to_string(),
    ];
    let ndcg = ndcg_at_k(&bad, &relevant, 4);
    assert!(ndcg < 1.0, "bad ranking should have nDCG < 1.0, got {ndcg}");
    assert!(ndcg > 0.0, "bad ranking should have nDCG > 0.0, got {ndcg}");

    // No relevant docs
    let empty: HashSet<String> = HashSet::new();
    assert_eq!(ndcg_at_k(&perfect, &empty, 3), 0.0);
}

#[test]
fn test_compute_ranking_metrics() {
    let ranked = vec![
        "a.md".to_string(),
        "b.md".to_string(),
        "c.md".to_string(),
        "d.md".to_string(),
        "e.md".to_string(),
    ];
    let relevant: HashSet<String> = ["a.md", "c.md"].iter().map(|s| s.to_string()).collect();

    let metrics = compute_ranking_metrics(&ranked, &relevant, &[3, 5]);
    assert_eq!(metrics.precision_at_k.len(), 2);
    assert_eq!(metrics.recall_at_k.len(), 2);
    assert_eq!(metrics.ndcg_at_k.len(), 2);
    assert_eq!(metrics.mrr, 1.0); // first relevant at rank 1

    // P@3 = 2/3
    assert!((metrics.precision_at_k[0].value - 2.0 / 3.0).abs() < 1e-9);
    assert_eq!(metrics.precision_at_k[0].k, 3);

    // P@5 = 2/5
    assert!((metrics.precision_at_k[1].value - 0.4).abs() < 1e-9);
}

#[test]
fn test_aggregate_ranking_metrics() {
    let m1 = RankingMetrics {
        precision_at_k: vec![MetricAtK { k: 5, value: 0.4 }],
        recall_at_k: vec![MetricAtK { k: 5, value: 1.0 }],
        mrr: 1.0,
        ndcg_at_k: vec![MetricAtK { k: 5, value: 0.8 }],
    };
    let m2 = RankingMetrics {
        precision_at_k: vec![MetricAtK { k: 5, value: 0.2 }],
        recall_at_k: vec![MetricAtK { k: 5, value: 0.5 }],
        mrr: 0.5,
        ndcg_at_k: vec![MetricAtK { k: 5, value: 0.6 }],
    };

    let agg = aggregate_ranking_metrics(&[m1, m2], &[5]);
    assert_eq!(agg.questions_with_relevance, 2);
    assert!((agg.mean_mrr - 0.75).abs() < 1e-9);
    assert!((agg.mean_precision_at_k[0].value - 0.3).abs() < 1e-9);
    assert!((agg.mean_recall_at_k[0].value - 0.75).abs() < 1e-9);
    assert!((agg.mean_ndcg_at_k[0].value - 0.7).abs() < 1e-9);
}

#[test]
fn test_rename_history_serialization() {
    let history = RenameHistory {
        renames: vec![
            RenameEntry {
                old_path: "docs/old/auth.md".to_string(),
                new_path: "docs/v2/auth.md".to_string(),
                commit: "abc123".to_string(),
            },
            RenameEntry {
                old_path: "docs/v2/auth.md".to_string(),
                new_path: "docs/current/auth.md".to_string(),
                commit: "def456".to_string(),
            },
        ],
        indexed_at: "1234567890".to_string(),
    };

    let json = serde_json::to_string_pretty(&history).unwrap();
    assert!(json.contains("\"old_path\": \"docs/old/auth.md\""));
    assert!(json.contains("\"new_path\": \"docs/v2/auth.md\""));
    assert!(json.contains("\"commit\": \"abc123\""));

    // Verify roundtrip
    let parsed: RenameHistory = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.renames.len(), 2);
}

#[test]
fn test_resolve_renamed_path_single_rename() {
    let history = RenameHistory {
        renames: vec![RenameEntry {
            old_path: "docs/old.md".to_string(),
            new_path: "docs/new.md".to_string(),
            commit: "abc123".to_string(),
        }],
        indexed_at: "0".to_string(),
    };

    assert_eq!(
        resolve_renamed_path("docs/old.md", &history),
        Some("docs/new.md".to_string())
    );
    assert_eq!(resolve_renamed_path("docs/other.md", &history), None);
}

#[test]
fn test_resolve_renamed_path_chain() {
    let history = RenameHistory {
        renames: vec![
            RenameEntry {
                old_path: "a.md".to_string(),
                new_path: "b.md".to_string(),
                commit: "1".to_string(),
            },
            RenameEntry {
                old_path: "b.md".to_string(),
                new_path: "c.md".to_string(),
                commit: "2".to_string(),
            },
            RenameEntry {
                old_path: "c.md".to_string(),
                new_path: "d.md".to_string(),
                commit: "3".to_string(),
            },
        ],
        indexed_at: "0".to_string(),
    };

    // Should follow the chain from a.md -> b.md -> c.md -> d.md
    assert_eq!(
        resolve_renamed_path("a.md", &history),
        Some("d.md".to_string())
    );
    // Starting from middle should also work
    assert_eq!(
        resolve_renamed_path("b.md", &history),
        Some("d.md".to_string())
    );
}

#[test]
fn test_compute_relative_path_same_dir() {
    let files: HashSet<String> = HashSet::new();
    assert_eq!(
        compute_relative_path("docs/foo.md", "docs/bar.md", &files),
        Some("bar.md".to_string())
    );
}

#[test]
fn test_compute_relative_path_subdirectory() {
    let files: HashSet<String> = HashSet::new();
    assert_eq!(
        compute_relative_path("docs/index.md", "docs/guides/auth.md", &files),
        Some("guides/auth.md".to_string())
    );
}

#[test]
fn test_compute_relative_path_parent_directory() {
    let files: HashSet<String> = HashSet::new();
    let result = compute_relative_path("docs/guides/auth.md", "docs/index.md", &files);
    assert!(result.is_some());
    assert!(result.unwrap().starts_with("../"));
}

#[test]
fn test_build_result_with_renames() {
    let result = BuildResult {
        index_path: ".yore".to_string(),
        files_indexed: 100,
        total_headings: 200,
        total_links: 50,
        unique_keywords: 500,
        duration_ms: 1000,
        renames_tracked: Some(25),
        total_relations: None,
    };

    let json = serde_json::to_string_pretty(&result).unwrap();
    assert!(json.contains("\"renames_tracked\": 25"));
}

#[test]
fn test_external_repos_path_extraction() {
    let toml = r#"
[[external.repos]]
path = "../runtime/docs"
prefix = "runtime"

[[external.repos]]
path = "../api-docs"
"#;
    let config: YoreConfig = toml::from_str(toml).unwrap();
    let external = config.external.unwrap();

    // Extract paths like the cmd_check_links dispatch does
    let paths: Vec<String> = external.repos.iter().map(|r| r.path.clone()).collect();

    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0], "../runtime/docs");
    assert_eq!(paths[1], "../api-docs");
}

fn make_file_entry(path: &str) -> FileEntry {
    FileEntry {
        path: path.to_string(),
        size_bytes: 0,
        line_count: 0,
        headings: Vec::new(),
        keywords: Vec::new(),
        body_keywords: Vec::new(),
        links: Vec::new(),
        simhash: 0,
        term_frequencies: HashMap::new(),
        doc_length: 0,
        minhash: Vec::new(),
        section_fingerprints: Vec::new(),
        adr_references: Vec::new(),
    }
}

fn make_forward_index(files: Vec<FileEntry>) -> ForwardIndex {
    let map = files
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();
    ForwardIndex {
        files: map,
        indexed_at: "now".to_string(),
        version: 1,
        source_root: String::new(),
        avg_doc_length: 0.0,
        idf_map: HashMap::new(),
    }
}

#[test]
fn test_parse_query_terms_punctuation_hyphen_case() {
    let terms = parse_query_terms("Hello, async-migration!", true);
    assert!(terms.contains(&"hello".to_string()));
    assert!(terms.contains(&"async-migration".to_string()));
}

#[test]
fn test_parse_query_terms_stopwords_only() {
    let terms = parse_query_terms("the and of", true);
    assert!(terms.is_empty());
}

#[test]
fn test_load_vocabulary_stopwords_merges_defaults_and_custom() {
    let default_words = load_vocabulary_stopwords(None, true).unwrap();
    assert!(default_words.contains("the"));
    assert!(default_words.contains("using"));

    let custom_path = "tmp-vocabulary-stopwords.txt";
    fs::write(custom_path, "custom\nThe\nvocab-test\n").unwrap();
    let merged_words = load_vocabulary_stopwords(Some(Path::new(custom_path)), true).unwrap();

    fs::remove_file(custom_path).unwrap();
    assert!(merged_words.contains("custom"));
    assert!(merged_words.contains("the"));
    assert!(merged_words.contains("vocab-test"));
}

#[test]
fn test_load_vocabulary_stopwords_can_disable_defaults() {
    let stopwords = load_vocabulary_stopwords(None, false).unwrap();
    assert!(!stopwords.contains("the"));
    assert!(!stopwords.contains("and"));
    assert!(stopwords.is_empty());
}

#[test]
fn test_build_auto_common_vocabulary_stopwords() {
    let candidates = vec![
        VocabularyCandidateTerm {
            term: "build".into(),
            surface: None,
            term_freq: 12,
            doc_freq: 2,
            first_file: "a".into(),
            first_line: 1,
            first_heading: "Build".into(),
        },
        VocabularyCandidateTerm {
            term: "yore".into(),
            surface: None,
            term_freq: 9,
            doc_freq: 3,
            first_file: "a".into(),
            first_line: 1,
            first_heading: "Yore".into(),
        },
        VocabularyCandidateTerm {
            term: "indexer".into(),
            surface: None,
            term_freq: 8,
            doc_freq: 5,
            first_file: "a".into(),
            first_line: 1,
            first_heading: "Index".into(),
        },
    ];

    let common = build_auto_common_vocabulary_stopwords(&candidates, 2);
    assert!(common.contains("build"));
    assert!(common.contains("yore"));
    assert_eq!(common.len(), 2);
}

#[test]
fn test_is_hygienic_vocabulary_term() {
    assert!(!is_hygienic_vocabulary_term("th"));
    assert!(is_hygienic_vocabulary_term("yore"));
    assert!(!is_hygienic_vocabulary_term("a1234567890"));
    assert!(!is_hygienic_vocabulary_term("12345"));
    assert!(!is_hygienic_vocabulary_term("v2.0"));
    assert!(!is_hygienic_vocabulary_term("x"));
}

#[test]
fn test_apply_vocabulary_limit_preserves_total_and_truncates_terms() {
    let terms = vec![
        VocabularyTerm {
            term: "alpha".into(),
            score: 3.0,
            count: 4,
        },
        VocabularyTerm {
            term: "beta".into(),
            score: 2.0,
            count: 3,
        },
        VocabularyTerm {
            term: "gamma".into(),
            score: 1.0,
            count: 2,
        },
    ];
    let (clipped, total) = apply_vocabulary_limit(terms, 2);
    assert_eq!(total, 3);
    assert_eq!(clipped.len(), 2);
    assert_eq!(clipped[0].term, "alpha");
    assert_eq!(clipped[1].term, "beta");
}

#[test]
fn test_render_vocabulary_lines() {
    let terms = vec![
        VocabularyTerm {
            term: "alpha".into(),
            score: 1.2,
            count: 7,
        },
        VocabularyTerm {
            term: "beta".into(),
            score: 0.9,
            count: 5,
        },
    ];
    assert_eq!(render_vocabulary_lines(&terms), "alpha\nbeta");
}

#[test]
fn test_render_vocabulary_prompt_normalizes_terms() {
    let terms = vec![
        VocabularyTerm {
            term: "alpha beta".into(),
            score: 1.0,
            count: 2,
        },
        VocabularyTerm {
            term: "gamma\x00delta".into(),
            score: 1.0,
            count: 2,
        },
        VocabularyTerm {
            term: "  spaced   out  ".into(),
            score: 1.0,
            count: 2,
        },
    ];
    assert_eq!(
        render_vocabulary_prompt(&terms),
        "alpha beta, gammadelta, spaced out"
    );
}

#[test]
fn test_vocabulary_term_json_shape() {
    let result = VocabularyResult {
        format: "json".into(),
        limit: 2,
        total: 3,
        terms: vec![
            VocabularyTerm {
                term: "alpha".into(),
                score: 2.0,
                count: 7,
            },
            VocabularyTerm {
                term: "beta".into(),
                score: 1.1,
                count: 4,
            },
        ],
        stopwords: None,
        used_default_stopwords: true,
        auto_common_terms: None,
        include_stemming: false,
    };
    let json_value: serde_json::Value = serde_json::to_value(&result).unwrap();
    assert_eq!(json_value["terms"][0]["term"], "alpha");
    assert_eq!(json_value["terms"][0]["score"], 2.0);
    assert_eq!(json_value["terms"][0]["count"], 7);
    assert_eq!(json_value["terms"].as_array().unwrap().len(), 2);
}

#[test]
fn test_resolve_vocabulary_surface_prefers_heading_surface() {
    let postings = vec![
        ReverseEntry {
            file: "notes.md".to_string(),
            line: Some(10),
            heading: Some("alpha term".to_string()),
            level: None,
        },
        ReverseEntry {
            file: "guide.md".to_string(),
            line: Some(2),
            heading: None,
            level: None,
        },
    ];
    let forward = make_forward_index(vec![
        make_file_entry("notes.md"),
        FileEntry {
            path: "guide.md".to_string(),
            size_bytes: 0,
            line_count: 0,
            headings: Vec::new(),
            keywords: vec!["term".to_string(), "other".to_string()],
            body_keywords: vec!["term".to_string()],
            links: Vec::new(),
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: Vec::new(),
            section_fingerprints: Vec::new(),
            adr_references: Vec::new(),
        },
    ]);
    let resolved = resolve_vocabulary_surface("term", &postings, Some(&forward)).unwrap();
    assert_eq!(resolved, "term");
}

#[test]
fn test_resolve_vocabulary_surface_fallbacks_to_forward_index() {
    let postings = vec![
        ReverseEntry {
            file: "notes.md".to_string(),
            line: Some(10),
            heading: None,
            level: None,
        },
        ReverseEntry {
            file: "guide.md".to_string(),
            line: Some(2),
            heading: None,
            level: None,
        },
    ];
    let forward = make_forward_index(vec![
        FileEntry {
            path: "notes.md".to_string(),
            size_bytes: 0,
            line_count: 0,
            headings: Vec::new(),
            keywords: vec!["word".to_string()],
            body_keywords: vec![],
            links: Vec::new(),
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: Vec::new(),
            section_fingerprints: Vec::new(),
            adr_references: Vec::new(),
        },
        FileEntry {
            path: "guide.md".to_string(),
            size_bytes: 0,
            line_count: 0,
            headings: Vec::new(),
            keywords: vec!["word".to_string()],
            body_keywords: vec![],
            links: Vec::new(),
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: Vec::new(),
            section_fingerprints: Vec::new(),
            adr_references: Vec::new(),
        },
    ]);
    let resolved = resolve_vocabulary_surface("word", &postings, Some(&forward)).unwrap();
    assert_eq!(resolved, "word");
}

#[test]
fn test_parse_query_terms_mixed_case() {
    let terms = parse_query_terms("TeSt CaSe", true);
    assert_eq!(terms, vec!["test".to_string(), "case".to_string()]);
}

#[test]
fn test_parse_query_phrases() {
    let parsed = parse_query("\"async migration\" plan", true);
    assert_eq!(
        parsed.terms,
        vec![
            "async".to_string(),
            "migration".to_string(),
            "plan".to_string()
        ]
    );
    assert_eq!(parsed.phrases.len(), 1);
    assert_eq!(
        parsed.phrases[0].terms,
        vec!["async".to_string(), "migration".to_string()]
    );
}

#[test]
fn test_expand_from_files_args_supports_list() {
    let dir = std::env::temp_dir().join(format!(
        "yore-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let list_path = dir.join("files.txt");
    fs::write(&list_path, "docs/a.md\n\n docs/b.md\n").unwrap();

    let args = vec![
        format!("@{}", list_path.to_string_lossy()),
        "docs/c.md".to_string(),
    ];
    let expanded = expand_from_files_args(&args).unwrap();

    assert_eq!(
        expanded,
        vec![
            "docs/a.md".to_string(),
            "docs/b.md".to_string(),
            "docs/c.md".to_string()
        ]
    );
}

#[test]
fn test_resolve_from_files_reports_missing() {
    let index = make_forward_index(vec![make_file_entry("docs/a.md")]);
    let inputs = vec!["./docs/a.md".to_string(), "docs/missing.md".to_string()];
    let (resolved, missing) = resolve_from_files(&inputs, &index);
    assert_eq!(resolved, vec!["docs/a.md".to_string()]);
    assert_eq!(missing, vec!["docs/missing.md".to_string()]);
}

#[test]
fn test_collect_sections_for_files_max_sections() {
    let dir = std::env::temp_dir().join(format!(
        "yore-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join("doc.md");
    fs::write(&file_path, "# Title\n\nBody\n\n## Sub\n\nMore").unwrap();
    let file_path_str = file_path.to_string_lossy().to_string();

    let entry = FileEntry {
        path: file_path_str.clone(),
        size_bytes: 0,
        line_count: 0,
        headings: Vec::new(),
        keywords: Vec::new(),
        body_keywords: Vec::new(),
        links: Vec::new(),
        simhash: 0,
        term_frequencies: HashMap::new(),
        doc_length: 0,
        minhash: Vec::new(),
        section_fingerprints: vec![
            SectionFingerprint {
                heading: "Title".to_string(),
                level: 1,
                line_start: 1,
                line_end: 3,
                simhash: 0,
            },
            SectionFingerprint {
                heading: "Sub".to_string(),
                level: 2,
                line_start: 5,
                line_end: 6,
                simhash: 0,
            },
        ],
        adr_references: vec![],
    };
    let index = make_forward_index(vec![entry]);
    let sections = collect_sections_for_files(&[file_path_str], &index, "", 1);
    assert_eq!(sections.len(), 1);
}

#[test]
fn test_build_mcp_handle_is_stable() {
    let section = SectionMatch {
        doc_path: "docs/aa-auth.md".to_string(),
        heading: "Authentication Overview".to_string(),
        line_start: 1,
        line_end: 11,
        bm25_score: 0.25,
        content: "# Authentication Overview\n\nAuthentication flow".to_string(),
        canonicality: 0.5,
    };

    let left = build_mcp_handle("authentication", &section);
    let right = build_mcp_handle("authentication", &section);

    assert_eq!(left, right);
    assert!(left.starts_with("ctx_"));
}

#[test]
fn test_compute_index_fingerprint_deterministic() {
    let mut files = HashMap::new();
    files.insert(
        "docs/auth.md".to_string(),
        FileEntry {
            path: "docs/auth.md".to_string(),
            size_bytes: 100,
            line_count: 10,
            headings: Vec::new(),
            keywords: Vec::new(),
            body_keywords: Vec::new(),
            links: Vec::new(),
            simhash: 0,
            term_frequencies: HashMap::new(),
            doc_length: 0,
            minhash: Vec::new(),
            section_fingerprints: Vec::new(),
            adr_references: Vec::new(),
        },
    );

    let index = ForwardIndex {
        files,
        indexed_at: "2026-03-26T00:00:00Z".to_string(),
        version: 5,
        source_root: String::new(),
        avg_doc_length: 0.0,
        idf_map: HashMap::new(),
    };

    let left = compute_index_fingerprint(&index);
    let right = compute_index_fingerprint(&index);
    assert_eq!(left, right);
    assert!(left.starts_with("idx_"));
    assert_eq!(left.len(), 20); // "idx_" + 16 hex chars
}

#[test]
fn test_compute_index_fingerprint_changes_with_index() {
    let make_index = |indexed_at: &str| {
        let mut files = HashMap::new();
        files.insert(
            "docs/auth.md".to_string(),
            FileEntry {
                path: "docs/auth.md".to_string(),
                size_bytes: 100,
                line_count: 10,
                headings: Vec::new(),
                keywords: Vec::new(),
                body_keywords: Vec::new(),
                links: Vec::new(),
                simhash: 0,
                term_frequencies: HashMap::new(),
                doc_length: 0,
                minhash: Vec::new(),
                section_fingerprints: Vec::new(),
                adr_references: Vec::new(),
            },
        );
        ForwardIndex {
            files,
            indexed_at: indexed_at.to_string(),
            version: 5,
            source_root: String::new(),
            avg_doc_length: 0.0,
            idf_map: HashMap::new(),
        }
    };

    let fp1 = compute_index_fingerprint(&make_index("2026-03-26T00:00:00Z"));
    let fp2 = compute_index_fingerprint(&make_index("2026-03-27T00:00:00Z"));
    assert_ne!(fp1, fp2);
}

#[test]
fn test_generate_trace_id_format() {
    let id = generate_trace_id("test-seed");
    assert!(id.starts_with("trc_"));
    assert_eq!(id.len(), 20); // "trc_" + 16 hex chars
}
