use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Question {
    pub id: usize,
    pub q: String,
    pub expect: Vec<String>,
    #[serde(default)]
    pub min_hits: Option<usize>,
    #[serde(default)]
    pub relevant_docs: Option<Vec<String>>,
}

#[derive(Serialize, Debug, Clone)]
pub struct MetricAtK {
    pub k: usize,
    pub value: f64,
}

#[derive(Serialize, Debug, Clone)]
pub struct RankingMetrics {
    pub precision_at_k: Vec<MetricAtK>,
    pub recall_at_k: Vec<MetricAtK>,
    pub mrr: f64,
    pub ndcg_at_k: Vec<MetricAtK>,
}

#[derive(Serialize, Debug)]
pub struct AggregateRankingMetrics {
    pub questions_with_relevance: usize,
    pub mean_precision_at_k: Vec<MetricAtK>,
    pub mean_recall_at_k: Vec<MetricAtK>,
    pub mean_mrr: f64,
    pub mean_ndcg_at_k: Vec<MetricAtK>,
}

#[derive(Debug, Clone)]
pub struct EvalResult {
    pub id: usize,
    pub question: String,
    pub hits: usize,
    pub total: usize,
    pub passed: bool,
    pub tokens: usize,
    pub ranked_docs: Vec<String>,
    pub ranking: Option<RankingMetrics>,
    pub digest: String,
}

// Link checking structures
#[derive(Serialize, Debug, Clone)]
pub struct BrokenLink {
    pub source_file: String,
    pub line_number: usize,
    pub link_text: String,
    pub link_target: String,
    pub error: String,
    pub anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
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
pub struct LinkSummaryByFile {
    pub file: String,
    pub counts: HashMap<String, usize>,
}

#[derive(Serialize, Debug)]
pub struct LinkSummaryByKind {
    pub kind: String,
    pub count: usize,
}

#[derive(Serialize, Debug)]
pub struct LinkCheckSummary {
    pub by_file: Vec<LinkSummaryByFile>,
    pub by_kind: Vec<LinkSummaryByKind>,
}

#[derive(Serialize, Debug)]
pub struct LinkCheckResult {
    pub total_links: usize,
    pub valid_links: usize,
    pub broken_links: usize,
    pub broken: Vec<BrokenLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<LinkCheckSummary>,
}

// Diff output structure
#[derive(Serialize, Debug)]
pub struct DiffResult {
    pub file1: String,
    pub file2: String,
    pub similarity: DiffSimilarity,
    pub shared_keywords: Vec<String>,
    pub only_in_file1: Vec<String>,
    pub only_in_file2: Vec<String>,
    pub shared_headings: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct DiffSimilarity {
    pub combined: f64,
    pub jaccard: f64,
    pub simhash: f64,
}

// Stats output structure
#[derive(Serialize, Debug)]
pub struct StatsResult {
    pub total_files: usize,
    pub unique_keywords: usize,
    pub total_headings: usize,
    pub body_keywords: usize,
    pub total_links: usize,
    pub index_version: u32,
    pub indexed_at: String,
    pub top_keywords: Vec<KeywordCount>,
}

#[derive(Serialize, Debug)]
pub struct KeywordCount {
    pub keyword: String,
    pub count: usize,
}

#[derive(Serialize, Debug)]
pub struct VocabularyResult {
    pub format: String,
    pub limit: usize,
    pub total: usize,
    pub terms: Vec<VocabularyTerm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopwords: Option<String>,
    pub used_default_stopwords: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_common_terms: Option<usize>,
    pub include_stemming: bool,
}

#[derive(Serialize, Debug)]
pub struct VocabularyTerm {
    pub term: String,
    pub score: f64,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct VocabularyCandidateTerm {
    pub term: String,
    pub surface: Option<String>,
    pub term_freq: usize,
    pub doc_freq: usize,
    pub first_file: String,
    pub first_line: usize,
    pub first_heading: String,
}

#[derive(Debug, Clone, Copy)]
pub struct VocabularyOptions<'a> {
    pub stopwords: Option<&'a Path>,
    pub include_stemming: bool,
    pub no_default_stopwords: bool,
    pub common_terms: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpScoreBreakdown {
    pub bm25: f64,
    pub canonicality: f64,
    pub combined: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpSourceRef {
    pub path: String,
    pub heading: String,
    pub line_start: usize,
    pub line_end: usize,
}

#[derive(Serialize, Debug, Default, Clone)]
pub struct McpPressure {
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpTrace {
    pub trace_id: String,
    pub index_fingerprint: String,
    pub strategy: String,
    pub expansion_path: Vec<String>,
}

#[derive(Serialize, Debug, Default)]
pub struct McpSearchBudget {
    pub max_results: usize,
    pub max_tokens: usize,
    pub max_bytes: usize,
    pub returned_results: usize,
    pub candidate_hits: usize,
    pub deduped_hits: usize,
    pub omitted_hits: usize,
    pub estimated_tokens: usize,
    pub bytes: usize,
}

#[derive(Serialize, Debug, Default)]
pub struct McpFetchBudget {
    pub max_tokens: usize,
    pub max_bytes: usize,
    pub estimated_tokens: usize,
    pub bytes: usize,
}

#[derive(Serialize, Debug)]
pub struct McpSearchResult {
    pub handle: String,
    pub rank: usize,
    pub source: McpSourceRef,
    pub scores: McpScoreBreakdown,
    pub preview: String,
    pub preview_tokens: usize,
    pub preview_bytes: usize,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truncation_reasons: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct McpFetchResult {
    pub source: McpSourceRef,
    pub scores: McpScoreBreakdown,
    pub preview: String,
    pub content: String,
    pub content_tokens: usize,
    pub content_bytes: usize,
}

#[derive(Serialize, Debug)]
pub struct McpSearchResponse {
    pub schema_version: u32,
    pub tool: String,
    pub query: String,
    pub selection_mode: String,
    pub budget: McpSearchBudget,
    pub pressure: McpPressure,
    pub trace: McpTrace,
    pub results: Vec<McpSearchResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_files: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct McpFetchResponse {
    pub schema_version: u32,
    pub tool: String,
    pub handle: String,
    pub budget: McpFetchBudget,
    pub pressure: McpPressure,
    pub trace: McpTrace,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<McpFetchResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpArtifact {
    pub schema_version: u32,
    pub handle: String,
    pub query: String,
    pub source: McpSourceRef,
    pub scores: McpScoreBreakdown,
    pub preview: String,
    pub content: String,
    pub created_at: String,
    #[serde(default)]
    pub index_fingerprint: String,
}

#[derive(Debug, Clone, Copy)]
pub struct McpSearchOptions {
    pub max_results: usize,
    pub max_tokens: usize,
    pub max_bytes: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct McpFetchOptions {
    pub max_tokens: usize,
    pub max_bytes: usize,
}

pub const DEFAULT_MCP_PROTOCOL_VERSION: &str = "2025-11-25";

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct McpInitializeParams {
    pub protocol_version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct McpToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct McpSearchToolArgs {
    pub query: String,
    pub from_files: Vec<String>,
    pub max_results: usize,
    pub max_tokens: usize,
    pub max_bytes: usize,
    pub index: Option<PathBuf>,
}

impl Default for McpSearchToolArgs {
    fn default() -> Self {
        Self {
            query: String::new(),
            from_files: Vec::new(),
            max_results: 5,
            max_tokens: 1200,
            max_bytes: 12000,
            index: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct McpFetchToolArgs {
    pub handle: String,
    pub max_tokens: usize,
    pub max_bytes: usize,
    pub index: Option<PathBuf>,
}

impl Default for McpFetchToolArgs {
    fn default() -> Self {
        Self {
            handle: String::new(),
            max_tokens: 4000,
            max_bytes: 20000,
            index: None,
        }
    }
}

// Mv output structure
#[derive(Serialize, Debug)]
pub struct MvResult {
    pub from: String,
    pub to: String,
    pub moved: bool,
    pub updated_files: Vec<String>,
}

// FixReferences output structure
#[derive(Serialize, Debug)]
pub struct FixReferencesResult {
    pub mapping_file: String,
    pub mappings_count: usize,
    pub updated_files: Vec<String>,
    pub applied: bool,
}

// Build output structure
#[derive(Serialize, Debug)]
pub struct BuildResult {
    pub index_path: String,
    pub files_indexed: usize,
    pub total_headings: usize,
    pub total_links: usize,
    pub unique_keywords: usize,
    pub duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renames_tracked: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_relations: Option<usize>,
}

// Eval JSON output structure
#[derive(Serialize, Debug)]
pub struct EvalJsonResult {
    pub questions_file: String,
    pub total_questions: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
    pub results: Vec<EvalQuestionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranking_metrics: Option<AggregateRankingMetrics>,
}

#[derive(Serialize, Debug)]
pub struct EvalQuestionResult {
    pub question: String,
    pub passed: bool,
    pub expected: Vec<String>,
    pub found: Vec<String>,
    pub missing: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranking: Option<RankingMetrics>,
}

// Policy / taxonomy structures
#[derive(Debug, Deserialize, Default)]
pub struct PolicyRule {
    /// Glob pattern to match files (e.g., "agents/plans/*.md")
    pub pattern: String,
    /// Required substrings that must appear in matching files
    #[serde(default)]
    pub must_contain: Vec<String>,
    /// Substrings that must NOT appear in matching files
    #[serde(default)]
    pub must_not_contain: Vec<String>,
    /// Optional rule name (for clearer reporting)
    #[serde(default)]
    pub name: Option<String>,
    /// Optional severity ("error" or "warn"), defaults to "error"
    #[serde(default)]
    pub severity: Option<String>,
    /// Optional minimum document length in lines
    #[serde(default)]
    pub min_length: Option<usize>,
    /// Optional maximum document length in lines
    #[serde(default)]
    pub max_length: Option<usize>,
    /// Optional maximum section length in lines
    #[serde(default)]
    pub max_section_length: Option<usize>,
    /// Optional regex to scope section-length rules to matching headings
    #[serde(default)]
    pub section_heading_regex: Option<String>,
    /// Required markdown headings (by text, without leading '#')
    #[serde(default)]
    pub required_headings: Vec<String>,
    /// Forbidden markdown headings (by text, without leading '#')
    #[serde(default)]
    pub forbidden_headings: Vec<String>,
    /// Required markdown link targets (resolved relative to file)
    #[serde(default)]
    pub must_link_to: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyConfig {
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

#[derive(Serialize, Debug)]
pub struct PolicyViolation {
    pub file: String,
    pub rule: String,
    pub message: String,
    pub severity: String,
    /// Always "policy_violation" so agents can key off kind
    pub kind: String,
}

#[derive(Serialize, Debug)]
pub struct PolicyCheckResult {
    pub policy_file: String,
    pub total_violations: usize,
    pub violations: Vec<PolicyViolation>,
}

#[derive(Serialize, Debug, Default)]
pub struct CombinedCheckResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<LinkCheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyCheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale: Option<StaleResult>,
}

#[derive(Serialize, Debug)]
pub struct StaleFile {
    pub file: String,
    pub days_since_modified: u64,
    pub inbound_links: usize,
}

#[derive(Serialize, Debug)]
pub struct StaleResult {
    pub total_stale: usize,
    pub files: Vec<StaleFile>,
}

#[derive(Serialize, Debug, Clone)]
pub struct HealthIssue {
    pub kind: String,
    pub severity: String,
    pub message: String,
    pub value: usize,
    pub threshold: usize,
}

#[derive(Serialize, Debug, Clone)]
pub struct HealthFileResult {
    pub file: String,
    pub status: String,
    pub issues: Vec<HealthIssue>,
}

#[derive(Serialize, Debug)]
pub struct HealthResult {
    pub total_files: usize,
    pub unhealthy_files: usize,
    pub warning_files: usize,
    pub files: Vec<HealthFileResult>,
}

#[derive(Serialize, Debug)]
pub struct GraphNode {
    pub id: String,
}

#[derive(Serialize, Debug)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct GraphExport {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

// Relation extraction structs (YEH-004)

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SectionRef {
    pub heading: String,
    pub line_start: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    LinksTo,
    SectionLinksTo,
    AdrReference,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelationEdge {
    pub source: String,
    pub target: String,
    pub kind: RelationKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_section: Option<SectionRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_section: Option<SectionRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RelationIndex {
    pub version: u32,
    pub indexed_at: String,
    pub total_edges: usize,
    pub edges: Vec<RelationEdge>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AdrRef {
    pub line: usize,
    pub raw_text: String,
    pub normalized_id: String,
}

#[derive(Serialize, Debug)]
pub struct ConsolidationGroup {
    pub canonical: String,
    pub merge_into: Vec<String>,
    pub canonical_score: f64,
    pub avg_similarity: f64,
    pub note: String,
}

#[derive(Serialize, Debug)]
pub struct ConsolidationResult {
    pub total_groups: usize,
    pub groups: Vec<ConsolidationGroup>,
}

#[derive(Serialize, Debug, Clone)]
pub struct LinkFix {
    pub file: String,
    pub old_target: String,
    pub new_target: String,
}

// Proposal structures for agent-friendly fix-links
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkFixProposal {
    pub source: String,
    pub line: usize,
    pub broken_target: String,
    pub candidates: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<usize>, // Index into candidates, or None to skip
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LinkFixProposalFile {
    /// Schema version for forward compatibility
    pub version: u32,
    /// Proposals for ambiguous link fixes
    pub proposals: Vec<LinkFixProposal>,
}

// Backlinks structures
#[derive(Serialize, Debug, Clone)]
pub struct Backlink {
    pub source_file: String,
    pub link_text: String,
    pub link_target: String,
    pub anchor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReferenceMapping {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct ReferenceMappingConfig {
    #[serde(default)]
    pub mappings: Vec<ReferenceMapping>,
}

#[derive(Serialize, Debug)]
pub struct BacklinksResult {
    pub target_file: String,
    pub total_backlinks: usize,
    pub backlinks: Vec<Backlink>,
}

// Orphans structures
#[derive(Serialize, Debug, Clone)]
pub struct OrphanFile {
    pub file: String,
    pub size_bytes: u64,
    pub line_count: usize,
}

#[derive(Serialize, Debug)]
pub struct OrphansResult {
    pub total_orphans: usize,
    pub orphans: Vec<OrphanFile>,
}

#[derive(Serialize, Debug, Clone)]
pub struct CanonicalOrphan {
    pub file: String,
    pub canonicality: f64,
    pub inbound_links: usize,
}

#[derive(Serialize, Debug)]
pub struct CanonicalOrphansResult {
    pub total_orphans: usize,
    pub threshold: f64,
    pub orphans: Vec<CanonicalOrphan>,
}

// Canonicality structures
#[derive(Serialize, Debug, Clone)]
pub struct CanonicalityScore {
    pub file: String,
    pub score: f64,
    pub reasons: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct CanonicalityResult {
    pub total_files: usize,
    pub files: Vec<CanonicalityScore>,
}

// Index structures
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub size_bytes: u64,
    pub line_count: usize,
    pub headings: Vec<Heading>,
    pub keywords: Vec<String>,
    pub body_keywords: Vec<String>, // keywords from full text
    pub links: Vec<Link>,
    pub simhash: u64, // content fingerprint
    #[serde(default)]
    pub term_frequencies: HashMap<String, usize>, // term counts for BM25
    #[serde(default)]
    pub doc_length: usize, // total terms for BM25
    #[serde(default)]
    pub minhash: Vec<u64>, // MinHash signature for LSH
    #[serde(default)]
    pub section_fingerprints: Vec<SectionFingerprint>, // NEW: section-level SimHash
    #[serde(default)]
    pub adr_references: Vec<AdrRef>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Heading {
    pub line: usize,
    pub level: usize,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Link {
    pub line: usize,
    pub text: String,
    pub target: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SectionFingerprint {
    pub heading: String,
    pub level: usize,
    pub line_start: usize,
    pub line_end: usize,
    pub simhash: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReverseEntry {
    pub file: String,
    pub line: Option<usize>,
    pub heading: Option<String>,
    pub level: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ForwardIndex {
    pub files: HashMap<String, FileEntry>,
    pub indexed_at: String,
    pub version: u32, // index version for compatibility
    #[serde(default)]
    pub source_root: String,
    #[serde(default)]
    pub avg_doc_length: f64, // NEW: average document length for BM25
    #[serde(default)]
    pub idf_map: HashMap<String, f64>, // NEW: IDF scores for BM25
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReverseIndex {
    pub keywords: HashMap<String, Vec<ReverseEntry>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct SectionMetrics {
    pub heading: String,
    pub level: usize,
    pub line_start: usize,
    pub line_end: usize,
    pub line_count: usize,
    pub word_count: usize,
    pub link_count: usize,
    pub list_item_count: usize,
    pub code_block_count: usize,
    pub has_completion_marker: bool,
    pub looks_like_part: bool,
    pub looks_like_changelog: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct DocumentMetrics {
    pub path: String,
    pub line_count: usize,
    pub word_count: usize,
    pub heading_count: usize,
    pub section_count: usize,
    pub link_count: usize,
    pub h1_count: usize,
    pub h2_count: usize,
    pub h3_count: usize,
    pub h4_plus_count: usize,
    pub code_block_count: usize,
    pub list_item_count: usize,
    pub table_row_count: usize,
    pub frontmatter_key_count: usize,
    pub metadata_line_count: usize,
    pub part_heading_count: usize,
    pub completion_heading_count: usize,
    pub changelog_heading_count: usize,
    pub changelog_entry_count: usize,
    pub longest_section_lines: usize,
    pub sections: Vec<SectionMetrics>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DocumentMetricsIndex {
    pub indexed_at: String,
    pub version: u32,
    pub files: HashMap<String, DocumentMetrics>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IndexStats {
    pub total_files: usize,
    pub total_keywords: usize,
    pub total_headings: usize,
    pub total_links: usize,
    pub indexed_at: String,
}

/// A single file rename event from git history
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RenameEntry {
    /// The old path before the rename
    pub old_path: String,
    /// The new path after the rename
    pub new_path: String,
    /// Git commit hash where the rename occurred
    pub commit: String,
}

/// Git rename history for tracking file moves
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct RenameHistory {
    /// All rename events, ordered from oldest to newest
    pub renames: Vec<RenameEntry>,
    /// Indexed at timestamp
    pub indexed_at: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct IndexProfileConfig {
    #[serde(default)]
    pub roots: Vec<String>,
    #[serde(default)]
    pub types: Vec<String>,
    pub output: Option<String>,
}

/// Severity override for link checking based on path patterns
#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)] // Config scaffolding for future severity filtering
pub struct SeverityOverride {
    pub pattern: String,
    pub severity: String,
}

/// Link checking configuration
#[derive(Deserialize, Debug, Clone, Default)]
#[allow(dead_code)] // Config scaffolding for future exclude patterns
pub struct LinkCheckConfig {
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default, rename = "severity-overrides")]
    pub severity_overrides: Vec<SeverityOverride>,
}

/// External repository configuration for cross-repo link validation
#[derive(Deserialize, Debug, Clone)]
pub struct ExternalRepo {
    pub path: String,
    #[serde(default)]
    #[allow(dead_code)] // Config scaffolding for future prefix support
    pub prefix: Option<String>,
}

/// External repositories configuration
#[derive(Deserialize, Debug, Clone, Default)]
pub struct ExternalConfig {
    #[serde(default)]
    pub repos: Vec<ExternalRepo>,
}

/// Policy configuration
#[derive(Deserialize, Debug, Clone, Default)]
#[allow(dead_code)] // Config scaffolding for future policy file reference
pub struct PolicyConfigRef {
    #[serde(default, rename = "rules-file")]
    pub rules_file: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct YoreConfig {
    #[serde(default)]
    pub index: HashMap<String, IndexProfileConfig>,
    #[serde(default, rename = "link-check")]
    #[allow(dead_code)] // Config scaffolding
    pub link_check: Option<LinkCheckConfig>,
    #[serde(default)]
    #[allow(dead_code)] // Config scaffolding
    pub policy: Option<PolicyConfigRef>,
    #[serde(default)]
    pub external: Option<ExternalConfig>,
}

// Assembly / context selection types

#[derive(Debug, Clone)]
pub struct SectionMatch {
    pub doc_path: String,
    pub heading: String,
    pub line_start: usize,
    pub line_end: usize,
    pub bm25_score: f64,
    pub content: String,
    pub canonicality: f64,
}

pub const MCP_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct ContextSelection {
    pub query_label: String,
    pub query_for_refiner: String,
    pub sections: Vec<SectionMatch>,
}

#[derive(Debug, Clone)]
pub enum ContextSelectionIssue {
    NoSearchableTerms,
    MissingFiles(Vec<String>),
    NoIndexedFilesMatched,
    NoRelevantSections(String),
}

#[derive(Debug, Clone)]
pub struct RefinedSection {
    pub section: SectionMatch,
    pub truncated: bool,
    pub truncation_reasons: Vec<String>,
}

// Search / query types

#[derive(Debug, Clone)]
pub struct ParsedQuery {
    pub terms: Vec<String>,
    pub phrases: Vec<PhraseGroup>,
}

#[derive(Debug, Clone)]
pub struct PhraseGroup {
    pub terms: Vec<String>,
}

// Cross-reference / assembly types

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RefType {
    MarkdownLink,
    AdrId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrossRef {
    pub ref_type: RefType,
    pub origin_doc_path: String,
    pub target_doc_path: String,
    pub target_anchor: Option<String>,
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DocType {
    Adr,
    Design,
    Ops,
    Other,
}
