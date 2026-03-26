use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::types::*;

// Helper functions

pub fn load_forward_index(index_dir: &Path) -> Result<ForwardIndex, Box<dyn std::error::Error>> {
    let path = index_dir.join("forward_index.json");
    let content =
        fs::read_to_string(&path).map_err(|_| "Index not found. Run 'yore build' first.")?;
    Ok(serde_json::from_str(&content)?)
}

/// Load the relation index; returns an empty index if the file does not exist (backward compat).
#[allow(dead_code)] // Used by upcoming YEH-005/006
pub fn load_relation_index(index_dir: &Path) -> RelationIndex {
    let path = index_dir.join("relations.json");
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(RelationIndex {
            version: 1,
            indexed_at: String::new(),
            total_edges: 0,
            edges: vec![],
        }),
        Err(_) => RelationIndex {
            version: 1,
            indexed_at: String::new(),
            total_edges: 0,
            edges: vec![],
        },
    }
}

pub fn load_document_metrics(
    index_dir: &Path,
) -> Result<DocumentMetricsIndex, Box<dyn std::error::Error>> {
    let path = index_dir.join("document_metrics.json");
    let content = fs::read_to_string(&path).map_err(|_| {
        "Health metrics not found. Re-run 'yore build' to persist document metrics."
    })?;
    Ok(serde_json::from_str(&content)?)
}

pub fn load_reverse_index(index_dir: &Path) -> Result<ReverseIndex, Box<dyn std::error::Error>> {
    let path = index_dir.join("reverse_index.json");
    let content =
        fs::read_to_string(&path).map_err(|_| "Index not found. Run 'yore build' first.")?;
    Ok(serde_json::from_str(&content)?)
}

pub fn default_query_stop_words() -> &'static [&'static str] {
    &[
        "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "had", "has", "have", "he",
        "in", "is", "it", "not", "of", "on", "or", "that", "the", "their", "there", "these",
        "they", "this", "to", "was", "we", "were", "what", "when", "where", "which", "who", "will",
        "with", "would", "you", "your", "did", "do", "does", "can", "could", "must", "shall",
        "should", "may", "might", "new", "using", "used", "use", "add", "set", "run", "get", "see",
        "only", "no", "so", "than", "then", "them", "all", "any", "both", "each", "more", "most",
        "some", "such", "own", "same", "just", "also", "now", "other", "into", "about", "up",
        "over",
    ]
}

pub fn default_vocabulary_stop_words() -> &'static [&'static str] {
    &[
        "a",
        "an",
        "and",
        "are",
        "as",
        "at",
        "be",
        "by",
        "for",
        "from",
        "had",
        "has",
        "have",
        "he",
        "in",
        "is",
        "it",
        "not",
        "of",
        "on",
        "or",
        "that",
        "the",
        "their",
        "there",
        "these",
        "they",
        "this",
        "to",
        "was",
        "we",
        "were",
        "what",
        "when",
        "where",
        "which",
        "who",
        "will",
        "with",
        "would",
        "you",
        "your",
        "did",
        "do",
        "does",
        "can",
        "could",
        "must",
        "shall",
        "should",
        "may",
        "might",
        "new",
        "using",
        "used",
        "use",
        "add",
        "set",
        "run",
        "get",
        "see",
        "only",
        "no",
        "so",
        "than",
        "then",
        "them",
        "all",
        "any",
        "both",
        "each",
        "more",
        "most",
        "some",
        "such",
        "own",
        "same",
        "just",
        "also",
        "now",
        "other",
        "into",
        "about",
        "up",
        "over",
        "document",
        "documents",
        "docs",
        "json",
        "changes",
        "change",
        "build",
        "output",
        "validation",
        "command",
        "commands",
        "prompting",
        "workflow",
        "core",
        "keep",
        "apply",
        "file",
        "files",
        "reporting",
        "pattern",
        "examples",
        "help",
        "format",
        "index",
        "indexes",
        "indexer",
        "indexing",
    ]
}

pub fn load_vocabulary_stopwords(
    stopwords: Option<&Path>,
    include_default: bool,
) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let mut words: HashSet<String> = default_vocabulary_stop_words()
        .iter()
        .map(|word| (*word).to_string())
        .collect();

    if !include_default {
        words.clear();
    }

    if let Some(path) = stopwords {
        let path_value = path.to_string_lossy().to_string();
        let content = fs::read_to_string(path)
            .map_err(|err| format!("Unable to read stop-word file '{path_value}': {err}"))?;

        for token in content.split_whitespace() {
            if !token.is_empty() {
                words.insert(token.to_lowercase());
            }
        }
    }

    Ok(words)
}

pub fn is_hygienic_vocabulary_term(term: &str) -> bool {
    if term.len() < 3 || term.len() > 48 {
        return false;
    }

    let mut digits = 0usize;
    let mut letters = 0usize;

    for ch in term.chars() {
        if ch.is_ascii_digit() {
            digits += 1;
        } else if ch.is_ascii_alphabetic() {
            letters += 1;
        } else if !matches!(ch, '-' | '_') {
            return false;
        }
    }

    if letters == 0 {
        return false;
    }

    if digits > 0 && digits.saturating_mul(10) >= term.len().saturating_mul(6) {
        return false;
    }

    true
}

pub fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

pub fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!("{}", duration.as_secs())
}

/// Extract file rename history from git
///
/// Runs `git log --name-status --diff-filter=R` to find all renames in the repo.
/// Returns empty history if not in a git repo or git is unavailable.
pub fn extract_git_renames(path: &Path) -> RenameHistory {
    use std::process::Command;

    let output = Command::new("git")
        .args([
            "log",
            "--name-status",
            "--diff-filter=R",
            "--pretty=format:%H",
            "-M",
            "--",
        ])
        .current_dir(path)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => {
            return RenameHistory {
                renames: vec![],
                indexed_at: chrono_now(),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut renames = Vec::new();
    let mut current_commit = String::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Check if this is a commit hash (40 hex chars)
        if line.len() == 40 && line.chars().all(|c| c.is_ascii_hexdigit()) {
            current_commit = line.to_string();
        } else if line.starts_with('R') {
            // Rename line: R<score>\told_path\tnew_path
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.len() == 3 {
                renames.push(RenameEntry {
                    old_path: parts[1].to_string(),
                    new_path: parts[2].to_string(),
                    commit: current_commit.clone(),
                });
            }
        }
    }

    // Reverse to get oldest-first order
    renames.reverse();

    RenameHistory {
        renames,
        indexed_at: chrono_now(),
    }
}

/// Look up the current path for a file that may have been renamed.
/// Returns the most recent path if renames exist, or None if no rename history.
pub fn resolve_renamed_path(old_path: &str, history: &RenameHistory) -> Option<String> {
    let mut current = old_path.to_string();
    let mut found_any = false;

    for entry in &history.renames {
        if entry.old_path == current {
            current.clone_from(&entry.new_path);
            found_any = true;
        }
    }

    if found_any {
        Some(current)
    } else {
        None
    }
}

/// Compute the relative path from source file to target file.
/// Returns the relative link path as it would appear in markdown.
pub fn compute_relative_path(
    source: &str,
    target: &str,
    _available_files: &HashSet<String>,
) -> Option<String> {
    let source_path = Path::new(source);
    let target_path = Path::new(target);

    // Get the directory containing the source file
    let source_dir = source_path.parent()?;

    // Try to compute relative path
    if let Ok(rel) = target_path.strip_prefix(source_dir) {
        return Some(rel.to_string_lossy().to_string());
    }

    // Need to go up directories - find common ancestor
    let source_components: Vec<_> = source_dir.components().collect();
    let target_components: Vec<_> = target_path.components().collect();

    // Find common prefix length
    let common_len = source_components
        .iter()
        .zip(target_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    // Build relative path: go up (source_components.len() - common_len) times, then down to target
    let ups = source_components.len() - common_len;
    let mut result = String::new();

    for _ in 0..ups {
        result.push_str("../");
    }

    // Add remaining target path components
    for (i, comp) in target_components.iter().enumerate().skip(common_len) {
        if i > common_len {
            result.push('/');
        }
        result.push_str(&comp.as_os_str().to_string_lossy());
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

// ============================================================================
// Context Assembly for LLMs (Phase 2)
// ============================================================================

pub fn combined_section_score(section: &SectionMatch) -> f64 {
    section.bm25_score * 0.7 + section.canonicality * 0.3
}

pub fn compare_sections_by_relevance(a: &SectionMatch, b: &SectionMatch) -> std::cmp::Ordering {
    combined_section_score(b)
        .partial_cmp(&combined_section_score(a))
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.doc_path.cmp(&b.doc_path))
        .then_with(|| a.line_start.cmp(&b.line_start))
        .then_with(|| a.line_end.cmp(&b.line_end))
        .then_with(|| a.heading.cmp(&b.heading))
}

pub fn normalize_content_for_dedupe(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn forward_index_source_root(index: &ForwardIndex) -> Option<PathBuf> {
    let trimmed = index.source_root.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

pub fn canonicalize_existing_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(path)
        } else {
            path.to_path_buf()
        }
    })
}

pub fn build_indexed_doc_key(path: &Path, source_root: &Path) -> String {
    if let Ok(stripped) = path.strip_prefix(source_root) {
        let normalized = normalize_path(stripped);
        if !normalized.is_empty() {
            return normalized;
        }
    }

    let normalized = normalize_path(path);
    if normalized.is_empty() {
        path.to_string_lossy().to_string()
    } else {
        normalized
    }
}

pub fn resolve_doc_fs_path(index: &ForwardIndex, doc_path: &str, entry: &FileEntry) -> PathBuf {
    let stored_path = Path::new(&entry.path);
    if stored_path.is_absolute() {
        return stored_path.to_path_buf();
    }

    if let Some(source_root) = forward_index_source_root(index) {
        let stored_candidate = source_root.join(stored_path);
        if stored_candidate.exists() {
            return stored_candidate;
        }

        let doc_candidate = source_root.join(doc_path);
        if doc_candidate.exists() {
            return doc_candidate;
        }
    }

    PathBuf::from(doc_path)
}

pub fn read_indexed_doc(
    index: &ForwardIndex,
    doc_path: &str,
    entry: &FileEntry,
) -> Result<String, io::Error> {
    fs::read_to_string(resolve_doc_fs_path(index, doc_path, entry))
}

pub fn dedupe_section_matches(sections: Vec<SectionMatch>) -> (Vec<SectionMatch>, usize) {
    let mut unique: Vec<SectionMatch> = Vec::new();
    let mut seen_content = HashSet::new();
    let mut deduped_hits = 0usize;

    for section in sections {
        let overlaps_existing = unique.iter().any(|existing| {
            existing.doc_path == section.doc_path
                && existing.line_start <= section.line_end
                && section.line_start <= existing.line_end
        });

        let content_key = normalize_content_for_dedupe(&section.content);
        let duplicate_content = !content_key.is_empty() && !seen_content.insert(content_key);

        if overlaps_existing || duplicate_content {
            deduped_hits += 1;
            continue;
        }

        unique.push(section);
    }

    (unique, deduped_hits)
}

pub fn floor_char_boundary(text: &str, limit: usize) -> usize {
    let mut idx = limit.min(text.len());
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

pub fn truncate_text_to_budget(
    text: &str,
    max_tokens: usize,
    max_bytes: usize,
) -> (String, bool, Vec<String>) {
    const TRUNCATION_MARKER: &str = " ...[truncated]";

    let mut reasons = Vec::new();
    let mut limit = text.len();

    let token_char_limit = max_tokens.saturating_mul(4);
    if token_char_limit > 0 && text.len() > token_char_limit {
        reasons.push("token_cap".to_string());
        limit = limit.min(token_char_limit);
    }

    if max_bytes > 0 && text.len() > max_bytes {
        reasons.push("byte_cap".to_string());
        limit = limit.min(max_bytes);
    }

    if reasons.is_empty() {
        return (text.to_string(), false, reasons);
    }

    let marker_len = TRUNCATION_MARKER.len();
    let mut marker_budget = usize::MAX;
    if token_char_limit > 0 {
        marker_budget = marker_budget.min(token_char_limit);
    }
    if max_bytes > marker_len {
        marker_budget = marker_budget.min(max_bytes);
    }

    if marker_budget > marker_len {
        limit = limit.min(marker_budget.saturating_sub(marker_len));
    }
    let boundary = floor_char_boundary(text, limit);
    let mut truncated = text[..boundary].trim_end().to_string();

    if marker_budget > marker_len && truncated.len() + marker_len <= marker_budget {
        truncated.push_str(TRUNCATION_MARKER);
    }

    (truncated, true, reasons)
}

pub fn normalize_path(path: &Path) -> String {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::Normal(c) => components.push(c.to_string_lossy().to_string()),
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            _ => {}
        }
    }

    components.join("/")
}
