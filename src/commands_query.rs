use crate::commands_audit::*;
use colored::Colorize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use crate::search::*;
use crate::types::*;
use crate::util::*;

pub(crate) struct QueryDiagnostics {
    pub tokens: Vec<String>,
    pub stems: Vec<String>,
    pub missing_terms: Vec<String>,
    pub idf_values: Vec<(String, String, f64)>,
    pub index_path: String,
    pub doc_count: usize,
}

pub(crate) fn build_query_diagnostics(
    parsed: &ParsedQuery,
    forward_index: &ForwardIndex,
    index_dir: &Path,
) -> QueryDiagnostics {
    let tokens = parsed.terms.clone();
    let stems: Vec<String> = tokens
        .iter()
        .map(|t| stem_word(&t.to_lowercase()))
        .collect();
    let mut missing_set: HashSet<String> = HashSet::new();
    let mut missing_terms = Vec::new();
    let mut idf_values = Vec::new();

    for term in &tokens {
        let stem = stem_word(&term.to_lowercase());
        let idf = *forward_index.idf_map.get(&stem).unwrap_or(&0.0);
        idf_values.push((term.clone(), stem.clone(), idf));
        if !forward_index.idf_map.contains_key(&stem) && missing_set.insert(term.clone()) {
            missing_terms.push(term.clone());
        }
    }

    QueryDiagnostics {
        tokens,
        stems,
        missing_terms,
        idf_values,
        index_path: index_dir.display().to_string(),
        doc_count: forward_index.files.len(),
    }
}

pub(crate) fn print_query_diagnostics(
    diagnostics: &QueryDiagnostics,
    include_scoring: bool,
    include_suggestions: bool,
) {
    println!("{}", "Diagnostics:".dimmed());
    println!(
        "  {} {}",
        "tokens:".dimmed(),
        if diagnostics.tokens.is_empty() {
            "(none)".to_string()
        } else {
            diagnostics.tokens.join(" ")
        }
    );
    println!(
        "  {} {}",
        "stems:".dimmed(),
        if diagnostics.stems.is_empty() {
            "(none)".to_string()
        } else {
            diagnostics.stems.join(" ")
        }
    );
    println!(
        "  {} {}",
        "missing:".dimmed(),
        if diagnostics.missing_terms.is_empty() {
            "(none)".to_string()
        } else {
            diagnostics.missing_terms.join(" ")
        }
    );
    println!(
        "  {} {} ({} docs)",
        "index:".dimmed(),
        diagnostics.index_path,
        diagnostics.doc_count
    );

    if include_scoring {
        let mut idf_parts = Vec::new();
        for (term, stem, idf) in &diagnostics.idf_values {
            idf_parts.push(format!("{term}->{stem}:{idf:.3}"));
        }
        println!(
            "  {} {}",
            "idf:".dimmed(),
            if idf_parts.is_empty() {
                "(none)".to_string()
            } else {
                idf_parts.join(", ")
            }
        );
        println!("  {} k1={:.2}, b={:.2}", "bm25:".dimmed(), BM25_K1, BM25_B);
    }

    if include_suggestions {
        println!(
            "  {} try fewer terms; use --no-stopwords; run yore stats; check index path",
            "suggestions:".dimmed()
        );
    }
}

pub(crate) struct QueryOptions {
    pub limit: usize,
    pub files_only: bool,
    pub json: bool,
    pub doc_terms: usize,
    pub explain: bool,
    pub require_phrases: bool,
    pub filter_stopwords: bool,
}

pub(crate) struct AssembleOptions {
    pub max_tokens: usize,
    pub max_sections: usize,
    pub depth: usize,
    pub format: String,
    pub doc_terms: usize,
    pub use_relations: bool,
}

pub(crate) struct HealthOptions {
    pub max_lines: usize,
    pub max_part_sections: usize,
    pub max_completed_lines: usize,
    pub max_changelog_entries: usize,
}

pub(crate) fn cmd_query(
    query: &str,
    index_dir: &Path,
    options: &QueryOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = parse_query(query, options.filter_stopwords);
    if parsed.terms.is_empty() {
        if options.json {
            let obj = serde_json::json!({
                "query": query,
                "error": "no_query_terms"
            });
            println!("{}", serde_json::to_string_pretty(&obj)?);
        } else {
            println!(
                "{}",
                "No searchable terms in query. Try different keywords or use --no-stopwords."
                    .yellow()
            );
        }
        return Ok(());
    }
    let _reverse_index = load_reverse_index(index_dir)?;
    let forward_index = load_forward_index(index_dir)?;
    let diagnostics = build_query_diagnostics(&parsed, &forward_index, index_dir);

    // Compute BM25 scores for all documents
    let mut file_scores: Vec<(String, f64)> = forward_index
        .files
        .iter()
        .map(|(path, entry)| {
            let score = bm25_score(
                &parsed.terms,
                entry,
                forward_index.avg_doc_length,
                &forward_index.idf_map,
            );
            (path.clone(), score)
        })
        .filter(|(_, score)| *score > 0.0)
        .collect();

    // Sort by BM25 score (descending)
    file_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let results = if parsed.phrases.is_empty() {
        file_scores.truncate(options.limit);
        file_scores
    } else {
        let candidate_cap = std::cmp::min(
            file_scores.len(),
            std::cmp::max(options.limit.saturating_mul(10), 100),
        );
        let mut candidates = file_scores[..candidate_cap].to_vec();

        for (path, score) in &mut candidates {
            let content = std::fs::read_to_string(Path::new(path)).unwrap_or_default();
            let content_terms = extract_keywords_with_options(&content, false);
            let mut matched_phrases = 0usize;

            for phrase in &parsed.phrases {
                if contains_phrase_tokens(&content_terms, &phrase.terms) {
                    matched_phrases += 1;
                }
            }

            if options.require_phrases && matched_phrases < parsed.phrases.len() {
                *score = 0.0;
            } else if matched_phrases > 0 {
                *score += matched_phrases as f64;
            }
        }

        if options.require_phrases {
            candidates.retain(|(_, score)| *score > 0.0);
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(options.limit);
        candidates
    };

    if options.json {
        let output: Vec<_> = results
            .iter()
            .map(|(path, score)| {
                let mut obj = serde_json::json!({
                    "path": path,
                    "score": score,
                    "query": query
                });
                if options.doc_terms > 0 {
                    if let Some(entry) = forward_index.files.get(path) {
                        let top_terms = get_top_doc_terms(
                            entry,
                            &forward_index.idf_map,
                            &parsed.terms,
                            options.doc_terms,
                        );
                        obj["doc_terms"] = serde_json::json!(top_terms);
                    }
                }
                obj
            })
            .collect();

        if options.explain {
            let notice = if output.is_empty() {
                Some("No data to explain.".to_string())
            } else {
                None
            };
            let diag_json = serde_json::json!({
                "tokens": diagnostics.tokens,
                "stems": diagnostics.stems,
                "missing_terms": diagnostics.missing_terms,
                "idf": diagnostics.idf_values.iter().map(|(term, stem, idf)| {
                    serde_json::json!({
                        "term": term,
                        "stem": stem,
                        "idf": idf
                    })
                }).collect::<Vec<_>>(),
                "bm25": {
                    "k1": BM25_K1,
                    "b": BM25_B,
                    "avg_doc_length": forward_index.avg_doc_length
                },
                "index_path": diagnostics.index_path,
                "doc_count": diagnostics.doc_count,
                "notice": notice,
                "suggestions": if output.is_empty() {
                    serde_json::json!(["try fewer terms", "use --no-stopwords", "run yore stats", "check index path"])
                } else {
                    serde_json::Value::Null
                }
            });
            let wrapped = serde_json::json!({
                "query": query,
                "results": output,
                "diagnostics": diag_json
            });
            println!("{}", serde_json::to_string_pretty(&wrapped)?);
        } else {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        return Ok(());
    }

    if results.is_empty() {
        println!("{}", "No results found.".yellow());
        if options.explain {
            println!("{}", "No data to explain.".dimmed());
        }
        print_query_diagnostics(&diagnostics, options.explain, true);
        return Ok(());
    }

    println!(
        "{} results for: {}\n",
        results.len().to_string().green().bold(),
        parsed.terms.join(" ").cyan()
    );

    for (file, score) in results {
        if options.files_only {
            println!("{file}");
        } else {
            println!("{} (score: {:.2})", file.cyan(), score);

            // Show doc terms if requested
            if options.doc_terms > 0 {
                if let Some(entry) = forward_index.files.get(&file) {
                    let top_terms = get_top_doc_terms(
                        entry,
                        &forward_index.idf_map,
                        &parsed.terms,
                        options.doc_terms,
                    );
                    if !top_terms.is_empty() {
                        println!("  {} {}", "terms:".dimmed(), top_terms.join(", "));
                    }
                }
            }

            // Show matching headings
            if let Some(entry) = forward_index.files.get(&file) {
                for heading in entry.headings.iter().take(3) {
                    let heading_keywords: HashSet<String> = extract_keywords(&heading.text)
                        .into_iter()
                        .map(|k| stem_word(&k))
                        .collect();

                    let matches: Vec<_> = parsed
                        .terms
                        .iter()
                        .filter(|t| heading_keywords.contains(&stem_word(&t.to_lowercase())))
                        .collect();

                    if !matches.is_empty() {
                        println!(
                            "  {} L{}: {}",
                            ">".dimmed(),
                            heading.line.to_string().dimmed(),
                            heading.text
                        );
                    }
                }
            }
            println!();
        }
    }

    if options.explain {
        print_query_diagnostics(&diagnostics, true, false);
    }

    Ok(())
}

pub(crate) fn cmd_similar(
    file: &Path,
    limit: usize,
    threshold: f64,
    json: bool,
    doc_terms: usize,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;

    // Get keywords for reference file - try multiple path formats
    let file_str = file.to_string_lossy().to_string();
    let file_with_dot = format!("./{}", file_str.trim_start_matches("./"));
    let file_without_dot = file_str.trim_start_matches("./").to_string();

    let (matched_path, ref_entry) = forward_index
        .files
        .get(&file_str)
        .map(|e| (file_str.clone(), e))
        .or_else(|| {
            forward_index
                .files
                .get(&file_with_dot)
                .map(|e| (file_with_dot.clone(), e))
        })
        .or_else(|| {
            forward_index
                .files
                .get(&file_without_dot)
                .map(|e| (file_without_dot.clone(), e))
        })
        .ok_or_else(|| format!("File not in index: {file_str}"))?;

    // Combine heading and body keywords
    let ref_keywords: HashSet<String> = ref_entry
        .keywords
        .iter()
        .chain(ref_entry.body_keywords.iter())
        .map(|k| k.to_lowercase())
        .collect();

    // For doc_terms, exclude the reference file's terms
    let ref_terms_vec: Vec<String> = ref_entry
        .body_keywords
        .iter()
        .chain(ref_entry.keywords.iter())
        .map(|k| k.to_lowercase())
        .collect();

    // Compare with all other files using both Jaccard and Simhash
    let mut similarities: Vec<(String, f64, f64, f64)> = Vec::new(); // (path, jaccard, simhash, combined)

    for (path, entry) in &forward_index.files {
        if path == &matched_path {
            continue;
        }

        let other_keywords: HashSet<String> = entry
            .keywords
            .iter()
            .chain(entry.body_keywords.iter())
            .map(|k| k.to_lowercase())
            .collect();

        let jaccard = jaccard_similarity(&ref_keywords, &other_keywords);
        let simhash_sim = simhash_similarity(ref_entry.simhash, entry.simhash);

        // Combined score: weighted average
        let combined = jaccard * 0.6 + simhash_sim * 0.4;

        if combined >= threshold {
            similarities.push((path.clone(), jaccard, simhash_sim, combined));
        }
    }

    // Sort by combined similarity
    similarities.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());
    similarities.truncate(limit);

    if json {
        let output: Vec<_> = similarities
            .iter()
            .map(|(p, j, s, c)| {
                let mut obj = serde_json::json!({
                    "path": p,
                    "jaccard": j,
                    "simhash": s,
                    "combined": c
                });
                if doc_terms > 0 {
                    if let Some(entry) = forward_index.files.get(p) {
                        let top_terms = get_top_doc_terms(
                            entry,
                            &forward_index.idf_map,
                            &ref_terms_vec,
                            doc_terms,
                        );
                        obj["doc_terms"] = serde_json::json!(top_terms);
                    }
                }
                obj
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if similarities.is_empty() {
        println!("{}", "No similar files found.".yellow());
        return Ok(());
    }

    println!("Files similar to: {}\n", matched_path.cyan());
    println!("{:>5} {:>5} {:>5}  Path", "Comb", "Jacc", "Sim");
    println!("{}", "-".repeat(60));

    for (path, jaccard, simhash_sim, combined) in similarities {
        let comb_pct = (combined * 100.0) as u32;
        let jacc_pct = (jaccard * 100.0) as u32;
        let sim_pct = (simhash_sim * 100.0) as u32;
        println!(
            "{:>4}% {:>4}% {:>4}%  {}",
            comb_pct.to_string().green(),
            jacc_pct.to_string().cyan(),
            sim_pct.to_string().yellow(),
            path
        );

        // Show doc terms if requested
        if doc_terms > 0 {
            if let Some(entry) = forward_index.files.get(&path) {
                let top_terms =
                    get_top_doc_terms(entry, &forward_index.idf_map, &ref_terms_vec, doc_terms);
                if !top_terms.is_empty() {
                    println!(
                        "                   {} {}",
                        "terms:".dimmed(),
                        top_terms.join(", ")
                    );
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn cmd_dupes(
    threshold: f64,
    group: bool,
    json: bool,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let start = Instant::now();

    // Build LSH buckets for fast duplicate detection
    let buckets = lsh_buckets(&forward_index.files, 16); // 16 bands x 8 rows = 128 hashes
    let mut candidates: HashSet<(String, String)> = HashSet::new();

    // Collect candidate pairs from buckets
    for paths in buckets.values() {
        if paths.len() > 1 {
            for i in 0..paths.len() {
                for j in (i + 1)..paths.len() {
                    let (p1, p2) = if paths[i] < paths[j] {
                        (paths[i].clone(), paths[j].clone())
                    } else {
                        (paths[j].clone(), paths[i].clone())
                    };
                    candidates.insert((p1, p2));
                }
            }
        }
    }

    let mut duplicates: Vec<(String, String, f64, f64, f64, f64)> = Vec::new(); // (path1, path2, jaccard, simhash, minhash, combined)

    // Compare candidate pairs
    for (path1, path2) in &candidates {
        if let (Some(entry1), Some(entry2)) = (
            forward_index.files.get(path1),
            forward_index.files.get(path2),
        ) {
            let kw1: HashSet<String> = entry1
                .keywords
                .iter()
                .chain(entry1.body_keywords.iter())
                .map(|k| k.to_lowercase())
                .collect();
            let kw2: HashSet<String> = entry2
                .keywords
                .iter()
                .chain(entry2.body_keywords.iter())
                .map(|k| k.to_lowercase())
                .collect();

            let jaccard = jaccard_similarity(&kw1, &kw2);
            let simhash_sim = simhash_similarity(entry1.simhash, entry2.simhash);
            let minhash_sim = minhash_similarity(&entry1.minhash, &entry2.minhash);
            let combined = jaccard * 0.4 + simhash_sim * 0.3 + minhash_sim * 0.3;

            if combined >= threshold {
                duplicates.push((
                    path1.clone(),
                    path2.clone(),
                    jaccard,
                    simhash_sim,
                    minhash_sim,
                    combined,
                ));
            }
        }
    }

    let elapsed = start.elapsed();

    // Sort by combined similarity
    duplicates.sort_by(|a, b| b.5.partial_cmp(&a.5).unwrap_or(std::cmp::Ordering::Equal));

    if json {
        let output: Vec<_> = duplicates
            .iter()
            .map(|(p1, p2, j, s, m, c)| {
                serde_json::json!({
                    "file1": p1,
                    "file2": p2,
                    "jaccard": j,
                    "simhash": s,
                    "minhash": m,
                    "combined": c
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if duplicates.is_empty() {
        println!("{}", "No duplicates found above threshold.".green());
        eprintln!(
            "LSH duplicate detection: {:?} ({} candidate pairs from {} buckets)",
            elapsed,
            candidates.len(),
            buckets.len()
        );
        return Ok(());
    }

    println!(
        "{} duplicate pairs found (threshold: {}%)",
        duplicates.len().to_string().yellow().bold(),
        (threshold * 100.0) as u32
    );
    eprintln!(
        "LSH duplicate detection: {:?} ({} candidates from {} buckets)\n",
        elapsed,
        candidates.len(),
        buckets.len()
    );

    if group {
        // Group duplicates
        let mut groups: HashMap<String, Vec<(String, f64)>> = HashMap::new();

        for (path1, path2, _, _, _, combined) in &duplicates {
            let group = groups.entry(path1.clone()).or_default();
            if !group.iter().any(|(p, _)| p == path2) {
                group.push((path2.clone(), *combined));
            }
        }

        for (file, related) in groups {
            println!("{}", file.cyan());
            for (r, sim) in related {
                println!("  {} {}% {}", "~".dimmed(), (sim * 100.0) as u32, r);
            }
            println!();
        }
    } else {
        for (path1, path2, jaccard, simhash_sim, minhash_sim, combined) in
            duplicates.iter().take(50)
        {
            let comb_pct = (combined * 100.0) as u32;
            println!(
                "{}% [J:{}% S:{}% M:{}%] {} <-> {}",
                comb_pct.to_string().yellow(),
                (jaccard * 100.0) as u32,
                (simhash_sim * 100.0) as u32,
                (minhash_sim * 100.0) as u32,
                path1.cyan(),
                path2
            );
        }

        if duplicates.len() > 50 {
            println!(
                "\n{}",
                format!("... and {} more", duplicates.len() - 50).dimmed()
            );
        }
    }

    Ok(())
}

pub(crate) fn compute_duplicate_pairs(
    forward_index: &ForwardIndex,
    threshold: f64,
) -> Vec<(String, String, f64)> {
    // Build LSH buckets for duplicate detection
    let buckets = lsh_buckets(&forward_index.files, 16); // 16 bands x 8 rows = 128 hashes
    let mut candidates: HashSet<(String, String)> = HashSet::new();

    // Collect candidate pairs from buckets
    for paths in buckets.values() {
        if paths.len() > 1 {
            for i in 0..paths.len() {
                for j in (i + 1)..paths.len() {
                    let (p1, p2) = if paths[i] < paths[j] {
                        (paths[i].clone(), paths[j].clone())
                    } else {
                        (paths[j].clone(), paths[i].clone())
                    };
                    candidates.insert((p1, p2));
                }
            }
        }
    }

    let mut pairs: Vec<(String, String, f64)> = Vec::new(); // (path1, path2, combined)

    for (path1, path2) in &candidates {
        if let (Some(entry1), Some(entry2)) = (
            forward_index.files.get(path1),
            forward_index.files.get(path2),
        ) {
            let kw1: HashSet<String> = entry1
                .keywords
                .iter()
                .chain(entry1.body_keywords.iter())
                .map(|k| k.to_lowercase())
                .collect();
            let kw2: HashSet<String> = entry2
                .keywords
                .iter()
                .chain(entry2.body_keywords.iter())
                .map(|k| k.to_lowercase())
                .collect();

            let jaccard = jaccard_similarity(&kw1, &kw2);
            let simhash_sim = simhash_similarity(entry1.simhash, entry2.simhash);
            let minhash_sim = minhash_similarity(&entry1.minhash, &entry2.minhash);
            let combined = jaccard * 0.4 + simhash_sim * 0.3 + minhash_sim * 0.3;

            if combined >= threshold {
                pairs.push((path1.clone(), path2.clone(), combined));
            }
        }
    }

    // Sort descending by similarity for stable output
    pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    pairs
}

pub(crate) fn build_consolidation_groups(
    forward_index: &ForwardIndex,
    pairs: &[(String, String, f64)],
) -> ConsolidationResult {
    use std::cmp::Ordering;

    // Build adjacency graph
    let mut adj: HashMap<String, HashSet<String>> = HashMap::new();
    let mut pair_sims: HashMap<(String, String), f64> = HashMap::new();

    for (a, b, sim) in pairs {
        adj.entry(a.clone()).or_default().insert(b.clone());
        adj.entry(b.clone()).or_default().insert(a.clone());

        let key = if a <= b {
            (a.clone(), b.clone())
        } else {
            (b.clone(), a.clone())
        };
        pair_sims.insert(key, *sim);
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut groups: Vec<ConsolidationGroup> = Vec::new();

    for start in adj.keys() {
        if visited.contains(start) {
            continue;
        }

        // BFS/DFS to collect connected component
        let mut stack = vec![start.clone()];
        let mut component: Vec<String> = Vec::new();

        while let Some(node) = stack.pop() {
            if !visited.insert(node.clone()) {
                continue;
            }
            component.push(node.clone());
            if let Some(neighbors) = adj.get(&node) {
                for n in neighbors {
                    if !visited.contains(n) {
                        stack.push(n.clone());
                    }
                }
            }
        }

        if component.len() < 2 {
            continue;
        }

        // Choose canonical doc via canonicality score
        component.sort(); // deterministic order
        let mut best: Option<(String, f64)> = None;
        for path in &component {
            if let Some(entry) = forward_index.files.get(path) {
                let (score, _reasons) = score_canonicality_with_reasons(path, entry);
                match best {
                    None => best = Some((path.clone(), score)),
                    Some((_, best_score)) => {
                        if score > best_score
                            || (score == best_score
                                && path.cmp(&best.as_ref().unwrap().0) == Ordering::Less)
                        {
                            best = Some((path.clone(), score));
                        }
                    }
                }
            }
        }

        let Some((canonical, canonical_score)) = best else {
            continue;
        };

        let mut merge_into: Vec<String> = component
            .iter()
            .filter(|p| *p != &canonical)
            .cloned()
            .collect();
        if merge_into.is_empty() {
            continue;
        }

        merge_into.sort();

        // Compute average similarity between canonical and others
        let mut total_sim = 0.0;
        let mut count = 0usize;
        for other in &merge_into {
            let key = if &canonical <= other {
                (canonical.clone(), other.clone())
            } else {
                (other.clone(), canonical.clone())
            };
            if let Some(sim) = pair_sims.get(&key) {
                total_sim += *sim;
                count += 1;
            }
        }
        let avg_similarity = if count > 0 {
            total_sim / (count as f64)
        } else {
            0.0
        };

        let note = format!(
            "Merge {} file(s) into canonical {}",
            merge_into.len(),
            canonical
        );

        groups.push(ConsolidationGroup {
            canonical,
            merge_into,
            canonical_score,
            avg_similarity,
            note,
        });
    }

    // Stable ordering: sort by canonical path
    groups.sort_by(|a, b| a.canonical.cmp(&b.canonical));

    ConsolidationResult {
        total_groups: groups.len(),
        groups,
    }
}

/// NEW: Show what's shared between two files
pub(crate) fn cmd_diff(
    file1: &Path,
    file2: &Path,
    index_dir: &Path,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;

    // Resolve paths
    let resolve_path = |f: &Path| -> Option<(String, &FileEntry)> {
        let s = f.to_string_lossy().to_string();
        let with_dot = format!("./{}", s.trim_start_matches("./"));
        let without_dot = s.trim_start_matches("./").to_string();

        forward_index
            .files
            .get(&s)
            .map(|e| (s.clone(), e))
            .or_else(|| {
                forward_index
                    .files
                    .get(&with_dot)
                    .map(|e| (with_dot.clone(), e))
            })
            .or_else(|| {
                forward_index
                    .files
                    .get(&without_dot)
                    .map(|e| (without_dot, e))
            })
    };

    let (path1, entry1) =
        resolve_path(file1).ok_or_else(|| format!("File not in index: {}", file1.display()))?;
    let (path2, entry2) =
        resolve_path(file2).ok_or_else(|| format!("File not in index: {}", file2.display()))?;

    // Compute similarities
    let kw1: HashSet<String> = entry1
        .keywords
        .iter()
        .chain(entry1.body_keywords.iter())
        .map(|k| k.to_lowercase())
        .collect();
    let kw2: HashSet<String> = entry2
        .keywords
        .iter()
        .chain(entry2.body_keywords.iter())
        .map(|k| k.to_lowercase())
        .collect();

    let shared: HashSet<_> = kw1.intersection(&kw2).cloned().collect();
    let only_in_1: HashSet<_> = kw1.difference(&kw2).cloned().collect();
    let only_in_2: HashSet<_> = kw2.difference(&kw1).cloned().collect();

    let jaccard = jaccard_similarity(&kw1, &kw2);
    let simhash_sim = simhash_similarity(entry1.simhash, entry2.simhash);
    let combined = jaccard * 0.6 + simhash_sim * 0.4;

    // Show shared headings
    let h1: HashSet<String> = entry1
        .headings
        .iter()
        .map(|h| h.text.to_lowercase())
        .collect();
    let h2: HashSet<String> = entry2
        .headings
        .iter()
        .map(|h| h.text.to_lowercase())
        .collect();
    let shared_headings: Vec<String> = h1.intersection(&h2).cloned().collect();

    if json {
        let mut shared_vec: Vec<_> = shared.iter().cloned().collect();
        shared_vec.sort();
        let mut only1_vec: Vec<_> = only_in_1.iter().cloned().collect();
        only1_vec.sort();
        let mut only2_vec: Vec<_> = only_in_2.iter().cloned().collect();
        only2_vec.sort();
        let mut headings_vec = shared_headings.clone();
        headings_vec.sort();

        let result = DiffResult {
            file1: path1,
            file2: path2,
            similarity: DiffSimilarity {
                combined,
                jaccard,
                simhash: simhash_sim,
            },
            shared_keywords: shared_vec,
            only_in_file1: only1_vec,
            only_in_file2: only2_vec,
            shared_headings: headings_vec,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("{}", "Comparison".green().bold());
    println!();
    println!("  File 1: {}", path1.cyan());
    println!("  File 2: {}", path2.cyan());
    println!();
    println!("{}", "Similarity Scores".green().bold());
    println!();
    println!("  Combined:    {}%", (combined * 100.0) as u32);
    println!(
        "  Jaccard:     {}% (keyword overlap)",
        (jaccard * 100.0) as u32
    );
    println!(
        "  SimHash:     {}% (content structure)",
        (simhash_sim * 100.0) as u32
    );
    println!();

    println!(
        "{} ({} keywords)",
        "Shared Keywords".green().bold(),
        shared.len()
    );
    let mut shared_vec: Vec<_> = shared.iter().collect();
    shared_vec.sort();
    for chunk in shared_vec.chunks(8) {
        println!(
            "  {}",
            chunk
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    println!();
    println!(
        "{} ({} keywords)",
        format!("Only in {}", path1.split('/').next_back().unwrap_or(&path1))
            .yellow()
            .bold(),
        only_in_1.len()
    );
    let mut only1_vec: Vec<_> = only_in_1.iter().take(24).collect();
    only1_vec.sort();
    for chunk in only1_vec.chunks(8) {
        println!(
            "  {}",
            chunk
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if only_in_1.len() > 24 {
        println!("  ... and {} more", only_in_1.len() - 24);
    }

    println!();
    println!(
        "{} ({} keywords)",
        format!("Only in {}", path2.split('/').next_back().unwrap_or(&path2))
            .yellow()
            .bold(),
        only_in_2.len()
    );
    let mut only2_vec: Vec<_> = only_in_2.iter().take(24).collect();
    only2_vec.sort();
    for chunk in only2_vec.chunks(8) {
        println!(
            "  {}",
            chunk
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if only_in_2.len() > 24 {
        println!("  ... and {} more", only_in_2.len() - 24);
    }

    if !shared_headings.is_empty() {
        println!();
        println!(
            "{} ({} headings)",
            "Identical Headings".red().bold(),
            shared_headings.len()
        );
        for h in shared_headings.iter().take(10) {
            println!("  - {h}");
        }
        if shared_headings.len() > 10 {
            println!("  ... and {} more", shared_headings.len() - 10);
        }
    }

    Ok(())
}

/// Find duplicate sections across documents
pub(crate) fn cmd_dupes_sections(
    threshold: f64,
    min_files: usize,
    json: bool,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let start = Instant::now();

    // Collect all sections from all files
    #[derive(Debug, Clone)]
    struct SectionInfo {
        file_path: String,
        heading: String,
        line_start: usize,
        line_end: usize,
        simhash: u64,
    }

    let mut all_sections: Vec<SectionInfo> = Vec::new();
    for (path, entry) in &forward_index.files {
        for section in &entry.section_fingerprints {
            all_sections.push(SectionInfo {
                file_path: path.clone(),
                heading: section.heading.clone(),
                line_start: section.line_start,
                line_end: section.line_end,
                simhash: section.simhash,
            });
        }
    }

    if all_sections.is_empty() {
        println!("{}", "No sections found in indexed files.".yellow());
        return Ok(());
    }

    // Group similar sections using SimHash similarity
    #[derive(Debug)]
    struct SectionCluster {
        heading: String,
        files: Vec<(String, f64, usize, usize)>, // (file_path, similarity, line_start, line_end)
        avg_simhash: u64,
    }

    let mut clusters: Vec<SectionCluster> = Vec::new();

    for section in &all_sections {
        let mut best_cluster_idx: Option<usize> = None;
        let mut best_similarity = 0.0;

        // Find best matching cluster
        for (cluster_idx, cluster) in clusters.iter().enumerate() {
            let similarity = simhash_similarity(section.simhash, cluster.avg_simhash);
            if similarity >= threshold && similarity > best_similarity {
                best_similarity = similarity;
                best_cluster_idx = Some(cluster_idx);
            }
        }

        if let Some(cluster_idx) = best_cluster_idx {
            // Add to existing cluster
            clusters[cluster_idx].files.push((
                section.file_path.clone(),
                best_similarity,
                section.line_start,
                section.line_end,
            ));
        } else {
            // Create new cluster
            clusters.push(SectionCluster {
                heading: section.heading.clone(),
                files: vec![(
                    section.file_path.clone(),
                    1.0,
                    section.line_start,
                    section.line_end,
                )],
                avg_simhash: section.simhash,
            });
        }
    }

    let elapsed = start.elapsed();

    // Filter clusters by min_files threshold
    let duplicate_clusters: Vec<_> = clusters
        .into_iter()
        .filter(|c| c.files.len() >= min_files)
        .collect();

    if duplicate_clusters.is_empty() {
        println!(
            "{}",
            format!(
                "No duplicate sections found with {} or more files at {}% threshold.",
                min_files,
                (threshold * 100.0) as u32
            )
            .green()
        );
        eprintln!(
            "Section analysis: {:?} ({} sections analyzed)",
            elapsed,
            all_sections.len()
        );
        return Ok(());
    }

    // Sort clusters by number of files (descending)
    let mut sorted_clusters = duplicate_clusters;
    sorted_clusters.sort_by(|a, b| b.files.len().cmp(&a.files.len()));

    if json {
        let output: Vec<_> = sorted_clusters
            .iter()
            .map(|cluster| {
                serde_json::json!({
                    "heading": cluster.heading,
                    "file_count": cluster.files.len(),
                    "files": cluster.files.iter().map(|(path, sim, start, end)| {
                        serde_json::json!({
                            "path": path,
                            "similarity": sim,
                            "line_start": start,
                            "line_end": end,
                        })
                    }).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!(
        "{} duplicate section clusters found (threshold: {}%, min files: {})",
        sorted_clusters.len().to_string().yellow().bold(),
        (threshold * 100.0) as u32,
        min_files
    );
    eprintln!(
        "Section analysis: {:?} ({} sections analyzed)\n",
        elapsed,
        all_sections.len()
    );

    for cluster in sorted_clusters.iter().take(20) {
        println!(
            "{} {} ({} files)",
            "Section:".cyan().bold(),
            cluster.heading.yellow(),
            cluster.files.len()
        );

        for (path, similarity, line_start, line_end) in &cluster.files {
            let sim_pct = (similarity * 100.0) as u32;
            println!(
                "  {}% {}:{}-{}",
                sim_pct.to_string().dimmed(),
                path,
                line_start,
                line_end
            );
        }
        println!();
    }

    if sorted_clusters.len() > 20 {
        println!(
            "{}",
            format!(
                "... and {} more section clusters",
                sorted_clusters.len() - 20
            )
            .dimmed()
        );
    }

    Ok(())
}
