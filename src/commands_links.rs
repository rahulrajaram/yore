use crate::commands_graph::*;
use colored::Colorize;
use globset::Glob;
use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::types::*;
use crate::util::*;

pub(crate) fn run_link_check(
    index_dir: &Path,
    root: Option<&Path>,
    include_summary: bool,
    summary_only: bool,
    external_paths: &[String],
) -> Result<LinkCheckResult, Box<dyn std::error::Error>> {
    // Load the forward index
    let forward_index = load_forward_index(index_dir)?;

    // Determine root directory for resolving relative paths
    let root_dir = if let Some(r) = root {
        r.to_path_buf()
    } else if let Some(source_root) = forward_index_source_root(&forward_index) {
        source_root
    } else {
        // Extract root from index by finding common prefix of all paths
        if let Some((first_path, _)) = forward_index.files.iter().next() {
            let first_path = Path::new(first_path);
            if let Some(parent) = first_path.parent() {
                // Walk up to find the common root
                let mut candidate = parent.to_path_buf();
                while candidate.parent().is_some() {
                    let parent_path = candidate.parent().unwrap();
                    // Check if this is the common root by checking if it contains "docs"
                    if candidate.file_name().and_then(|s| s.to_str()) == Some("docs") {
                        break;
                    }
                    candidate = parent_path.to_path_buf();
                }
                candidate.parent().unwrap_or(Path::new(".")).to_path_buf()
            } else {
                Path::new(".").to_path_buf()
            }
        } else {
            Path::new(".").to_path_buf()
        }
    };

    // Build file set for fast lookup (keys of the HashMap)
    let file_set: HashSet<String> = forward_index.files.keys().cloned().collect();

    // Build heading index for anchor validation
    let mut heading_index: HashMap<String, HashSet<String>> = HashMap::new();
    for (path, entry) in &forward_index.files {
        let mut anchors = HashSet::new();
        for heading in &entry.headings {
            // Convert heading text to anchor format (lowercase, replace spaces with hyphens)
            let anchor = heading.text.to_lowercase().replace(' ', "-");
            anchors.insert(anchor);
        }
        heading_index.insert(path.clone(), anchors);
    }

    let mut broken_links = Vec::new();
    let mut total_links = 0;

    // Cache file lines for context snippets
    let mut file_lines_cache: HashMap<String, Vec<String>> = HashMap::new();

    // Summary accumulators
    let mut counts_by_file: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut counts_by_kind: HashMap<String, usize> = HashMap::new();

    // Iterate through all files and check their links
    for (file_path, entry) in &forward_index.files {
        for link in &entry.links {
            total_links += 1;

            let target = &link.target;

            // Skip external links (http://, https://, mailto:, etc.)
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

            let line_number = link.line;

            // Resolve relative path
            let resolved_path = if link_path.is_empty() {
                // Just an anchor in the current file
                file_path.clone()
            } else if let Some(stripped) = link_path.strip_prefix('/') {
                // Absolute path from root
                root_dir.join(stripped).to_string_lossy().to_string()
            } else {
                // Relative path
                let source_path = Path::new(file_path);
                if let Some(parent) = source_path.parent() {
                    parent.join(&link_path).to_string_lossy().to_string()
                } else {
                    link_path.clone()
                }
            };

            // Normalize path (remove ./ and resolve ../)
            let normalized_path = normalize_path(Path::new(&resolved_path));

            // Placeholder targets: treat as lower-severity broken links
            if !link_path.is_empty() && is_placeholder_target(&link_path) {
                let context = get_link_context(&mut file_lines_cache, file_path, line_number)?;
                let kind = LinkKind::Placeholder;
                record_link_kind(&mut counts_by_file, &mut counts_by_kind, file_path, &kind);
                broken_links.push(BrokenLink {
                    source_file: file_path.clone(),
                    line_number,
                    link_text: link.text.clone(),
                    link_target: target.clone(),
                    error: format!("Placeholder link target: {link_path}"),
                    anchor: anchor.clone(),
                    context,
                });
                continue;
            }

            // File-level checks only when there is an explicit path component
            if !link_path.is_empty() {
                let meta = fs::metadata(&normalized_path).ok();
                let exists = meta.is_some();
                let is_dir = meta.as_ref().is_some_and(std::fs::Metadata::is_dir);

                if exists && is_dir {
                    // Valid directory reference
                    record_link_kind(
                        &mut counts_by_file,
                        &mut counts_by_kind,
                        file_path,
                        &LinkKind::DirectoryReference,
                    );
                } else if exists {
                    // File exists on disk but may not be indexed (e.g., code)
                    if !file_set.contains(&normalized_path) {
                        let ext = file_extension(&normalized_path);
                        let kind = if is_code_extension(&ext) {
                            LinkKind::CodeReference
                        } else {
                            LinkKind::ExternalReference
                        };
                        record_link_kind(
                            &mut counts_by_file,
                            &mut counts_by_kind,
                            file_path,
                            &kind,
                        );
                    }
                } else {
                    // File not found locally - check external repos
                    let mut found_in_external = false;
                    for ext_path in external_paths {
                        // Check if the link might be pointing to an external repo
                        // by seeing if the normalized path contains the external path pattern
                        if normalized_path.contains(ext_path) {
                            // The link references an external repo path, try to resolve it
                            if Path::new(&normalized_path).exists() {
                                found_in_external = true;
                                record_link_kind(
                                    &mut counts_by_file,
                                    &mut counts_by_kind,
                                    file_path,
                                    &LinkKind::ExternalReference,
                                );
                                break;
                            }
                        }
                        // Also check if it's a relative path that would resolve to external repo
                        let resolved_ext = Path::new(ext_path)
                            .join(Path::new(&link_path).file_name().unwrap_or_default());
                        if resolved_ext.exists() {
                            found_in_external = true;
                            record_link_kind(
                                &mut counts_by_file,
                                &mut counts_by_kind,
                                file_path,
                                &LinkKind::ExternalReference,
                            );
                            break;
                        }
                    }

                    if found_in_external {
                        continue;
                    }

                    // Missing target file: classify as doc_missing or code_missing
                    let ext = file_extension(&normalized_path);
                    let kind = if is_code_extension(&ext) {
                        LinkKind::CodeMissing
                    } else {
                        LinkKind::DocMissing
                    };
                    let context = get_link_context(&mut file_lines_cache, file_path, line_number)?;
                    record_link_kind(&mut counts_by_file, &mut counts_by_kind, file_path, &kind);
                    broken_links.push(BrokenLink {
                        source_file: file_path.clone(),
                        line_number,
                        link_text: link.text.clone(),
                        link_target: target.clone(),
                        error: format!("Target file not found: {normalized_path}"),
                        anchor: anchor.clone(),
                        context,
                    });
                    continue;
                }
            }

            // Check anchor if present
            if let Some(ref anchor_text) = anchor {
                let target_file = if link_path.is_empty() {
                    file_path
                } else {
                    &normalized_path
                };

                if let Some(anchors) = heading_index.get(target_file) {
                    if !anchors.contains(anchor_text as &str) {
                        let context =
                            get_link_context(&mut file_lines_cache, file_path, line_number)?;
                        let kind = LinkKind::AnchorMissing;
                        record_link_kind(
                            &mut counts_by_file,
                            &mut counts_by_kind,
                            file_path,
                            &kind,
                        );
                        broken_links.push(BrokenLink {
                            source_file: file_path.clone(),
                            line_number,
                            link_text: link.text.clone(),
                            link_target: target.clone(),
                            error: format!("Anchor not found: #{anchor_text}"),
                            anchor: Some(anchor_text.clone()),
                            context,
                        });
                    }
                } else {
                    let context = get_link_context(&mut file_lines_cache, file_path, line_number)?;
                    let kind = LinkKind::AnchorUnverified;
                    record_link_kind(&mut counts_by_file, &mut counts_by_kind, file_path, &kind);
                    broken_links.push(BrokenLink {
                        source_file: file_path.clone(),
                        line_number,
                        link_text: link.text.clone(),
                        link_target: target.clone(),
                        error: format!(
                            "Could not verify anchor (file has no headings): #{anchor_text}"
                        ),
                        anchor: Some(anchor_text.clone()),
                        context,
                    });
                }
            }
        }
    }

    let valid_links = total_links - broken_links.len();

    let mut result = LinkCheckResult {
        total_links,
        valid_links,
        broken_links: broken_links.len(),
        broken: broken_links.clone(),
        summary: None,
    };

    // Build summary if requested
    if include_summary || summary_only {
        let mut by_file_vec: Vec<LinkSummaryByFile> = counts_by_file
            .into_iter()
            .map(|(file, counts)| LinkSummaryByFile { file, counts })
            .collect();
        by_file_vec.sort_by(|a, b| a.file.cmp(&b.file));

        let mut by_kind_vec: Vec<LinkSummaryByKind> = counts_by_kind
            .into_iter()
            .map(|(kind, count)| LinkSummaryByKind { kind, count })
            .collect();
        by_kind_vec.sort_by(|a, b| a.kind.cmp(&b.kind));

        result.summary = Some(LinkCheckSummary {
            by_file: by_file_vec,
            by_kind: by_kind_vec,
        });
    }

    Ok(result)
}

/// User-facing link check command that prints results.
pub(crate) fn cmd_check_links(
    index_dir: &Path,
    json: bool,
    root: Option<&Path>,
    summary_flag: bool,
    summary_only: bool,
    external_paths: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let include_summary = summary_flag || summary_only || !json;
    let result = run_link_check(
        index_dir,
        root,
        include_summary,
        summary_only,
        external_paths,
    )?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    // Recompute root directory for display purposes only
    let forward_index = load_forward_index(index_dir)?;
    let display_root = if let Some(r) = root {
        r.to_path_buf()
    } else if let Some((first_path, _)) = forward_index.files.iter().next() {
        let first_path = Path::new(first_path);
        first_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        Path::new(".").to_path_buf()
    };

    println!(
        "{} {}",
        "Checking links in".cyan().bold(),
        display_root.display()
    );
    println!();

    println!("{}", "Link Check Results".cyan().bold());
    println!("{}", "=".repeat(60));
    println!();
    println!("Total links:  {}", result.total_links);
    println!(
        "Valid links:  {} {}",
        result.valid_links,
        "✓".green().bold()
    );
    println!(
        "Broken links: {} {}",
        result.broken_links,
        if result.broken_links == 0 {
            "✓".green().bold().to_string()
        } else {
            "✗".red().bold().to_string()
        }
    );
    println!();

    if let Some(summary) = &result.summary {
        println!("{}", "Summary by kind:".cyan().bold());
        for item in &summary.by_kind {
            println!("  - {:<18} {}", item.kind, item.count);
        }
        println!();
    }

    if !summary_only && !result.broken.is_empty() {
        println!("{}", "Broken Links:".red().bold());
        println!();

        for (idx, link) in result.broken.iter().enumerate() {
            println!("[{}] {}", idx + 1, link.source_file.white().bold());
            println!("    Link: [{}]({})", link.link_text, link.link_target);
            if link.line_number > 0 {
                println!("    Line: {}", link.line_number);
            }
            if let Some(ref ctx) = link.context {
                println!("    Context: {ctx}");
            }
            println!("    Error: {}", link.error.red());
            println!();
        }
    }

    Ok(())
}

/// Load a single-line context snippet for a link location.
pub(crate) fn get_link_context(
    cache: &mut HashMap<String, Vec<String>>,
    file_path: &str,
    line_number: usize,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if line_number == 0 {
        return Ok(None);
    }

    // Load and cache file lines if needed
    if !cache.contains_key(file_path) {
        let content = fs::read_to_string(file_path)?;
        let lines: Vec<String> = content
            .lines()
            .map(std::string::ToString::to_string)
            .collect();
        cache.insert(file_path.to_string(), lines);
    }

    let lines = cache.get(file_path).unwrap();
    if line_number == 0 || line_number > lines.len() {
        return Ok(None);
    }

    let mut line = lines[line_number - 1].clone();
    if line.len() > 160 {
        line.truncate(157);
        line.push_str("...");
    }

    Ok(Some(line))
}

pub(crate) fn load_policy_config(path: &Path) -> Result<PolicyConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let cfg: PolicyConfig = serde_yaml::from_str(&content)?;
    Ok(cfg)
}

pub(crate) fn rule_severity(rule: &PolicyRule) -> String {
    rule.severity.as_deref().unwrap_or("error").to_string()
}

pub(crate) fn rule_name(rule: &PolicyRule) -> String {
    rule.name.clone().unwrap_or_else(|| rule.pattern.clone())
}

#[derive(Debug)]
pub(crate) struct PolicySection {
    heading: String,
    line_start: usize,
    line_end: usize,
}

pub(crate) fn parse_policy_sections(content: &str) -> Vec<PolicySection> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let heading_re = Regex::new(r"^(#{1,6})\s+(.+)$").unwrap();
    let mut sections: Vec<PolicySection> = Vec::new();
    let mut current: Option<PolicySection> = None;

    for (idx, line) in lines.iter().enumerate() {
        if let Some(caps) = heading_re.captures(line) {
            let heading = caps
                .get(2)
                .map_or_else(|| "Untitled".to_string(), |m| m.as_str().trim().to_string());

            if let Some(mut prev) = current.take() {
                if idx > 0 {
                    prev.line_end = idx;
                }
                sections.push(prev);
            }

            current = Some(PolicySection {
                heading,
                line_start: idx + 1,
                line_end: lines.len(),
            });
        }
    }

    if let Some(mut last) = current {
        last.line_end = lines.len();
        sections.push(last);
    }

    if sections.is_empty() {
        sections.push(PolicySection {
            heading: "Full Document".to_string(),
            line_start: 1,
            line_end: lines.len(),
        });
    }

    sections
}

#[derive(Debug)]
pub(crate) struct LinkTarget {
    path: String,
    anchor: Option<String>,
}

pub(crate) fn extract_markdown_link_targets(file_path: &str, content: &str) -> Vec<LinkTarget> {
    let mut targets = Vec::new();
    let link_regex = Regex::new(r"(!?)\[(?P<label>[^\]]+)\]\((?P<target>[^)]+)\)").unwrap();

    let origin_dir = Path::new(file_path)
        .parent()
        .unwrap_or_else(|| Path::new("."));

    for caps in link_regex.captures_iter(content) {
        if caps.get(1).is_some_and(|m| m.as_str() == "!") {
            continue;
        }

        let target_str = match caps.name("target") {
            Some(t) => t.as_str(),
            None => continue,
        };

        if target_str.starts_with("http://")
            || target_str.starts_with("https://")
            || target_str.starts_with("mailto:")
            || target_str.starts_with("ftp://")
        {
            continue;
        }

        let (path_part, anchor) = if let Some(hash_pos) = target_str.find('#') {
            (
                &target_str[..hash_pos],
                Some(target_str[hash_pos + 1..].to_string()),
            )
        } else {
            (target_str, None)
        };

        if path_part.is_empty() {
            continue;
        }

        let lc = path_part.to_ascii_lowercase();
        if !lc.ends_with(".md") && !lc.ends_with(".txt") && !lc.ends_with(".rst") {
            continue;
        }

        let target_path = if let Some(stripped) = path_part.strip_prefix('/') {
            PathBuf::from(stripped)
        } else {
            origin_dir.join(path_part)
        };

        let normalized = normalize_path(&target_path);
        targets.push(LinkTarget {
            path: normalized,
            anchor,
        });
    }

    targets
}

pub(crate) fn normalize_required_link(file_path: &str, required: &str) -> (String, Option<String>) {
    let (path_part, anchor) = if let Some(hash_pos) = required.find('#') {
        (
            &required[..hash_pos],
            Some(required[hash_pos + 1..].to_string()),
        )
    } else {
        (required, None)
    };

    if path_part.starts_with("http://")
        || path_part.starts_with("https://")
        || path_part.starts_with("mailto:")
        || path_part.starts_with("ftp://")
    {
        return (required.to_string(), anchor);
    }

    let path_part = path_part.trim_start_matches("./");
    let resolved = if path_part.is_empty() {
        PathBuf::from(file_path)
    } else if path_part.starts_with("../") {
        let origin_dir = Path::new(file_path)
            .parent()
            .unwrap_or_else(|| Path::new("."));
        origin_dir.join(path_part)
    } else if path_part.starts_with('/') || path_part.contains('/') {
        PathBuf::from(path_part.trim_start_matches('/'))
    } else {
        let origin_dir = Path::new(file_path)
            .parent()
            .unwrap_or_else(|| Path::new("."));
        origin_dir.join(path_part)
    };

    (normalize_path(&resolved), anchor)
}

pub(crate) fn collect_policy_violations_for_content(
    rule: &PolicyRule,
    file_path: &str,
    content: &str,
) -> Vec<PolicyViolation> {
    let mut violations = Vec::new();

    // Required substrings
    for needle in &rule.must_contain {
        if !content.contains(needle) {
            violations.push(PolicyViolation {
                file: file_path.to_string(),
                rule: rule_name(rule),
                message: format!("Missing required content: {needle:?}"),
                severity: rule_severity(rule),
                kind: "policy_violation".to_string(),
            });
        }
    }

    // Forbidden substrings
    for needle in &rule.must_not_contain {
        if content.contains(needle) {
            violations.push(PolicyViolation {
                file: file_path.to_string(),
                rule: rule_name(rule),
                message: format!("Forbidden content present: {needle:?}"),
                severity: rule_severity(rule),
                kind: "policy_violation".to_string(),
            });
        }
    }

    // Length-based checks (line count)
    let line_count = content.lines().count();
    if let Some(min_len) = rule.min_length {
        if line_count < min_len {
            violations.push(PolicyViolation {
                file: file_path.to_string(),
                rule: rule_name(rule),
                message: format!(
                    "Document too short: {line_count} lines (min required: {min_len})"
                ),
                severity: rule_severity(rule),
                kind: "policy_violation".to_string(),
            });
        }
    }
    if let Some(max_len) = rule.max_length {
        if line_count > max_len {
            violations.push(PolicyViolation {
                file: file_path.to_string(),
                rule: rule_name(rule),
                message: format!("Document too long: {line_count} lines (max allowed: {max_len})"),
                severity: rule_severity(rule),
                kind: "policy_violation".to_string(),
            });
        }
    }

    // Heading-based checks
    if !rule.required_headings.is_empty() || !rule.forbidden_headings.is_empty() {
        let heading_re = Regex::new(r"^(#{1,6})\s+(.+)$").unwrap();
        let mut headings: Vec<String> = Vec::new();

        for line in content.lines() {
            if let Some(caps) = heading_re.captures(line) {
                if let Some(text_match) = caps.get(2) {
                    let text = text_match.as_str().trim().to_string();
                    headings.push(text);
                }
            }
        }

        // Required headings (by text)
        for h in &rule.required_headings {
            if !headings.iter().any(|t| t == h) {
                violations.push(PolicyViolation {
                    file: file_path.to_string(),
                    rule: rule_name(rule),
                    message: format!("Missing required heading: {h:?}"),
                    severity: rule_severity(rule),
                    kind: "policy_violation".to_string(),
                });
            }
        }

        // Forbidden headings (by text)
        for h in &rule.forbidden_headings {
            if headings.iter().any(|t| t == h) {
                violations.push(PolicyViolation {
                    file: file_path.to_string(),
                    rule: rule_name(rule),
                    message: format!("Forbidden heading present: {h:?}"),
                    severity: rule_severity(rule),
                    kind: "policy_violation".to_string(),
                });
            }
        }
    }

    // Section length checks (line count)
    if let Some(max_section_len) = rule.max_section_length {
        let heading_filter = match rule.section_heading_regex.as_deref() {
            Some(pattern) => match Regex::new(pattern) {
                Ok(re) => Some(re),
                Err(_) => {
                    violations.push(PolicyViolation {
                        file: file_path.to_string(),
                        rule: rule_name(rule),
                        message: format!("Invalid section heading regex: {pattern:?}"),
                        severity: rule_severity(rule),
                        kind: "policy_violation".to_string(),
                    });
                    return violations;
                }
            },
            None => None,
        };

        for section in parse_policy_sections(content) {
            if let Some(ref re) = heading_filter {
                if !re.is_match(&section.heading) {
                    continue;
                }
            }

            let section_len = if section.line_end >= section.line_start {
                section.line_end - section.line_start + 1
            } else {
                0
            };

            if section_len > max_section_len {
                violations.push(PolicyViolation {
                    file: file_path.to_string(),
                    rule: rule_name(rule),
                    message: format!(
                        "Section too long: {:?} is {} lines (max allowed: {})",
                        section.heading, section_len, max_section_len
                    ),
                    severity: rule_severity(rule),
                    kind: "policy_violation".to_string(),
                });
            }
        }
    }

    // Required link checks
    if !rule.must_link_to.is_empty() {
        let targets = extract_markdown_link_targets(file_path, content);
        let mut target_paths: HashSet<String> = HashSet::new();
        let mut target_keys: HashSet<String> = HashSet::new();

        for target in targets {
            target_paths.insert(target.path.clone());
            let key = match target.anchor {
                Some(anchor) => format!("{}#{}", target.path, anchor),
                None => target.path.clone(),
            };
            target_keys.insert(key);
        }

        for required in &rule.must_link_to {
            let (req_path, req_anchor) = normalize_required_link(file_path, required);
            let satisfied = if let Some(anchor) = req_anchor {
                target_keys.contains(&format!("{req_path}#{anchor}"))
            } else {
                target_paths.contains(&req_path)
            };

            if !satisfied {
                violations.push(PolicyViolation {
                    file: file_path.to_string(),
                    rule: rule_name(rule),
                    message: format!("Missing required link: {required:?}"),
                    severity: rule_severity(rule),
                    kind: "policy_violation".to_string(),
                });
            }
        }
    }

    violations
}

pub(crate) fn run_policy_check(
    index_dir: &Path,
    policy_path: &Path,
) -> Result<PolicyCheckResult, Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let policy = load_policy_config(policy_path)?;

    let mut violations = Vec::new();

    for rule in &policy.rules {
        let glob = Glob::new(&rule.pattern)?;
        let matcher = glob.compile_matcher();

        for file_path in forward_index.files.keys() {
            if !matcher.is_match(file_path.as_str()) {
                continue;
            }

            let content = fs::read_to_string(file_path.as_str())?;
            let mut rule_violations =
                collect_policy_violations_for_content(rule, file_path, &content);
            violations.append(&mut rule_violations);
        }
    }

    Ok(PolicyCheckResult {
        policy_file: policy_path.to_string_lossy().to_string(),
        total_violations: violations.len(),
        violations,
    })
}

pub(crate) fn cmd_policy(
    config_path: &Path,
    index_dir: &Path,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !config_path.exists() {
        return Err(format!("Policy file not found: {}", config_path.display()).into());
    }

    let result = run_policy_check(index_dir, config_path)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if result.violations.is_empty() {
        println!(
            "{} No policy violations found ({}).",
            "✓".green().bold(),
            result.policy_file
        );
        return Ok(());
    }

    println!(
        "{} Policy violations found using {}",
        "✗".red().bold(),
        result.policy_file
    );
    println!("{}", "=".repeat(60));
    println!();

    for v in &result.violations {
        println!("{}", v.file.white().bold());
        println!("  Rule: {}", v.rule);
        println!("  Severity: {}", v.severity);
        println!("  Kind: {}", v.kind);
        println!("  Message: {}", v.message);
        println!();
    }

    println!("Total violations: {}", result.total_violations);

    Ok(())
}

/// Suggest a new link target based on available files in the index.
/// Very conservative: only rewrites when there is exactly one file with
/// the same filename as the link target and that file lives under the
/// same parent directory as the source file.
/// Find all candidate files that match the broken link's filename
pub(crate) fn find_link_candidates(
    source_file: &str,
    link_path: &str,
    available_files: &HashSet<String>,
) -> Vec<String> {
    if link_path.is_empty() {
        return vec![];
    }

    let Some(link_filename) = Path::new(link_path).file_name().and_then(|s| s.to_str()) else {
        return vec![];
    };

    let source_path = Path::new(source_file);
    let source_parent = source_path.parent().unwrap_or(Path::new("."));

    // Find all candidates whose filename matches
    let mut candidates: Vec<String> = available_files
        .iter()
        .filter(|p| {
            Path::new(p)
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|name| name == link_filename)
        })
        .map(|candidate| {
            // Try to create a relative path from source to candidate
            let candidate_path = Path::new(candidate);
            if let Ok(stripped) = candidate_path.strip_prefix(source_parent) {
                let rel = stripped.to_string_lossy().to_string();
                if !rel.is_empty() {
                    return rel;
                }
            }
            // Fall back to returning the full path
            candidate.clone()
        })
        .collect();

    candidates.sort();
    candidates
}

#[allow(dead_code)] // Utility for future interactive fix mode
pub(crate) fn suggest_new_link_target(
    source_file: &str,
    link_path: &str,
    available_files: &HashSet<String>,
) -> Option<String> {
    let candidates = find_link_candidates(source_file, link_path, available_files);
    if candidates.len() == 1 {
        Some(candidates.into_iter().next().unwrap())
    } else {
        None
    }
}

pub(crate) fn cmd_fix_links(
    index_dir: &Path,
    dry_run: bool,
    apply: bool,
    propose: Option<PathBuf>,
    apply_decisions: Option<PathBuf>,
    json: bool,
    use_git_history: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Handle apply-decisions mode: read and apply a proposal file
    if let Some(decisions_path) = apply_decisions {
        return apply_link_decisions(&decisions_path, dry_run, json);
    }

    // Validate mode flags for regular operation
    let propose_mode = propose.is_some();
    if !propose_mode && !dry_run && !apply {
        return Err("Specify --dry-run, --apply, or --propose <file>".into());
    }

    let forward_index = load_forward_index(index_dir)?;
    let available_files: HashSet<String> = forward_index.files.keys().cloned().collect();

    // Load git rename history if requested and available
    let rename_history: Option<RenameHistory> = if use_git_history {
        let rename_path = index_dir.join("rename_history.json");
        if rename_path.exists() {
            let content = fs::read_to_string(&rename_path)?;
            Some(serde_json::from_str(&content)?)
        } else {
            eprintln!(
                "Warning: --use-git-history requested but no rename_history.json found. \
                 Run 'yore build --track-renames' first."
            );
            None
        }
    } else {
        None
    };

    let mut fixes: Vec<LinkFix> = Vec::new();
    let mut proposals: Vec<LinkFixProposal> = Vec::new();

    for (file_path, entry) in &forward_index.files {
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

            // Check if link resolves
            let source_path = Path::new(file_path);
            let resolved = if link_path.is_empty() {
                file_path.clone()
            } else if let Some(parent) = source_path.parent() {
                parent.join(&link_path).to_string_lossy().to_string()
            } else {
                link_path.clone()
            };

            let normalized = normalize_path(Path::new(&resolved));
            if available_files.contains(&normalized) {
                continue;
            }

            // Find candidates using index-based matching
            let mut candidates = find_link_candidates(file_path, &link_path, &available_files);

            // If no candidates found and git history is available, check for renames
            if candidates.is_empty() {
                if let Some(ref history) = rename_history {
                    // Try to resolve the old path to its current location
                    if let Some(new_path) = resolve_renamed_path(&normalized, history) {
                        // Check if the new path exists in available files
                        if available_files.contains(&new_path) {
                            // Convert to relative path from source
                            if let Some(rel) =
                                compute_relative_path(file_path, &new_path, &available_files)
                            {
                                candidates.push(rel);
                            } else {
                                candidates.push(new_path);
                            }
                        }
                    }
                }
            }

            if candidates.is_empty() {
                continue;
            }

            if candidates.len() == 1 {
                // Unambiguous fix
                let mut new_target = candidates[0].clone();
                if let Some(ref a) = anchor {
                    new_target.push('#');
                    new_target.push_str(a);
                }
                if new_target != *target {
                    fixes.push(LinkFix {
                        file: file_path.clone(),
                        old_target: target.clone(),
                        new_target,
                    });
                }
            } else if propose_mode {
                // Multiple candidates - add to proposals
                proposals.push(LinkFixProposal {
                    source: file_path.clone(),
                    line: link.line,
                    broken_target: target.clone(),
                    candidates,
                    decision: None,
                });
            }
        }
    }

    // Handle propose mode: write proposals to file
    if let Some(propose_path) = propose {
        let proposal_file = LinkFixProposalFile {
            version: 1,
            proposals,
        };
        let yaml = serde_yaml::to_string(&proposal_file)?;
        fs::write(&propose_path, &yaml)?;

        if json {
            #[derive(Serialize)]
            struct ProposeResult {
                proposal_file: String,
                unambiguous_fixes: usize,
                ambiguous_proposals: usize,
            }
            let result = ProposeResult {
                proposal_file: propose_path.to_string_lossy().to_string(),
                unambiguous_fixes: fixes.len(),
                ambiguous_proposals: proposal_file.proposals.len(),
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!(
                "{} Wrote {} ambiguous proposals to {}",
                "Propose:".cyan().bold(),
                proposal_file.proposals.len(),
                propose_path.display()
            );
            println!(
                "{} {} unambiguous fixes available (use --apply to apply)",
                "Info:".yellow(),
                fixes.len()
            );
        }
        return Ok(());
    }

    // Regular fix mode (dry-run or apply)
    if fixes.is_empty() {
        if json {
            println!(r#"{{"fixes": [], "applied": false}}"#);
        } else {
            println!("{}", "No safe link fixes found.".green().bold());
        }
        return Ok(());
    }

    // Group fixes by file
    let mut fixes_by_file: HashMap<String, Vec<LinkFix>> = HashMap::new();
    for fix in &fixes {
        fixes_by_file
            .entry(fix.file.clone())
            .or_default()
            .push(fix.clone());
    }

    if json {
        let result = serde_json::json!({
            "fixes": fixes.iter().map(|f| {
                serde_json::json!({
                    "file": f.file,
                    "old_target": f.old_target,
                    "new_target": f.new_target
                })
            }).collect::<Vec<_>>(),
            "applied": apply
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!(
            "{} Proposed link fixes in {} file(s):",
            if dry_run { "Previewing" } else { "Applying" },
            fixes_by_file.len()
        );
        for (file, file_fixes) in &fixes_by_file {
            println!("{}", file.white().bold());
            for f in file_fixes {
                println!("  {} -> {}", f.old_target.red(), f.new_target.green());
            }
        }
    }

    if apply {
        for (file, file_fixes) in &fixes_by_file {
            let content = fs::read_to_string(file)?;
            let mut new_content = content.clone();
            for f in file_fixes {
                let old = format!("]({})", f.old_target);
                let new = format!("]({})", f.new_target);
                new_content = new_content.replace(&old, &new);
            }
            if new_content != content {
                fs::write(file, new_content)?;
            }
        }
        if !json {
            println!("{}", "Link fixes applied.".green().bold());
        }
    }

    Ok(())
}

/// Apply decisions from a proposal file
pub(crate) fn apply_link_decisions(
    decisions_path: &Path,
    dry_run: bool,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(decisions_path)?;
    let proposal_file: LinkFixProposalFile = serde_yaml::from_str(&content)?;

    let mut fixes: Vec<LinkFix> = Vec::new();

    for proposal in &proposal_file.proposals {
        if let Some(decision_idx) = proposal.decision {
            if decision_idx < proposal.candidates.len() {
                let mut new_target = proposal.candidates[decision_idx].clone();
                // Preserve anchor if present in broken_target
                if let Some(idx) = proposal.broken_target.find('#') {
                    new_target.push_str(&proposal.broken_target[idx..]);
                }
                fixes.push(LinkFix {
                    file: proposal.source.clone(),
                    old_target: proposal.broken_target.clone(),
                    new_target,
                });
            }
        }
    }

    if fixes.is_empty() {
        if json {
            println!(
                r#"{{"fixes": [], "applied": false, "message": "No decisions made in proposal file"}}"#
            );
        } else {
            println!(
                "{} No decisions found in {}. Set 'decision' field to candidate index.",
                "Note:".yellow(),
                decisions_path.display()
            );
        }
        return Ok(());
    }

    // Group and apply
    let mut fixes_by_file: HashMap<String, Vec<LinkFix>> = HashMap::new();
    for fix in &fixes {
        fixes_by_file
            .entry(fix.file.clone())
            .or_default()
            .push(fix.clone());
    }

    if json {
        let result = serde_json::json!({
            "fixes": fixes.iter().map(|f| {
                serde_json::json!({
                    "file": f.file,
                    "old_target": f.old_target,
                    "new_target": f.new_target
                })
            }).collect::<Vec<_>>(),
            "applied": !dry_run
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!(
            "{} {} link fixes from decisions:",
            if dry_run { "Would apply" } else { "Applying" },
            fixes.len()
        );
        for (file, file_fixes) in &fixes_by_file {
            println!("{}", file.white().bold());
            for f in file_fixes {
                println!("  {} -> {}", f.old_target.red(), f.new_target.green());
            }
        }
    }

    if !dry_run {
        for (file, file_fixes) in &fixes_by_file {
            let content = fs::read_to_string(file)?;
            let mut new_content = content.clone();
            for f in file_fixes {
                let old = format!("]({})", f.old_target);
                let new = format!("]({})", f.new_target);
                new_content = new_content.replace(&old, &new);
            }
            if new_content != content {
                fs::write(file, new_content)?;
            }
        }
        if !json {
            println!("{}", "Link fixes applied.".green().bold());
        }
    }

    Ok(())
}

pub(crate) fn apply_reference_mapping_to_content(content: &str, from: &str, to: &str) -> String {
    let old = format!("]({from})");
    let new = format!("]({to})");
    content.replace(&old, &new)
}

pub(crate) fn load_reference_mappings(
    path: &Path,
) -> Result<ReferenceMappingConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let cfg: ReferenceMappingConfig = serde_yaml::from_str(&content)?;
    Ok(cfg)
}

pub(crate) fn cmd_fix_references(
    index_dir: &Path,
    mapping_path: &Path,
    dry_run: bool,
    apply: bool,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !dry_run && !apply {
        return Err("Specify either --dry-run or --apply".into());
    }
    if !mapping_path.exists() {
        return Err(format!("Mapping file not found: {}", mapping_path.display()).into());
    }

    let mappings_cfg = load_reference_mappings(mapping_path)?;
    if mappings_cfg.mappings.is_empty() {
        if json {
            let result = FixReferencesResult {
                mapping_file: mapping_path.to_string_lossy().to_string(),
                mappings_count: 0,
                updated_files: vec![],
                applied: apply,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!(
                "{} No mappings defined in {}",
                "Note:".yellow(),
                mapping_path.display()
            );
        }
        return Ok(());
    }

    let forward_index = load_forward_index(index_dir)?;

    let mut changed_files: Vec<String> = Vec::new();

    for file_path in forward_index.files.keys() {
        let content = fs::read_to_string(file_path)?;
        let mut new_content = content.clone();

        for m in &mappings_cfg.mappings {
            new_content = apply_reference_mapping_to_content(&new_content, &m.from, &m.to);
        }

        if new_content != content {
            if dry_run {
                changed_files.push(file_path.clone());
            } else if apply {
                fs::write(file_path, new_content)?;
                changed_files.push(file_path.clone());
            }
        }
    }

    changed_files.sort();

    if json {
        let result = FixReferencesResult {
            mapping_file: mapping_path.to_string_lossy().to_string(),
            mappings_count: mappings_cfg.mappings.len(),
            updated_files: changed_files,
            applied: apply,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if changed_files.is_empty() {
        println!(
            "{} No references needed updating based on {}",
            "Note:".yellow(),
            mapping_path.display()
        );
    } else {
        println!(
            "{} Updated references in {} file(s) using mapping {}",
            if dry_run { "Would update" } else { "Updated" },
            changed_files.len(),
            mapping_path.display()
        );
        for f in changed_files {
            println!("  {f}");
        }
    }

    Ok(())
}
