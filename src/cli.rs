use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
///      `backlinks`, `orphans`, `canonicality`, and `canonical-orphans`.
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
     `backlinks`, `orphans`, `canonicality`, and `canonical-orphans`.
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
    yore canonicality --index .yore --threshold 0.7
    yore canonical-orphans --index .yore --threshold 0.7

OUTPUT FORMATS

  Most inspection commands support --json for structured output suitable for
  CI pipelines and automation agents. Commands with JSON support:

    build, eval, query, similar, dupes, dupes-sections, check, check-links,
    fix-links, backlinks, orphans, canonicality, canonical-orphans, stale,
    vocabulary, suggest-consolidation, policy, diff, stats, mv, fix-references

  Example: yore check-links --index .yore --json | jq '.broken[]'"#
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Config file path
    #[arg(short, long, global = true, default_value = ".yore.toml")]
    pub config: PathBuf,

    /// Profile name to load from config (limits which roots are indexed; use a full-root profile for whole-repo review)
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// Quiet mode - suppress non-essential output
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Subcommand)]
pub enum Commands {
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
    ///
    /// Run multiple checks in one pass (links, policy, stale).
    ///
    /// Designed for CI and automation; always emits JSON output.
    ///
    /// Limitations:
    ///   - `--dupes` is accepted but not currently executed.
    ///
    /// Related:
    ///   - `yore check-links`, `yore policy`, `yore stale`
    ///
    /// Example:
    ///   yore check --links --taxonomy --policy .yore-policy.yaml --index .yore --ci
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

        /// Kinds/check IDs that should cause a non-zero exit code (comma-separated; repeat flag to pass multiple)
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
    /// Detect structural document-health issues from build-time metrics.
    ///
    /// Uses persisted document and section metrics emitted by `yore build`
    /// to flag oversized docs, accumulator-style section growth, stale
    /// completed sections, and changelog sprawl.
    ///
    /// Examples:
    ///   yore health docs/plan.md --index .yore
    ///   yore health --all --index .yore --json
    Health {
        /// Specific file to inspect
        file: Option<PathBuf>,

        /// Evaluate every indexed document with persisted metrics
        #[arg(long)]
        all: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Maximum lines before a file is flagged as bloated
        #[arg(long, default_value = "500")]
        max_lines: usize,

        /// Maximum count of "Part N" headings before accumulator risk is flagged
        #[arg(long, default_value = "8")]
        max_part_sections: usize,

        /// Maximum retained lines across completion-marked sections
        #[arg(long, default_value = "50")]
        max_completed_lines: usize,

        /// Maximum changelog list items before changelog bloat is flagged
        #[arg(long, default_value = "15")]
        max_changelog_entries: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
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
    /// Limitations:
    ///   - Only indexes the extensions listed in `--types`.
    ///   - Ignores binary files and content outside the selected roots.
    ///   - `--track-renames` requires a git repo with history.
    ///
    /// Related:
    ///   - `yore stats`, `yore query`, `yore assemble`
    ///
    /// Examples:
    ///   yore build docs --output .yore --types md,txt --json
    ///   yore build . --output .yore --exclude node_modules --exclude target
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

        /// Output as JSON (query results include the original query text)
        #[arg(long)]
        json: bool,

        /// Track file renames using git history
        #[arg(long)]
        track_renames: bool,
    },

    /// Search the index for relevant documents using BM25.
    ///
    /// Accepts free-text terms, ranks documents with BM25 using the
    /// precomputed index, and optionally returns machine-readable JSON.
    ///
    /// Useful for quick inspection by humans and for agents that want to
    /// select candidate files before assembling full context.
    ///
    /// Limitations:
    ///   - Only searches indexed files; run `yore build` first.
    ///   - Ranking is term-based, not semantic.
    ///
    /// Related:
    ///   - `yore assemble`, `yore similar`, `yore stats`
    ///
    /// Examples:
    ///   yore query kubernetes deployment --index .yore --limit 5
    ///   yore query --query '"async migration"' --phrase --index .yore --files-only
    Query {
        /// Search terms
        terms: Vec<String>,

        /// Raw query string (avoids shell-quoting pitfalls; overrides positional terms)
        #[arg(long)]
        query: Option<String>,

        /// Maximum results to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,

        /// Show only file paths
        #[arg(short = 'l', long)]
        files_only: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Show top N distinctive terms per result (0 = disabled)
        #[arg(long, default_value = "0")]
        doc_terms: usize,

        /// Show query diagnostics and scoring details (JSON output wraps query + results + diagnostics)
        #[arg(long)]
        explain: bool,

        /// Do not filter stopwords from the query
        #[arg(long)]
        no_stopwords: bool,

        /// Require exact adjacency matches for quoted segments (use --query to include quotes)
        #[arg(long)]
        phrase: bool,

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
    /// Limitations:
    ///   - The reference file must be in the index.
    ///   - Similarity is heuristic, not semantic.
    ///
    /// Related:
    ///   - `yore dupes`, `yore diff`, `yore query`
    ///
    /// Examples:
    ///   yore similar docs/adr/ADR-0013-retries.md --index .yore --limit 5
    ///   yore similar docs/architecture/AUTH.md --threshold 0.4 --json
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

        /// Show top N distinctive terms per result (0 = disabled)
        #[arg(long, default_value = "0")]
        doc_terms: usize,

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
    /// Limitations:
    ///   - Similarity is heuristic and may miss paraphrases.
    ///   - Tune `--threshold` for larger or smaller corpora.
    ///
    /// Related:
    ///   - `yore dupes-sections`, `yore diff`, `yore suggest-consolidation`
    ///
    /// Examples:
    ///   yore dupes --index .yore --threshold 0.35 --group
    ///   yore dupes --index .yore --threshold 0.5 --json
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
    /// Limitations:
    ///   - Section similarity uses SimHash; reworded sections may be missed.
    ///   - Smaller sections may require a lower `--threshold`.
    ///
    /// Related:
    ///   - `yore dupes`, `yore diff`, `yore suggest-consolidation`
    ///
    /// Examples:
    ///   yore dupes-sections --index .yore --threshold 0.7 --min-files 2
    ///   yore dupes-sections --index .yore --threshold 0.85 --min-files 5 --json
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
    /// Limitations:
    ///   - Not a line-by-line diff; uses indexed keywords/headings.
    ///   - Both files must be indexed.
    ///
    /// Related:
    ///   - `yore dupes`, `yore dupes-sections`, `yore similar`
    ///
    /// Examples:
    ///   yore diff docs/old.md docs/new.md --index .yore --json
    ///   yore diff docs/plan.md docs/status.md --index .yore
    Diff {
        /// First file
        file1: PathBuf,

        /// Second file
        file2: PathBuf,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show high-level index statistics.
    ///
    /// Prints counts of files, headings, links, and top keywords, which
    /// is useful for sanity-checking an index and monitoring drift over time.
    ///
    /// Limitations:
    ///   - Reports only what is in the index, not the live filesystem.
    ///
    /// Related:
    ///   - `yore build`, `yore query`
    ///
    /// Examples:
    ///   yore stats --index .yore --top-keywords 20 --json
    ///   yore stats --index docs/.index --top-keywords 50
    Stats {
        /// Show top N keywords
        #[arg(long, default_value = "20")]
        top_keywords: usize,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Interactive query REPL over the index.
    ///
    /// Starts a simple read-eval-print loop where you can type queries
    /// and inspect results quickly while iterating on documentation.
    ///
    /// Limitations:
    ///   - No persistence or scripting; use `yore query` for batch runs.
    ///
    /// Related:
    ///   - `yore query`, `yore stats`
    ///
    /// Examples:
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
    /// Limitations:
    ///   - Uses indexed content only; run `yore build` first.
    ///   - Cross-reference expansion follows internal links only.
    ///
    /// Related:
    ///   - `yore query`, `yore eval`, `yore build`
    ///
    /// Examples:
    ///   yore assemble "How does authentication work?" \
    ///     --index .yore --max-tokens 8000 --depth 1 > context.md
    ///   yore assemble "async migration status" --index .yore --max-sections 10
    ///   yore assemble --from-files docs/adr/ADR-0010.md docs/adr/ADR-0011.md --index .yore
    Assemble {
        /// Natural language query/question (required unless --from-files is used)
        #[arg(required_unless_present = "from_files")]
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

        /// Show top N distinctive terms per source document (0 = disabled)
        #[arg(long, default_value = "0")]
        doc_terms: usize,

        /// Assemble context from explicit files (supports @list.txt)
        #[arg(long, value_name = "PATH", num_args = 1..)]
        from_files: Vec<String>,

        /// Use persisted relation graph for cross-reference expansion
        #[arg(long)]
        use_relations: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Experimental MCP-oriented context tools with bounded preview/fetch contracts.
    ///
    /// This surface is JSON-first and intentionally narrow: search/preview
    /// returns compact snippets plus opaque handles, and fetch returns more
    /// detail only when explicitly asked.
    ///
    /// Related:
    ///   - `yore query`, `yore assemble`
    ///
    /// Examples:
    ///   yore mcp search-context "authentication flow" --index .yore
    ///   yore mcp fetch-context ctx_1234abcd --index .yore
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
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
    /// Limitations:
    ///   - Uses substring matching; does not grade semantic answers.
    ///   - False positives/negatives are possible; tune expectations.
    ///
    /// Related:
    ///   - `yore assemble`, `yore query`
    ///
    /// Examples:
    ///   yore eval --questions questions.jsonl --index .yore --json
    ///   yore eval --questions questions.jsonl --index .yore
    Eval {
        /// Path to questions JSONL file
        #[arg(long, default_value = "questions.jsonl")]
        questions: PathBuf,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Values of k for precision@k, recall@k, nDCG@k (comma-separated)
        #[arg(long, value_delimiter = ',', default_values_t = vec![5, 10])]
        k: Vec<usize>,
    },

    /// Derive a deterministic vocabulary list from a built index.
    ///
    /// Use this command when you want a compact candidate vocabulary for
    /// prompt engineering, glossary generation, or documentation normalization.
    ///
    /// Output formats:
    ///   - `lines` (default): one term per line for easy filtering scripts
    ///   - `json`: structured payload with `term`, `score`, and `count`
    ///   - `prompt`: comma-separated terms for LLM initial prompts
    ///
    /// Usage guidance:
    ///   1. Build an index: `yore build <path> --output .yore`
    ///   2. Generate vocabulary candidates:
    ///      - `yore vocabulary --index .yore --limit 200 --format lines`
    ///      - `yore vocabulary --index .yore --format json --limit 50`
    ///      - `yore vocabulary --index .yore --format prompt --limit 150`
    ///   3. Optionally remove common words:
    ///      - `yore vocabulary --index .yore --stopwords my.stopwords`
    ///      - `yore vocabulary --index .yore --format json --json`
    ///      - `yore vocabulary --index .yore --common-terms 20`
    ///      - `yore vocabulary --index .yore --no-default-stopwords --common-terms 40`
    ///      - `yore vocabulary --index .yore --no-default-stopwords --stopwords my.stopwords`
    ///
    /// Limitations:
    ///   - Ranking is deterministic but may still evolve as stop-word defaults
    ///     or indexing heuristics are tuned.
    ///   - `--common-terms` derives a corpus-frequency stoplist and may remove
    ///     domain terms in very small projects.
    Vocabulary {
        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Maximum number of terms to return
        #[arg(short = 'n', long, default_value = "100")]
        limit: usize,

        /// Output format: lines, json, or prompt
        #[arg(long, default_value = "lines")]
        format: String,

        /// Alias for `--format json`
        #[arg(long)]
        json: bool,

        /// Path to an additional stop-word list (optional; one word per line)
        #[arg(long)]
        stopwords: Option<PathBuf>,

        /// Keep stem-only terms when no non-stem surface form is available
        #[arg(long)]
        include_stemming: bool,

        /// Keep built-in stopword filtering enabled (set false with --no-default-stopwords)
        #[arg(long)]
        no_default_stopwords: bool,

        /// Exclude the top N corpus-common terms before applying other filters
        #[arg(long, default_value = "0")]
        common_terms: usize,
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
    /// Limitations:
    ///   - Does not fetch external URLs; external links are not validated.
    ///   - Only checks files within the index roots.
    ///
    /// Related:
    ///   - `yore fix-links`, `yore export-graph`, `yore backlinks`
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
    /// Limitations:
    ///   - Only considers indexed markdown links (not external URLs).
    ///
    /// Related:
    ///   - `yore orphans`, `yore export-graph`
    ///
    /// Examples:
    ///   yore backlinks docs/architecture/DEPLOYMENT-GUIDE.md --index .yore
    ///   yore backlinks docs/README.md --index .yore --json
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
    /// Limitations:
    ///   - Entry-point docs (README/INDEX) may be intentionally orphaned.
    ///   - Only considers links in the index.
    ///
    /// Related:
    ///   - `yore backlinks`, `yore canonical-orphans`
    ///
    /// Examples:
    ///   yore orphans --index .yore --exclude README
    ///   yore orphans --index .yore --exclude README --exclude INDEX --json
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
    /// Limitations:
    ///   - Heuristic scoring; validate with `dupes` and human review.
    ///
    /// Related:
    ///   - `yore suggest-consolidation`, `yore canonical-orphans`
    ///
    /// Examples:
    ///   yore canonicality --index .yore --threshold 0.7
    ///   yore canonicality --index .yore --json
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

    /// Find canonical documents with no inbound links.
    ///
    /// Filters documents by canonicality score and reports those that are
    /// not linked to by any other indexed document.
    ///
    /// Limitations:
    ///   - Only considers inbound links in the index roots.
    ///   - Canonicality is heuristic, not semantic.
    ///
    /// Related:
    ///   - `yore canonicality`, `yore orphans`, `yore backlinks`
    ///
    /// Examples:
    ///   yore canonical-orphans --index .yore --threshold 0.7
    ///   yore canonical-orphans --index .yore --threshold 0.8 --json
    CanonicalOrphans {
        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Minimum canonicality score (0.0 to 1.0)
        #[arg(short, long, default_value = "0.7")]
        threshold: f64,
    },

    /// Automatically fix a subset of broken relative links.
    ///
    /// This command uses heuristics over the index to propose safe,
    /// mechanical rewrites for links that appear to point to the wrong
    /// file (for example, the right filename in the wrong directory).
    ///
    /// For agent-friendly operation, use --propose to output ambiguous
    /// cases to a YAML file, then --apply-decisions to apply choices.
    ///
    /// Limitations:
    ///   - Only fixes a conservative subset of relative links.
    ///   - Ambiguous targets require `--propose` + `--apply-decisions`.
    ///
    /// Related:
    ///   - `yore check-links`, `yore mv`, `yore fix-references`
    ///
    /// Examples:
    ///   yore fix-links --index .yore --dry-run
    ///   yore fix-links --index .yore --apply
    ///   yore fix-links --index .yore --propose proposals.yaml
    ///   yore fix-links --index .yore --apply-decisions proposals.yaml
    FixLinks {
        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,

        /// Show proposed edits without modifying any files
        #[arg(long)]
        dry_run: bool,

        /// Apply changes to files on disk (only unambiguous fixes)
        #[arg(long)]
        apply: bool,

        /// Output ambiguous link fixes to a YAML file for agent/human review
        #[arg(long)]
        propose: Option<PathBuf>,

        /// Apply decisions from a previously generated proposal file
        #[arg(long)]
        apply_decisions: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Use git rename history to suggest fixes for moved files
        #[arg(long)]
        use_git_history: bool,
    },

    /// Rewrite references according to an explicit mapping file.
    ///
    /// This promotes the `mv --update-refs` machinery into a more general
    /// bulk rewrite tool, suitable for large documentation reorganizations.
    ///
    /// Limitations:
    ///   - Does not move files; only rewrites references.
    ///   - Requires a mapping file that lists exact from/to pairs.
    ///
    /// Related:
    ///   - `yore mv`, `yore fix-links`
    ///
    /// Examples:
    ///   yore fix-references --mapping mappings.yaml --index .yore --dry-run --json
    ///   yore fix-references --mapping mappings.yaml --index .yore --apply
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

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Move a documentation file and optionally update inbound references.
    ///
    /// This is a thin, ergonomic wrapper around link rewrite logic. When
    /// --update-refs is used, all Markdown links that point to the old
    /// path are rewritten to point to the new path.
    ///
    /// Limitations:
    ///   - Only updates links in indexed files; run `yore build` first.
    ///   - Does not update external repositories or URLs.
    ///
    /// Related:
    ///   - `yore fix-references`, `yore fix-links`, `yore check-links`
    ///
    /// Examples:
    ///   yore mv docs/old/auth.md docs/architecture/AUTH.md --update-refs --index .yore --json
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

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Report potentially stale documentation based on age and inbound links.
    ///
    /// Uses file modification time and inbound link counts from the index
    /// to highlight documents that may be unmaintained or dead.
    ///
    /// Limitations:
    ///   - Staleness is heuristic; validate before deleting.
    ///   - Depends on file mtime and inbound links only.
    ///
    /// Related:
    ///   - `yore orphans`, `yore canonicality`
    ///
    /// Examples:
    ///   yore stale --index .yore --days 90 --min-inlinks 0 --json
    ///   yore stale --index .yore --days 30 --min-inlinks 1
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
    /// Limitations:
    ///   - Graph only includes indexed documents and internal links.
    ///
    /// Related:
    ///   - `yore backlinks`, `yore check-links`
    ///
    /// Examples:
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

    /// Show relation paths between documents via the persisted relation graph.
    ///
    /// Displays how a source document connects to other documents through
    /// links, section links, and ADR references. Requires `relations.json`
    /// from `yore build`.
    ///
    /// Examples:
    ///   yore paths docs/architecture.md --index .yore
    ///   yore paths docs/architecture.md --json --index .yore
    ///   yore paths docs/architecture.md --depth 2 --index .yore
    Paths {
        /// Source file to show paths from
        source: String,

        /// Traversal depth (1 = direct edges, 2 = two hops)
        #[arg(short = 'd', long, default_value = "1")]
        depth: usize,

        /// Filter by edge kind: links_to, section_links_to, adr_reference
        #[arg(long)]
        kind: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Suggest document consolidation based on duplicates and canonicality.
    ///
    /// Uses duplicate detection and canonicality scoring to propose a
    /// canonical document and a set of files that should be merged into it.
    ///
    /// Limitations:
    ///   - Suggestions are heuristic; review before merging or deleting.
    ///
    /// Related:
    ///   - `yore dupes`, `yore canonicality`, `yore diff`
    ///
    /// Examples:
    ///   yore suggest-consolidation --threshold 0.7 --json --index .yore
    ///   yore suggest-consolidation --threshold 0.6 --index .yore
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
    /// forbidden content, and reports any violations it finds. Rules can
    /// also enforce maximum section length (optionally filtered by heading
    /// regex) and required markdown links.
    /// Required links treat absolute paths as repo-root relative, and
    /// resolve relative paths against the source file.
    ///
    /// Limitations:
    ///   - Rules operate on indexed content; run `yore build` first.
    ///   - Content checks are literal substring matches.
    ///
    /// Related:
    ///   - `yore check --taxonomy`, `yore check-links`
    ///
    /// Examples:
    ///   yore policy --config .yore-policy.yaml --index .yore --json
    ///   yore policy --config .yore-policy.yaml --index .yore
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

#[derive(Subcommand)]
pub enum McpCommands {
    /// Return bounded previews plus opaque handles for follow-up fetches.
    #[command(name = "search-context", alias = "preview-context")]
    SearchContext {
        /// Natural language query/question (required unless --from-files is used)
        #[arg(required_unless_present = "from_files")]
        query: Vec<String>,

        /// Maximum preview results to return
        #[arg(long, default_value = "5")]
        max_results: usize,

        /// Maximum total tokens across all previews (approximate)
        #[arg(long, default_value = "1200")]
        max_tokens: usize,

        /// Maximum total bytes across all previews
        #[arg(long, default_value = "12000")]
        max_bytes: usize,

        /// Search/preview from explicit files instead of a query (supports @list.txt)
        #[arg(long, value_name = "PATH", num_args = 1..)]
        from_files: Vec<String>,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Expand a previously returned opaque handle.
    #[command(name = "fetch-context", alias = "expand-context")]
    FetchContext {
        /// Opaque handle returned by `search-context`
        handle: String,

        /// Maximum tokens in fetched content (approximate)
        #[arg(long, default_value = "4000")]
        max_tokens: usize,

        /// Maximum bytes in fetched content
        #[arg(long, default_value = "20000")]
        max_bytes: usize,

        /// Index directory
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },

    /// Serve the bounded preview/fetch tools over MCP stdio transport.
    ///
    /// This wraps the existing `search_context` and `fetch_context`
    /// contracts so MCP clients can call Yore without scraping CLI stdout.
    ///
    /// Examples:
    ///   yore mcp serve --index .yore
    Serve {
        /// Default index directory for MCP tool calls
        #[arg(short, long, default_value = ".yore")]
        index: PathBuf,
    },
}
