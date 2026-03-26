use colored::Colorize;
use ignore::WalkBuilder;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::assemble::extract_relations;
use crate::search::*;
use crate::types::*;
use crate::util::*;

#[allow(clippy::too_many_arguments)]
pub fn cmd_build(
    path: &Path,
    output: &Path,
    types: &str,
    exclude: &[String],
    quiet: bool,
    roots: Option<&[PathBuf]>,
    json: bool,
    track_renames: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let source_root = canonicalize_existing_path(&std::env::current_dir()?);

    if !quiet && !json {
        println!("{} {}", "Indexing".cyan().bold(), path.display());
    }

    // Parse file types
    let extensions: HashSet<String> = types.split(',').map(|s| s.trim().to_lowercase()).collect();

    // Build walker with ignore patterns
    let mut builder = WalkBuilder::new(path);
    builder.hidden(true).git_ignore(true).git_global(true);

    // Add custom excludes
    for pattern in exclude {
        builder.add_ignore(Path::new(pattern));
    }

    // Collect files
    let mut forward_index = ForwardIndex {
        files: HashMap::new(),
        indexed_at: chrono_now(),
        version: 4, // Version 4 adds source_root metadata for portable file resolution
        source_root: source_root.to_string_lossy().to_string(),
        avg_doc_length: 0.0,
        idf_map: HashMap::new(),
    };

    let mut reverse_index = ReverseIndex {
        keywords: HashMap::new(),
    };
    let mut document_metrics_index = DocumentMetricsIndex {
        indexed_at: chrono_now(),
        version: 1,
        files: HashMap::new(),
    };

    let mut file_count = 0;
    let mut total_headings = 0;
    let mut total_links = 0;

    for entry in builder.build().filter_map(std::result::Result::ok) {
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // If roots are configured, skip files outside those roots
        if let Some(root_list) = roots {
            let mut inside_any_root = false;
            for root in root_list {
                if path.starts_with(root) {
                    inside_any_root = true;
                    break;
                }
            }
            if !inside_any_root {
                continue;
            }
        }

        // Check extension
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();

        if !extensions.contains(&ext) {
            continue;
        }

        // Skip common non-content directories
        let path_str = path.to_string_lossy();
        if path_str.contains("node_modules")
            || path_str.contains(".git/")
            || path_str.contains("target/")
            || path_str.contains("vendor/")
            || path_str.contains("venv/")
            || path_str.contains("__pycache__")
        {
            continue;
        }

        // Index the file
        if let Ok((mut entry, mut metrics)) = index_file(path) {
            let physical_path = canonicalize_existing_path(path);
            let rel_path = build_indexed_doc_key(&physical_path, &source_root);
            entry.path = physical_path.to_string_lossy().to_string();
            metrics.path.clone_from(&rel_path);

            // Update reverse index with heading keywords
            for keyword in &entry.keywords {
                let stemmed = stem_word(&keyword.to_lowercase());
                reverse_index
                    .keywords
                    .entry(stemmed)
                    .or_default()
                    .push(ReverseEntry {
                        file: rel_path.clone(),
                        line: None,
                        heading: None,
                        level: None,
                    });
            }

            // Update reverse index with body keywords
            for keyword in &entry.body_keywords {
                let stemmed = stem_word(&keyword.to_lowercase());
                reverse_index
                    .keywords
                    .entry(stemmed)
                    .or_default()
                    .push(ReverseEntry {
                        file: rel_path.clone(),
                        line: None,
                        heading: None,
                        level: None,
                    });
            }

            for heading in &entry.headings {
                let words = extract_keywords(&heading.text);
                for word in words {
                    let stemmed = stem_word(&word.to_lowercase());
                    reverse_index
                        .keywords
                        .entry(stemmed)
                        .or_default()
                        .push(ReverseEntry {
                            file: rel_path.clone(),
                            line: Some(heading.line),
                            heading: Some(heading.text.clone()),
                            level: Some(heading.level),
                        });
                }
            }

            total_headings += entry.headings.len();
            total_links += entry.links.len();
            file_count += 1;

            document_metrics_index
                .files
                .insert(rel_path.clone(), metrics);
            forward_index.files.insert(rel_path, entry);
        }
    }

    // Compute BM25 statistics (IDF and average document length)
    let total_docs = forward_index.files.len() as f64;
    let mut doc_frequencies: HashMap<String, usize> = HashMap::new();
    let mut total_length = 0;

    // Compute document frequencies
    for entry in forward_index.files.values() {
        total_length += entry.doc_length;
        for term in entry.term_frequencies.keys() {
            *doc_frequencies.entry(term.clone()).or_insert(0) += 1;
        }
    }

    // Compute IDF scores (with floor to handle high-frequency terms)
    let mut idf_map: HashMap<String, f64> = HashMap::new();
    for (term, df) in doc_frequencies {
        // Standard BM25 IDF can go negative when df > 50% of docs.
        // We floor at a small positive value so common terms still contribute.
        let idf = ((total_docs - df as f64 + 0.5) / (df as f64 + 0.5))
            .ln()
            .max(0.1);
        idf_map.insert(term, idf);
    }

    forward_index.avg_doc_length = if total_docs > 0.0 {
        total_length as f64 / total_docs
    } else {
        0.0
    };
    forward_index.idf_map = idf_map;

    // Create output directory
    fs::create_dir_all(output)?;

    // Write indexes
    let forward_path = output.join("forward_index.json");
    let reverse_path = output.join("reverse_index.json");
    let stats_path = output.join("stats.json");
    let metrics_path = output.join("document_metrics.json");

    fs::write(&forward_path, serde_json::to_string_pretty(&forward_index)?)?;
    fs::write(&reverse_path, serde_json::to_string_pretty(&reverse_index)?)?;
    fs::write(
        &metrics_path,
        serde_json::to_string_pretty(&document_metrics_index)?,
    )?;

    let stats = IndexStats {
        total_files: file_count,
        total_keywords: reverse_index.keywords.len(),
        total_headings,
        total_links,
        indexed_at: chrono_now(),
    };
    fs::write(&stats_path, serde_json::to_string_pretty(&stats)?)?;

    // Extract and persist relation edges
    let relation_index = extract_relations(&forward_index);
    let relations_count = relation_index.total_edges;
    let relations_path = output.join("relations.json");
    fs::write(
        &relations_path,
        serde_json::to_string_pretty(&relation_index)?,
    )?;

    // Track git renames if requested
    let renames_count = if track_renames {
        if !quiet && !json {
            println!("  Extracting git rename history...");
        }
        let rename_history = extract_git_renames(path);
        let count = rename_history.renames.len();
        let rename_path = output.join("rename_history.json");
        fs::write(&rename_path, serde_json::to_string_pretty(&rename_history)?)?;
        if !quiet && !json {
            println!("  Tracked {count} file renames");
        }
        Some(count)
    } else {
        None
    };

    let elapsed = start.elapsed();

    if json {
        let result = BuildResult {
            index_path: output.to_string_lossy().to_string(),
            files_indexed: file_count,
            total_headings,
            total_links,
            unique_keywords: reverse_index.keywords.len(),
            duration_ms: elapsed.as_millis(),
            renames_tracked: renames_count,
            total_relations: Some(relations_count),
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if !quiet {
        println!();
        println!("{}", "Index Statistics".green().bold());
        println!("  Files indexed:    {}", file_count.to_string().cyan());
        println!(
            "  Unique keywords:  {}",
            reverse_index.keywords.len().to_string().cyan()
        );
        println!("  Total headings:   {}", total_headings.to_string().cyan());
        println!("  Total links:      {}", total_links.to_string().cyan());
        println!("  Relations:        {}", relations_count.to_string().cyan());
        println!("  Time elapsed:     {elapsed:.2?}");
        println!();
        println!(
            "{} {}",
            "Indexes written to".green(),
            output.display().to_string().cyan()
        );
    }

    Ok(())
}

pub fn index_file(path: &Path) -> Result<(FileEntry, DocumentMetrics), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let metadata = fs::metadata(path)?;

    let lines: Vec<&str> = content.lines().collect();
    let line_count = lines.len();

    // Extract headings (markdown)
    let heading_re = Regex::new(r"^(#{1,6})\s+(.+)$")?;
    let mut headings = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = heading_re.captures(line) {
            headings.push(Heading {
                line: i + 1,
                level: caps.get(1).map_or(1, |m| m.as_str().len()),
                text: caps
                    .get(2)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
            });
        }
    }

    // Extract links
    let link_re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)")?;
    let mut links = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        for caps in link_re.captures_iter(line) {
            links.push(Link {
                line: i + 1,
                text: caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
                target: caps
                    .get(2)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
            });
        }
    }

    // Extract keywords from headings
    let mut keywords: HashSet<String> = HashSet::new();
    for heading in &headings {
        for kw in extract_keywords(&heading.text) {
            keywords.insert(stem_word(&kw));
        }
    }

    // NEW: Extract keywords from full body text
    let mut body_keywords: HashSet<String> = HashSet::new();
    for line in &lines {
        // Skip code blocks
        if line.starts_with("```") || line.starts_with("    ") {
            continue;
        }
        for kw in extract_keywords(line) {
            body_keywords.insert(stem_word(&kw));
        }
    }
    // Remove heading keywords from body to avoid duplication
    for kw in &keywords {
        body_keywords.remove(kw);
    }

    // NEW: Compute term frequencies for BM25
    let mut term_frequencies: HashMap<String, usize> = HashMap::new();
    let mut total_terms = 0;

    for line in &lines {
        // Skip code blocks
        if line.starts_with("```") || line.starts_with("    ") {
            continue;
        }
        let words = extract_keywords(line);
        for word in words {
            let stemmed = stem_word(&word);
            *term_frequencies.entry(stemmed).or_insert(0) += 1;
            total_terms += 1;
        }
    }

    // NEW: Compute MinHash signature
    let all_keywords: Vec<String> = keywords
        .iter()
        .chain(body_keywords.iter())
        .cloned()
        .collect();
    let minhash = compute_minhash(&all_keywords, 128);

    // NEW: Compute section-level SimHash fingerprints
    let section_fingerprints = index_sections(&content, &headings);
    let metrics =
        compute_document_metrics(&path.to_string_lossy(), &content, &lines, &headings, &links);

    // Compute simhash fingerprint
    let simhash = compute_simhash(&content);

    // Extract ADR references from content
    let adr_regex = Regex::new(r"\bADR[-_ ]?(\d{2,4})\b").unwrap();
    let mut adr_references = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        for caps in adr_regex.captures_iter(line) {
            if let Some(num_match) = caps.get(1) {
                let num_val: usize = num_match.as_str().parse().unwrap_or(0);
                adr_references.push(AdrRef {
                    line: i + 1,
                    raw_text: caps.get(0).unwrap().as_str().to_string(),
                    normalized_id: format!("{num_val:03}"),
                });
            }
        }
    }

    Ok((
        FileEntry {
            path: path.to_string_lossy().to_string(),
            size_bytes: metadata.len(),
            line_count,
            headings,
            keywords: keywords.into_iter().collect(),
            body_keywords: body_keywords.into_iter().collect(),
            links,
            simhash,
            term_frequencies,
            doc_length: total_terms,
            minhash,
            section_fingerprints,
            adr_references,
        },
        metrics,
    ))
}

pub fn compute_document_metrics(
    path: &str,
    content: &str,
    lines: &[&str],
    headings: &[Heading],
    links: &[Link],
) -> DocumentMetrics {
    let word_re = Regex::new(r"[A-Za-z0-9_][A-Za-z0-9_-]*").unwrap();
    let list_re = Regex::new(r"^(\s*[-+*]\s+|\s*\d+\.\s+)").unwrap();
    let metadata_re =
        Regex::new(r"^(?:\*\*[^*]+\*\*|[A-Za-z][A-Za-z0-9 _/\-]{1,40}):\s+\S").unwrap();

    let mut h1_count = 0;
    let mut h2_count = 0;
    let mut h3_count = 0;
    let mut h4_plus_count = 0;
    let mut part_heading_count = 0;
    let mut completion_heading_count = 0;
    let mut changelog_heading_count = 0;

    for heading in headings {
        match heading.level {
            1 => h1_count += 1,
            2 => h2_count += 1,
            3 => h3_count += 1,
            _ => h4_plus_count += 1,
        }
        if heading_looks_like_part(&heading.text) {
            part_heading_count += 1;
        }
        if heading_has_completion_marker(&heading.text) {
            completion_heading_count += 1;
        }
        if heading_looks_like_changelog(&heading.text) {
            changelog_heading_count += 1;
        }
    }

    let code_block_count = count_code_blocks(lines);
    let list_item_count = lines
        .iter()
        .filter(|line| list_re.is_match(line.trim_end()))
        .count();
    let table_row_count = lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.matches('|').count() >= 2
        })
        .count();
    let word_count = word_re.find_iter(content).count();
    let (frontmatter_key_count, metadata_scan_start) = extract_frontmatter_key_count(lines);
    let metadata_line_count = lines
        .iter()
        .enumerate()
        .skip(metadata_scan_start)
        .take_while(|(_, line)| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .filter(|(_, line)| metadata_re.is_match(line.trim()))
        .count();

    let sections = compute_section_metrics(lines, headings, links);
    let longest_section_lines = sections
        .iter()
        .map(|section| section.line_count)
        .max()
        .unwrap_or(0);
    let changelog_entry_count = sections
        .iter()
        .filter(|section| section.looks_like_changelog)
        .map(|section| section.list_item_count)
        .sum();

    DocumentMetrics {
        path: path.to_string(),
        line_count: lines.len(),
        word_count,
        heading_count: headings.len(),
        section_count: sections.len(),
        link_count: links.len(),
        h1_count,
        h2_count,
        h3_count,
        h4_plus_count,
        code_block_count,
        list_item_count,
        table_row_count,
        frontmatter_key_count,
        metadata_line_count,
        part_heading_count,
        completion_heading_count,
        changelog_heading_count,
        changelog_entry_count,
        longest_section_lines,
        sections,
    }
}

pub fn extract_frontmatter_key_count(lines: &[&str]) -> (usize, usize) {
    if lines.first().map(|line| line.trim()) != Some("---") {
        return (0, 0);
    }

    let mut key_count = 0;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        let trimmed = line.trim();
        if trimmed == "---" {
            return (key_count, idx + 1);
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.contains(':') {
            key_count += 1;
        }
    }

    (0, 0)
}

pub fn heading_looks_like_part(text: &str) -> bool {
    let trimmed = text.trim().to_ascii_lowercase();
    trimmed.starts_with("part ")
        && trimmed
            .split_whitespace()
            .nth(1)
            .is_some_and(|token| token.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
}

pub fn heading_has_completion_marker(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("done")
        || lowered.contains("complete")
        || lowered.contains("completed")
        || lowered.contains("resolved")
}

pub fn heading_looks_like_changelog(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered.contains("changelog")
        || lowered.contains("release notes")
        || lowered == "changes"
        || lowered.ends_with(" changes")
        || lowered.ends_with(" history")
}

pub fn count_code_blocks(lines: &[&str]) -> usize {
    let mut count = 0;
    let mut in_block = false;

    for line in lines {
        if line.trim_start().starts_with("```") {
            if !in_block {
                count += 1;
            }
            in_block = !in_block;
        }
    }

    count
}

pub fn compute_section_metrics(
    lines: &[&str],
    headings: &[Heading],
    links: &[Link],
) -> Vec<SectionMetrics> {
    let word_re = Regex::new(r"[A-Za-z0-9_][A-Za-z0-9_-]*").unwrap();
    let list_re = Regex::new(r"^(\s*[-+*]\s+|\s*\d+\.\s+)").unwrap();
    let mut sections = Vec::new();

    for idx in 0..headings.len() {
        let start = headings[idx].line.saturating_sub(1);
        let end = headings
            .get(idx + 1)
            .map_or(lines.len(), |heading| heading.line.saturating_sub(1));
        let section_lines = &lines[start..end];
        let section_text = section_lines.join("\n");
        let line_start = start + 1;
        let line_end = end;

        sections.push(SectionMetrics {
            heading: headings[idx].text.clone(),
            level: headings[idx].level,
            line_start,
            line_end,
            line_count: end.saturating_sub(start),
            word_count: word_re.find_iter(&section_text).count(),
            link_count: links
                .iter()
                .filter(|link| link.line >= line_start && link.line <= line_end)
                .count(),
            list_item_count: section_lines
                .iter()
                .filter(|line| list_re.is_match(line.trim_end()))
                .count(),
            code_block_count: count_code_blocks(section_lines),
            has_completion_marker: heading_has_completion_marker(&headings[idx].text),
            looks_like_part: heading_looks_like_part(&headings[idx].text),
            looks_like_changelog: heading_looks_like_changelog(&headings[idx].text),
        });
    }

    sections
}
