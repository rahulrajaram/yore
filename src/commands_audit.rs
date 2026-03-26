use crate::commands_query::*;
use colored::Colorize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::assemble::*;
use crate::types::*;
use crate::util::*;

pub(crate) fn cmd_orphans(
    index_dir: &Path,
    json: bool,
    exclude_patterns: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    // Load the forward index
    let forward_index = load_forward_index(index_dir)?;

    if !json {
        println!("{}", "Finding orphaned files...".cyan().bold());
        println!();
    }

    // Build a set of all files that are linked to
    let mut linked_files: HashSet<String> = HashSet::new();

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
            let (link_path, _) = if let Some(idx) = target.find('#') {
                (
                    target[..idx].to_string(),
                    Some(target[idx + 1..].to_string()),
                )
            } else {
                (target.clone(), None)
            };

            // Skip anchor-only links
            if link_path.is_empty() {
                continue;
            }

            // Resolve relative path from source file
            let resolved_path = if let Some(stripped) = link_path.strip_prefix('/') {
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
            linked_files.insert(normalized_link);
        }
    }

    // Find files that are NOT in the linked set
    let mut orphans = Vec::new();

    for (file_path, entry) in &forward_index.files {
        // Check if this file has any inbound links
        if !linked_files.contains(file_path) {
            // Check exclude patterns
            let mut excluded = false;
            for pattern in exclude_patterns {
                if file_path.contains(pattern) {
                    excluded = true;
                    break;
                }
            }

            if excluded {
                continue;
            }

            orphans.push(OrphanFile {
                file: file_path.clone(),
                size_bytes: entry.size_bytes,
                line_count: entry.line_count,
            });
        }
    }

    // Sort orphans by file path
    orphans.sort_by(|a, b| a.file.cmp(&b.file));

    let result = OrphansResult {
        total_orphans: orphans.len(),
        orphans: orphans.clone(),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("{}", "Orphaned Files".cyan().bold());
        println!("{}", "=".repeat(60));
        println!();
        println!("Total orphans: {}", orphans.len());
        println!();

        if orphans.is_empty() {
            println!(
                "{}",
                "No orphaned files found. All documents are linked!".green()
            );
            println!();
        } else {
            for (idx, orphan) in orphans.iter().enumerate() {
                println!("[{}] {}", idx + 1, orphan.file.white().bold());
                println!(
                    "    Size: {} bytes, Lines: {}",
                    orphan.size_bytes, orphan.line_count
                );
                println!();
            }

            println!("{}", "Cleanup suggestions:".yellow().bold());
            println!("  1. Review each file to determine if it's still needed");
            println!("  2. Add links from relevant documents if the content is valuable");
            println!("  3. Delete or archive files that are no longer relevant");
            println!("  4. Entry point files (README.md) may intentionally have no backlinks");
            println!();
            println!("{}", "To exclude patterns:".cyan());
            println!("  yore orphans --exclude README --exclude INDEX");
        }
    }

    Ok(())
}

pub(crate) fn build_inbound_link_counts(forward_index: &ForwardIndex) -> HashMap<String, usize> {
    let mut inbound_counts: HashMap<String, usize> = HashMap::new();

    for (source_path, entry) in &forward_index.files {
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

            let resolved_path = if let Some(stripped) = link_path.strip_prefix('/') {
                stripped.to_string()
            } else {
                let source_file_path = Path::new(source_path);
                if let Some(parent) = source_file_path.parent() {
                    parent.join(&link_path).to_string_lossy().to_string()
                } else {
                    link_path.clone()
                }
            };

            let normalized_link = normalize_path(Path::new(&resolved_path));
            *inbound_counts.entry(normalized_link).or_insert(0) += 1;
        }
    }

    inbound_counts
}

pub(crate) fn cmd_canonical_orphans(
    index_dir: &Path,
    threshold: f64,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let inbound_counts = build_inbound_link_counts(&forward_index);

    let mut orphans = Vec::new();

    for (file_path, entry) in &forward_index.files {
        let inbound_links = *inbound_counts.get(file_path).unwrap_or(&0);
        if inbound_links > 0 {
            continue;
        }

        let score = score_canonicality(file_path, entry);
        if score >= threshold {
            orphans.push(CanonicalOrphan {
                file: file_path.clone(),
                canonicality: score,
                inbound_links,
            });
        }
    }

    orphans.sort_by(|a, b| {
        b.canonicality
            .partial_cmp(&a.canonicality)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file.cmp(&b.file))
    });

    let result = CanonicalOrphansResult {
        total_orphans: orphans.len(),
        threshold,
        orphans: orphans.clone(),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    println!("{}", "Canonical Orphans".cyan().bold());
    println!("{}", "=".repeat(60));
    println!();
    println!("Threshold: {threshold}");
    println!("Total canonical orphans: {}", orphans.len());
    println!();

    if orphans.is_empty() {
        println!(
            "{}",
            "No canonical documents without inbound links found.".green()
        );
        return Ok(());
    }

    for (idx, orphan) in orphans.iter().enumerate() {
        println!("[{}] {}", idx + 1, orphan.file.white().bold());
        println!(
            "    Canonicality: {:.2}, Inbound links: {}",
            orphan.canonicality, orphan.inbound_links
        );
        println!();
    }

    Ok(())
}

/// Score canonicality with reasons
pub(crate) fn score_canonicality_with_reasons(
    doc_path: &str,
    _entry: &FileEntry,
) -> (f64, Vec<String>) {
    let mut score: f64 = 0.5; // baseline
    let mut reasons = Vec::new();

    let path_lower = doc_path.to_lowercase();

    // Path-based boosts
    if path_lower.contains("docs/adr/") || path_lower.contains("docs/architecture/") {
        score += 0.2;
        reasons.push("Architecture/ADR document (+0.2)".to_string());
    }
    if path_lower.contains("docs/index/") {
        score += 0.15;
        reasons.push("Index document (+0.15)".to_string());
    }
    if path_lower.contains("scratch")
        || path_lower.contains("archive")
        || path_lower.contains("old")
    {
        score -= 0.3;
        reasons.push("Scratch/archive/old location (-0.3)".to_string());
    }
    if path_lower.contains("deprecated") || path_lower.contains("backup") {
        score -= 0.25;
        reasons.push("Deprecated/backup location (-0.25)".to_string());
    }

    // Filename patterns
    let filename = Path::new(doc_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    if filename.contains("readme") || filename.contains("index") {
        score += 0.1;
        reasons.push("README/INDEX file (+0.1)".to_string());
    }
    if filename.contains("guide") || filename.contains("runbook") || filename.contains("plan") {
        score += 0.1;
        reasons.push("Guide/runbook/plan document (+0.1)".to_string());
    }

    // Clamp to [0.0, 1.0]
    let final_score = score.clamp(0.0, 1.0);

    if reasons.is_empty() {
        reasons.push("Baseline score (0.5)".to_string());
    }

    (final_score, reasons)
}

/// Show canonicality scores for all documents
pub(crate) fn cmd_canonicality(
    index_dir: &Path,
    json: bool,
    threshold: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load the forward index
    let forward_index = load_forward_index(index_dir)?;

    if !json {
        println!("{}", "Computing canonicality scores...".cyan().bold());
        println!();
    }

    let mut scored_files = Vec::new();

    for (file_path, entry) in &forward_index.files {
        let (score, reasons) = score_canonicality_with_reasons(file_path, entry);

        if score >= threshold {
            scored_files.push(CanonicalityScore {
                file: file_path.clone(),
                score,
                reasons,
            });
        }
    }

    // Sort by score descending
    scored_files.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let result = CanonicalityResult {
        total_files: scored_files.len(),
        files: scored_files.clone(),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("{}", "Canonicality Scores".cyan().bold());
        println!("{}", "=".repeat(60));
        println!();
        println!(
            "Total files: {} (threshold: {})",
            scored_files.len(),
            threshold
        );
        println!();

        // Group by score ranges
        let high_canon: Vec<_> = scored_files.iter().filter(|s| s.score >= 0.7).collect();
        let medium_canon: Vec<_> = scored_files
            .iter()
            .filter(|s| s.score >= 0.5 && s.score < 0.7)
            .collect();
        let low_canon: Vec<_> = scored_files.iter().filter(|s| s.score < 0.5).collect();

        println!(
            "{} High canonicality (≥0.7): {} files",
            "📚".green(),
            high_canon.len()
        );
        for file in high_canon.iter().take(10) {
            println!("  [{:.2}] {}", file.score, file.file.white().bold());
            for reason in &file.reasons {
                println!("         - {reason}");
            }
        }
        if high_canon.len() > 10 {
            println!("  ... and {} more", high_canon.len() - 10);
        }
        println!();

        println!(
            "{} Medium canonicality (0.5-0.7): {} files",
            "📄".yellow(),
            medium_canon.len()
        );
        for file in medium_canon.iter().take(5) {
            println!("  [{:.2}] {}", file.score, file.file);
        }
        if medium_canon.len() > 5 {
            println!("  ... and {} more", medium_canon.len() - 5);
        }
        println!();

        println!(
            "{} Low canonicality (<0.5): {} files",
            "📋".red(),
            low_canon.len()
        );
        for file in low_canon.iter().take(5) {
            println!("  [{:.2}] {}", file.score, file.file);
            for reason in &file.reasons {
                println!("         - {reason}");
            }
        }
        if low_canon.len() > 5 {
            println!("  ... and {} more", low_canon.len() - 5);
        }
        println!();

        println!("{}", "What does this mean?".yellow().bold());
        println!("  - High scores: Authoritative, well-placed documents");
        println!("  - Medium scores: Standard documentation");
        println!("  - Low scores: Scratch work, archived, or deprecated content");
        println!();
        println!("{}", "For decision support:".cyan());
        println!("  - Trust high-canon docs when resolving conflicts");
        println!("  - Review low-canon docs for potential archival");
        println!("  - Use threshold flag to filter: --threshold 0.6");
    }

    Ok(())
}

pub(crate) fn cmd_suggest_consolidation(
    index_dir: &Path,
    threshold: f64,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;

    let pairs = compute_duplicate_pairs(&forward_index, threshold);
    if pairs.is_empty() {
        println!(
            "{} No consolidation candidates found above threshold {}.",
            "Info:".yellow(),
            threshold
        );
        return Ok(());
    }

    let result = build_consolidation_groups(&forward_index, &pairs);

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if result.groups.is_empty() {
        println!(
            "{} Duplicate pairs found but no multi-file groups to consolidate.",
            "Info:".yellow()
        );
        return Ok(());
    }

    println!("{}", "Consolidation Suggestions".cyan().bold());
    println!("{}", "=".repeat(60));
    println!(
        "Total groups: {} (threshold: {:.2})",
        result.total_groups, threshold
    );
    println!();

    for group in &result.groups {
        println!("{}", group.canonical.white().bold());
        println!(
            "  Canonical score: {:.2}, Avg similarity: {:.2}",
            group.canonical_score, group.avg_similarity
        );
        println!("  Merge into canonical:");
        for m in &group.merge_into {
            println!("    - {m}");
        }
        println!("  Note: {}", group.note);
        println!();
    }

    Ok(())
}
