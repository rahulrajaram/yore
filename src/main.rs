use ahash::AHasher;
use clap::{Parser, Subcommand};
use colored::Colorize;
use ignore::WalkBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use globset::Glob;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// yore – Deterministic documentation indexer and context assembly engine.
///
/// Yore indexes markdown and text documentation, computes BM25 statistics,
/// section fingerprints, link graphs, and canonicality scores, and then
/// assembles minimal, high‑signal context for large language models (LLMs)
/// and automation agents.
///
/// Typical workflow:
///   1. Build an index over your docs with `yore build`.
///   2. Inspect and clean the docs with `query`, `dupes*`, `check-links`,
///      `backlinks`, `orphans`, and `canonicality`.
///   3. Assemble an answer‑ready context for an LLM with `yore assemble`.
///
/// All commands are deterministic and operate over the on‑disk index in
/// `--index` (default: `.yore`).
#[derive(Parser)]
#[command(
    name = "yore",
    author,
    version,
    about = "Fast, deterministic documentation indexer and LLM context assembler",
    long_about = r#"yore is a deterministic documentation indexer and context
assembly engine for large language models (LLMs) and automation agents.

It walks a documentation tree, builds on-disk forward and reverse indexes
(BM25 term statistics, section fingerprints, link graphs, canonicality scores),
and then assembles minimal, high-signal context for a given question.

Typical workflow:
  1. Build an index over your docs with `yore build`.
  2. Inspect and clean the docs using `query`, `dupes*`, `check-links`,
     `backlinks`, `orphans`, and `canonicality`.
  3. Assemble an answer-ready context for an LLM with `yore assemble`.

All commands operate deterministically over the on-disk index in `--index`
(default: `.yore`)."#,
    after_long_help = r#"EXAMPLES

  Build an index over docs/ and write it to .yore:
    yore build docs --output .yore --types md,txt

  Search the index for a free-text query:
    yore query kubernetes deployment --index .yore --limit 5

  Assemble context for an LLM question:
    yore assemble "How does authentication work?" \
      --index .yore --max-tokens 8000 --depth 1 > context.md

  Evaluate retrieval quality against a questions file:
    yore eval --questions questions.jsonl --index .yore

  Inspect structure and documentation quality:
    yore dupes --index .yore
    yore dupes-sections --index .yore --threshold 0.7
    yore check-links --index .yore --json
    yore backlinks docs/architecture/DEPLOYMENT-GUIDE.md --index .yore
    yore orphans --index .yore --exclude README
    yore canonicality --index .yore --threshold 0.7"#
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Config file path
    #[arg(short, long, global = true, default_value = ".yore.toml")]
    config: PathBuf,

    /// Profile name to load from config (limits which roots are indexed; use a full-root profile for whole-repo review)
    #[arg(long, global = true)]
    profile: Option<String>,

    /// Quiet mode - suppress non-essential output
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run one or more documentation checks in a single entrypoint.
    ///
    /// This is the recommended command for CI and agents. It can run
    /// link checks, duplicate detection, taxonomy/policy rules, and
    /// staleness checks, and it supports CI-friendly exit codes.
    ///
    /// Examples:
    ///   # Basic link check (default index)
    ///   yore check --links
    ///
    ///   # CI mode: fail on missing docs or code
    ///   yore check --links --ci --fail-on doc_missing,code_missing
    ///
    ///   # Run links + staleness + taxonomy in one shot
    ///   yore check --links --stale --taxonomy --policy taxonomy.yaml
    Check {
        /// Run link validation (same engine as `check-links`)
        #[arg(long)]
        links: bool,

        /// Run duplicate detection (same engine as `dupes`)
        #[arg(long)]
        dupes: bool,

        /// Run taxonomy / policy checks from a YAML file
        #[arg(long)]
        taxonomy: bool,

        /// Run staleness checks based on mtime and inbound links
        #[arg(long)]
        stale: bool,

        /// CI mode: machine-friendly output and exit codes
        #[arg(long)]
        ci: bool,

        /// Kinds/check IDs that should cause a non-zero exit code (comma or space separated)
        #[arg(long, value_delimiter = ',')]
        fail_on: Vec<String>,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Policy file for taxonomy checks (YAML)
        #[arg(long)]
        policy: Option<PathBuf>,

        /// Staleness threshold in days (files older than this are candidates)
        #[arg(long, default_value = "30")]
        stale_days: u64,
    },
    /// Build forward and reverse indexes over documentation.
    ///
    /// Walks a directory tree, indexes Markdown/text files, and writes
    /// forward and reverse indexes into `--output` (default: `.yore`).
    ///
    /// Agents typically run this once at startup or as part of CI, then
    /// call other commands (`query`, `assemble`, `dupes*`, etc.) against
    /// the resulting index.
    ///
    /// Example:
    ///   yore build docs --output .yore --types md,txt
    Build {
        /// Path to index
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output directory for indexes
        #[arg(short, long, default_value = ".yore")]
        output: PathBuf,

        /// File extensions to index (comma-separated)
        #[arg(short, long, default_value = "md,txt,rst")]
        types: String,

        /// Patterns to exclude (can be repeated)
        #[arg(short, long)]
        exclude: Vec<String>,
    },

    /// Search the index for relevant documents using BM25.
    ///
    /// Accepts free-text terms, ranks documents with BM25 using the
    /// precomputed index, and optionally returns machine-readable JSON.
    ///
    /// Useful for quick inspection by humans and for agents that want to
    /// select candidate files before assembling full context.
    ///
    /// Example:
    ///   yore query kubernetes deployment --index .yore --limit 5
    Query {
        /// Search terms
        terms: Vec<String>,

        /// Maximum results to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,

        /// Show only file paths
        #[arg(short = 'l', long)]
        files_only: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Find documents similar to a reference file.
    ///
    /// Uses both keyword overlap and SimHash fingerprints to identify
    /// documents that are textually similar to the given file.
    ///
    /// Useful for de-duplicating design docs, spotting outdated copies,
    /// or finding related ADRs and guides.
    ///
    /// Example:
    ///   yore similar docs/adr/ADR-0013-retries.md --index .yore --limit 5
    Similar {
        /// Reference file
        file: PathBuf,

        /// Maximum results to show
        #[arg(short = 'n', long, default_value = "5")]
        limit: usize,

        /// Similarity threshold (0.0 to 1.0)
        #[arg(short, long, default_value = "0.3")]
        threshold: f64,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Find duplicate or heavily overlapping documents.
    ///
    /// Groups or lists documents that share a large fraction of content,
    /// based on MinHash and SimHash signatures stored in the index.
    ///
    /// Useful for documentation cleanup and for agents choosing which
    /// version of a document to treat as canonical.
    ///
    /// Example:
    ///   yore dupes --index .yore --threshold 0.35 --group
    Dupes {
        /// Similarity threshold (0.0 to 1.0)
        #[arg(short, long, default_value = "0.35")]
        threshold: f64,

        /// Group duplicates together
        #[arg(long)]
        group: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Find duplicate sections across documents.
    ///
    /// Identifies individual sections (for example headings and their
    /// bodies) that appear in multiple files, even when the files are
    /// otherwise different.
    ///
    /// Helpful for detecting repeated how-to blocks, copy-pasted API
    /// descriptions, or repeated ADR fragments.
    ///
    /// Example:
    ///   yore dupes-sections --index .yore --threshold 0.7 --min-files 2
    DupesSections {
        /// Similarity threshold (0.0 to 1.0)
        #[arg(short, long, default_value = "0.7")]
        threshold: f64,

        /// Minimum number of files sharing a section
        #[arg(short = 'n', long, default_value = "2")]
        min_files: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Show overlapping content and shared sections between two files.
    ///
    /// Compares two files using the index and reports what content they
    /// share, helping you understand drift or duplication between them.
    ///
    /// Example:
    ///   yore diff docs/old.md docs/new.md --index .yore
    Diff {
        /// First file
        file1: PathBuf,

        /// Second file
        file2: PathBuf,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Show high-level index statistics.
    ///
    /// Prints counts of files, headings, links, and top keywords, which
    /// is useful for sanity-checking an index and monitoring drift over time.
    ///
    /// Example:
    ///   yore stats --index .yore --top-keywords 20
    Stats {
        /// Show top N keywords
        #[arg(long, default_value = "20")]
        top_keywords: usize,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Interactive query REPL over the index.
    ///
    /// Starts a simple read-eval-print loop where you can type queries
    /// and inspect results quickly while iterating on documentation.
    ///
    /// Example:
    ///   yore repl --index .yore
    Repl {
        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Assemble a high-signal context digest for LLM consumption.
    ///
    /// Runs the full retrieval pipeline: BM25 ranking, section selection,
    /// link and ADR expansion, extractive refinement, and token-budgeted
    /// trimming to produce a markdown context for a natural language query.
    ///
    /// This is the primary entry point for agents and tools that want a
    /// deterministic, reproducible context to send to an LLM.
    ///
    /// Example:
    ///   yore assemble "How does authentication work?" \
    ///     --index .yore --max-tokens 8000 --depth 1 > context.md
    Assemble {
        /// Natural language query/question
        query: Vec<String>,

        /// Maximum tokens in output (approximate)
        #[arg(short = 't', long, default_value = "8000")]
        max_tokens: usize,

        /// Maximum sections to include
        #[arg(short = 's', long, default_value = "20")]
        max_sections: usize,

        /// Cross-reference expansion depth
        #[arg(short = 'd', long, default_value = "1")]
        depth: usize,

        /// Output format
        #[arg(short = 'f', long, default_value = "markdown")]
        format: String,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Evaluate the retrieval pipeline against test questions.
    ///
    /// Given a JSONL questions file with expected substrings, runs the
    /// same retrieval/assembly pipeline used by `assemble` and reports
    /// whether each question's expected answers were retrieved.
    ///
    /// Useful for regression testing and measuring improvements to docs
    /// or index configuration.
    ///
    /// Example:
    ///   yore eval --questions questions.jsonl --index .yore
    Eval {
        /// Path to questions JSONL file
        #[arg(short, long, default_value = "questions.jsonl")]
        questions: PathBuf,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Check all markdown links for validity.
    ///
    /// Parses all markdown links in indexed documents, resolves relative and
    /// absolute paths, and reports broken targets and anchors.
    ///
    /// Can emit JSON for automated checks in CI or for agents that want to
    /// repair links automatically, including a grouped summary by file and
    /// by issue kind (doc_missing, code_missing, placeholder, etc.).
    ///
    /// Examples:
    ///   # Basic JSON output over default index
    ///   yore check-links --index .yore --json
    ///
    ///   # Docs-only profile with summary for CI
    ///   yore --profile docs check-links --json --summary-only
    CheckLinks {
        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Root directory for resolving relative paths
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Include a grouped summary of link issues
        #[arg(long)]
        summary: bool,

        /// Only show the summary (suppress individual link entries)
        #[arg(long)]
        summary_only: bool,
    },

    /// Find all files that link to a specific file.
    ///
    /// Traverses the link graph to list every document that links to the
    /// given target file, including optional anchors.
    ///
    /// Useful for understanding impact of changes, cleaning up docs, and
    /// deciding whether a document is safe to delete.
    ///
    /// Example:
    ///   yore backlinks docs/architecture/DEPLOYMENT-GUIDE.md --index .yore
    Backlinks {
        /// File to find backlinks for
        file: String,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Find orphaned files with no inbound links.
    ///
    /// Lists documents that are not linked to from anywhere else in the
    /// documentation graph (subject to `--exclude` filters).
    ///
    /// Helpful for identifying dead, experimental, or forgotten documents
    /// that may be candidates for deletion or consolidation.
    ///
    /// Example:
    ///   yore orphans --index .yore --exclude README
    Orphans {
        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Exclude files matching pattern (can be repeated)
        #[arg(short, long)]
        exclude: Vec<String>,
    },

    /// Show canonicality scores for all documents.
    ///
    /// Computes a heuristic "authority" score per document based on naming,
    /// path, and link structure so agents can consistently pick canonical
    /// sources of truth when multiple documents overlap.
    ///
    /// Example:
    ///   yore canonicality --index .yore --threshold 0.7
    Canonicality {
        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Minimum score threshold (0.0 to 1.0)
        #[arg(short, long, default_value = "0.0")]
        threshold: f64,
    },

    /// Automatically fix a subset of broken relative links.
    ///
    /// This command uses heuristics over the index to propose safe,
    /// mechanical rewrites for links that appear to point to the wrong
    /// file (for example, the right filename in the wrong directory).
    ///
    /// Examples:
    ///   yore fix-links --index .yore --dry-run
    ///   yore fix-links --index .yore --apply
    FixLinks {
        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Show proposed edits without modifying any files
        #[arg(long)]
        dry_run: bool,

        /// Apply changes to files on disk
        #[arg(long)]
        apply: bool,
    },

    /// Rewrite references according to an explicit mapping file.
    ///
    /// This promotes the `mv --update-refs` machinery into a more general
    /// bulk rewrite tool, suitable for large documentation reorganizations.
    ///
    /// Example:
    ///   yore fix-references --mapping mappings.yaml --index .yore --dry-run
    FixReferences {
        /// Path to reference mapping configuration (YAML)
        #[arg(short, long)]
        mapping: PathBuf,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Show planned changes without modifying files
        #[arg(long)]
        dry_run: bool,

        /// Apply changes to files on disk
        #[arg(long)]
        apply: bool,
    },

    /// Move a documentation file and optionally update inbound references.
    ///
    /// This is a thin, ergonomic wrapper around link rewrite logic. When
    /// --update-refs is used, all Markdown links that point to the old
    /// path are rewritten to point to the new path.
    ///
    /// Examples:
    ///   yore mv docs/old/auth.md docs/architecture/AUTH.md --update-refs --index .yore
    ///   yore mv agents/tmp/note.md agents/archive/note.md --index .yore
    Mv {
        /// Source path to move from
        from: PathBuf,

        /// Destination path to move to
        to: PathBuf,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Update inbound links that reference the old path
        #[arg(long)]
        update_refs: bool,

        /// Show planned changes without modifying files
        #[arg(long)]
        dry_run: bool,
    },

    /// Report potentially stale documentation based on age and inbound links.
    ///
    /// Uses file modification time and inbound link counts from the index
    /// to highlight documents that may be unmaintained or dead.
    ///
    /// Example:
    ///   yore stale --index .yore --days 90 --min-inlinks 0 --json
    Stale {
        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Minimum age in days to consider a file stale
        #[arg(long, default_value = "90")]
        days: u64,

        /// Minimum inbound link count (files with >= this many links are included)
        #[arg(long, default_value = "0")]
        min_inlinks: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Export the documentation link graph.
    ///
    /// Emits either a JSON representation or a Graphviz DOT file
    /// describing links between indexed documents.
    ///
    /// Example:
    ///   yore export-graph --format json --index .yore
    ///   yore export-graph --format dot --index .yore > graph.dot
    ExportGraph {
        /// Output format: "json" or "dot"
        #[arg(long, default_value = "json")]
        format: String,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Suggest document consolidation based on duplicates and canonicality.
    ///
    /// Uses duplicate detection and canonicality scoring to propose a
    /// canonical document and a set of files that should be merged into it.
    ///
    /// Example:
    ///   yore suggest-consolidation --threshold 0.7 --json --index .yore
    SuggestConsolidation {
        /// Minimum duplicate similarity threshold (0.0 to 1.0)
        #[arg(long, default_value = "0.7")]
        threshold: f64,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Check documentation against declarative policy rules.
    ///
    /// Reads a YAML policy file describing path patterns and required or
    /// forbidden content, and reports any violations it finds.
    ///
    /// Example:
    ///   yore policy --config .yore-policy.yaml --index .yore --json
    Policy {
        /// Path to policy configuration (YAML)
        #[arg(long, default_value = ".yore-policy.yaml")]
        config: PathBuf,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

// Evaluation structures
#[derive(Serialize, Deserialize, Debug, Clone)]
struct Question {
    id: usize,
    q: String,
    expect: Vec<String>,
    #[serde(default)]
    min_hits: Option<usize>,
}

#[derive(Debug, Clone)]
struct EvalResult {
    id: usize,
    question: String,
    hits: usize,
    total: usize,
    passed: bool,
    tokens: usize,
}

// Link checking structures
#[derive(Serialize, Debug, Clone)]
struct BrokenLink {
    source_file: String,
    line_number: usize,
    link_text: String,
    link_target: String,
    error: String,
    anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
enum LinkKind {
    DocMissing,
    CodeMissing,
    Placeholder,
    CodeReference,
    DirectoryReference,
    ExternalReference,
    AnchorMissing,
    AnchorUnverified,
}

#[derive(Serialize, Debug)]
struct LinkSummaryByFile {
    file: String,
    counts: HashMap<String, usize>,
}

#[derive(Serialize, Debug)]
struct LinkSummaryByKind {
    kind: String,
    count: usize,
}

#[derive(Serialize, Debug)]
struct LinkCheckSummary {
    by_file: Vec<LinkSummaryByFile>,
    by_kind: Vec<LinkSummaryByKind>,
}

#[derive(Serialize, Debug)]
struct LinkCheckResult {
    total_links: usize,
    valid_links: usize,
    broken_links: usize,
    broken: Vec<BrokenLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<LinkCheckSummary>,
}

// Policy / taxonomy structures
#[derive(Debug, Deserialize, Default)]
struct PolicyRule {
    /// Glob pattern to match files (e.g., "agents/plans/*.md")
    pattern: String,
    /// Required substrings that must appear in matching files
    #[serde(default)]
    must_contain: Vec<String>,
    /// Substrings that must NOT appear in matching files
    #[serde(default)]
    must_not_contain: Vec<String>,
    /// Optional rule name (for clearer reporting)
    #[serde(default)]
    name: Option<String>,
    /// Optional severity ("error" or "warn"), defaults to "error"
    #[serde(default)]
    severity: Option<String>,
    /// Optional minimum document length in lines
    #[serde(default)]
    min_length: Option<usize>,
    /// Optional maximum document length in lines
    #[serde(default)]
    max_length: Option<usize>,
    /// Required markdown headings (by text, without leading '#')
    #[serde(default)]
    required_headings: Vec<String>,
    /// Forbidden markdown headings (by text, without leading '#')
    #[serde(default)]
    forbidden_headings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PolicyConfig {
    #[serde(default)]
    rules: Vec<PolicyRule>,
}

#[derive(Serialize, Debug)]
struct PolicyViolation {
    file: String,
    rule: String,
    message: String,
    severity: String,
    /// Always "policy_violation" so agents can key off kind
    kind: String,
}

#[derive(Serialize, Debug)]
struct PolicyCheckResult {
    policy_file: String,
    total_violations: usize,
    violations: Vec<PolicyViolation>,
}

#[derive(Serialize, Debug, Default)]
struct CombinedCheckResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    links: Option<LinkCheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy: Option<PolicyCheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stale: Option<StaleResult>,
}

#[derive(Serialize, Debug)]
struct StaleFile {
    file: String,
    days_since_modified: u64,
    inbound_links: usize,
}

#[derive(Serialize, Debug)]
struct StaleResult {
    total_stale: usize,
    files: Vec<StaleFile>,
}

#[derive(Serialize, Debug)]
struct GraphNode {
    id: String,
}

#[derive(Serialize, Debug)]
struct GraphEdge {
    source: String,
    target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    anchor: Option<String>,
}

#[derive(Serialize, Debug)]
struct GraphExport {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[derive(Serialize, Debug)]
struct ConsolidationGroup {
    canonical: String,
    merge_into: Vec<String>,
    canonical_score: f64,
    avg_similarity: f64,
    note: String,
}

#[derive(Serialize, Debug)]
struct ConsolidationResult {
    total_groups: usize,
    groups: Vec<ConsolidationGroup>,
}

#[derive(Debug, Clone)]
struct LinkFix {
    file: String,
    old_target: String,
    new_target: String,
}

// Backlinks structures
#[derive(Serialize, Debug, Clone)]
struct Backlink {
    source_file: String,
    link_text: String,
    link_target: String,
    anchor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReferenceMapping {
    from: String,
    to: String,
}

#[derive(Debug, Deserialize)]
struct ReferenceMappingConfig {
    #[serde(default)]
    mappings: Vec<ReferenceMapping>,
}

#[derive(Serialize, Debug)]
struct BacklinksResult {
    target_file: String,
    total_backlinks: usize,
    backlinks: Vec<Backlink>,
}

// Orphans structures
#[derive(Serialize, Debug, Clone)]
struct OrphanFile {
    file: String,
    size_bytes: u64,
    line_count: usize,
}

#[derive(Serialize, Debug)]
struct OrphansResult {
    total_orphans: usize,
    orphans: Vec<OrphanFile>,
}

// Canonicality structures
#[derive(Serialize, Debug, Clone)]
struct CanonicalityScore {
    file: String,
    score: f64,
    reasons: Vec<String>,
}

#[derive(Serialize, Debug)]
struct CanonicalityResult {
    total_files: usize,
    files: Vec<CanonicalityScore>,
}

// Index structures
#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileEntry {
    path: String,
    size_bytes: u64,
    line_count: usize,
    headings: Vec<Heading>,
    keywords: Vec<String>,
    body_keywords: Vec<String>, // keywords from full text
    links: Vec<Link>,
    simhash: u64, // content fingerprint
    #[serde(default)]
    term_frequencies: HashMap<String, usize>, // term counts for BM25
    #[serde(default)]
    doc_length: usize, // total terms for BM25
    #[serde(default)]
    minhash: Vec<u64>, // MinHash signature for LSH
    #[serde(default)]
    section_fingerprints: Vec<SectionFingerprint>, // NEW: section-level SimHash
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Heading {
    line: usize,
    level: usize,
    text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Link {
    line: usize,
    text: String,
    target: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SectionFingerprint {
    heading: String,
    level: usize,
    line_start: usize,
    line_end: usize,
    simhash: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ReverseEntry {
    file: String,
    line: Option<usize>,
    heading: Option<String>,
    level: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ForwardIndex {
    files: HashMap<String, FileEntry>,
    indexed_at: String,
    version: u32, // index version for compatibility
    #[serde(default)]
    avg_doc_length: f64, // NEW: average document length for BM25
    #[serde(default)]
    idf_map: HashMap<String, f64>, // NEW: IDF scores for BM25
}

#[derive(Serialize, Deserialize, Debug)]
struct ReverseIndex {
    keywords: HashMap<String, Vec<ReverseEntry>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct IndexStats {
    total_files: usize,
    total_keywords: usize,
    total_headings: usize,
    total_links: usize,
    indexed_at: String,
}

#[derive(Deserialize, Debug, Clone)]
struct IndexProfileConfig {
    #[serde(default)]
    roots: Vec<String>,
    #[serde(default)]
    types: Vec<String>,
    output: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct YoreConfig {
    #[serde(default)]
    index: HashMap<String, IndexProfileConfig>,
}

fn load_config(path: &Path, quiet: bool) -> Option<YoreConfig> {
    if !path.exists() {
        return None;
    }

    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            if !quiet {
                eprintln!(
                    "{}: failed to read config {}: {}",
                    "warning".yellow(),
                    path.display(),
                    e
                );
            }
            return None;
        }
    };

    match toml::from_str::<YoreConfig>(&contents) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            if !quiet {
                eprintln!(
                    "{}: failed to parse config {}: {}",
                    "warning".yellow(),
                    path.display(),
                    e
                );
            }
            None
        }
    }
}

fn resolve_build_params(
    path: PathBuf,
    output: PathBuf,
    types: String,
    profile: Option<&str>,
    config: &Option<YoreConfig>,
) -> (PathBuf, PathBuf, String, Option<Vec<PathBuf>>) {
    // Defaults from CLI definition
    let default_path = PathBuf::from(".");
    let default_output = PathBuf::from(".yore");
    let default_types = "md,txt,rst".to_string();

    let mut effective_path = path;
    let mut effective_output = output;
    let mut effective_types = types;
    let mut roots: Option<Vec<PathBuf>> = None;

    if let (Some(profile_name), Some(cfg)) = (profile, config.as_ref()) {
        if let Some(profile_cfg) = cfg.index.get(profile_name) {
            // Roots: if present, use them as allowed roots (multi-root support)
            if !profile_cfg.roots.is_empty() {
                let rs: Vec<PathBuf> = profile_cfg.roots.iter().map(PathBuf::from).collect();
                roots = Some(rs);
                // Use repo root (".") as walk root when using multiple roots
                effective_path = default_path.clone();
            }

            // Types: only override when CLI used the default
            if effective_types == default_types && !profile_cfg.types.is_empty() {
                effective_types = profile_cfg.types.join(",");
            }

            // Output: only override when CLI used the default
            if effective_output == default_output {
                if let Some(ref out) = profile_cfg.output {
                    effective_output = PathBuf::from(out);
                }
            }
        }
    }

    (effective_path, effective_output, effective_types, roots)
}

fn resolve_index_path(
    index: PathBuf,
    profile: Option<&str>,
    config: &Option<YoreConfig>,
) -> PathBuf {
    let default_index = PathBuf::from(".yore");

    if index != default_index {
        return index;
    }

    if let (Some(profile_name), Some(cfg)) = (profile, config.as_ref()) {
        if let Some(profile_cfg) = cfg.index.get(profile_name) {
            if let Some(ref out) = profile_cfg.output {
                return PathBuf::from(out);
            }
        }
    }

    index
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}: {}", "error".red().bold(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Handle SIGPIPE / broken pipe panics gracefully (e.g., when piping into `head`).
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("{}", info);
        if msg.contains("Broken pipe (os error 32)") {
            // Treat broken pipe as a normal early exit with success.
            std::process::exit(0);
        }
        default_hook(info);
    }));

    let cli = Cli::parse();
    let config = load_config(&cli.config, cli.quiet);

    let result = match cli.command {
        Commands::Check {
            links,
            dupes: _,
            taxonomy,
            stale,
            ci,
            fail_on,
            index,
            policy,
            stale_days: _,
        } => {
            let index_path =
                resolve_index_path(index, cli.profile.as_deref(), &config);

            let mut combined = CombinedCheckResult::default();

            // Run link checks if requested
            if links {
                let include_summary = true;
                let link_result = run_link_check(&index_path, None, include_summary, false)?;
                combined.links = Some(link_result);
            }

            // Run policy checks if requested
            if taxonomy {
                let policy_path = match policy {
                    Some(p) => p,
                    None => PathBuf::from(".yore-policy.yaml"),
                };
                let policy_result = run_policy_check(&index_path, &policy_path)?;
                combined.policy = Some(policy_result);
            }

            // Run staleness checks if requested (using default thresholds for now)
            if stale {
                let stale_result = run_stale_check(&index_path, 90, 0)?;
                combined.stale = Some(stale_result);
            }

            // For now, `check` always prints JSON.
            let json_str = serde_json::to_string_pretty(&combined)?;
            println!("{}", json_str);

            // CI/fail-on logic: allow both link kinds and policy severities.
            if ci && !fail_on.is_empty() {
                let mut should_fail = false;

                // Link-based failure conditions (existing behavior)
                if links {
                    if let Some(link_result) = &combined.links {
                        if let Some(summary) = &link_result.summary {
                            for key in &fail_on {
                                if let Some(kind) =
                                    summary.by_kind.iter().find(|k| &k.kind == key)
                                {
                                    if kind.count > 0 {
                                        should_fail = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                // Policy-based failure conditions: keyed by severity.
                // Supported keys:
                //   - "policy_error"  – fail if any violation has severity "error"
                //   - "policy_warn"   – fail if any violation has severity "warn" / "warning"
                if taxonomy {
                    if let Some(policy_result) = &combined.policy {
                        let fail_on_error = fail_on.iter().any(|k| k == "policy_error");
                        let fail_on_warn = fail_on
                            .iter()
                            .any(|k| k == "policy_warn" || k == "policy_warning");

                        if fail_on_error || fail_on_warn {
                            for v in &policy_result.violations {
                                let sev = v.severity.as_str();
                                if (fail_on_error && sev == "error")
                                    || (fail_on_warn
                                        && (sev == "warn" || sev == "warning"))
                                {
                                    should_fail = true;
                                    break;
                                }
                            }
                        }
                    }
                }

                if should_fail {
                    std::process::exit(1);
                }
            }

            Ok(())
        }
        Commands::Build {
            path,
            output,
            types,
            exclude,
        } => {
            let (path, output, types, roots) =
                resolve_build_params(path, output, types, cli.profile.as_deref(), &config);
            cmd_build(&path, &output, &types, &exclude, cli.quiet, roots.as_deref())
        }
        Commands::Query {
            terms,
            limit,
            files_only,
            json,
            index,
        } => cmd_query(&terms, limit, files_only, json, &index),
        Commands::Similar {
            file,
            limit,
            threshold,
            json,
            index,
        } => cmd_similar(&file, limit, threshold, json, &index),
        Commands::Dupes {
            threshold,
            group,
            json,
            index,
        } => cmd_dupes(threshold, group, json, &index),
        Commands::DupesSections {
            threshold,
            min_files,
            json,
            index,
        } => cmd_dupes_sections(threshold, min_files, json, &index),
        Commands::Diff {
            file1,
            file2,
            index,
        } => cmd_diff(&file1, &file2, &index),
        Commands::Stats {
            top_keywords,
            index,
        } => cmd_stats(top_keywords, &index),
        Commands::Repl { index } => cmd_repl(&index),
        Commands::Assemble {
            query,
            max_tokens,
            max_sections,
            depth,
            format,
            index,
        } => cmd_assemble(
            &query.join(" "),
            max_tokens,
            max_sections,
            depth,
            &format,
            &index,
        ),
        Commands::Eval { questions, index } => cmd_eval(&questions, &index),
        Commands::CheckLinks {
            index,
            json,
            root,
            summary,
            summary_only,
        } => {
            let index_path =
                resolve_index_path(index, cli.profile.as_deref(), &config);
            cmd_check_links(&index_path, json, root.as_deref(), summary, summary_only)
        }
        Commands::Backlinks { file, index, json } => cmd_backlinks(&file, &index, json),
        Commands::Orphans {
            index,
            json,
            exclude,
        } => cmd_orphans(&index, json, &exclude),
        Commands::Canonicality {
            index,
            json,
            threshold,
        } => cmd_canonicality(&index, json, threshold),
        Commands::ExportGraph { format, index } => {
            cmd_export_graph(&index, &format)
        }
        Commands::SuggestConsolidation {
            threshold,
            json,
            index,
        } => cmd_suggest_consolidation(&index, threshold, json),
        Commands::Policy {
            config,
            index,
            json,
        } => cmd_policy(&config, &index, json),
        Commands::FixLinks {
            index,
            dry_run,
            apply,
        } => cmd_fix_links(&index, dry_run, apply),
        Commands::FixReferences {
            mapping,
            index,
            dry_run,
            apply,
        } => cmd_fix_references(&index, &mapping, dry_run, apply),
        Commands::Mv {
            from,
            to,
            index,
            update_refs,
            dry_run,
        } => cmd_mv(&from, &to, &index, update_refs, dry_run),
        Commands::Stale {
            index,
            days,
            min_inlinks,
            json,
        } => cmd_stale(&index, days, min_inlinks, json),
    };
    result
}

fn cmd_build(
    path: &Path,
    output: &Path,
    types: &str,
    exclude: &[String],
    quiet: bool,
    roots: Option<&[PathBuf]>,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    if !quiet {
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
        version: 3, // Version 3 includes BM25 (term_frequencies, idf_map) and MinHash
        avg_doc_length: 0.0,
        idf_map: HashMap::new(),
    };

    let mut reverse_index = ReverseIndex {
        keywords: HashMap::new(),
    };

    let mut file_count = 0;
    let mut total_headings = 0;
    let mut total_links = 0;

    for entry in builder.build().filter_map(|e| e.ok()) {
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
            .map(|e| e.to_lowercase())
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
        if let Ok(entry) = index_file(path) {
            let rel_path = path
                .strip_prefix(std::env::current_dir()?)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

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

    fs::write(&forward_path, serde_json::to_string_pretty(&forward_index)?)?;
    fs::write(&reverse_path, serde_json::to_string_pretty(&reverse_index)?)?;

    let stats = IndexStats {
        total_files: file_count,
        total_keywords: reverse_index.keywords.len(),
        total_headings,
        total_links,
        indexed_at: chrono_now(),
    };
    fs::write(&stats_path, serde_json::to_string_pretty(&stats)?)?;

    let elapsed = start.elapsed();

    if !quiet {
        println!();
        println!("{}", "Index Statistics".green().bold());
        println!("  Files indexed:    {}", file_count.to_string().cyan());
        println!(
            "  Unique keywords:  {}",
            reverse_index.keywords.len().to_string().cyan()
        );
        println!("  Total headings:   {}", total_headings.to_string().cyan());
        println!("  Total links:      {}", total_links.to_string().cyan());
        println!("  Time elapsed:     {:.2?}", elapsed);
        println!();
        println!(
            "{} {}",
            "Indexes written to".green(),
            output.display().to_string().cyan()
        );
    }

    Ok(())
}

fn index_file(path: &Path) -> Result<FileEntry, Box<dyn std::error::Error>> {
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
                level: caps.get(1).map(|m| m.as_str().len()).unwrap_or(1),
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

    // Compute simhash fingerprint
    let simhash = compute_simhash(&content);

    Ok(FileEntry {
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
    })
}

fn extract_keywords(text: &str) -> Vec<String> {
    let stop_words: HashSet<&str> = [
        "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by",
        "from", "as", "is", "was", "are", "were", "been", "be", "have", "has", "had", "do", "does",
        "did", "will", "would", "could", "should", "may", "might", "must", "shall", "can", "need",
        "this", "that", "these", "those", "i", "you", "he", "she", "it", "we", "they", "what",
        "which", "who", "whom", "whose", "where", "when", "why", "how", "all", "each", "every",
        "both", "few", "more", "most", "other", "some", "such", "no", "nor", "not", "only", "own",
        "same", "so", "than", "too", "very", "just", "also", "now", "here", "using", "used", "use",
        "new", "first", "last", "next", "then", "see", "get", "set", "run", "add", "create",
        "update", "delete",
    ]
    .into_iter()
    .collect();

    let word_re = Regex::new(r"[a-zA-Z][a-zA-Z0-9_-]*").unwrap();

    word_re
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .filter(|w| w.len() >= 3 && !stop_words.contains(w.as_str()))
        .collect()
}

/// Simple suffix-stripping stemmer
fn stem_word(word: &str) -> String {
    let w = word.to_lowercase();

    // Common suffixes to strip
    let suffixes = [
        "ization", "ational", "iveness", "fulness", "ousness", "ation", "ement", "ment", "able",
        "ible", "ness", "ical", "ings", "ing", "ies", "ive", "ful", "ous", "ity", "ed", "ly", "er",
        "es", "s",
    ];

    for suffix in suffixes {
        if w.len() > suffix.len() + 2 && w.ends_with(suffix) {
            return w[..w.len() - suffix.len()].to_string();
        }
    }

    w
}

/// Compute simhash fingerprint for content
fn compute_simhash(content: &str) -> u64 {
    let mut v = [0i32; 64];

    // Extract features (word shingles)
    let words: Vec<&str> = content.split_whitespace().collect();

    for window in words.windows(3) {
        let shingle = format!("{} {} {}", window[0], window[1], window[2]);
        let h = hash_string(&shingle);

        for (i, item) in v.iter_mut().enumerate() {
            if (h >> i) & 1 == 1 {
                *item += 1;
            } else {
                *item -= 1;
            }
        }
    }

    // Convert to fingerprint
    let mut fingerprint: u64 = 0;
    for (i, item) in v.iter().enumerate() {
        if *item > 0 {
            fingerprint |= 1 << i;
        }
    }

    fingerprint
}

fn hash_string(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Count differing bits between two simhashes (Hamming distance)
fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Convert hamming distance to similarity (0.0 to 1.0)
fn simhash_similarity(a: u64, b: u64) -> f64 {
    let distance = hamming_distance(a, b);
    1.0 - (distance as f64 / 64.0)
}

/// Index sections of a document with SimHash fingerprints
fn index_sections(content: &str, headings: &[Heading]) -> Vec<SectionFingerprint> {
    let lines: Vec<&str> = content.lines().collect();
    let mut sections = Vec::new();

    if headings.is_empty() {
        return sections;
    }

    for i in 0..headings.len() {
        let start = headings[i].line.saturating_sub(1);
        let end = headings
            .get(i + 1)
            .map(|h| h.line.saturating_sub(1))
            .unwrap_or(lines.len());

        // Extract section text
        let section_text = lines[start..end].join("\n");

        sections.push(SectionFingerprint {
            heading: headings[i].text.clone(),
            level: headings[i].level,
            line_start: start + 1,
            line_end: end,
            simhash: compute_simhash(&section_text),
        });
    }

    sections
}

/// Compute MinHash signature for a set of keywords
fn compute_minhash(keywords: &[String], num_hashes: usize) -> Vec<u64> {
    let mut hashes = vec![u64::MAX; num_hashes];

    for keyword in keywords {
        for (i, hash_slot) in hashes.iter_mut().enumerate().take(num_hashes) {
            let mut hasher = AHasher::default();
            keyword.hash(&mut hasher);
            i.hash(&mut hasher); // Use index as seed
            let h = hasher.finish();

            *hash_slot = (*hash_slot).min(h);
        }
    }

    hashes
}

/// Compute MinHash similarity (Jaccard estimate)
fn minhash_similarity(a: &[u64], b: &[u64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();

    matches as f64 / a.len() as f64
}

/// Compute BM25 score for a document given query terms
fn bm25_score(
    query_terms: &[String],
    doc: &FileEntry,
    avg_doc_length: f64,
    idf_map: &HashMap<String, f64>,
) -> f64 {
    const K1: f64 = 1.5;
    const B: f64 = 0.75;

    if doc.doc_length == 0 {
        return 0.0;
    }

    let mut score = 0.0;
    let norm_factor = 1.0 - B + B * (doc.doc_length as f64 / avg_doc_length);

    for term in query_terms {
        let stemmed = stem_word(&term.to_lowercase());
        let tf = *doc.term_frequencies.get(&stemmed).unwrap_or(&0) as f64;
        let idf = idf_map.get(&stemmed).unwrap_or(&0.0);

        if tf > 0.0 {
            score += idf * (tf * (K1 + 1.0)) / (tf + K1 * norm_factor);
        }
    }

    score
}

/// Build LSH buckets for fast duplicate detection
fn lsh_buckets(files: &HashMap<String, FileEntry>, bands: usize) -> HashMap<u64, Vec<String>> {
    let rows_per_band = 128 / bands; // Assuming 128 hashes
    let mut buckets: HashMap<u64, Vec<String>> = HashMap::new();

    for (path, entry) in files {
        if entry.minhash.is_empty() {
            continue; // Skip files without MinHash
        }

        for band in 0..bands {
            let start = band * rows_per_band;
            let end = (start + rows_per_band).min(entry.minhash.len());

            // Hash this band's values
            let mut hasher = AHasher::default();
            for val in &entry.minhash[start..end] {
                val.hash(&mut hasher);
            }
            let band_hash = hasher.finish();

            buckets.entry(band_hash).or_default().push(path.clone());
        }
    }

    buckets
}

fn cmd_query(
    terms: &[String],
    limit: usize,
    files_only: bool,
    json: bool,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let _reverse_index = load_reverse_index(index_dir)?;
    let forward_index = load_forward_index(index_dir)?;

    // Compute BM25 scores for all documents
    let mut file_scores: Vec<(String, f64)> = forward_index
        .files
        .iter()
        .map(|(path, entry)| {
            let score = bm25_score(
                terms,
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
    file_scores.truncate(limit);

    let results = file_scores;

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
        return Ok(());
    }

    if results.is_empty() {
        println!("{}", "No results found.".yellow());
        return Ok(());
    }

    println!(
        "{} results for: {}\n",
        results.len().to_string().green().bold(),
        terms.join(" ").cyan()
    );

    for (file, score) in results {
        if files_only {
            println!("{}", file);
        } else {
            println!("{} (score: {:.2})", file.cyan(), score);

            // Show matching headings
            if let Some(entry) = forward_index.files.get(&file) {
                for heading in entry.headings.iter().take(3) {
                    let heading_keywords: HashSet<String> = extract_keywords(&heading.text)
                        .into_iter()
                        .map(|k| stem_word(&k))
                        .collect();

                    let matches: Vec<_> = terms
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

    Ok(())
}

fn cmd_similar(
    file: &Path,
    limit: usize,
    threshold: f64,
    json: bool,
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
        .ok_or_else(|| format!("File not in index: {}", file_str))?;

    // Combine heading and body keywords
    let ref_keywords: HashSet<String> = ref_entry
        .keywords
        .iter()
        .chain(ref_entry.body_keywords.iter())
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
                serde_json::json!({
                    "path": p,
                    "jaccard": j,
                    "simhash": s,
                    "combined": c
                })
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
    }

    Ok(())
}

fn cmd_dupes(
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

fn compute_duplicate_pairs(
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

fn build_consolidation_groups(
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

        let (canonical, canonical_score) = match best {
            Some(v) => v,
            None => continue,
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
fn cmd_diff(
    file1: &Path,
    file2: &Path,
    index_dir: &Path,
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
    let shared_headings: Vec<_> = h1.intersection(&h2).collect();

    if !shared_headings.is_empty() {
        println!();
        println!(
            "{} ({} headings)",
            "Identical Headings".red().bold(),
            shared_headings.len()
        );
        for h in shared_headings.iter().take(10) {
            println!("  - {}", h);
        }
        if shared_headings.len() > 10 {
            println!("  ... and {} more", shared_headings.len() - 10);
        }
    }

    Ok(())
}

/// Find duplicate sections across documents
fn cmd_dupes_sections(
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

    for section in all_sections.iter() {
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

fn cmd_stats(top_keywords: usize, index_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let reverse_index = load_reverse_index(index_dir)?;

    // Count keyword occurrences
    let mut keyword_counts: Vec<_> = reverse_index
        .keywords
        .iter()
        .map(|(k, v)| (k.clone(), v.len()))
        .collect();
    keyword_counts.sort_by(|a, b| b.1.cmp(&a.1));

    let total_headings: usize = forward_index.files.values().map(|e| e.headings.len()).sum();
    let total_links: usize = forward_index.files.values().map(|e| e.links.len()).sum();
    let total_body_keywords: usize = forward_index
        .files
        .values()
        .map(|e| e.body_keywords.len())
        .sum();

    println!("{}", "Index Statistics".green().bold());
    println!();
    println!(
        "  Total files:       {}",
        forward_index.files.len().to_string().cyan()
    );
    println!(
        "  Unique keywords:   {}",
        reverse_index.keywords.len().to_string().cyan()
    );
    println!("  Total headings:    {}", total_headings.to_string().cyan());
    println!(
        "  Body keywords:     {}",
        total_body_keywords.to_string().cyan()
    );
    println!("  Total links:       {}", total_links.to_string().cyan());
    println!(
        "  Index version:     {}",
        forward_index.version.to_string().dimmed()
    );
    println!("  Indexed at:        {}", forward_index.indexed_at.dimmed());
    println!();
    println!(
        "{}",
        format!("Top {} Keywords", top_keywords).green().bold()
    );
    println!();

    for (keyword, count) in keyword_counts.iter().take(top_keywords) {
        let bar = "=".repeat((count / 2).min(40));
        println!("  {:>20} {:>4} {}", keyword.cyan(), count, bar.dimmed());
    }

    Ok(())
}

fn cmd_repl(index_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "yore interactive mode (v2)".green().bold());
    println!("Commands: query <terms>, similar <file>, dupes, diff <f1> <f2>, stats, help, quit\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("{} ", ">".cyan().bold());
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "quit" | "exit" | "q" => break,
            "help" | "?" => {
                println!("  query <terms...>   - Search for keywords");
                println!("  similar <file>     - Find similar files");
                println!("  dupes              - Find duplicates");
                println!("  diff <f1> <f2>     - Compare two files");
                println!("  stats              - Show statistics");
                println!("  quit               - Exit");
            }
            "query" => {
                let terms: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                if terms.is_empty() {
                    println!("{}", "Usage: query <terms...>".yellow());
                } else {
                    let _ = cmd_query(&terms, 10, false, false, index_dir);
                }
            }
            "similar" => {
                if parts.len() < 2 {
                    println!("{}", "Usage: similar <file>".yellow());
                } else {
                    let _ = cmd_similar(Path::new(parts[1]), 5, 0.3, false, index_dir);
                }
            }
            "dupes" => {
                let _ = cmd_dupes(0.35, false, false, index_dir);
            }
            "diff" => {
                if parts.len() < 3 {
                    println!("{}", "Usage: diff <file1> <file2>".yellow());
                } else {
                    let _ = cmd_diff(Path::new(parts[1]), Path::new(parts[2]), index_dir);
                }
            }
            "stats" => {
                let _ = cmd_stats(10, index_dir);
            }
            _ => {
                // Treat as query
                let terms: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
                let _ = cmd_query(&terms, 10, false, false, index_dir);
            }
        }
        println!();
    }

    Ok(())
}

// Helper functions

fn load_forward_index(index_dir: &Path) -> Result<ForwardIndex, Box<dyn std::error::Error>> {
    let path = index_dir.join("forward_index.json");
    let content =
        fs::read_to_string(&path).map_err(|_| "Index not found. Run 'yore build' first.")?;
    Ok(serde_json::from_str(&content)?)
}

fn load_reverse_index(index_dir: &Path) -> Result<ReverseIndex, Box<dyn std::error::Error>> {
    let path = index_dir.join("reverse_index.json");
    let content =
        fs::read_to_string(&path).map_err(|_| "Index not found. Run 'yore build' first.")?;
    Ok(serde_json::from_str(&content)?)
}

fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
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

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!("{}", duration.as_secs())
}

// ============================================================================
// Context Assembly for LLMs (Phase 2)
// ============================================================================

#[derive(Debug, Clone)]
struct SectionMatch {
    doc_path: String,
    heading: String,
    line_start: usize,
    line_end: usize,
    bm25_score: f64,
    content: String,
    canonicality: f64,
}

// Cross-reference expansion (Phase 2.2)

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RefType {
    MarkdownLink,
    AdrId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CrossRef {
    ref_type: RefType,
    origin_doc_path: String,
    target_doc_path: String,
    target_anchor: Option<String>,
    raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DocType {
    Adr,    // Priority 1
    Design, // Priority 2
    Ops,    // Priority 3
    Other,  // Priority 4
}

/// Search for relevant sections using BM25 scoring
fn search_relevant_sections(
    query: &str,
    index: &ForwardIndex,
    max_sections: usize,
) -> Vec<SectionMatch> {
    let query_terms: Vec<String> = query
        .split_whitespace()
        .map(|s| stem_word(&s.to_lowercase()))
        .collect();

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
            // Use indexed sections
            for section in &entry.section_fingerprints {
                // Read the actual section content
                if let Ok(content) = fs::read_to_string(doc_path) {
                    let lines: Vec<&str> = content.lines().collect();
                    let start = section.line_start.saturating_sub(1);
                    let end = section.line_end.min(lines.len());

                    if start < end {
                        let section_content = lines[start..end].join("\n");

                        all_sections.push(SectionMatch {
                            doc_path: doc_path.to_string(),
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
            if let Ok(content) = fs::read_to_string(doc_path) {
                all_sections.push(SectionMatch {
                    doc_path: doc_path.to_string(),
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

    // Sort by combined score: BM25 * 0.7 + canonicality * 0.3
    all_sections.sort_by(|a, b| {
        let score_a = a.bm25_score * 0.7 + a.canonicality * 0.3;
        let score_b = b.bm25_score * 0.7 + b.canonicality * 0.3;
        score_b
            .partial_cmp(&score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Take top N sections
    all_sections.into_iter().take(max_sections).collect()
}

/// Score document canonicality based on path, recency, and patterns
fn score_canonicality(doc_path: &str, _entry: &FileEntry) -> f64 {
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
fn distill_to_markdown(sections: &[SectionMatch], query: &str, max_tokens: usize) -> String {
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
         **Actual Tokens Used:** ~{}\n\n\
         ---\n\n\
         ## Usage with LLM\n\n\
         Paste this digest into your LLM conversation, then ask:\n\n\
         > Using only the information in the context above, answer: \"{}\"\n\
         > Be explicit when something is not documented in the context.\n",
        used_tokens, query
    );

    output.push_str(&footer);

    output
}

/// Estimate token count (rough approximation: 1 token ≈ 4 chars)
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Build ADR index mapping ADR numbers to file paths
fn build_adr_index(index: &ForwardIndex) -> HashMap<String, String> {
    let mut adr_map = HashMap::new();
    let adr_regex = Regex::new(r"ADR[-_]?(\d{2,4})").unwrap();

    for path in index.files.keys() {
        let path_lower = path.to_lowercase();
        if path_lower.contains("/adr/") || path_lower.contains("adr-") {
            if let Some(caps) = adr_regex.captures(path) {
                if let Some(num_str) = caps.get(1) {
                    // Zero-pad to 3 digits
                    let num: usize = num_str.as_str().parse().unwrap_or(0);
                    let normalized = format!("{:03}", num);
                    adr_map.insert(normalized, path.clone());
                }
            }
        }
    }

    adr_map
}

/// Parse markdown links from a section's content
fn parse_markdown_links(section: &SectionMatch, origin_dir: &Path) -> Vec<CrossRef> {
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
            if !path_part.ends_with(".md")
                && !path_part.ends_with(".txt")
                && !path_part.ends_with(".rst")
            {
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

/// Normalize a path (resolve .. and .)
fn normalize_path(path: &Path) -> String {
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

/// Parse ADR ID references from section content
fn parse_adr_ids(section: &SectionMatch, adr_index: &HashMap<String, String>) -> Vec<CrossRef> {
    let mut refs = Vec::new();

    // Regex: ADR-013, ADR 13, ADR_0013
    let adr_regex = Regex::new(r"\bADR[-_ ]?(?P<num>\d{2,4})\b").unwrap();

    for caps in adr_regex.captures_iter(&section.content) {
        if let Some(num) = caps.name("num") {
            let num_str = num.as_str();
            let num_val: usize = num_str.parse().unwrap_or(0);

            // Zero-pad to 3 digits
            let normalized = format!("{:03}", num_val);

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
fn collect_crossrefs(
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
fn classify_target_doc(path: &str) -> DocType {
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
fn select_sections_for_adr(
    doc_path: &str,
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

    if let Ok(content) = fs::read_to_string(doc_path) {
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
fn select_sections_for_design(
    doc_path: &str,
    entry: &FileEntry,
    anchor: Option<&str>,
    max_sections: usize,
) -> Vec<SectionMatch> {
    let mut sections = Vec::new();

    if let Ok(content) = fs::read_to_string(doc_path) {
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
fn select_sections_for_ops(
    doc_path: &str,
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

    if let Ok(content) = fs::read_to_string(doc_path) {
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
fn select_sections_for_other(doc_path: &str, entry: &FileEntry) -> Vec<SectionMatch> {
    let mut sections = Vec::new();

    if let Ok(content) = fs::read_to_string(doc_path) {
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
fn resolve_crossrefs(
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
        let entry = match index.files.get(&target_path) {
            Some(e) => e,
            None => continue, // Doc not in index
        };

        let doc_type = classify_target_doc(&target_path);

        // Select sections based on doc type
        let mut doc_sections = match doc_type {
            DocType::Adr => select_sections_for_adr(&target_path, entry, MAX_SECTIONS_PER_ADR),
            DocType::Design => {
                // Check if any ref has an anchor
                let anchor = refs.iter().find_map(|r| r.target_anchor.as_deref());
                select_sections_for_design(&target_path, entry, anchor, MAX_SECTIONS_PER_DESIGN)
            }
            DocType::Ops => select_sections_for_ops(&target_path, entry, MAX_SECTIONS_PER_OPS),
            DocType::Other => select_sections_for_other(&target_path, entry),
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

// ============================================================================
// Extractive Refiner (Phase 2.3)
// ============================================================================

/// Split text into sentences using simple regex
fn split_sentences(text: &str) -> Vec<String> {
    // Preserve code blocks
    let code_block_re = Regex::new(r"```[\s\S]*?```").unwrap();
    let mut code_blocks = Vec::new();
    let mut placeholder_text = text.to_string();

    // Extract code blocks and replace with placeholders
    for (i, caps) in code_block_re.captures_iter(text).enumerate() {
        let code = caps.get(0).unwrap().as_str();
        code_blocks.push(code.to_string());
        placeholder_text = placeholder_text.replace(code, &format!("__CODE_BLOCK_{}__", i));
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
        let placeholder = format!("__CODE_BLOCK_{}__", i);
        for sentence in &mut sentences {
            *sentence = sentence.replace(&placeholder, code);
        }
    }

    sentences
}

/// Score a sentence for relevance
fn score_sentence(
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
    score += overlap_count as f64 * W_LEXICAL;

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
fn extract_heading(text: &str) -> (String, String) {
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
fn refine_section(
    section: &SectionMatch,
    query_terms: &[String],
    max_tokens: usize,
) -> SectionMatch {
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
        return section.clone();
    }

    // Check if section has cross-references
    let has_crossref =
        body.to_lowercase().contains("adr") || body.contains("[") && body.contains("](");

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

    // Trim to token budget if needed
    let tokens = estimate_tokens(&refined_text);
    let final_text = if tokens > max_tokens {
        let char_limit = max_tokens * 4;
        refined_text[..char_limit.min(refined_text.len())].to_string()
    } else {
        refined_text
    };

    SectionMatch {
        doc_path: section.doc_path.clone(),
        heading: section.heading.clone(),
        line_start: section.line_start,
        line_end: section.line_end,
        bm25_score: section.bm25_score,
        content: final_text,
        canonicality: section.canonicality,
    }
}

/// Apply extractive refinement to all sections
fn apply_extractive_refiner(
    sections: Vec<SectionMatch>,
    query: &str,
    max_tokens_per_section: usize,
) -> Vec<SectionMatch> {
    let query_terms: Vec<String> = query
        .split_whitespace()
        .map(|s| stem_word(&s.to_lowercase()))
        .collect();

    sections
        .into_iter()
        .map(|section| refine_section(&section, &query_terms, max_tokens_per_section))
        .collect()
}

/// Main assemble command handler
fn cmd_assemble(
    query: &str,
    max_tokens: usize,
    max_sections: usize,
    depth: usize,
    format: &str,
    index_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if format != "markdown" {
        return Err("Only markdown format is supported currently".into());
    }

    let forward_index = load_forward_index(index_dir)?;

    // Phase 1: Primary section selection
    let primary_sections = search_relevant_sections(query, &forward_index, max_sections);

    if primary_sections.is_empty() {
        println!("# No relevant sections found for query: \"{}\"", query);
        return Ok(());
    }

    let primary_tokens: usize = primary_sections
        .iter()
        .map(|s| estimate_tokens(&s.content))
        .sum();

    // Phase 2: Cross-reference expansion (if depth > 0)
    let mut all_sections = primary_sections.clone();

    if depth > 0 {
        // Build ADR index
        let adr_index = build_adr_index(&forward_index);

        // Collect cross-references
        let crossrefs = collect_crossrefs(&primary_sections, &adr_index);

        // Calculate xref token budget
        const XREF_TOKEN_FRACTION: f64 = 0.3;
        const XREF_TOKEN_ABS_MAX: usize = 2000;

        let xref_cap = ((max_tokens as f64 * XREF_TOKEN_FRACTION) as usize).min(XREF_TOKEN_ABS_MAX);
        let remaining_tokens = max_tokens.saturating_sub(primary_tokens);
        let xref_token_budget = remaining_tokens.min(xref_cap);

        if xref_token_budget > 0 && !crossrefs.is_empty() {
            // Get primary doc paths for deduplication
            let primary_docs: HashSet<String> = primary_sections
                .iter()
                .map(|s| s.doc_path.clone())
                .collect();

            // Resolve cross-references
            let xref_sections =
                resolve_crossrefs(&crossrefs, &primary_docs, &forward_index, xref_token_budget);

            // Merge cross-ref sections
            all_sections.extend(xref_sections);
        }
    }

    // Phase 3: Extractive refinement (increase signal density)
    let max_tokens_per_section = max_tokens / all_sections.len().max(1);
    let refined_sections = apply_extractive_refiner(all_sections, query, max_tokens_per_section);

    // Phase 4: Distill to markdown
    let digest = distill_to_markdown(&refined_sections, query, max_tokens);

    println!("{}", digest);

    Ok(())
}

/// Evaluation command handler - runs retrieval pipeline against test questions
fn cmd_eval(questions_path: &Path, index_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Load questions from JSONL file
    let questions_content = fs::read_to_string(questions_path)?;
    let questions: Vec<Question> = questions_content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<Vec<_>, _>>()?;

    if questions.is_empty() {
        println!("No questions found in {}", questions_path.display());
        return Ok(());
    }

    // Load index once
    let forward_index = load_forward_index(index_dir)?;

    // Run evaluation for each question
    let mut results = Vec::new();

    for question in &questions {
        // Run assemble internally (capture output as string)
        let primary_sections = search_relevant_sections(&question.q, &forward_index, 20);

        if primary_sections.is_empty() {
            results.push(EvalResult {
                id: question.id,
                question: question.q.clone(),
                hits: 0,
                total: question.expect.len(),
                passed: false,
                tokens: 0,
            });
            continue;
        }

        let primary_tokens: usize = primary_sections
            .iter()
            .map(|s| estimate_tokens(&s.content))
            .sum();

        // Cross-reference expansion
        let mut all_sections = primary_sections.clone();
        let adr_index = build_adr_index(&forward_index);
        let crossrefs = collect_crossrefs(&primary_sections, &adr_index);

        const XREF_TOKEN_FRACTION: f64 = 0.3;
        const XREF_TOKEN_ABS_MAX: usize = 2000;
        let max_tokens: usize = 8000; // Default for eval

        let xref_cap = ((max_tokens as f64 * XREF_TOKEN_FRACTION) as usize).min(XREF_TOKEN_ABS_MAX);
        let remaining_tokens = max_tokens.saturating_sub(primary_tokens);
        let xref_token_budget = remaining_tokens.min(xref_cap);

        if xref_token_budget > 0 && !crossrefs.is_empty() {
            let primary_docs: HashSet<String> = primary_sections
                .iter()
                .map(|s| s.doc_path.clone())
                .collect();

            let xref_sections =
                resolve_crossrefs(&crossrefs, &primary_docs, &forward_index, xref_token_budget);

            all_sections.extend(xref_sections);
        }

        // Extractive refinement
        let max_tokens_per_section = max_tokens / all_sections.len().max(1);
        let refined_sections =
            apply_extractive_refiner(all_sections, &question.q, max_tokens_per_section);

        // Distill to markdown
        let digest = distill_to_markdown(&refined_sections, &question.q, max_tokens);

        // Check coverage of expected substrings
        let digest_lower = digest.to_lowercase();
        let hits = question
            .expect
            .iter()
            .filter(|e| digest_lower.contains(&e.to_lowercase()))
            .count();

        let min_hits = question.min_hits.unwrap_or(question.expect.len());
        let passed = hits >= min_hits;
        let tokens = estimate_tokens(&digest);

        results.push(EvalResult {
            id: question.id,
            question: question.q.clone(),
            hits,
            total: question.expect.len(),
            passed,
            tokens,
        });
    }

    // Print results
    println!("\n{}", "Evaluation Results".cyan().bold());
    println!("{}", "=".repeat(60));
    println!();

    for result in &results {
        let status = if result.passed {
            "✓".green().bold()
        } else {
            "✗".red().bold()
        };

        println!("[{}] {}", result.id, result.question.white().bold());
        println!("  - hits: {}/{} {}", result.hits, result.total, status);
        println!("  - size: {} tokens", result.tokens);
        println!();
    }

    // Print summary
    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();
    let pass_rate = (passed as f64 / total as f64 * 100.0) as usize;

    println!("{}", "=".repeat(60));
    println!("{}", "Summary".cyan().bold());
    println!("  Passed: {}/{} ({}%)", passed, total, pass_rate);
    println!("  Failed: {}/{}", total - passed, total);
    println!();

    if passed < total {
        println!("{}", "Failed Questions:".yellow().bold());
        for result in &results {
            if !result.passed {
                println!(
                    "  - [{}] {} (hits: {}/{})",
                    result.id, result.question, result.hits, result.total
                );
            }
        }
        println!();
    }

    Ok(())
}

/// Core link checking engine used by both `check` and `check-links`.
/// Returns a structured `LinkCheckResult` without printing.
fn run_link_check(
    index_dir: &Path,
    root: Option<&Path>,
    include_summary: bool,
    summary_only: bool,
) -> Result<LinkCheckResult, Box<dyn std::error::Error>> {
    // Load the forward index
    let forward_index = load_forward_index(index_dir)?;

    // Determine root directory for resolving relative paths
    let root_dir = if let Some(r) = root {
        r.to_path_buf()
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
                let context =
                    get_link_context(&mut file_lines_cache, file_path, line_number)?;
                let kind = LinkKind::Placeholder;
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
                    error: format!("Placeholder link target: {}", link_path),
                    anchor: anchor.clone(),
                    context,
                });
                continue;
            }

            // File-level checks only when there is an explicit path component
            if !link_path.is_empty() {
                let meta = fs::metadata(&normalized_path).ok();
                let exists = meta.is_some();
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);

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
                    // Missing target file: classify as doc_missing or code_missing
                    let ext = file_extension(&normalized_path);
                    let kind = if is_code_extension(&ext) {
                        LinkKind::CodeMissing
                    } else {
                        LinkKind::DocMissing
                    };
                    let context =
                        get_link_context(&mut file_lines_cache, file_path, line_number)?;
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
                        error: format!("Target file not found: {}", normalized_path),
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
                            error: format!("Anchor not found: #{}", anchor_text),
                            anchor: Some(anchor_text.clone()),
                            context,
                        });
                    }
                } else {
                    let context =
                        get_link_context(&mut file_lines_cache, file_path, line_number)?;
                    let kind = LinkKind::AnchorUnverified;
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
                        error: format!(
                            "Could not verify anchor (file has no headings): #{}",
                            anchor_text
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
fn cmd_check_links(
    index_dir: &Path,
    json: bool,
    root: Option<&Path>,
    summary_flag: bool,
    summary_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let include_summary = summary_flag || summary_only || !json;
    let result = run_link_check(index_dir, root, include_summary, summary_only)?;

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
        first_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
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
                println!("    Context: {}", ctx);
            }
            println!("    Error: {}", link.error.red());
            println!();
        }
    }

    Ok(())
}

/// Load a single-line context snippet for a link location.
fn get_link_context(
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
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
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

fn load_policy_config(path: &Path) -> Result<PolicyConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let cfg: PolicyConfig = serde_yaml::from_str(&content)?;
    Ok(cfg)
}

fn rule_severity(rule: &PolicyRule) -> String {
    rule.severity
        .as_deref()
        .unwrap_or("error")
        .to_string()
}

fn rule_name(rule: &PolicyRule) -> String {
    rule.name
        .clone()
        .unwrap_or_else(|| rule.pattern.clone())
}

fn collect_policy_violations_for_content(
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
                message: format!("Missing required content: {:?}", needle),
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
                message: format!("Forbidden content present: {:?}", needle),
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
                    "Document too short: {} lines (min required: {})",
                    line_count, min_len
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
                message: format!(
                    "Document too long: {} lines (max allowed: {})",
                    line_count, max_len
                ),
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
                    message: format!("Missing required heading: {:?}", h),
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
                    message: format!("Forbidden heading present: {:?}", h),
                    severity: rule_severity(rule),
                    kind: "policy_violation".to_string(),
                });
            }
        }
    }

    violations
}

fn run_policy_check(
    index_dir: &Path,
    policy_path: &Path,
) -> Result<PolicyCheckResult, Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let policy = load_policy_config(policy_path)?;

    let mut violations = Vec::new();

    for rule in &policy.rules {
        let glob = Glob::new(&rule.pattern)?;
        let matcher = glob.compile_matcher();

        for (file_path, _entry) in &forward_index.files {
            if !matcher.is_match(file_path) {
                continue;
            }

            let content = fs::read_to_string(file_path)?;
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

fn cmd_policy(
    config_path: &Path,
    index_dir: &Path,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !config_path.exists() {
        return Err(format!(
            "Policy file not found: {}",
            config_path.display()
        )
        .into());
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
fn suggest_new_link_target(
    source_file: &str,
    link_path: &str,
    available_files: &HashSet<String>,
) -> Option<String> {
    if link_path.is_empty() {
        return None;
    }

    let link_filename = Path::new(link_path)
        .file_name()
        .and_then(|s| s.to_str())?;

    // Find all candidates whose filename matches
    let mut candidates: Vec<&str> = available_files
        .iter()
        .map(|s| s.as_str())
        .filter(|p| {
            Path::new(p)
                .file_name()
                .and_then(|s| s.to_str())
                .map(|name| name == link_filename)
                .unwrap_or(false)
        })
        .collect();

    if candidates.len() != 1 {
        return None;
    }

    let candidate = Path::new(candidates[0]);
    let source_path = Path::new(source_file);
    let source_parent = source_path.parent().unwrap_or(Path::new("."));

    // Only handle the simple case where candidate is under the same parent
    if let Ok(stripped) = candidate.strip_prefix(source_parent) {
        let rel = stripped.to_string_lossy().to_string();
        if !rel.is_empty() {
            return Some(rel);
        }
    }

    None
}

fn cmd_fix_links(
    index_dir: &Path,
    dry_run: bool,
    apply: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !dry_run && !apply {
        return Err("Specify either --dry-run or --apply".into());
    }

    let forward_index = load_forward_index(index_dir)?;

    // Build set of available files from the index
    let available_files: HashSet<String> = forward_index.files.keys().cloned().collect();

    let mut fixes: Vec<LinkFix> = Vec::new();

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

            // Split off anchor (we only rewrite the path component)
            let (link_path, anchor) = if let Some(idx) = target.find('#') {
                (
                    target[..idx].to_string(),
                    Some(target[idx + 1..].to_string()),
                )
            } else {
                (target.clone(), None)
            };

            // Only consider links that do not resolve to an existing indexed file
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

            if let Some(new_rel) = suggest_new_link_target(file_path, &link_path, &available_files)
            {
                let mut new_target = new_rel;
                if let Some(a) = anchor {
                    new_target.push('#');
                    new_target.push_str(&a);
                }
                if new_target != *target {
                    fixes.push(LinkFix {
                        file: file_path.clone(),
                        old_target: target.clone(),
                        new_target,
                    });
                }
            }
        }
    }

    if fixes.is_empty() {
        println!("{}", "No safe link fixes found.".green().bold());
        return Ok(());
    }

    // Group fixes by file
    let mut fixes_by_file: HashMap<String, Vec<LinkFix>> = HashMap::new();
    for fix in fixes {
        fixes_by_file
            .entry(fix.file.clone())
            .or_default()
            .push(fix);
    }

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
        println!("{}", "Link fixes applied.".green().bold());
    }

    Ok(())
}

fn apply_reference_mapping_to_content(
    content: &str,
    from: &str,
    to: &str,
) -> String {
    let old = format!("]({})", from);
    let new = format!("]({})", to);
    content.replace(&old, &new)
}

fn load_reference_mappings(
    path: &Path,
) -> Result<ReferenceMappingConfig, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let cfg: ReferenceMappingConfig = serde_yaml::from_str(&content)?;
    Ok(cfg)
}

fn cmd_fix_references(
    index_dir: &Path,
    mapping_path: &Path,
    dry_run: bool,
    apply: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !dry_run && !apply {
        return Err("Specify either --dry-run or --apply".into());
    }
    if !mapping_path.exists() {
        return Err(format!(
            "Mapping file not found: {}",
            mapping_path.display()
        )
        .into());
    }

    let mappings_cfg = load_reference_mappings(mapping_path)?;
    if mappings_cfg.mappings.is_empty() {
        println!(
            "{} No mappings defined in {}",
            "Note:".yellow(),
            mapping_path.display()
        );
        return Ok(());
    }

    let forward_index = load_forward_index(index_dir)?;

    let mut changed_files: Vec<String> = Vec::new();

    for (file_path, _entry) in &forward_index.files {
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
            println!("  {}", f);
        }
    }

    Ok(())
}

fn cmd_mv(
    from: &Path,
    to: &Path,
    index_dir: &Path,
    update_refs: bool,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let from_str = from.to_string_lossy().to_string();
    let to_str = to.to_string_lossy().to_string();

    if dry_run {
        println!("{}", "Dry run:".cyan().bold());
    }

    println!(
        "{} {} -> {}",
        if dry_run { "Would move" } else { "Moving" },
        from_str,
        to_str
    );

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

        if files_to_update.is_empty() {
            println!(
                "{} No inbound links found for {} in index {}",
                "Note:".yellow(),
                from_str,
                index_dir.display()
            );
            return Ok(());
        }

        println!(
            "{} Updating references in {} file(s)",
            if dry_run { "Would update" } else { "Updating" },
            files_to_update.len()
        );

        for file in files_to_update {
            let content = fs::read_to_string(&file)?;
            let new_content = apply_reference_mapping_to_content(&content, &from_str, &to_str);
            if dry_run {
                if content != new_content {
                    println!("  {} (references would change)", file);
                }
            } else if content != new_content {
                fs::write(&file, new_content)?;
                println!("  {}", file);
            }
        }
    }

    Ok(())
}

fn compute_inbound_link_counts(
    forward_index: &ForwardIndex,
) -> HashMap<String, usize> {
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

fn cmd_export_graph(
    index_dir: &Path,
    format: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;

    // Map normalized paths to canonical file keys
    let mut norm_to_key: HashMap<String, String> = HashMap::new();
    for path in forward_index.files.keys() {
        let normalized = normalize_path(Path::new(path));
        norm_to_key.entry(normalized).or_insert_with(|| path.clone());
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
                    println!("  \"{}\" -> \"{}\" [label=\"{}\"];", src, dst, label);
                } else {
                    println!("  \"{}\" -> \"{}\";", src, dst);
                }
            }
            println!("}}");
        }
        other => {
            return Err(format!("Unsupported format: {}", other).into());
        }
    }

    Ok(())
}

fn run_stale_check(
    index_dir: &Path,
    days: u64,
    min_inlinks: usize,
) -> Result<StaleResult, Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;
    let inbound_counts = compute_inbound_link_counts(&forward_index);

    let now = std::time::SystemTime::now();
    let mut files = Vec::new();

    for (file_path, _) in &forward_index.files {
        let meta = fs::metadata(file_path);
        if meta.is_err() {
            continue;
        }
        let meta = meta?;
        let modified = meta.modified().unwrap_or(now);
        let age = now
            .duration_since(modified)
            .unwrap_or_default()
            .as_secs()
            / 86_400;

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

fn cmd_stale(
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
            f.file,
            f.days_since_modified,
            f.inbound_links
        );
    }

    Ok(())
}

fn is_placeholder_target(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();

    matches!(
        lower.as_str(),
        "url" | "text" | "todo" | "link" | "tbd"
    ) || lower.starts_with("/path/to/")
        || lower.starts_with("../path/to/")
        || lower.contains("replace-me")
}

fn is_code_extension(ext: &str) -> bool {
    matches!(
        ext,
        "py" | "ts" | "tsx" | "json" | "yaml" | "yml" | "png" | "svg"
    )
}

fn file_extension(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn record_link_kind(
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

    let entry = by_file
        .entry(file.to_string())
        .or_insert_with(HashMap::new);
    entry
        .entry(kind_name)
        .and_modify(|c| *c += 1)
        .or_insert(1);
}

/// Find all files that link to a specific file
fn cmd_backlinks(
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
                    println!("    Anchor: #{}", anchor);
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

/// Find orphaned files with no inbound links
fn cmd_orphans(
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

/// Score canonicality with reasons
fn score_canonicality_with_reasons(doc_path: &str, _entry: &FileEntry) -> (f64, Vec<String>) {
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
fn cmd_canonicality(
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
                println!("         - {}", reason);
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
                println!("         - {}", reason);
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

fn cmd_suggest_consolidation(
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
            println!("    - {}", m);
        }
        println!("  Note: {}", group.note);
        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jaccard_similarity() {
        let set1: HashSet<String> = ["foo", "bar", "baz"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let set2: HashSet<String> = ["bar", "baz", "qux"]
            .iter()
            .map(|s| s.to_string())
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
        let long_content: String = (0..25).map(|i| format!("line{}\n", i)).collect();
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

        let content = r#"
# Title

## Objective

Some content here.

## Deprecated
"#;

        let violations =
            collect_policy_violations_for_content(&rule, "docs/example.md", content);

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
    fn test_suggest_new_link_target_same_dir() {
        let mut available = HashSet::new();
        available.insert("docs/guide/auth.md".to_string());
        available.insert("docs/guide/other.md".to_string());

        // Source and target are in the same parent; filename matches exactly one file
        let suggested = suggest_new_link_target(
            "docs/guide/README.md",
            "auth.md",
            &available,
        );
        // Expect a simple relative path suggestion
        assert_eq!(suggested.as_deref(), Some("auth.md"));
    }

    #[test]
    fn test_apply_reference_mapping_to_content() {
        let content = "See [auth](docs/old/auth.md) for details.";
        let updated =
            apply_reference_mapping_to_content(content, "docs/old/auth.md", "docs/architecture/AUTH.md");
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
            },
        );

        let forward_index = ForwardIndex {
            files,
            indexed_at: chrono_now(),
            version: 3,
            avg_doc_length: 0.0,
            idf_map: HashMap::new(),
        };

        let pairs = vec![(
            "docs/a.md".to_string(),
            "docs/b.md".to_string(),
            0.9_f64,
        )];

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
            },
        );

        let forward_index = ForwardIndex {
            files,
            indexed_at: "0".to_string(),
            version: 3,
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
        fs::write(
            path,
            "first line\nsecond line with a link\nthird line\n",
        )
        .unwrap();

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
}
