use crate::commands_links::*;
use crate::commands_query::*;
use colored::Colorize;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::types::*;
use crate::util::*;

pub(crate) fn cmd_mv(
    from: &Path,
    to: &Path,
    index_dir: &Path,
    update_refs: bool,
    dry_run: bool,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let from_str = from.to_string_lossy().to_string();
    let to_str = to.to_string_lossy().to_string();

    let mut updated_files: Vec<String> = Vec::new();

    if !dry_run {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(from, to)?;
    }

    if update_refs {
        let forward_index = load_forward_index(index_dir)?;

        // Group by file for rewrites
        let mut files_to_update: HashSet<String> = HashSet::new();
        for (file_path, entry) in &forward_index.files {
            for link in &entry.links {
                if link.target == from_str {
                    files_to_update.insert(file_path.clone());
                }
            }
        }

        for file in &files_to_update {
            let content = fs::read_to_string(file)?;
            let new_content = apply_reference_mapping_to_content(&content, &from_str, &to_str);
            if content != new_content {
                if !dry_run {
                    fs::write(file, &new_content)?;
                }
                updated_files.push(file.clone());
            }
        }
    }

    updated_files.sort();

    if json {
        let result = MvResult {
            from: from_str,
            to: to_str,
            moved: !dry_run,
            updated_files,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    // Human-readable output
    if dry_run {
        println!("{}", "Dry run:".cyan().bold());
    }

    println!(
        "{} {} -> {}",
        if dry_run { "Would move" } else { "Moving" },
        from_str,
        to_str
    );

    if update_refs {
        if updated_files.is_empty() {
            println!(
                "{} No inbound links found for {} in index {}",
                "Note:".yellow(),
                from_str,
                index_dir.display()
            );
        } else {
            println!(
                "{} Updating references in {} file(s)",
                if dry_run { "Would update" } else { "Updating" },
                updated_files.len()
            );
            for file in updated_files {
                if dry_run {
                    println!("  {file} (references would change)");
                } else {
                    println!("  {file}");
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn compute_inbound_link_counts(forward_index: &ForwardIndex) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for (source_path, entry) in &forward_index.files {
        let source_base = Path::new(source_path);
        for link in &entry.links {
            let target = &link.target;
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.starts_with("ftp://")
            {
                continue;
            }

            let (link_path, _) = if let Some(idx) = target.find('#') {
                (
                    target[..idx].to_string(),
                    Some(target[idx + 1..].to_string()),
                )
            } else {
                (target.clone(), None)
            };

            if link_path.is_empty() {
                continue;
            }

            let resolved = if let Some(parent) = source_base.parent() {
                parent.join(&link_path).to_string_lossy().to_string()
            } else {
                link_path.clone()
            };
            let normalized = normalize_path(Path::new(&resolved));
            *counts.entry(normalized).or_insert(0) += 1;
        }
    }

    counts
}

/// Show relation paths from a source document via the persisted relation graph.
pub(crate) fn cmd_paths(
    source: &str,
    depth: usize,
    kind_filter: Option<&str>,
    json: bool,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let relation_index = load_relation_index(index_dir);
    if relation_index.edges.is_empty() {
        if json {
            println!("{{\"source\":\"{source}\",\"paths\":[]}}");
        } else {
            println!(
                "{} No relations found. Run 'yore build' first.",
                "Info:".yellow()
            );
        }
        return Ok(());
    }

    let depth = depth.clamp(1, 3);

    // Normalize source: try exact match, then suffix match
    let all_sources: HashSet<&str> = relation_index
        .edges
        .iter()
        .flat_map(|e| [e.source.as_str(), e.target.as_str()])
        .collect();

    let resolved_source = if all_sources.contains(source) {
        source.to_string()
    } else {
        // Try suffix match
        if let Some(s) = all_sources
            .iter()
            .find(|s| s.ends_with(source) || source.ends_with(*s))
        {
            (*s).to_string()
        } else {
            if json {
                println!("{{\"source\":\"{source}\",\"paths\":[]}}");
            } else {
                println!(
                    "{} '{}' not found in relation graph.",
                    "Info:".yellow(),
                    source
                );
            }
            return Ok(());
        }
    };

    // BFS traversal up to depth
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(resolved_source.clone());
    let mut frontier: Vec<String> = vec![resolved_source.clone()];
    let mut result_edges: Vec<&RelationEdge> = Vec::new();

    for _ in 0..depth {
        let mut next_frontier: Vec<String> = Vec::new();
        for node in &frontier {
            for edge in &relation_index.edges {
                if &edge.source != node {
                    continue;
                }
                // Apply kind filter
                if let Some(kf) = kind_filter {
                    let edge_kind = match &edge.kind {
                        RelationKind::LinksTo => "links_to",
                        RelationKind::SectionLinksTo => "section_links_to",
                        RelationKind::AdrReference => "adr_reference",
                    };
                    if edge_kind != kf {
                        continue;
                    }
                }
                result_edges.push(edge);
                if !visited.contains(&edge.target) {
                    visited.insert(edge.target.clone());
                    next_frontier.push(edge.target.clone());
                }
            }
        }
        frontier = next_frontier;
    }

    if json {
        #[derive(Serialize)]
        struct PathsResult<'a> {
            source: &'a str,
            depth: usize,
            total_edges: usize,
            edges: &'a [&'a RelationEdge],
        }
        let result = PathsResult {
            source: &resolved_source,
            depth,
            total_edges: result_edges.len(),
            edges: &result_edges,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!(
            "{} {} (depth {})",
            "Paths from".green().bold(),
            resolved_source.cyan(),
            depth
        );
        println!();

        if result_edges.is_empty() {
            println!("  No outgoing edges found.");
        } else {
            for edge in &result_edges {
                let kind_label = match &edge.kind {
                    RelationKind::LinksTo => "links_to",
                    RelationKind::SectionLinksTo => "section_links_to",
                    RelationKind::AdrReference => "adr_reference",
                };
                let mut detail = String::new();
                if let Some(anchor) = &edge.anchor {
                    use std::fmt::Write;
                    let _ = write!(detail, " #{anchor}");
                }
                if let Some(src_sec) = &edge.source_section {
                    use std::fmt::Write;
                    let _ = write!(detail, " [from: {}]", src_sec.heading);
                }
                if let Some(tgt_sec) = &edge.target_section {
                    use std::fmt::Write;
                    let _ = write!(detail, " [to: {}]", tgt_sec.heading);
                }
                if let Some(raw) = &edge.raw_text {
                    use std::fmt::Write;
                    let _ = write!(detail, " ({raw})");
                }
                println!(
                    "  {} {} -> {}{}",
                    kind_label.yellow(),
                    edge.source,
                    edge.target.cyan(),
                    detail
                );
            }
            println!();
            println!("  {} edges total", result_edges.len());
        }
    }

    Ok(())
}

pub(crate) fn cmd_export_graph(
    index_dir: &Path,
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;

    // Map normalized paths to canonical file keys
    let mut norm_to_key: HashMap<String, String> = HashMap::new();
    for path in forward_index.files.keys() {
        let normalized = normalize_path(Path::new(path));
        norm_to_key
            .entry(normalized)
            .or_insert_with(|| path.clone());
    }

    let mut nodes: Vec<GraphNode> = forward_index
        .files
        .keys()
        .cloned()
        .map(|id| GraphNode { id })
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));

    let mut edges: Vec<GraphEdge> = Vec::new();

    for (source_path, entry) in &forward_index.files {
        let source_base = Path::new(source_path);

        for link in &entry.links {
            let target = &link.target;

            // Skip external links
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.starts_with("ftp://")
            {
                continue;
            }

            // Split off anchor
            let (link_path, anchor) = if let Some(idx) = target.find('#') {
                (
                    target[..idx].to_string(),
                    Some(target[idx + 1..].to_string()),
                )
            } else {
                (target.clone(), None)
            };

            if link_path.is_empty() {
                continue;
            }

            let resolved = if let Some(parent) = source_base.parent() {
                parent.join(&link_path).to_string_lossy().to_string()
            } else {
                link_path.clone()
            };
            let normalized = normalize_path(Path::new(&resolved));

            if let Some(target_key) = norm_to_key.get(&normalized) {
                edges.push(GraphEdge {
                    source: source_path.clone(),
                    target: target_key.clone(),
                    anchor,
                });
            }
        }
    }

    if edges.is_empty() {
        println!(
            "{} No internal documentation links found to export.",
            "Info:".yellow()
        );
        return Ok(());
    }

    match format {
        "json" => {
            let export = GraphExport { nodes, edges };
            println!("{}", serde_json::to_string_pretty(&export)?);
        }
        "dot" => {
            println!("digraph yore_docs {{");
            for edge in &edges {
                let src = edge.source.replace('"', "\\\"");
                let dst = edge.target.replace('"', "\\\"");
                if let Some(anchor) = &edge.anchor {
                    let label = anchor.replace('"', "\\\"");
                    println!("  \"{src}\" -> \"{dst}\" [label=\"{label}\"];");
                } else {
                    println!("  \"{src}\" -> \"{dst}\";");
                }
            }
            println!("}}");
        }
        other => {
            return Err(format!("Unsupported format: {other}").into());
        }
    }

    Ok(())
}

pub(crate) fn run_stale_check(
    index_dir: &Path,
    days: u64,
    min_inlinks: usize,
) -> Result<StaleResult, Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let inbound_counts = compute_inbound_link_counts(&forward_index);

    let now = std::time::SystemTime::now();
    let mut files = Vec::new();

    for file_path in forward_index.files.keys() {
        let meta = fs::metadata(file_path);
        if meta.is_err() {
            continue;
        }
        let meta = meta?;
        let modified = meta.modified().unwrap_or(now);
        let age = now.duration_since(modified).unwrap_or_default().as_secs() / 86_400;

        let inlinks = *inbound_counts.get(file_path).unwrap_or(&0);

        if age >= days && inlinks >= min_inlinks {
            files.push(StaleFile {
                file: file_path.clone(),
                days_since_modified: age,
                inbound_links: inlinks,
            });
        }
    }

    files.sort_by(|a, b| b.days_since_modified.cmp(&a.days_since_modified));

    Ok(StaleResult {
        total_stale: files.len(),
        files,
    })
}

pub(crate) fn cmd_stale(
    index_dir: &Path,
    days: u64,
    min_inlinks: usize,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = run_stale_check(index_dir, days, min_inlinks)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if result.files.is_empty() {
        println!(
            "{} No stale files found (threshold: {} days, min_inlinks: {}).",
            "✓".green().bold(),
            days,
            min_inlinks
        );
        return Ok(());
    }

    println!(
        "{} Stale files (>= {} days old, inbound_links >= {}):",
        "Stale".yellow().bold(),
        days,
        min_inlinks
    );
    println!("{}", "=".repeat(60));
    for f in &result.files {
        println!(
            "{} ({} days, {} inbound links)",
            f.file, f.days_since_modified, f.inbound_links
        );
    }

    Ok(())
}

pub(crate) fn resolve_health_target_key(
    file: &Path,
    index_dir: &Path,
    metrics_index: &DocumentMetricsIndex,
) -> Option<String> {
    let input = normalize_path(file);
    let without_dot = input.trim_start_matches("./").to_string();
    let with_dot = format!("./{without_dot}");

    for candidate in [&input, &without_dot, &with_dot] {
        if metrics_index.files.contains_key(candidate) {
            return Some(candidate.clone());
        }
    }

    let absolute = canonicalize_existing_path(file);
    let absolute_normalized = normalize_path(&absolute);
    if metrics_index.files.contains_key(&absolute_normalized) {
        return Some(absolute_normalized);
    }

    if let Ok(forward_index) = load_forward_index(index_dir) {
        if let Some(source_root) = forward_index_source_root(&forward_index) {
            let derived = build_indexed_doc_key(&absolute, &source_root);
            if metrics_index.files.contains_key(&derived) {
                return Some(derived);
            }
        }
    }

    None
}

pub(crate) fn evaluate_document_health(
    metrics: &DocumentMetrics,
    options: &HealthOptions,
) -> HealthFileResult {
    let mut issues = Vec::new();

    if metrics.line_count > options.max_lines {
        issues.push(HealthIssue {
            kind: "bloated-file".to_string(),
            severity: "error".to_string(),
            message: format!(
                "{} lines exceeds the configured threshold",
                metrics.line_count
            ),
            value: metrics.line_count,
            threshold: options.max_lines,
        });
    }

    if metrics.part_heading_count >= options.max_part_sections {
        issues.push(HealthIssue {
            kind: "accumulator-pattern".to_string(),
            severity: "error".to_string(),
            message: format!(
                "{} \"Part N\" headings suggest an accumulating narrative doc",
                metrics.part_heading_count
            ),
            value: metrics.part_heading_count,
            threshold: options.max_part_sections,
        });
    }

    let completed_section_lines: usize = metrics
        .sections
        .iter()
        .filter(|section| section.has_completion_marker)
        .map(|section| section.line_count)
        .sum();
    if completed_section_lines > options.max_completed_lines {
        issues.push(HealthIssue {
            kind: "stale-completed".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "{completed_section_lines} retained lines sit under completion-marked sections"
            ),
            value: completed_section_lines,
            threshold: options.max_completed_lines,
        });
    }

    if metrics.changelog_entry_count > options.max_changelog_entries {
        issues.push(HealthIssue {
            kind: "changelog-bloat".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "{} changelog-style entries exceed the configured threshold",
                metrics.changelog_entry_count
            ),
            value: metrics.changelog_entry_count,
            threshold: options.max_changelog_entries,
        });
    }

    let status = if issues.iter().any(|issue| issue.severity == "error") {
        "unhealthy"
    } else if issues.iter().any(|issue| issue.severity == "warning") {
        "warning"
    } else {
        "healthy"
    };

    HealthFileResult {
        file: metrics.path.clone(),
        status: status.to_string(),
        issues,
    }
}

pub(crate) fn cmd_health(
    file: Option<&Path>,
    all: bool,
    index_dir: &Path,
    options: &HealthOptions,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if all == file.is_some() {
        return Err("pass either a file path or --all".into());
    }

    let metrics_index = load_document_metrics(index_dir)?;
    let total_files = metrics_index.files.len();
    let mut files = Vec::new();

    if let Some(file_path) = file {
        let key =
            resolve_health_target_key(file_path, index_dir, &metrics_index).ok_or_else(|| {
                format!(
                    "File not found in document metrics index: {}",
                    file_path.display()
                )
            })?;
        let metrics = metrics_index.files.get(&key).ok_or_else(|| {
            format!(
                "File not found in document metrics index: {}",
                file_path.display()
            )
        })?;
        files.push(evaluate_document_health(metrics, options));
    } else {
        let mut all_results: Vec<HealthFileResult> = metrics_index
            .files
            .values()
            .map(|metrics| evaluate_document_health(metrics, options))
            .filter(|result| !result.issues.is_empty())
            .collect();
        all_results.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.status.cmp(&b.status)));
        files = all_results;
    }

    let unhealthy_files = files
        .iter()
        .filter(|file| file.status == "unhealthy")
        .count();
    let warning_files = files.iter().filter(|file| file.status == "warning").count();
    let result = HealthResult {
        total_files,
        unhealthy_files,
        warning_files,
        files,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if result.files.is_empty() {
        println!("{}", "✓ No health issues detected.".green().bold());
        return Ok(());
    }

    for file_result in &result.files {
        let label = match file_result.status.as_str() {
            "unhealthy" => "UNHEALTHY".red().bold(),
            "warning" => "WARNING".yellow().bold(),
            _ => "HEALTHY".green().bold(),
        };
        println!(
            "{}: {} ({} issue{})",
            file_result.file,
            label,
            file_result.issues.len(),
            if file_result.issues.len() == 1 {
                ""
            } else {
                "s"
            }
        );
        for issue in &file_result.issues {
            println!(
                "  {:<20} {:<7} {} (value: {}, threshold: {})",
                issue.kind,
                issue.severity.to_uppercase(),
                issue.message,
                issue.value,
                issue.threshold
            );
        }
        println!();
    }

    Ok(())
}

pub(crate) fn is_placeholder_target(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();

    matches!(lower.as_str(), "url" | "text" | "todo" | "link" | "tbd")
        || lower.starts_with("/path/to/")
        || lower.starts_with("../path/to/")
        || lower.contains("replace-me")
}

pub(crate) fn is_code_extension(ext: &str) -> bool {
    matches!(
        ext,
        "py" | "ts" | "tsx" | "json" | "yaml" | "yml" | "png" | "svg"
    )
}

pub(crate) fn file_extension(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

pub(crate) fn record_link_kind(
    by_file: &mut HashMap<String, HashMap<String, usize>>,
    by_kind: &mut HashMap<String, usize>,
    file: &str,
    kind: &LinkKind,
) {
    let kind_name = match kind {
        LinkKind::DocMissing => "doc_missing",
        LinkKind::CodeMissing => "code_missing",
        LinkKind::Placeholder => "placeholder",
        LinkKind::CodeReference => "code_reference",
        LinkKind::DirectoryReference => "directory_reference",
        LinkKind::ExternalReference => "external_reference",
        LinkKind::AnchorMissing => "anchor_missing",
        LinkKind::AnchorUnverified => "anchor_unverified",
    }
    .to_string();

    by_kind
        .entry(kind_name.clone())
        .and_modify(|c| *c += 1)
        .or_insert(1);

    let entry = by_file.entry(file.to_string()).or_default();
    entry.entry(kind_name).and_modify(|c| *c += 1).or_insert(1);
}

/// Find all files that link to a specific file
pub(crate) fn cmd_backlinks(
    target_file: &str,
    index_dir: &Path,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load the forward index
    let forward_index = load_forward_index(index_dir)?;

    // Normalize the target file path for comparison
    let normalized_target = normalize_path(Path::new(target_file));

    if !json {
        println!(
            "{} {}",
            "Finding backlinks for".cyan().bold(),
            normalized_target.white().bold()
        );
        println!();
    }

    let mut backlinks = Vec::new();

    // Iterate through all files and check if they link to the target
    for (source_path, entry) in &forward_index.files {
        for link in &entry.links {
            let target = &link.target;

            // Skip external links
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.starts_with("ftp://")
            {
                continue;
            }

            // Parse link to separate file path and anchor
            let (link_path, anchor) = if let Some(idx) = target.find('#') {
                (
                    target[..idx].to_string(),
                    Some(target[idx + 1..].to_string()),
                )
            } else {
                (target.clone(), None)
            };

            // Resolve relative path from source file
            let resolved_path = if link_path.is_empty() {
                // Just an anchor in the current file
                source_path.clone()
            } else if let Some(stripped) = link_path.strip_prefix('/') {
                // Absolute path - strip leading / and use as-is
                stripped.to_string()
            } else {
                // Relative path
                let source_file_path = Path::new(source_path);
                if let Some(parent) = source_file_path.parent() {
                    parent.join(&link_path).to_string_lossy().to_string()
                } else {
                    link_path.clone()
                }
            };

            // Normalize the resolved path
            let normalized_link = normalize_path(Path::new(&resolved_path));

            // Check if this link points to our target file
            if normalized_link == normalized_target {
                backlinks.push(Backlink {
                    source_file: source_path.clone(),
                    link_text: link.text.clone(),
                    link_target: target.clone(),
                    anchor,
                });
            }
        }
    }

    // Sort backlinks by source file for consistent output
    backlinks.sort_by(|a, b| a.source_file.cmp(&b.source_file));

    let result = BacklinksResult {
        target_file: normalized_target.clone(),
        total_backlinks: backlinks.len(),
        backlinks: backlinks.clone(),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("{}", "Backlinks Found".cyan().bold());
        println!("{}", "=".repeat(60));
        println!();
        println!("Total backlinks: {}", backlinks.len());
        println!();

        if backlinks.is_empty() {
            println!(
                "{}",
                "No backlinks found. This file is not referenced by any other file.".yellow()
            );
            println!();
            println!("{}", "This may indicate:".yellow());
            println!("  - An orphaned document (consider reviewing for deletion)");
            println!("  - A new document that needs linking");
            println!("  - An entry point document (like README.md)");
        } else {
            for (idx, backlink) in backlinks.iter().enumerate() {
                println!("[{}] {}", idx + 1, backlink.source_file.white().bold());
                println!(
                    "    Link: [{}]({})",
                    backlink.link_text, backlink.link_target
                );
                if let Some(anchor) = &backlink.anchor {
                    println!("    Anchor: #{anchor}");
                }
                println!();
            }

            println!("{}", "Safe to delete?".yellow().bold());
            println!(
                "  {} These {} file(s) link to this document.",
                "⚠".yellow(),
                backlinks.len()
            );
            println!("  Review and update references before deletion.");
        }
    }

    Ok(())
}
