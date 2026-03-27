<img src="./assets/yorelogo.png" alt="Yore" height="80"/>

# yore – Deterministic Documentation Indexer and Context Assembly Engine

[![CI](https://github.com/rahulrajaram/yore/actions/workflows/ci.yml/badge.svg)](https://github.com/rahulrajaram/yore/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**yore** is a fast, deterministic tool for indexing, analyzing, and retrieving documentation from a filesystem and assembling that information into high‑quality context for large language models (LLMs) and automation agents.

Where traditional search tools return a list of matching files, yore is designed to answer a more specific question:

> “Given this question and a fixed token budget, what *exact* slice of the documentation should an LLM see to reason correctly and safely?”

Yore combines BM25 search, structural analysis, link graph inspection, duplicate detection, and extractive refinement into a reproducible pipeline that can be used directly by humans or programmatically by agents.

---

## 1. Concepts and Terminology

Before diving into commands, it helps to define a few terms that appear throughout this README.

### Documentation sprawl

“Documentation sprawl” refers to the way documentation accumulates over time:

- Multiple files describe the same feature with slightly different details.
- Older documents are left in the tree and never removed or clearly marked as deprecated.
- Temporary or scratch notes are committed and live alongside canonical documentation.
- Engineers searching for “authentication” might see ten files with overlapping names and no clear indication of which one is authoritative.

Yore is designed to operate in exactly this environment and make it tractable for both humans and LLMs.

### Architecture Decision Record (ADR) and ADR chain

An **ADR (Architecture Decision Record)** is a small document that records a single architectural decision: the context, the decision itself, and the consequences. Projects often store ADRs under a directory such as `docs/adr/ADR-0001-some-decision.md`.

An **ADR chain** is the sequence of ADRs that refer to one another over time, for example:

- `ADR-0013` introduces retry semantics.
- `ADR-0022` modifies the retry timing.
- `ADR-0035` deprecates a previous approach.

LLMs frequently need this historical context to answer “why” questions correctly. Yore is able to recognize ADR references (for example, `ADR-013`) and pull those records into the context it assembles.

### Canonical document

In a large repository, several documents may cover similar topics. A **canonical document** is the one that should be treated as the primary source of truth for a topic.

Yore computes a **canonicality score** per document based on path, naming conventions, recency, and other signals, and exposes those scores so tools and agents can make consistent, automated decisions.

---

## 2. What Yore Does

At a high level, yore provides:

- **Indexing** of documentation files (Markdown, text, etc.) using BM25 and structural metadata.
- **Search and analysis** over that index: free‑text search, duplicate detection, canonicality scoring, link graph queries.
- **Context assembly for LLMs**, including cross‑reference expansion and extractive refinement controlled by an explicit token budget.
- **Bounded agent retrieval**, with JSON-first preview/fetch flows that keep large context off transcript until explicitly requested.
- **Quality checks**, such as link validation and an evaluation harness for retrieval correctness.

Some example questions Yore helps answer:

- “Which documents describe Kubernetes deployment, and which one is canonical?”
- “What ADRs exist for authentication and session isolation?”
- “What documents are unreferenced and safe to clean up?”
- “What is the smallest, highest‑signal context I can give an LLM for ‘How do I deploy a new service?’ within 8,000 tokens?”

---

## 3. How Yore Differs from Traditional Search Tools

Yore is not a replacement for Lucene, Elasticsearch, Meilisearch, or ripgrep. Instead, it builds on similar primitives and adds additional layers specifically for documentation curation and LLM context assembly.

### 3.1 Comparison matrix

| Capability / Tool                | **Yore**                            | Lucene / Tantivy                     | Elasticsearch / OpenSearch           | Meilisearch                          | ripgrep                              |
|----------------------------------|-------------------------------------|--------------------------------------|--------------------------------------|--------------------------------------|--------------------------------------|
| Primary use case                 | Doc indexing + LLM context assembly | General‑purpose search library       | Scalable full‑text search cluster    | Simple search API for applications   | Fast text search in files            |
| Retrieval model                  | BM25 + structural and link signals  | BM25 / scoring plugins               | BM25 + scoring / aggregations        | BM25‑like                            | Regex / literal matching              |
| Cross‑reference expansion        | Yes (Markdown links, ADR refs)      | No (caller must implement)           | No (caller must implement)           | No                                   | No                                   |
| Duplicate detection (docs/sections) | Yes (Jaccard + MinHash + SimHash)  | No (custom code required)            | No                                   | No                                   | No                                   |
| Canonicality scoring             | Yes (path, naming, recency signals) | No                                   | No                                   | No                                   | No                                   |
| Link graph analysis (backlinks, orphans) | Yes                         | No                                   | No                                   | No                                   | No                                   |
| LLM‑aware token budgeting        | Yes (per‑query token budget)        | No                                   | No                                   | No                                   | No                                   |
| Extractive refinement            | Yes (sentence‑level, code‑preserving) | No                                  | No                                   | No                                   | No                                   |
| Deterministic output             | Yes (no sampling, no embeddings)    | Yes                                  | Yes (given same index)               | Yes                                  | Yes                                  |
| Designed for agent integration   | Yes                                 | Caller‑defined                       | Caller‑defined                       | Caller‑defined                       | Caller‑defined                       |

You can use lucene‑like tools to implement the core search primitive. Yore sits at a higher level, orchestrating retrieval, link following, refinement, and evaluation in a way that is explicitly designed for LLMs and documentation maintenance agents.

---

## 4. Architecture Overview

Yore operates in four main phases:

1. **Indexing**
   The `yore build` command walks a directory tree, identifies documents of interest (for example, `*.md`), and builds an index that includes:

   - BM25 term statistics
   - Section boundaries and fingerprints
   - Link information (Markdown links and ADR references)
   - Basic metadata (path, size, timestamps)

2. **Retrieval and analysis**
   Commands such as `yore query`, `yore dupes`, `yore dupes-sections`, `yore canonicality`, `yore canonical-orphans`, `yore check-links`, `yore backlinks`, and `yore orphans` operate against this index to answer questions about relevance, duplication, authority, and link structure.

3. **Context assembly for LLMs and agents**
   Yore supports two retrieval shapes over the same index:

   - `yore assemble` for a markdown digest you want to hand directly to an LLM.
   - `yore mcp search-context` / `yore mcp fetch-context` for bounded JSON previews and explicit follow-up expansion.
   - `yore mcp serve` to expose those same bounded tools over MCP stdio transport for editor and agent clients.

   Both paths reuse the same deterministic retrieval building blocks:

   - BM25 to select the most relevant documents and sections.
   - Cross‑reference expansion to include linked ADRs and design docs where appropriate.
   - Extractive refinement to keep code blocks, lists, and high‑value sentences while removing low‑signal prose.
   - Final budget-aware trimming for either markdown digests or compact JSON previews.

4. **Evaluation and governance**
   The `yore eval` command uses a JSONL question file to validate whether the assembled contexts contain expected substrings, enabling regression detection and measurable improvements to retrieval quality.

All operations are deterministic: given the same index and configuration, yore will produce the same outputs.

---

## 5. Installation

### From crates.io (recommended)

```bash
cargo install yore-cli
```

### From source

```bash
git clone https://github.com/rahulrajaram/yore.git
cd yore
cargo install --path .
```

### Verify installation

```bash
yore --version
```

---

## 6. Quick Start

### 6.1 Build an index

Create an index over Markdown files in `docs/`:

```bash
yore build docs --output docs/.index --types md
```

### 6.2 Run a search query

Use BM25‑based search over the index:

```bash
yore query kubernetes deployment --index docs/.index
yore query --query '"async migration"' --phrase --index docs/.index
yore query --query '"async migration" plan' --phrase --explain --index docs/.index
yore query --query '"async migration" plan' --phrase --explain --json --index docs/.index
```

### 6.3 Detect duplicate content

Identify duplicate sections and documents:

```bash
# Duplicate sections across documents
yore dupes-sections --index docs/.index --threshold 0.7

# Duplicate documents
yore dupes --index docs/.index --threshold 0.35
```

### 6.4 Assemble context for an LLM

Generate a context digest for a question:

```bash
yore assemble "How does authentication work?" \
  --depth 1 \
  --max-tokens 8000 \
  --index docs/.index > context.md
```

Use `assemble` when you intentionally want one markdown digest to hand to an LLM.

For agent integrations and IDE/status-bar flows, prefer the bounded two-step MCP-oriented path first:

```bash
# Step 1: preview compact snippets and capture opaque handles
yore mcp search-context "How does authentication work?" \
  --max-results 5 \
  --max-tokens 1200 \
  --max-bytes 12000 \
  --index docs/.index

# Step 2: expand only the specific handle you need
yore mcp fetch-context ctx_1234abcd \
  --max-tokens 4000 \
  --max-bytes 20000 \
  --index docs/.index
```

That flow keeps transcript pressure low: search returns previews, source references, truncation metadata, and handles; fetch returns more detail only on explicit follow-up.

If you want a real MCP server process instead of shelling out to the CLI contract directly:

```bash
yore mcp serve --index docs/.index
```

That stdio server registers `search_context` and `fetch_context` as MCP tools and returns the same contract via `structuredContent`.

### 6.5 Evaluate retrieval quality

Run the evaluation harness against a test set of questions:

```bash
yore eval --questions questions.jsonl --index docs/.index
yore eval --questions questions.jsonl --index docs/.index --json --k 3,5,10
```

### 6.6 Link and structure analysis

Validate links and inspect the documentation structure:

```bash
# Find broken links and anchors
yore check --links --index docs/.index

# Show who links to a specific document
yore backlinks docs/architecture/DEPLOYMENT-GUIDE.md --index docs/.index

# Find documents with no inbound links
yore orphans --index docs/.index --exclude README

# Find stale documentation (90+ days, no inbound links)
yore stale --index docs/.index --days 90 --min-inlinks 0 --json

# Show canonical documents by authority score
yore canonicality --index docs/.index --threshold 0.7
```

---

## 7. Command Reference

This section provides a concise reference for each major command. All commands that operate on an index accept `--index <index-dir>`.

For the most up-to-date, agent-friendly documentation for each command, you can also use the built-in help:

- `yore --help` – High-level overview, workflow, and examples
- `yore help <command>` – Manpage-style description, options, and usage examples for a specific subcommand (for example, `yore help assemble`)

### 7.1 `yore build`

Builds a forward and reverse index over a directory tree.

```bash
yore build <path> --output <index-dir> --types <extensions>
```

**Key options**

* `--output, -o` – Index directory (default: `.yore`)
* `--types, -t` – Comma‑separated list of file extensions to index (default: `md,txt,rst`)
* `--exclude, -e` – Glob‑style patterns to exclude (repeatable)

**Example**

```bash
yore build docs --output docs/.index --types md,txt
```

---

### 7.2 `yore query`

Runs a BM25 search across the index.

```bash
yore query <terms...> --index <index-dir>
```

**Key options**

* `--limit, -n` – Maximum number of results (default: 10)
* `--files-only, -l` – Only show file paths
* `--json` – Emit machine‑readable JSON; query results include the original query text
* `--query` – Raw query string that overrides positional terms (avoids shell quoting)
* `--phrase` – Require adjacency for quoted segments in the query (quotes must be part of the query string)
* `--no-stopwords` – Keep stopwords in query matching
* `--doc-terms` – Show top N distinctive terms per result (0 disables)
* `--explain` – Emit diagnostics; with `--json`, output becomes `{ query, results, diagnostics }`
  * Diagnostics fields: `tokens`, `stems`, `missing_terms`, `idf`, `bm25`, `index_path`, `doc_count`

**Query syntax**

Queries are tokenized the same way as indexing (letters and numbers plus `_` and `-`), lowercased, and stemmed. Stopwords are removed by default; use `--no-stopwords` to keep them. Quoted phrases are only enforced when `--phrase` is set, and the quotes must be part of the query string (use `--query` to include them).

`--explain` prints diagnostics to stdout for plain output. With `--json`, the same diagnostics are wrapped in machine-readable form:

```json
{
  "query": "async migration plan",
  "results": [...],
  "diagnostics": {
    "tokens": ["async", "migration", "plan"],
    "stems": ["async", "migration", "plan"],
    "missing_terms": ["plan"],
    "idf": [{ "term": "async", "stem": "async", "idf": 1.0 }],
    "bm25": {
      "k1": 1.5,
      "b": 0.75,
      "avg_doc_length": 220.0
    },
    "index_path": "docs/.index",
    "doc_count": 42
  }
}
```

**Example**

```bash
yore query kubernetes deployment --limit 5 --index docs/.index
yore query --query '"async migration" plan' --phrase --explain --index docs/.index
```

---

### 7.3 `yore dupes`

Finds duplicate or highly similar documents across the corpus.

```bash
yore dupes --index <index-dir>
```

**Key options**

* `--threshold, -t` – Similarity threshold (default: 0.35)
* `--group` – Group duplicates together
* `--json` – Emit JSON output

The similarity score is a combined metric using Jaccard overlap, SimHash, and MinHash, for example:

* 40% Jaccard
* 30% SimHash
* 30% MinHash

**Example**

```bash
yore dupes --threshold 0.4 --group --json --index docs/.index
```

Tip: If the duplication is embedded in a section of a larger file, use
`yore dupes-sections` and then `yore diff` to confirm overlap. Use `--json`
output when wiring into automation.

---

### 7.4 `yore dupes-sections`

Identifies duplicate sections across different documents.

```bash
yore dupes-sections --index <index-dir>
```

**Key options**

* `--threshold, -t` – SimHash similarity threshold (default: 0.7)
* `--min-files, -n` – Minimum number of distinct files sharing a similar section (default: 2)
* `--json` – Emit JSON output

**Example**

```bash
# Find sections appearing in 5+ files with ≥ 85% similarity
yore dupes-sections --threshold 0.85 --min-files 5 --json --index docs/.index
```

Tip: For partial copy/paste blocks, lower `--threshold` and inspect
candidate pairs with `yore diff`.

---

### 7.5 `yore assemble`

Assembles a context digest for LLM consumption from the indexed documentation.

```bash
yore assemble <query> --index <index-dir>
```

**Pipeline steps**

1. BM25 primary document and section selection
2. Cross‑reference expansion (Markdown links and ADR references)
3. Extractive refinement (preserves code blocks, lists, headings; keeps high‑value sentences)
4. Final token‑aware trimming and markdown digest generation

**Key options**

* `--max-tokens, -t` – Total token budget for the digest (default: 8000)
* `--max-sections, -s` – Maximum sections to include (default: 20)
* `--depth, -d` – Cross‑reference expansion depth (default: 1, maximum 2)
* `--format, -f` – Output format (`markdown` is the default)
* `--doc-terms` – Show top N distinctive terms per source document (0 disables)
* `--from-files` – Assemble from explicit files instead of a query (supports `@list.txt`)

**Example**

```bash
yore assemble "How does the authentication system work?" \
  --max-tokens 6000 \
  --depth 1 \
  --index docs/.index > context.md

# Assemble from explicit files
yore assemble --from-files docs/adr/ADR-0010.md docs/adr/ADR-0011.md --index docs/.index
yore assemble --from-files @file-list.txt --index docs/.index
```

---

### 7.5A `yore mcp`

Provides a bounded JSON-first contract for agent retrieval.

```bash
yore mcp search-context <query> --index <index-dir>
yore mcp fetch-context <handle> --index <index-dir>
yore mcp serve --index <index-dir>
```

**Search/fetch contract**

* `search-context` returns compact previews, source references, budget usage, truncation reasons, and opaque `ctx_...` handles.
* `fetch-context` expands one handle at a time and applies its own token/byte caps before returning content.
* `serve` exposes those same operations as MCP tools named `search_context` and `fetch_context` over stdio transport.
* Handles are stored under `<index-dir>/mcp_handles/` when writable, with an automatic temp-dir fallback if the index directory is read-only.
* Large artifacts stay off transcript by default; callers must opt into expansion explicitly.

**Key options**

* `search-context --max-results` – Hard top-k cap for preview hits (default: 5)
* `search-context --max-tokens` – Hard total token cap across previews (default: 1200)
* `search-context --max-bytes` – Hard total byte cap across previews (default: 12000)
* `search-context --from-files` – Preview from an explicit file list instead of a query
* `fetch-context --max-tokens` – Hard token cap for fetched content (default: 4000)
* `fetch-context --max-bytes` – Hard byte cap for fetched content (default: 20000)

**Examples**

```bash
# Preview top bounded hits for an agent
yore mcp search-context "session revocation flow" --index docs/.index

# Selection-first preview for an IDE action or changed-file workflow
yore mcp search-context --from-files docs/auth.md docs/adr/ADR-0012.md --index docs/.index

# Expand one result only when you actually need it
yore mcp fetch-context ctx_1234abcd --index docs/.index

# Serve the same contract over MCP stdio for editor/agent clients
yore mcp serve --index docs/.index
```

Use this flow when transcript discipline matters: status bars, editor copilots, thin MCP servers, or any agent loop where a raw markdown dump would be too expensive.

For MCP clients, `yore mcp serve` returns the same search/fetch payloads inside `structuredContent`, and mirrors them as compact JSON text content for clients that only read text tool output.

**Integration Contract v1**

* `schema_version: 1` is the contract anchor.
* While `schema_version` remains `1`, existing field names and meanings are stable.
* Additive fields may appear in v1, but existing fields will not be renamed or repurposed.
* Any breaking change to payload shape or semantics requires a schema version bump.

**Recommended defaults**

* `search-context --max-results 5`
* `search-context --max-tokens 1200`
* `search-context --max-bytes 12000`
* `fetch-context --max-tokens 4000`
* `fetch-context --max-bytes 20000`

**Stable top-level fields**

* `search-context`: `schema_version`, `tool`, `query`, `selection_mode`, `budget`, `pressure`, `results`, `error`, `message`, `missing_files`
* `fetch-context`: `schema_version`, `tool`, `handle`, `budget`, `pressure`, `query`, `result`, `error`, `message`

**Stable nested fields**

* `budget.max_results`, `budget.max_tokens`, `budget.max_bytes`, `budget.returned_results`, `budget.candidate_hits`, `budget.deduped_hits`, `budget.omitted_hits`, `budget.estimated_tokens`, `budget.bytes`
* `pressure.truncated`, `pressure.reasons`
* `results[].handle`, `results[].rank`, `results[].source.path`, `results[].source.heading`, `results[].source.line_start`, `results[].source.line_end`
* `results[].scores.bm25`, `results[].scores.canonicality`, `results[].scores.combined`
* `results[].preview`, `results[].preview_tokens`, `results[].preview_bytes`, `results[].truncated`, `results[].truncation_reasons`
* `result.source.path`, `result.source.heading`, `result.source.line_start`, `result.source.line_end`
* `result.scores.bm25`, `result.scores.canonicality`, `result.scores.combined`
* `result.preview`, `result.content`, `result.content_tokens`, `result.content_bytes`

**Semantics**

* `candidate_hits` counts raw section candidates before dedupe.
* `deduped_hits` counts candidates removed because they overlapped an already selected section or had duplicate normalized content.
* `omitted_hits` counts relevant deduped hits that were not returned because of caps.
* `pressure.truncated` means the overall response hit a cap or included at least one truncated payload.
* `pressure.reasons` reports response-level pressure using `result_cap`, `token_cap`, and `byte_cap`.
* `results[].truncated` and `results[].truncation_reasons` report truncation for an individual preview.
* `results[].handle` is an opaque handle persisted under `<index-dir>/mcp_handles/` when possible, with a temp-dir fallback for read-only indexes; callers should treat it as opaque and use `fetch-context` rather than reading artifact files directly.
* Handles are deterministic for a fixed query, source path, section span, and section content.

**Smoke test**

The checked-in fixture corpus lives under `tests/fixtures/mcp-smoke/docs/` and is indexed by default because Yore includes `txt` in its default types. Run this exact command in CI or locally:

```bash
bash scripts/mcp-smoke-test.sh
```

That script:

* copies the fixture corpus into a temp workspace
* builds an index
* runs `search-context`
* extracts the returned `ctx_...` handle
* runs `fetch-context`
* asserts the expected contract shape, truncation signal, and handle expansion

**Fixture-backed example**

```bash
yore mcp search-context authentication \
  --max-results 3 \
  --max-tokens 120 \
  --max-bytes 600 \
  --index .yore-smoke
```

```json
{
  "schema_version": 1,
  "tool": "search_context",
  "query": "authentication",
  "selection_mode": "query",
  "budget": {
    "max_results": 3,
    "max_tokens": 120,
    "max_bytes": 600,
    "returned_results": 1,
    "candidate_hits": 2,
    "deduped_hits": 1,
    "omitted_hits": 0,
    "estimated_tokens": 40,
    "bytes": 160
  },
  "pressure": {
    "truncated": false
  },
  "results": [
    {
      "handle": "ctx_d76396f763601873",
      "rank": 1,
      "source": {
        "path": "docs/aa-auth.txt",
        "heading": "Authentication Overview",
        "line_start": 1,
        "line_end": 11
      },
      "scores": {
        "bm25": 0.19397590361445782,
        "canonicality": 0.5,
        "combined": 0.28578313253012044
      },
      "preview": "# Authentication Overview\n\nAuthentication flow validates credentials against the identity store and issues a session token\n\nAuthentication step 1 keeps the audi",
      "preview_tokens": 40,
      "preview_bytes": 160,
      "truncated": false
    }
  ]
}
```

```bash
yore mcp fetch-context ctx_d76396f763601873 \
  --max-tokens 40 \
  --max-bytes 220 \
  --index .yore-smoke
```

```json
{
  "schema_version": 1,
  "tool": "fetch_context",
  "handle": "ctx_d76396f763601873",
  "budget": {
    "max_tokens": 40,
    "max_bytes": 220,
    "estimated_tokens": 40,
    "bytes": 160
  },
  "pressure": {
    "truncated": true,
    "reasons": [
      "token_cap",
      "byte_cap"
    ]
  },
  "query": "authentication",
  "result": {
    "source": {
      "path": "docs/aa-auth.txt",
      "heading": "Authentication Overview",
      "line_start": 1,
      "line_end": 11
    },
    "scores": {
      "bm25": 0.19397590361445785,
      "canonicality": 0.5,
      "combined": 0.28578313253012044
    },
    "preview": "# Authentication Overview\n\nAuthentication flow validates credentials against the identity store and issues a session token\n\nAuthentication step 1 keeps the audi",
    "content": "# Authentication Overview\n\nAuthentication flow validates credentials against the identity store and issues a session token.\nEvery successful logi ...[truncated]",
    "content_tokens": 40,
    "content_bytes": 160
  }
}
```

---

### 7.6 `yore eval`

Evaluates the retrieval pipeline against a set of test questions.

```bash
yore eval --questions <jsonl-file> --index <index-dir> [--k <k1,k2,...>]
```

Each line in the JSONL file represents a test question:

```json
{"id": 1, "q": "How does auth work?", "expect": ["session", "token"], "min_hits": 2}
```

To measure ranked retrieval quality, add `relevant_docs` to a question:

```json
{"id": 2, "q": "deployment steps", "expect": ["docker"], "relevant_docs": ["docs/guides/deployment.md"]}
```

Yore assembles context for each question, checks for expected substrings, and reports per‑question hits and an overall pass rate. When `relevant_docs` is present, yore also computes precision@k, recall@k, MRR, and nDCG@k over the initial BM25 retrieval ranking. Questions without `relevant_docs` produce the existing output only (backward compatible).

**Key options**

* `--questions` – Path to questions JSONL file (default: `questions.jsonl`)
* `--index` – Index directory (default: `.yore`)
* `--json` – Emit JSON output
* `--k` – Comma‑separated k values for precision@k, recall@k, nDCG@k (default: `5,10`)

**Example**

```bash
yore eval --questions questions.jsonl --index docs/.index
yore eval --questions questions.jsonl --index docs/.index --json --k 3,5,10
```

---

### 7.7 `yore check`

Runs one or more documentation checks in a single entrypoint.
Output is always JSON for CI and automation.

```bash
yore check [--links] [--dupes] [--taxonomy --policy <file>] [--stale] --index <index-dir> [--stale-days <N>] [--ci --fail-on <kinds>]
```

**Key options**

* `--links` – Run link validation (same engine as `check-links`)
* `--dupes` – Accepted by the checker, but duplicate detection is currently not executed there (use `yore dupes` directly)
* `--taxonomy` – Run policy checks using a YAML policy file
* `--policy` – Path to policy config (default: `.yore-policy.yaml`)
* `--stale` – Run stale-document checks
* `--stale-days` – Age threshold in days for stale checks (default: 30)
* `--ci` – Enable CI‑style exit codes
* `--fail-on` – Comma‑separated list of kinds/severities that should cause a non‑zero exit code (for example `doc_missing,code_missing,policy_error`)

**Examples**

```bash
# Links only
yore check --links --index docs/.index

# Links + policy checks in one run
yore check --links --taxonomy --policy .yore-policy.yaml --index docs/.index

# CI mode: fail when there are missing docs or policy errors
yore check --links --taxonomy --policy .yore-policy.yaml \
  --index docs/.index \
  --ci --fail-on doc_missing,policy_error
```

**Policy example**

```yaml
rules:
  - name: "STATUS docs are summaries"
    pattern: "**/IMPLEMENTATION_STATUS.md"
    max_length: 400
    max_section_length: 50
    section_heading_regex: "(?i)async"
    must_link_to:
      - "docs/ASYNC_MIGRATION_COMPLETE_SUMMARY.md"
```

`max_section_length` applies to sections whose heading matches
`section_heading_regex`. `must_link_to` checks internal markdown links;
paths with `/` are treated as repo-root relative, and `./`/`../` paths are
resolved relative to the file.

---

### 7.8 `yore check-links`

Validates all Markdown links and anchors in the indexed documents.

```bash
yore check-links --index <index-dir>
```

**Key options**

* `--json` – Emit machine‑readable JSON
* `--root, -r` – Root directory for resolving relative paths (if different from index root)
* `--summary` / `--summary-only` – Include or show only a grouped summary by file and by kind (`doc_missing`, `code_missing`, `placeholder`, etc.)

Note: `--root` only applies to `check-links`. Other commands use index roots and profiles.

The command reports broken links, missing target files, and invalid anchors, including source file and line location.

**Example**

```bash
yore check-links --index docs/.index --json
```

---

### 7.9 `yore fix-links`

Automatically fixes a conservative subset of broken relative links.

```bash
yore fix-links --index <index-dir> [--dry-run|--apply]
```

**Key options**

* `--dry-run` – Show proposed edits without modifying files
* `--apply` – Apply changes to files on disk
* `--propose` – Output ambiguous link fixes to a YAML file
* `--apply-decisions` – Apply choices from a previous proposal file
* `--json` – Emit JSON output
* `--use-git-history` – Use git rename history when suggesting fixes for moved files

The command looks for links whose targets do not correspond to any indexed file but whose filename matches exactly one indexed document under the same directory tree. It then rewrites those link targets to point to the matching file.

**Examples**

```bash
# Preview safe link fixes
yore fix-links --index docs/.index --dry-run

# Apply safe link fixes
yore fix-links --index docs/.index --apply
```

---

### 7.10 `yore mv`

Moves a documentation file and optionally updates inbound references.

```bash
yore mv <from> <to> --index <index-dir> [--update-refs] [--dry-run]
```

**Key options**

* `--update-refs` – Rewrite Markdown links that point to `<from>` so they point to `<to>`
* `--dry-run` – Show planned moves/rewrites without modifying files
* `--json` – Emit JSON output

**Examples**

```bash
# Move a file and update all inbound links
yore mv docs/old/auth.md docs/architecture/AUTH.md --index docs/.index --update-refs

# Preview changes only
yore mv docs/old/auth.md docs/architecture/AUTH.md --index docs/.index --update-refs --dry-run
```

---

### 7.11 `yore fix-references`

Rewrites references according to an explicit mapping file, useful for bulk reorganizations.

```bash
yore fix-references --mapping <file> --index <index-dir> [--dry-run|--apply]
```

The mapping file is a small YAML document:

```yaml
mappings:
  - from: docs/old/auth.md
    to: docs/architecture/AUTH.md
  - from: docs/old/payments.md
    to: docs/architecture/PAYMENTS.md
```

Each mapping is applied across all indexed files by rewriting `]({from})` to `]({to})`.

**Key options**

* `--mapping` – Path to reference mapping file (`from`/`to` pairs)
* `--index` – Index directory (default: `.yore`)
* `--dry-run` – Show planned changes without modifying files
* `--apply` – Apply changes to files
* `--json` – Emit JSON output

**Examples**

```bash
# Preview bulk reference changes only
yore fix-references --mapping mappings.yaml --index docs/.index --dry-run

# Apply bulk reference changes
yore fix-references --mapping mappings.yaml --index docs/.index --apply
```

---

### 7.12 `yore backlinks`

Lists all documents that link to a specified file.

```bash
yore backlinks <file> --index <index-dir>
```

**Key options**

* `--json` – Emit JSON output

This is useful for safe deletion or refactoring: you can see which documents reference a given file before modifying or removing it.

**Example**

```bash
yore backlinks docs/architecture/DEPLOYMENT-GUIDE.md --index docs/.index
```

---

### 7.13 `yore stale`

Reports potentially stale documentation based on modification time and inbound links.

```bash
yore stale --index <index-dir> --days <N> --min-inlinks <M> [--json]
```

**Key options**

* `--days` – Minimum age in days to consider a file stale (default: 90)
* `--min-inlinks` – Minimum inbound link count (files with >= this many links are included; default: 0)
* `--json` – Emit JSON output

**Example**

```bash
yore stale --index docs/.index --days 90 --min-inlinks 0 --json
```

---

### 7.14 `yore orphans`

Finds documents with no inbound links (potential cleanup candidates or undocumented islands).

```bash
yore orphans --index <index-dir>
```

**Key options**

* `--json` – Emit JSON output
* `--exclude, -e` – Exclude files matching a pattern (repeatable), for example `README` or `INDEX`

**Example**

```bash
# Find orphans excluding README and INDEX files
yore orphans --index docs/.index --exclude README --exclude INDEX
```

---

### 7.15 `yore canonicality`

Reports canonicality scores for documents based on path, naming, and other trust signals.

```bash
yore canonicality --index <index-dir>
```

**Key options**

* `--json` – Emit JSON output
* `--threshold, -t` – Minimum score threshold (0.0–1.0, default: 0.0)

**Scoring factors** (example configuration):

* Architecture / ADR directories: +0.20
* Index / overview documents: +0.15
* README / Guide / Runbook filenames: +0.10
* Scratch / archive / old directories: −0.30
* Deprecated / backup indicators: −0.25

**Example**

```bash
# Show only high‑authority documents
yore canonicality --index docs/.index --threshold 0.7
```

---

### 7.16 `yore canonical-orphans`

Reports canonical documents with zero inbound links.

```bash
yore canonical-orphans --index <index-dir> --threshold <0.0-1.0>
```

**Key options**

* `--json` – Emit JSON output
* `--threshold, -t` – Minimum canonicality score (0.0-1.0, default: 0.7)

**Example**

```bash
yore canonical-orphans --index docs/.index --threshold 0.7
```

---

### 7.17 `yore export-graph`

Exports the documentation link graph as JSON or Graphviz DOT.

```bash
yore export-graph --index <index-dir> --format <json|dot>
```

**Examples**

```bash
# JSON graph for downstream tooling
yore export-graph --index docs/.index --format json > graph.json

# DOT graph for visualization
yore export-graph --index docs/.index --format dot > graph.dot
```

---

### 7.18 `yore suggest-consolidation`

Suggests consolidation candidates based on duplicate detection and canonicality scoring.

```bash
yore suggest-consolidation --index <index-dir> --threshold <0.0–1.0> [--json]
```

Each suggestion identifies a canonical document and a set of files that are strong duplication candidates to merge into it.

**Example**

```bash
yore suggest-consolidation --index docs/.index --threshold 0.7 --json
```

---

### 7.19 `yore vocabulary`

Derive a deterministic list of domain-relevant vocabulary terms from indexed content.

```bash
yore vocabulary --index <index-dir>
```

**Key options**

* `--index, -i` – Index directory (default: `.yore`)
* `--limit, -n` – Maximum number of terms to return (default: `100`)
* `--format` – Output format: `lines`, `json`, or `prompt` (default: `lines`)
* `--json` – Alias for `--format json`
* `--stopwords` – Optional custom stop-word file path
* `--include-stemming` – Include stem-only terms when no surface form is available
* `--no-default-stopwords` – Disable built-in vocabulary stop-words
* `--common-terms <N>` – Drop the top `N` most common corpus terms before ranking vocabulary

**Output modes**

* `lines` (default): one term per line
* `json`: structured payload including `format`, `limit`, `total`, and `terms`
* `prompt`: comma-separated terms for LLM prompts

**Example**

```bash
yore vocabulary --index docs/.index --limit 40 --format lines
yore vocabulary --index docs/.index --format prompt --limit 100
yore vocabulary --index docs/.index --format json --limit 25 --stopwords .yore-stopwords.txt
yore vocabulary --index docs/.index --limit 80 --common-terms 20
yore vocabulary --index docs/.index --no-default-stopwords --stopwords /usr/share/dict/words
```

---

### 7.20 `yore similar`

Finds documents similar to a reference file.

```bash
yore similar <file> --index <index-dir>
```

**Key options**

* `--limit, -n` – Maximum number of results (default: 5)
* `--threshold` – Similarity threshold (0.0–1.0, default: 0.3)
* `--json` – Emit machine‑readable JSON
* `--doc-terms` – Show top N distinctive terms per result (0 disables)

**Example**

```bash
yore similar docs/adr/ADR-0013-retries.md --index docs/.index --limit 5
yore similar docs/architecture/AUTH.md --threshold 0.4 --json --index docs/.index
```

---

### 7.21 `yore diff`

Show overlapping content and shared sections between two files.

```bash
yore diff <file1> <file2> --index <index-dir>
```

**Key options**

* `--json` – Emit JSON output

**Example**

```bash
yore diff docs/old.md docs/new.md --index docs/.index --json
```

---

### 7.22 `yore stats`

Show high-level index statistics.

```bash
yore stats --index <index-dir>
```

**Key options**

* `--top-keywords` – Number of top keywords to show (default: 20)
* `--json` – Emit JSON output

**Example**

```bash
yore stats --index docs/.index --top-keywords 20 --json
```

---

### 7.23 `yore repl`

Start an interactive query REPL over the index.

```bash
yore repl --index <index-dir>
```

**Example**

```bash
yore repl --index docs/.index
```

---

### 7.24 `yore policy`

Check documentation against declarative policy rules.

```bash
yore policy --config <file> --index <index-dir>
```

**Key options**

* `--config` – Policy file path (default: `.yore-policy.yaml`)
* `--index, -i` – Index directory (default: `.yore`)
* `--json` – Emit JSON output

**Example**

```bash
yore policy --config .yore-policy.yaml --index docs/.index --json
```

---

## 8. Configuration and Profiles

Yore can optionally be configured via a `.yore.toml` file at the repository root. This allows you to define named index profiles and reuse them across commands.

```toml
[index.docs]
roots = ["docs"]
types = ["md"]
output = "docs/.index"

[index.docs_plus_agents]
roots = ["docs", "agents"]
types = ["md"]
output = ".yore-docs-plus-agents"
```

You can then reference these profiles from the CLI:

```bash
# Build the docs-only index defined above
yore --profile docs build

# Run link checks against the docs profile without spelling out --index
yore --profile docs check-links --json --summary
```

CLI flags always override profile settings when explicitly provided (for example, passing `--index` or `--types`).

> **Important:** Profiles control which roots are indexed. If you care about reviewing **all** documentation (including scattered notes, ADRs, and embedded docs), make sure you also have a full-repo profile (for example, `roots = ["."]`) or run `yore build .` without a profile. Overly narrow profiles will cause Yore to ignore files outside the declared roots, which is useful for focused checks but detrimental for whole-repo documentation review.

---

## 9. Use Cases

### Documentation cleanup

Use duplicate and orphan detection to simplify and de‑duplicate the documentation tree.

```bash
# Duplicate sections (raw)
yore dupes-sections --index docs/.index --json

# Wrapper script example (if present)
./scripts/docs/find-duplicates.sh | jq .
```

### LLM‑ready context for agents

Generate precise, high‑signal context for agent tasks:

```bash
yore assemble "How do I deploy a new service?" \
  --max-tokens 8000 \
  --depth 1 \
  --index docs/.index > context.md
```

Agents can treat `context.md` as the only trusted context when answering the question.

### Documentation‑steward agent integration

Yore is designed to be used as the backing engine for a documentation‑maintenance agent. Typical agent workflows include:

* Validating that all links are resolvable (`yore check-links`).
* Locating duplicates before consolidation (`yore dupes`, `yore dupes-sections`).
* Identifying canonical documents for a topic (`yore canonicality`).
* Discovering orphaned documents (`yore orphans`).
* Finding all inbound references to a document before moving or deleting it (`yore backlinks`).

---

## 10. Determinism and Performance

Yore is intentionally deterministic:

* The same index and configuration always produce the same search results and assembled contexts.
* No embeddings, no approximate nearest neighbor search, and no sampling are used.

This enables:

* Reliable regression testing via `yore eval`.
* Cacheable results in CI or agent pipelines.
* Predictable behavior for long‑running automation.

Observed performance characteristics on a mid‑sized corpus (illustrative, not a guarantee):

* Indexing approximately 200–300 files: on the order of seconds.
* Querying with BM25: typically well under 10 ms per query.
* Evaluation over a small test set: a few seconds.

Actual performance depends on repository size, hardware, and configuration.

---

## 10. Case Study: AI-Assisted Documentation Audit

> **Evaluator:** Claude Code (Opus 4.5) as documentation-steward agent

### The Project

A full-stack monorepo with ~1,800 source files and ~375K lines of code:

| Type       | Files | Lines of Code |
|------------|-------|---------------|
| Python     | 620   | 121,389       |
| TypeScript | 141   | 24,191        |
| Markdown   | 602   | 228,459       |
| YAML       | 291   | —             |
| Shell      | 150   | —             |

Yore indexed 365 of the markdown files (those in `docs/` and `agents/`), producing 12,486 unique keywords and 12,841 heading entries.

### The Problem

Auditing docs in a large monorepo: finding duplicates, identifying similar files, assembling context for LLM analysis.

### Why Not Just Use the LLM Directly?

Without yore, the agent would need to:
- **Read files to find relevant docs:** ~50 Read tool calls, scanning manually
- **Compare for duplicates:** N×N = 66,430 pairs to evaluate
- **Token cost to ingest:** 365 files × ~500 tokens = 182,500 tokens
- **Context limits:** Can't fit corpus in memory; requires chunked passes
- **Latency:** Minutes of inference vs milliseconds of index lookup

With yore, the agent queries a pre-built index. The LLM never touches irrelevant files.

### Performance Comparison

| Operation          | yore    | grep        | LLM-based            |
|--------------------|---------|-------------|----------------------|
| Keyword search     | 0.07s   | 1.88s       | ~30s + tokens        |
| Duplicate scan     | 1ms     | impossible  | ~10min + 182K tokens |
| Index build        | <1s     | N/A         | N/A                  |

### What Yore Found (in 1ms)

```
Duplicates:  12 file pairs via LSH, 49 section clusters across 314 files
Actionable:  66% overlap between two definitions → consolidation candidate
```

### The Key Insight

`yore assemble "auth setup" --max-tokens 3000` returns a token-budgeted digest with source citations—pre-filtered, ranked, ready for analysis. The LLM processes 3K relevant tokens instead of 182K raw tokens.

### Summary

| Metric              | Without yore | With yore | Improvement     |
|---------------------|--------------|-----------|-----------------|
| Tokens consumed     | 182,500      | 3,000     | **98% savings** |
| Duplicate detection | ~10 minutes  | 1ms       | **600,000x**    |
| Search latency      | 1.88s        | 0.07s     | **27x faster**  |

**Yore sits between raw grep and expensive LLM inference.** It handles filtering and similarity math so the LLM can focus on reasoning, not searching.

*— Claude Code (Opus 4.5), November 2025*

---

## 11. License

Yore is licensed under the MIT License.

## 12. References

Yore implements several well-established algorithms and documentation patterns.
The following references represent the foundational ideas and techniques directly used in Yore’s design and implementation.

### Core Ranking and Retrieval

**1. Okapi BM25**
Robertson, S. E., & Walker, S.
*Some simple effective approximations to the 2–Poisson model for probabilistic weighted retrieval.*
SIGIR ’94.
Defines the BM25 ranking function Yore uses as the primary retrieval model.

### Duplicate and Similarity Detection

**2. MinHash & Locality-Sensitive Hashing (LSH)**
Broder, A.
*On the resemblance and containment of documents.*
Compression and Complexity of Sequences 1997.
Introduces MinHash, used in Yore for approximate Jaccard similarity.

**3. SimHash**
Charikar, M.
*Similarity estimation techniques from rounding algorithms.*
STOC 2002.
Defines SimHash, which Yore uses for near-duplicate and section-level similarity detection.

### Documentation Structure and Cross-Referencing

**4. Architecture Decision Records (ADR pattern)**
Nygard, Michael.
*Documenting Architecture Decisions.*
2011.
Establishes the ADR format that Yore recognizes, parses, and expands during cross-reference resolution.

### Extractive Techniques for High-Signal Summaries

**5. TextRank (Sentence Ranking for Extractive Summaries)**
Mihalcea, R., & Tarau, P.
*TextRank: Bringing Order into Texts.*
EMNLP 2004.
Provides the conceptual basis for Yore’s sentence-level scoring and extractive refinement.
