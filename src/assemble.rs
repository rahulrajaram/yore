use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::search::*;
use crate::types::*;
use crate::util::*;

pub(crate) fn search_relevant_sections(
    query: &str,
    index: &ForwardIndex,
    max_sections: usize,
) -> Vec<SectionMatch> {
    let query_terms = parse_query_terms(query, true);
    if query_terms.is_empty() {
        return Vec::new();
    }

    let mut all_sections: Vec<SectionMatch> = Vec::new();

    // First, get top documents by BM25
    let mut doc_scores: Vec<(&String, &FileEntry, f64)> = index
        .files
        .iter()
        .map(|(path, entry)| {
            let score = bm25_score(&query_terms, entry, index.avg_doc_length, &index.idf_map);
            (path, entry, score)
        })
        .filter(|(_, _, score)| *score > 0.01)
        .collect();

    doc_scores.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // Take top 20 documents
    for (doc_path, entry, doc_score) in doc_scores.iter().take(20) {
        let canonicality = score_canonicality(doc_path, entry);

        // Split document into sections based on section_fingerprints
        if !entry.section_fingerprints.is_empty() {
            if let Ok(content) = read_indexed_doc(index, doc_path, entry) {
                let lines: Vec<&str> = content.lines().collect();

                // Use indexed sections
                for section in &entry.section_fingerprints {
                    let start = section.line_start.saturating_sub(1);
                    let end = section.line_end.min(lines.len());

                    if start < end {
                        let section_content = lines[start..end].join("\n");

                        all_sections.push(SectionMatch {
                            doc_path: (*doc_path).to_string(),
                            heading: section.heading.clone(),
                            line_start: section.line_start,
                            line_end: section.line_end,
                            bm25_score: *doc_score, // Use doc-level score for now
                            content: section_content,
                            canonicality,
                        });
                    }
                }
            }
        } else {
            // Fallback: treat whole doc as one section
            if let Ok(content) = read_indexed_doc(index, doc_path, entry) {
                all_sections.push(SectionMatch {
                    doc_path: (*doc_path).to_string(),
                    heading: "Full Document".to_string(),
                    line_start: 1,
                    line_end: content.lines().count(),
                    bm25_score: *doc_score,
                    content,
                    canonicality,
                });
            }
        }
    }

    // Sort by combined score with deterministic tie-breaks.
    all_sections.sort_by(compare_sections_by_relevance);

    // Take top N sections
    all_sections.into_iter().take(max_sections).collect()
}

/// Score document canonicality based on path, recency, and patterns
pub(crate) fn score_canonicality(doc_path: &str, _entry: &FileEntry) -> f64 {
    let mut score: f64 = 0.5; // baseline

    let path_lower = doc_path.to_lowercase();

    // Path-based boosts
    if path_lower.contains("docs/adr/") || path_lower.contains("docs/architecture/") {
        score += 0.2;
    }
    if path_lower.contains("docs/index/") {
        score += 0.15;
    }
    if path_lower.contains("scratch")
        || path_lower.contains("archive")
        || path_lower.contains("old")
    {
        score -= 0.3;
    }
    if path_lower.contains("deprecated") || path_lower.contains("backup") {
        score -= 0.25;
    }

    // Filename patterns
    let filename = Path::new(doc_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    if filename.contains("readme") || filename.contains("index") {
        score += 0.1;
    }
    if filename.contains("guide") || filename.contains("runbook") || filename.contains("plan") {
        score += 0.1;
    }

    // Recency (approximate - we don't have mtime in index yet)
    // For now, we'll just use this as a placeholder
    // In future: add last_modified to FileEntry

    // Clamp to [0.0, 1.0]
    score.clamp(0.0, 1.0)
}

/// Distill sections into markdown digest within token budget
pub(crate) fn distill_to_markdown(
    sections: &[SectionMatch],
    query: &str,
    max_tokens: usize,
) -> String {
    let mut output = String::new();
    let mut used_tokens = 0;

    // Header
    let header = format!(
        "# Context Digest for: \"{}\"\n\n\
         **Generated:** {}\n\
         **Token Budget:** {}\n\
         **Documents Scanned:** N/A\n\
         **Sections Selected:** {}\n\n\
         ---\n\n",
        query,
        chrono_now(),
        max_tokens,
        sections.len()
    );
    output.push_str(&header);
    used_tokens += estimate_tokens(&header);

    // Group sections by document
    let mut doc_groups: HashMap<String, Vec<&SectionMatch>> = HashMap::new();
    for section in sections {
        doc_groups
            .entry(section.doc_path.clone())
            .or_default()
            .push(section);
    }

    // Top Relevant Documents section
    output.push_str("## Top Relevant Documents\n\n");
    used_tokens += 10;

    let mut ranked_docs: Vec<_> = doc_groups.iter().collect();
    ranked_docs.sort_by(|a, b| {
        let score_a = a.1[0].bm25_score * 0.7 + a.1[0].canonicality * 0.3;
        let score_b = b.1[0].bm25_score * 0.7 + b.1[0].canonicality * 0.3;
        score_b
            .partial_cmp(&score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (idx, (doc_path, doc_sections)) in ranked_docs.iter().enumerate().take(10) {
        let section = doc_sections[0];
        let combined_score = section.bm25_score * 0.7 + section.canonicality * 0.3;
        let doc_line = format!(
            "{}. **{}** (score: {:.2}, canonical: {:.2})\n   - Sections included: {}\n\n",
            idx + 1,
            doc_path,
            combined_score,
            section.canonicality,
            doc_sections.len()
        );
        output.push_str(&doc_line);
        used_tokens += estimate_tokens(&doc_line);
    }

    output.push_str("---\n\n## Distilled Content\n\n");
    used_tokens += 10;

    // Add sections
    for section in sections {
        if used_tokens >= max_tokens {
            output.push_str("\n\n*[Content truncated due to token budget]*\n");
            break;
        }

        let section_header = format!(
            "### {} (from {})\n\n**Source:** {}:{}-{} (canonical: {:.2})\n\n",
            section.heading,
            section.doc_path,
            section.doc_path,
            section.line_start,
            section.line_end,
            section.canonicality
        );

        // Estimate how much space we need
        let section_tokens = estimate_tokens(&section_header) + estimate_tokens(&section.content);

        if used_tokens + section_tokens > max_tokens {
            // Try to fit a truncated version
            let remaining_tokens = max_tokens - used_tokens;
            let chars_to_include = remaining_tokens * 4; // rough approximation

            if chars_to_include > 200 {
                output.push_str(&section_header);
                output.push_str(&section.content[..chars_to_include.min(section.content.len())]);
                output.push_str("\n\n*[Section truncated]*\n");
            }
            break;
        }

        output.push_str(&section_header);
        output.push_str(&section.content);
        output.push_str("\n\n---\n\n");

        used_tokens += section_tokens;
    }

    // Metadata footer
    let footer = format!(
        "\n## Metadata\n\n\
         **Canonicality Scores:**\n\
         - 0.90+: Authoritative source, prefer over other docs\n\
         - 0.70-0.89: Reliable, current documentation\n\
         - 0.50-0.69: Secondary or supporting documentation\n\
         - <0.50: Potentially stale, use with caution\n\n\
         **Actual Tokens Used:** ~{used_tokens}\n\n\
         ---\n\n\
         ## Usage with LLM\n\n\
         Paste this digest into your LLM conversation, then ask:\n\n\
         > Using only the information in the context above, answer: \"{query}\"\n\
         > Be explicit when something is not documented in the context.\n"
    );

    output.push_str(&footer);

    output
}

/// Estimate token count (rough approximation: 1 token ≈ 4 chars)
pub(crate) fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Build ADR index mapping ADR numbers to file paths
/// Extract all deterministic relation edges from a forward index.
/// Produces document-level links, section-level links, and ADR reference edges.
pub fn extract_relations(forward_index: &ForwardIndex) -> RelationIndex {
    // Build normalized-path-to-key map (sorted iteration for determinism)
    let mut norm_to_key: HashMap<String, String> = HashMap::new();
    let mut sorted_keys: Vec<&String> = forward_index.files.keys().collect();
    sorted_keys.sort();
    for key in &sorted_keys {
        let normalized = normalize_path(Path::new(key));
        norm_to_key
            .entry(normalized)
            .or_insert_with(|| (*key).clone());
    }

    let adr_index = build_adr_index(forward_index);
    let mut edges: Vec<RelationEdge> = Vec::new();

    for source_key in &sorted_keys {
        let entry = &forward_index.files[*source_key];
        let source_base = Path::new(source_key.as_str());

        // Document & section edges from links
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

            let target_key = match norm_to_key.get(&normalized) {
                Some(k) => k.clone(),
                None => continue,
            };

            // Skip self-links
            if &target_key == *source_key {
                continue;
            }

            // Document-level LinksTo edge
            edges.push(RelationEdge {
                source: (*source_key).clone(),
                target: target_key.clone(),
                kind: RelationKind::LinksTo,
                anchor: anchor.clone(),
                source_section: None,
                target_section: None,
                raw_text: None,
            });

            // Section-level edge
            let source_section = find_containing_section(&entry.section_fingerprints, link.line);
            if source_section.is_some() {
                let target_section = anchor.as_deref().and_then(|a| {
                    forward_index
                        .files
                        .get(&target_key)
                        .and_then(|te| resolve_anchor_to_section(te, a))
                });

                edges.push(RelationEdge {
                    source: (*source_key).clone(),
                    target: target_key.clone(),
                    kind: RelationKind::SectionLinksTo,
                    anchor: anchor.clone(),
                    source_section,
                    target_section,
                    raw_text: None,
                });
            }
        }

        // ADR reference edges
        for adr_ref in &entry.adr_references {
            if let Some(target_path) = adr_index.get(&adr_ref.normalized_id) {
                // Skip self-links
                if target_path == *source_key {
                    continue;
                }

                let source_section =
                    find_containing_section(&entry.section_fingerprints, adr_ref.line);

                edges.push(RelationEdge {
                    source: (*source_key).clone(),
                    target: target_path.clone(),
                    kind: RelationKind::AdrReference,
                    anchor: None,
                    source_section,
                    target_section: None,
                    raw_text: Some(adr_ref.raw_text.clone()),
                });
            }
        }
    }

    edges.sort();
    edges.dedup();

    RelationIndex {
        version: 1,
        indexed_at: chrono_now(),
        total_edges: edges.len(),
        edges,
    }
}

/// Parse markdown links from a section's content
pub fn parse_markdown_links(section: &SectionMatch, origin_dir: &Path) -> Vec<CrossRef> {
    let mut refs = Vec::new();

    // Regex: [text](target) - we'll filter out ![image] manually
    let link_regex = Regex::new(r"(!?)\[(?P<label>[^\]]+)\]\((?P<target>[^)]+)\)").unwrap();

    for caps in link_regex.captures_iter(&section.content) {
        // Skip if this is an image link (starts with !)
        if caps.get(1).is_some_and(|m| m.as_str() == "!") {
            continue;
        }

        if let (Some(label), Some(target)) = (caps.name("label"), caps.name("target")) {
            let target_str = target.as_str();

            // Skip external links
            if target_str.starts_with("http://")
                || target_str.starts_with("https://")
                || target_str.starts_with("mailto:")
            {
                continue;
            }

            // Parse target: path.md#anchor
            let (path_part, anchor) = if let Some(hash_pos) = target_str.find('#') {
                (
                    &target_str[..hash_pos],
                    Some(target_str[hash_pos + 1..].to_string()),
                )
            } else {
                (target_str, None)
            };

            // Skip non-markdown links
            let lc = path_part.to_ascii_lowercase();
            if !lc.ends_with(".md") && !lc.ends_with(".txt") && !lc.ends_with(".rst") {
                continue;
            }

            // Resolve relative path
            let target_path = if path_part.starts_with('/') {
                // Absolute path within repo - strip leading /
                PathBuf::from(path_part.trim_start_matches('/'))
            } else {
                // Relative path - resolve from origin doc's directory
                origin_dir.join(path_part)
            };

            // Normalize path
            let normalized = normalize_path(&target_path);

            // Skip self-links
            if normalized == section.doc_path {
                continue;
            }

            refs.push(CrossRef {
                ref_type: RefType::MarkdownLink,
                origin_doc_path: section.doc_path.clone(),
                target_doc_path: normalized,
                target_anchor: anchor,
                raw_text: label.as_str().to_string(),
            });
        }
    }

    refs
}

/// Find the section containing a given line number
pub fn find_containing_section(sections: &[SectionFingerprint], line: usize) -> Option<SectionRef> {
    for section in sections {
        if section.line_start <= line && line <= section.line_end {
            return Some(SectionRef {
                heading: section.heading.clone(),
                line_start: section.line_start,
            });
        }
    }
    None
}

/// Resolve an anchor fragment to a section in the target file entry
pub fn resolve_anchor_to_section(entry: &FileEntry, anchor: &str) -> Option<SectionRef> {
    let anchor_slug = anchor.to_lowercase().replace([' ', '_'], "-");
    for section in &entry.section_fingerprints {
        let heading_slug = section.heading.to_lowercase().replace(' ', "-");
        if heading_slug == anchor_slug || heading_slug.contains(&anchor_slug) {
            return Some(SectionRef {
                heading: section.heading.clone(),
                line_start: section.line_start,
            });
        }
    }
    None
}
pub(crate) fn build_adr_index(index: &ForwardIndex) -> HashMap<String, String> {
    let mut adr_map = HashMap::new();
    let adr_regex = Regex::new(r"ADR[-_]?(\d{2,4})").unwrap();

    for path in index.files.keys() {
        let path_lower = path.to_lowercase();
        if path_lower.contains("/adr/") || path_lower.contains("adr-") {
            if let Some(caps) = adr_regex.captures(path) {
                if let Some(num_str) = caps.get(1) {
                    // Zero-pad to 3 digits
                    let num: usize = num_str.as_str().parse().unwrap_or(0);
                    let normalized = format!("{num:03}");
                    adr_map.insert(normalized, path.clone());
                }
            }
        }
    }

    adr_map
}

/// Parse ADR ID references from section content
pub(crate) fn parse_adr_ids(
    section: &SectionMatch,
    adr_index: &HashMap<String, String>,
) -> Vec<CrossRef> {
    let mut refs = Vec::new();

    // Regex: ADR-013, ADR 13, ADR_0013
    let adr_regex = Regex::new(r"\bADR[-_ ]?(?P<num>\d{2,4})\b").unwrap();

    for caps in adr_regex.captures_iter(&section.content) {
        if let Some(num) = caps.name("num") {
            let num_str = num.as_str();
            let num_val: usize = num_str.parse().unwrap_or(0);

            // Zero-pad to 3 digits
            let normalized = format!("{num_val:03}");

            // Lookup in ADR index
            if let Some(target_path) = adr_index.get(&normalized) {
                // Skip if same file
                if target_path == &section.doc_path {
                    continue;
                }

                refs.push(CrossRef {
                    ref_type: RefType::AdrId,
                    origin_doc_path: section.doc_path.clone(),
                    target_doc_path: target_path.clone(),
                    target_anchor: None,
                    raw_text: caps.get(0).unwrap().as_str().to_string(),
                });
            }
        }
    }

    refs
}

/// Collect and deduplicate cross-references from primary sections
pub(crate) fn collect_crossrefs(
    sections: &[SectionMatch],
    adr_index: &HashMap<String, String>,
) -> Vec<CrossRef> {
    let mut all_refs = Vec::new();

    for section in sections {
        // Get parent directory of origin doc
        let origin_dir = Path::new(&section.doc_path)
            .parent()
            .unwrap_or_else(|| Path::new("."));

        // Parse markdown links
        all_refs.extend(parse_markdown_links(section, origin_dir));

        // Parse ADR IDs
        all_refs.extend(parse_adr_ids(section, adr_index));
    }

    // Deduplicate by (origin_doc_path, target_doc_path, target_anchor)
    let mut seen: HashSet<(String, String, Option<String>)> = HashSet::new();
    let mut unique_refs = Vec::new();

    for r in all_refs {
        let key = (
            r.origin_doc_path.clone(),
            r.target_doc_path.clone(),
            r.target_anchor.clone(),
        );

        if !seen.contains(&key) {
            seen.insert(key);
            unique_refs.push(r);
        }
    }

    unique_refs
}

/// Classify target document by type
pub(crate) fn classify_target_doc(path: &str) -> DocType {
    let path_lower = path.to_lowercase();

    if path_lower.contains("/adr/") || path_lower.contains("adr-") {
        DocType::Adr
    } else if path_lower.contains("architecture") || path_lower.contains("design") {
        DocType::Design
    } else if path_lower.contains("runbook")
        || path_lower.contains("operations")
        || path_lower.contains("ops")
    {
        DocType::Ops
    } else {
        DocType::Other
    }
}

/// Select sections from an ADR doc
pub(crate) fn select_sections_for_adr(
    doc_path: &str,
    index: &ForwardIndex,
    entry: &FileEntry,
    max_sections: usize,
) -> Vec<SectionMatch> {
    let mut sections = Vec::new();

    // Priority sections: Context, Decision, Consequences
    let priority_keywords = [
        "context",
        "decision",
        "consequences",
        "motivation",
        "rationale",
        "summary",
    ];

    if let Ok(content) = read_indexed_doc(index, doc_path, entry) {
        let lines: Vec<&str> = content.lines().collect();

        // Try to use section fingerprints
        for section in &entry.section_fingerprints {
            if sections.len() >= max_sections {
                break;
            }

            // Check if this is a priority section
            let heading_lower = section.heading.to_lowercase();
            let is_priority = priority_keywords
                .iter()
                .any(|kw| heading_lower.contains(kw));

            if is_priority || sections.is_empty() {
                // Include this section
                let start = section.line_start.saturating_sub(1);
                let end = section.line_end.min(lines.len());

                if start < end {
                    let section_content = lines[start..end].join("\n");

                    sections.push(SectionMatch {
                        doc_path: doc_path.to_string(),
                        heading: section.heading.clone(),
                        line_start: section.line_start,
                        line_end: section.line_end,
                        bm25_score: 0.0, // Cross-ref sections don't have BM25 scores
                        content: section_content,
                        canonicality: score_canonicality(doc_path, entry),
                    });
                }
            }
        }

        // If no sections found, include the first section or full doc
        if sections.is_empty() && !lines.is_empty() {
            sections.push(SectionMatch {
                doc_path: doc_path.to_string(),
                heading: "Full Document".to_string(),
                line_start: 1,
                line_end: lines.len().min(100), // Limit to first 100 lines
                bm25_score: 0.0,
                content: lines[..lines.len().min(100)].join("\n"),
                canonicality: score_canonicality(doc_path, entry),
            });
        }
    }

    sections
}

/// Select sections from a design/architecture doc
pub(crate) fn select_sections_for_design(
    doc_path: &str,
    index: &ForwardIndex,
    entry: &FileEntry,
    anchor: Option<&str>,
    max_sections: usize,
) -> Vec<SectionMatch> {
    let mut sections = Vec::new();

    if let Ok(content) = read_indexed_doc(index, doc_path, entry) {
        let lines: Vec<&str> = content.lines().collect();

        // If anchor is specified, try to find matching section
        if let Some(anchor_str) = anchor {
            let anchor_lower = anchor_str.to_lowercase().replace(['-', '_'], " ");

            for section in &entry.section_fingerprints {
                let heading_lower = section.heading.to_lowercase();
                let heading_slug = heading_lower.replace(' ', "-");

                if heading_slug.contains(&anchor_str.replace(' ', "-"))
                    || heading_lower.contains(&anchor_lower)
                {
                    // Found matching section
                    let start = section.line_start.saturating_sub(1);
                    let end = section.line_end.min(lines.len());

                    if start < end {
                        let section_content = lines[start..end].join("\n");

                        sections.push(SectionMatch {
                            doc_path: doc_path.to_string(),
                            heading: section.heading.clone(),
                            line_start: section.line_start,
                            line_end: section.line_end,
                            bm25_score: 0.0,
                            content: section_content,
                            canonicality: score_canonicality(doc_path, entry),
                        });
                    }

                    break; // Found the target section
                }
            }
        }

        // If no anchor or not found, include first few sections
        if sections.is_empty() {
            for section in entry.section_fingerprints.iter().take(max_sections) {
                let start = section.line_start.saturating_sub(1);
                let end = section.line_end.min(lines.len());

                if start < end {
                    let section_content = lines[start..end].join("\n");

                    sections.push(SectionMatch {
                        doc_path: doc_path.to_string(),
                        heading: section.heading.clone(),
                        line_start: section.line_start,
                        line_end: section.line_end,
                        bm25_score: 0.0,
                        content: section_content,
                        canonicality: score_canonicality(doc_path, entry),
                    });
                }
            }
        }

        // Fallback: if still no sections, include beginning of doc
        if sections.is_empty() && !lines.is_empty() {
            sections.push(SectionMatch {
                doc_path: doc_path.to_string(),
                heading: "Introduction".to_string(),
                line_start: 1,
                line_end: lines.len().min(50),
                bm25_score: 0.0,
                content: lines[..lines.len().min(50)].join("\n"),
                canonicality: score_canonicality(doc_path, entry),
            });
        }
    }

    sections
}

/// Select sections from an ops/runbook doc
pub(crate) fn select_sections_for_ops(
    doc_path: &str,
    index: &ForwardIndex,
    entry: &FileEntry,
    max_sections: usize,
) -> Vec<SectionMatch> {
    let mut sections = Vec::new();

    // Keywords for ops docs
    let ops_keywords = [
        "deploy",
        "restart",
        "rollback",
        "monitor",
        "troubleshoot",
        "debug",
        "fix",
        "restore",
    ];

    if let Ok(content) = read_indexed_doc(index, doc_path, entry) {
        let lines: Vec<&str> = content.lines().collect();

        // Prioritize sections with ops keywords
        for section in &entry.section_fingerprints {
            if sections.len() >= max_sections {
                break;
            }

            let heading_lower = section.heading.to_lowercase();
            let is_ops = ops_keywords.iter().any(|kw| heading_lower.contains(kw));

            if is_ops {
                let start = section.line_start.saturating_sub(1);
                let end = section.line_end.min(lines.len());

                if start < end {
                    let section_content = lines[start..end].join("\n");

                    sections.push(SectionMatch {
                        doc_path: doc_path.to_string(),
                        heading: section.heading.clone(),
                        line_start: section.line_start,
                        line_end: section.line_end,
                        bm25_score: 0.0,
                        content: section_content,
                        canonicality: score_canonicality(doc_path, entry),
                    });
                }
            }
        }

        // If no ops sections found, include first section
        if sections.is_empty() && !entry.section_fingerprints.is_empty() {
            let section = &entry.section_fingerprints[0];
            let start = section.line_start.saturating_sub(1);
            let end = section.line_end.min(lines.len());

            if start < end {
                let section_content = lines[start..end].join("\n");

                sections.push(SectionMatch {
                    doc_path: doc_path.to_string(),
                    heading: section.heading.clone(),
                    line_start: section.line_start,
                    line_end: section.line_end,
                    bm25_score: 0.0,
                    content: section_content,
                    canonicality: score_canonicality(doc_path, entry),
                });
            }
        }
    }

    sections
}

/// Select sections from an "other" type doc
pub(crate) fn select_sections_for_other(
    doc_path: &str,
    index: &ForwardIndex,
    entry: &FileEntry,
) -> Vec<SectionMatch> {
    let mut sections = Vec::new();

    if let Ok(content) = read_indexed_doc(index, doc_path, entry) {
        let lines: Vec<&str> = content.lines().collect();

        // Include only the first section (overview)
        if !entry.section_fingerprints.is_empty() {
            let section = &entry.section_fingerprints[0];
            let start = section.line_start.saturating_sub(1);
            let end = section.line_end.min(lines.len());

            if start < end {
                let section_content = lines[start..end].join("\n");

                sections.push(SectionMatch {
                    doc_path: doc_path.to_string(),
                    heading: section.heading.clone(),
                    line_start: section.line_start,
                    line_end: section.line_end,
                    bm25_score: 0.0,
                    content: section_content,
                    canonicality: score_canonicality(doc_path, entry),
                });
            }
        }
    }

    sections
}

/// Resolve cross-references into additional sections to include
pub(crate) fn resolve_crossrefs(
    crossrefs: &[CrossRef],
    primary_docs: &HashSet<String>,
    index: &ForwardIndex,
    xref_token_budget: usize,
) -> Vec<SectionMatch> {
    const MAX_SECTIONS_PER_ADR: usize = 3;
    const MAX_SECTIONS_PER_DESIGN: usize = 2;
    const MAX_SECTIONS_PER_OPS: usize = 2;
    const MAX_TOKENS_PER_XREF_DOC: usize = 600;

    let mut xref_sections = Vec::new();
    let mut remaining_budget = xref_token_budget;
    let mut visited_docs: HashSet<String> = primary_docs.clone();

    // Group crossrefs by target doc
    let mut doc_refs: HashMap<String, Vec<&CrossRef>> = HashMap::new();
    for cr in crossrefs {
        // Skip if already in primary docs or visited
        if visited_docs.contains(&cr.target_doc_path) {
            continue;
        }

        doc_refs
            .entry(cr.target_doc_path.clone())
            .or_default()
            .push(cr);
    }

    // Sort target docs by priority and score
    let mut target_docs: Vec<(String, Vec<&CrossRef>)> = doc_refs.into_iter().collect();
    target_docs.sort_by(|a, b| {
        let type_a = classify_target_doc(&a.0);
        let type_b = classify_target_doc(&b.0);

        // First by doc type priority
        let cmp = type_a.cmp(&type_b);
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }

        // Then by number of references (descending)
        b.1.len().cmp(&a.1.len())
    });

    // Process each target doc in priority order
    for (target_path, refs) in target_docs {
        if remaining_budget == 0 {
            break;
        }

        // Get file entry
        let Some(entry) = index.files.get(&target_path) else {
            continue; // Doc not in index
        };

        let doc_type = classify_target_doc(&target_path);

        // Select sections based on doc type
        let mut doc_sections = match doc_type {
            DocType::Adr => {
                select_sections_for_adr(&target_path, index, entry, MAX_SECTIONS_PER_ADR)
            }
            DocType::Design => {
                // Check if any ref has an anchor
                let anchor = refs.iter().find_map(|r| r.target_anchor.as_deref());
                select_sections_for_design(
                    &target_path,
                    index,
                    entry,
                    anchor,
                    MAX_SECTIONS_PER_DESIGN,
                )
            }
            DocType::Ops => {
                select_sections_for_ops(&target_path, index, entry, MAX_SECTIONS_PER_OPS)
            }
            DocType::Other => select_sections_for_other(&target_path, index, entry),
        };

        // Apply per-doc token budget
        let mut doc_tokens = 0;
        let mut filtered_sections = Vec::new();

        for section in doc_sections.drain(..) {
            let section_tokens = estimate_tokens(&section.content);

            if doc_tokens + section_tokens > MAX_TOKENS_PER_XREF_DOC {
                break; // Exceeded per-doc limit
            }

            if remaining_budget < section_tokens {
                break; // Exceeded global budget
            }

            doc_tokens += section_tokens;
            remaining_budget -= section_tokens;
            filtered_sections.push(section);
        }

        if !filtered_sections.is_empty() {
            visited_docs.insert(target_path.clone());
            xref_sections.extend(filtered_sections);
        }
    }

    xref_sections
}

/// Resolve cross-references using the persisted relation graph (graph-aware mode).
/// Finds all documents reachable from primary docs via relation edges and
/// includes their sections within the token budget.
pub(crate) fn resolve_crossrefs_from_relations(
    relation_index: &RelationIndex,
    primary_docs: &HashSet<String>,
    index: &ForwardIndex,
    xref_token_budget: usize,
) -> Vec<SectionMatch> {
    const MAX_TOKENS_PER_XREF_DOC: usize = 600;

    // Collect target docs reachable from primary docs, with edge info
    let mut target_edges: HashMap<String, Vec<&RelationEdge>> = HashMap::new();
    for edge in &relation_index.edges {
        if primary_docs.contains(&edge.source) && !primary_docs.contains(&edge.target) {
            target_edges
                .entry(edge.target.clone())
                .or_default()
                .push(edge);
        }
    }

    // Sort targets: more edges = higher priority, then by doc type, then alphabetical
    let mut targets: Vec<(String, Vec<&RelationEdge>)> = target_edges.into_iter().collect();
    targets.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));

    let mut xref_sections = Vec::new();
    let mut remaining_budget = xref_token_budget;
    let mut visited: HashSet<String> = primary_docs.clone();

    for (target_path, edges) in targets {
        if remaining_budget == 0 {
            break;
        }
        if visited.contains(&target_path) {
            continue;
        }

        let Some(entry) = index.files.get(&target_path) else {
            continue;
        };

        // Pick anchor from first edge that has one
        let anchor = edges.iter().find_map(|e| e.anchor.as_deref());

        // Select sections: if anchor, try targeted; otherwise first few sections
        let doc_type = classify_target_doc(&target_path);
        let max_sections = match doc_type {
            DocType::Adr => 3,
            DocType::Design => 2,
            DocType::Ops => 2,
            DocType::Other => 2,
        };

        let mut doc_sections = match doc_type {
            DocType::Adr => select_sections_for_adr(&target_path, index, entry, max_sections),
            DocType::Design => {
                select_sections_for_design(&target_path, index, entry, anchor, max_sections)
            }
            DocType::Ops => select_sections_for_ops(&target_path, index, entry, max_sections),
            DocType::Other => select_sections_for_other(&target_path, index, entry),
        };

        // Apply token budget
        let mut doc_tokens = 0;
        let mut filtered = Vec::new();
        for section in doc_sections.drain(..) {
            let section_tokens = estimate_tokens(&section.content);
            if doc_tokens + section_tokens > MAX_TOKENS_PER_XREF_DOC {
                break;
            }
            if remaining_budget < section_tokens {
                break;
            }
            doc_tokens += section_tokens;
            remaining_budget -= section_tokens;
            filtered.push(section);
        }

        visited.insert(target_path);
        xref_sections.extend(filtered);
    }

    xref_sections
}

// ============================================================================
// Extractive Refiner (Phase 2.3)
// ============================================================================

/// Split text into sentences using simple regex
pub(crate) fn split_sentences(text: &str) -> Vec<String> {
    // Preserve code blocks
    let code_block_re = Regex::new(r"```[\s\S]*?```").unwrap();
    let mut code_blocks = Vec::new();
    let mut placeholder_text = text.to_string();

    // Extract code blocks and replace with placeholders
    for (i, caps) in code_block_re.captures_iter(text).enumerate() {
        let code = caps.get(0).unwrap().as_str();
        code_blocks.push(code.to_string());
        placeholder_text = placeholder_text.replace(code, &format!("__CODE_BLOCK_{i}__"));
    }

    // Split on sentence boundaries: period/exclamation/question followed by space
    // We'll use a simpler approach: split on these punctuation marks and then filter
    let parts: Vec<&str> = placeholder_text.split(&['.', '!', '?']).collect();
    let mut sentences = Vec::new();

    for part in parts {
        let trimmed = part.trim();
        // Keep sentences that are substantial (>10 chars) and start with a letter/number
        if trimmed.len() > 10 {
            let first_char = trimmed.chars().next().unwrap_or(' ');
            if first_char.is_alphanumeric() || first_char == '#' {
                sentences.push(trimmed.to_string());
            }
        }
    }

    // Restore code blocks
    for (i, code) in code_blocks.iter().enumerate() {
        let placeholder = format!("__CODE_BLOCK_{i}__");
        for sentence in &mut sentences {
            *sentence = sentence.replace(&placeholder, code);
        }
    }

    sentences
}

/// Score a sentence for relevance
pub(crate) fn score_sentence(
    sentence: &str,
    query_terms: &[String],
    is_first: bool,
    section_has_crossref: bool,
) -> f64 {
    let mut score = 0.0;

    // Weight factors
    const W_LEXICAL: f64 = 2.0;
    const W_KEYWORD: f64 = 1.5;
    const W_CODE: f64 = 3.0;
    const W_FIRST: f64 = 0.3;
    const W_CROSSREF: f64 = 1.0;

    let sentence_lower = sentence.to_lowercase();

    // 1. Lexical overlap with query
    let mut overlap_count = 0;
    for term in query_terms {
        if sentence_lower.contains(&term.to_lowercase()) {
            overlap_count += 1;
        }
    }
    score += f64::from(overlap_count) * W_LEXICAL;

    // 2. High-value keywords
    let keywords = [
        "deploy",
        "deployment",
        "restart",
        "auth",
        "authentication",
        "session",
        "state",
        "error",
        "failure",
        "retry",
        "timeout",
        "architecture",
        "design",
        "decision",
        "invariant",
        "must",
        "should",
        "requires",
        "context",
        "rationale",
        "consequence",
        "kubernetes",
        "container",
        "pod",
        "service",
        "config",
        "configuration",
        "security",
        "permission",
        "rbac",
        "policy",
        "test",
        "testing",
    ];

    for keyword in &keywords {
        if sentence_lower.contains(keyword) {
            score += W_KEYWORD;
        }
    }

    // 3. Contains code or config
    if sentence.contains("```")
        || sentence.contains("    ")
        || sentence.contains("kubectl")
        || sentence.contains("docker")
        || sentence.contains("make")
        || sentence.contains("cargo")
        || sentence.contains("python")
        || sentence.contains("bash")
    {
        score += W_CODE;
    }

    // 4. First sentence bias
    if is_first {
        score += W_FIRST;
    }

    // 5. Cross-reference bonus
    if section_has_crossref
        && (sentence_lower.contains("adr")
            || sentence_lower.contains("see ")
            || sentence_lower.contains("refer")
            || sentence_lower.contains("described in"))
    {
        score += W_CROSSREF;
    }

    score
}

/// Extract heading from section text
pub(crate) fn extract_heading(text: &str) -> (String, String) {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return (String::new(), String::new());
    }

    // Check if first line is a heading
    let first_line = lines[0].trim();
    if first_line.starts_with('#') {
        let heading = first_line.to_string();
        let body = lines[1..].join("\n");
        (heading, body)
    } else {
        (String::new(), text.to_string())
    }
}

/// Refine a single section by extracting high-signal sentences
pub(crate) fn refine_section(
    section: &SectionMatch,
    query_terms: &[String],
    max_tokens: usize,
) -> RefinedSection {
    let (heading, body) = extract_heading(&section.content);

    // Extract code blocks - preserve them fully
    let code_block_re = Regex::new(r"```[\s\S]*?```").unwrap();
    let code_blocks: Vec<String> = code_block_re
        .captures_iter(&body)
        .map(|cap| cap.get(0).unwrap().as_str().to_string())
        .collect();

    // Extract lists - preserve them
    let list_re = Regex::new(r"(?m)^[\s]*[-*+]\s+.+$").unwrap();
    let list_items: Vec<String> = list_re
        .captures_iter(&body)
        .map(|cap| cap.get(0).unwrap().as_str().to_string())
        .collect();

    // Extract subheadings - preserve them
    let subheading_re = Regex::new(r"(?m)^#{2,6}\s+.+$").unwrap();
    let subheadings: Vec<String> = subheading_re
        .captures_iter(&body)
        .map(|cap| cap.get(0).unwrap().as_str().to_string())
        .collect();

    // Split into sentences
    let sentences = split_sentences(&body);

    if sentences.is_empty() {
        return RefinedSection {
            section: section.clone(),
            truncated: false,
            truncation_reasons: Vec::new(),
        };
    }

    // Check if section has cross-references
    let has_crossref =
        body.to_lowercase().contains("adr") || body.contains('[') && body.contains("](");

    // Score each sentence
    let mut scored_sentences: Vec<(String, f64)> = sentences
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let score = score_sentence(s, query_terms, i == 0, has_crossref);
            (s.clone(), score)
        })
        .collect();

    // Sort by score (descending)
    scored_sentences.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Keep top K sentences
    let total_sentences = sentences.len();
    let k = 6.max((total_sentences as f64 * 0.4).ceil() as usize);

    let top_sentences: Vec<String> = scored_sentences
        .iter()
        .take(k)
        .map(|(s, _)| s.clone())
        .collect();

    // Reconstruct section
    let mut refined_parts = Vec::new();

    // Add heading
    if !heading.is_empty() {
        refined_parts.push(heading.clone());
    }

    // Add preserved elements in order of appearance
    let mut all_preserved = Vec::new();
    all_preserved.extend(code_blocks);
    all_preserved.extend(list_items);
    all_preserved.extend(subheadings);

    // Add top sentences
    for sentence in &top_sentences {
        refined_parts.push(sentence.clone());
    }

    // Add preserved elements
    for item in &all_preserved {
        if !refined_parts.iter().any(|p| p.contains(item)) {
            refined_parts.push(item.clone());
        }
    }

    let refined_text = refined_parts.join("\n\n");
    let (final_text, truncated, truncation_reasons) =
        truncate_text_to_budget(&refined_text, max_tokens, 0);

    RefinedSection {
        section: SectionMatch {
            doc_path: section.doc_path.clone(),
            heading: section.heading.clone(),
            line_start: section.line_start,
            line_end: section.line_end,
            bm25_score: section.bm25_score,
            content: final_text,
            canonicality: section.canonicality,
        },
        truncated,
        truncation_reasons,
    }
}

/// Apply extractive refinement to all sections
pub(crate) fn apply_extractive_refiner(
    sections: Vec<SectionMatch>,
    query: &str,
    max_tokens_per_section: usize,
) -> Vec<RefinedSection> {
    let query_terms = parse_query_terms(query, true);

    sections
        .into_iter()
        .map(|section| refine_section(&section, &query_terms, max_tokens_per_section))
        .collect()
}

pub(crate) fn expand_from_files_args(
    args: &[String],
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut expanded = Vec::new();

    for arg in args {
        if let Some(list_path) = arg.strip_prefix('@') {
            let content = fs::read_to_string(list_path)?;
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    expanded.push(trimmed.to_string());
                }
            }
        } else {
            expanded.push(arg.to_string());
        }
    }

    Ok(expanded)
}

pub(crate) fn resolve_indexed_path(input: &str, index: &ForwardIndex) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut candidates = Vec::new();
    candidates.push(trimmed.to_string());
    candidates.push(trimmed.trim_start_matches("./").to_string());

    let normalized = normalize_path(Path::new(trimmed));
    if !normalized.is_empty() {
        candidates.push(normalized);
    }

    if Path::new(trimmed).is_absolute() {
        if let Some(source_root) = forward_index_source_root(index) {
            if let Ok(stripped) = Path::new(trimmed).strip_prefix(&source_root) {
                let stripped_str = stripped.to_string_lossy().to_string();
                if !stripped_str.is_empty() {
                    candidates.push(stripped_str);
                }
                let normalized_stripped = normalize_path(stripped);
                if !normalized_stripped.is_empty() {
                    candidates.push(normalized_stripped);
                }
            }
        }
    }

    let mut seen = HashSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }
        if index.files.contains_key(&candidate) {
            return Some(candidate);
        }
        let with_dot = format!("./{}", candidate.trim_start_matches("./"));
        if index.files.contains_key(&with_dot) {
            return Some(with_dot);
        }
    }

    None
}

pub(crate) fn resolve_from_files(
    inputs: &[String],
    index: &ForwardIndex,
) -> (Vec<String>, Vec<String>) {
    let mut resolved = Vec::new();
    let mut missing = Vec::new();
    let mut seen = HashSet::new();

    for input in inputs {
        if let Some(path) = resolve_indexed_path(input, index) {
            if seen.insert(path.clone()) {
                resolved.push(path);
            }
        } else {
            missing.push(input.clone());
        }
    }

    (resolved, missing)
}

pub(crate) fn collect_sections_for_files(
    file_paths: &[String],
    index: &ForwardIndex,
    query: &str,
    max_sections: usize,
) -> Vec<SectionMatch> {
    let query_terms = if query.is_empty() {
        Vec::new()
    } else {
        parse_query_terms(query, true)
    };
    let mut all_sections = Vec::new();

    for path in file_paths {
        let Some(entry) = index.files.get(path) else {
            continue;
        };
        let doc_score = if query_terms.is_empty() {
            1.0
        } else {
            bm25_score(&query_terms, entry, index.avg_doc_length, &index.idf_map)
        };
        let canonicality = score_canonicality(path, entry);

        if !entry.section_fingerprints.is_empty() {
            if let Ok(content) = read_indexed_doc(index, path, entry) {
                let lines: Vec<&str> = content.lines().collect();
                for section in &entry.section_fingerprints {
                    let start = section.line_start.saturating_sub(1);
                    let end = section.line_end.min(lines.len());
                    if start < end {
                        let section_content = lines[start..end].join("\n");
                        all_sections.push(SectionMatch {
                            doc_path: path.to_string(),
                            heading: section.heading.clone(),
                            line_start: section.line_start,
                            line_end: section.line_end,
                            bm25_score: doc_score,
                            content: section_content,
                            canonicality,
                        });
                    }
                }
            }
        } else if let Ok(content) = read_indexed_doc(index, path, entry) {
            all_sections.push(SectionMatch {
                doc_path: path.to_string(),
                heading: "Full Document".to_string(),
                line_start: 1,
                line_end: content.lines().count(),
                bm25_score: doc_score,
                content,
                canonicality,
            });
        }
    }

    all_sections.sort_by(compare_sections_by_relevance);

    all_sections.into_iter().take(max_sections).collect()
}

pub(crate) fn collect_context_selection(
    query: &str,
    from_files: &[String],
    index: &ForwardIndex,
    max_sections: usize,
) -> Result<ContextSelection, ContextSelectionIssue> {
    let query_label = if query.trim().is_empty() {
        "selected files".to_string()
    } else {
        query.to_string()
    };
    let query_for_refiner = if query.trim().is_empty() {
        String::new()
    } else {
        query.to_string()
    };

    let sections = if !from_files.is_empty() {
        let expanded = expand_from_files_args(from_files)
            .map_err(|_| ContextSelectionIssue::NoIndexedFilesMatched)?;
        let (resolved, missing) = resolve_from_files(&expanded, index);

        if !missing.is_empty() {
            return Err(ContextSelectionIssue::MissingFiles(missing));
        }

        if resolved.is_empty() {
            return Err(ContextSelectionIssue::NoIndexedFilesMatched);
        }

        collect_sections_for_files(&resolved, index, query, max_sections)
    } else {
        let query_terms = parse_query_terms(query, true);
        if query_terms.is_empty() {
            return Err(ContextSelectionIssue::NoSearchableTerms);
        }
        search_relevant_sections(query, index, max_sections)
    };

    if sections.is_empty() {
        return Err(ContextSelectionIssue::NoRelevantSections(query_label));
    }

    Ok(ContextSelection {
        query_label,
        query_for_refiner,
        sections,
    })
}
