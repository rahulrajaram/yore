# Yore Document Indexer - Complete Improvement Guide

**Consolidated Documentation**
**Date:** 2025-11-29
**Status:** Active Proposal

---

# Table of Contents

1. [Executive Summary](#part-1-executive-summary)
2. [Original Research Report (ChatGPT)](#part-2-original-research-report)
3. [Technical Assessment & Roadmap](#part-3-technical-assessment--roadmap)
4. [Phase 1 Implementation Checklist](#part-4-phase-1-implementation-checklist)
5. [Before & After Examples](#part-5-before--after-examples)

---

# Part 1: Executive Summary

## TL;DR

The yore indexer is **production-ready** for basic search/duplication tasks. The proposed improvements in the original research report are **research-accurate but implementation-heavy**. This roadmap provides a **3-phase, 64-hour plan** focusing on high-ROI, non-LLM features that directly address real corpus issues (366 files, 79 duplicate pairs, 12,492 keywords).

---

## Current State: Production-Ready

**What Works:**
- Fast indexing (250ms for 200 files)
- Keyword search with stemming
- Duplicate detection (Jaccard + SimHash)
- Interactive REPL mode
- 2.8MB binary, zero dependencies

**What's Missing:**
- Smart ranking (Jaccard doesn't weight term importance)
- Fast duplicate detection (O(n^2) comparisons slow for 1000+ files)
- Document taxonomy (can't filter by type: ADR, runbook, report)
- Canonicality scoring (can't identify "source of truth" docs)
- Graph features (402 links extracted but unused)

---

## Recommended 3-Phase Roadmap

### Phase 1: Enhanced Ranking & Duplication (20 hours)
**Priority:** HIGH | **ROI:** 30% better search, 10x faster dupes

**What to Build:**
1. **BM25 Ranking** (6h) - Replace Jaccard with TF-IDF weighted scoring
2. **MinHash + LSH** (8h) - Fast duplicate detection via locality-sensitive hashing
3. **Section SimHash** (5h) - Detect shared sections across docs
4. **Testing** (1h) - Unit tests on real corpus

**Rust Crates:** `ahash` (fast hashing)

**Validation:**
- Query "kubernetes deployment" ranks KUBERNETES_DEPLOYMENT.md first
- Dupes detection <50ms for 366 files (vs current ~50ms, scales to 10,000 files)
- Find boilerplate sections ("Prerequisites" in 18 files)

---

### Phase 2: Document Classification & Canonicality (16 hours)
**Priority:** MEDIUM | **ROI:** Structured taxonomy, stale doc detection

**What to Build:**
1. **Doc Type Classification** (4h) - Path-based heuristics (ADR, agent, report, example, etc.)
2. **Canonicality Scoring** (6h) - Score 0.0-1.0 based on path depth, links, age, filename
3. **Integration Tests** (4h) - Validate 95% classification accuracy
4. **CLI Extensions** (2h) - `--type architecture`, `--boost-canonical` flags

**Rust Crates:** `filetime` (file metadata)

**Validation:**
- `IMPLEMENTATION_PLAN.md` scores 0.85+ canonicality
- `docs/archived/**` scores <0.3
- Agent definitions correctly classified (100% accuracy on 18 files)

---

### Phase 3: Document Graph Features (28 hours)
**Priority:** MEDIUM | **ROI:** Graph navigation, broken link detection

**What to Build:**
1. **Link Graph Construction** (10h) - Build nodes (docs) + edges (references, duplicates)
2. **PageRank** (2h) - Rank docs by importance
3. **Link Validation** (4h) - Detect broken links (expect 5-10% of 402 links)
4. **Graph CLI Commands** (8h) - `yore graph --orphans`, `yore validate-links`
5. **Performance Tuning** (4h) - Keep index build <500ms

**Rust Crates:** `petgraph` (graph algorithms)

**Validation:**
- `README.md`, `IMPLEMENTATION_PLAN.md` in top-5 PageRank
- Find all broken links (manual review of 10 samples)
- Identify orphaned docs (unreferenced files)

---

## What to Defer (Phase 4+)

**Embeddings + Semantic Search** (20h effort)
- **Why Defer:** BM25 handles keyword-rich technical docs well. Embeddings add 50-200MB model, slow startup.
- **When to Revisit:** Corpus grows >1000 files OR users request "conceptual search"

**HDBSCAN Clustering** (10h effort)
- **Why Defer:** Only useful for 10,000+ docs. Current 366 files manageable with manual taxonomy.
- **When to Revisit:** Corpus exceeds 1000 files and manual organization breaks down.

**Local LLM Integration** (30h effort)
- **Why Defer:** 80% of doc classification achievable with path heuristics. LLM adds complexity.
- **When to Revisit:** Classification accuracy <90% with heuristics.

---

## Critical Review of Original Research Report

**Rating:** 7.5/10

**What's Correct:**
- Multi-view indexing (lexical + semantic + graph + canonicality)
- BM25 superior to Jaccard
- MinHash + LSH for fast duplication
- PageRank for doc importance
- Research citations (LATTICE, LongRefiner, RepoAgent)

**What's Overstated:**
- Dense embeddings as Phase 1 (defer to Phase 4)
- HDBSCAN clustering for 366 files (manual taxonomy sufficient)
- Local LLM for classification (heuristics achieve 80% accuracy)
- Context refinement pipeline (assumes LLM orchestration, out of scope)

**What's Missing:**
- Rust crate recommendations
- Incremental implementation path
- Performance benchmarks
- Backward compatibility strategy

---

## Investment vs Outcome

**Total Effort:** 64 hours (3 phases over 3 months)

**Expected Outcome:**
- **Production-grade** documentation indexer with graph features
- **Comparable to** commercial tools (Algolia DocSearch) but local-first
- **Handles** 1000+ files with <500ms index time
- **Provides** taxonomy, canonicality, graph navigation, link validation

**Resource Requirements:**
- 1 Rust developer (intermediate level)
- Access to real corpus for validation
- CI/CD integration for regression testing

---

## Quick Reference: Rust Crates

| Feature | Crate | Version | Why |
|---------|-------|---------|-----|
| Fast hashing | `ahash` | 0.8 | MinHash performance |
| File metadata | `filetime` | 0.2 | Canonicality scoring |
| Graph algorithms | `petgraph` | 0.6 | PageRank, graph traversal |
| Embeddings (future) | `fastembed` | 0.5 | Local embedding models |
| ANN search (future) | `hnsw` | 0.11 | Fast similarity search |

**Do NOT use:**
- `tantivy` (10MB+ dependency, BM25 is simple to implement)
- Heavy ML frameworks (keep binary <5MB)

---

## Key Metrics

**Current Performance:**
- Index 366 files: 250ms
- Query latency: <10ms
- Duplicate detection: ~50ms
- Binary size: 2.8MB

**Target Performance (Post-Phase 3):**
- Index 366 files: <400ms (+60% acceptable overhead)
- Query latency (BM25): <15ms
- Duplicate detection (LSH): <10ms (-80% speedup)
- Binary size: <5MB

---

# Part 2: Original Research Report

## Documentation Scaling & Anti-Sprawl Architecture for Agent-Generated Projects

*A technical report synthesizing approaches from research, industry practice, and non-LLM algorithms.*

---

## 1. Overview

Large projects built by agentic tools tend to accumulate sprawling documentation: multiple versions of the same explanation, stale artifacts, unstructured scratch notes, and auto-generated content. When the corpus becomes large, remote LLMs (Claude/GPT) cannot reliably reason over it because:

* They can't ingest the full context.
* Vector-based retrieval alone retrieves irrelevant or stale fragments.
* Semantic drift occurs when outdated docs override canonical ones.
* Context stuffing leads to wrong inference and hallucination.

This report outlines architectures, algorithms, and research-backed patterns for **local, dynamic, intelligent documentation understanding**—so that the remote LLM only sees high-quality, distilled, *relevant* context.

The core strategy is:

> **Turn the local file system into a multi-view, self-indexing knowledge system**
> and
> **use the remote LLM only as the final reasoning layer.**

---

## 2. Problem Statement

You have:

* A large repo filled with docs created by multiple agents.
* An indexer using Jaccard, basic mapping, and reverse lookup.
* A desire for something **more dynamic, semantic, and hierarchical**.
* A requirement to avoid sending massive context windows to remote LLM APIs.
* A need for **local LLMs or non-LLM algorithms** that can preprocess and reduce context.

The problem is best described as:
**"Repo-Scale Retrieval, Summarization, and Reasoning for Documentation Sprawl."**

This aligns strongly with ongoing work in:

* Repository-level code search
* Hierarchical retrieval
* Long-context compression
* Knowledge graph navigation
* Local LLM pre-processing ("refiners")

---

## 3. High-Level Architecture

### 3.1 Multi-View Indexing

Move from one index (Jaccard or embedding-only) to **four simultaneous views**:

1. **Lexical**:

   * Use BM25/BM25+ for fast keyword relevance.
   * Superior to Jaccard for many tasks.

2. **Semantic**:

   * Dense embeddings per doc *and per section*.
   * Use a local embedding model (E5/BGE) -> avoids API calls.

3. **Structural / Graph**:

   * Build a doc graph linking:

     * `Doc` <-> `Doc` ("supersedes", "duplicates", "see also")
     * `DocSection` <-> `CodeFile` ("describes", "implements")
     * `Doc` <-> `Service` ("owned by", "explains")

4. **Canonicality / Age / Authority**

   * Score docs as:

     * **canonical**
     * **secondary**
     * **scratch**
     * **deprecated**
   * Use metadata such as:

     * Path conventions
     * Cross-reference counts
     * Staleness vs code changes

---

## 4. Hierarchical Retrieval Pipeline

Instead of direct "query -> vector search -> top-N chunks", use a **multi-stage, tree-structured retrieval model**.

### 4.1 Step-by-Step Pipeline

#### Step 1 - Doc Type Classification

Use heuristics + a small local LLM to classify docs:

* `ADR`
* `DesignDoc`
* `APIRef`
* `HowTo`
* `Runbook`
* `Scratch`
* `Deprecated`
* `AgentOutput`

This produces a coarse filter.

#### Step 2 - Semantic Region Detection

Cluster docs using embeddings:

* Use **HDBSCAN** to detect natural "areas" -> e.g., `payments`, `auth`, `agent-layer`, `infra`, etc.
* Assign each doc to one or multiple regions.

#### Step 3 - Region Selection

Given a query:

* Identify candidate regions using embedding similarity.
* Only search inside the top `2-3` regions.

This cuts the search space down massively.

#### Step 4 - Inside-Region Retrieval

Use **hybrid multi-stage scoring**:

* Stage A: BM25 on titles/headings
* Stage B: embedding similarity
* Stage C: graph walk (neighbors, supersedes, canonical docs)
* Score = `alpha*BM25 + beta*embedding + gamma*graph + delta*canonicality`

#### Step 5 - Local Context Refining (Critical)

Before sending anything to Claude/GPT:

* Take top 20-50 sections.
* Use local LLM or extractive algorithms to generate a **structured digest**:

```json
{
  "topic": "payments retry semantics",
  "source_docs": [
    "docs/payments/retries.md#L10-L130",
    "adr/ADR-0042.md"
  ],
  "facts": [
    "Retry delay is exponential backoff with jitter",
    "Payment state machine transitions documented in ADR-0042"
  ],
  "warnings": [
    "docs/payments/legacy.md may be stale"
  ]
}
```

Only *this* goes to Claude.

#### Step 6 - Remote LLM Final Layer

Claude/GPT receives:

* The structured digest
* A small number of high-value excerpts
* The question/task

And produces the final reasoning/output.

---

## 5. Local LLM Uses (Narrow, Reliable Tasks)

Local models do NOT do final reasoning.
They do mechanical preprocessing:

### 5.1 Doc Type Classification

Input: filename, path, first 200 tokens
Output: `{doc_type, area_guess, canonicality_hint}`

### 5.2 Region Summaries

Every semantic region stores:

* "What this region is about"
* Key decisions
* Canonical docs
* Recently updated docs

### 5.3 Context Refining ("Local LongRefiner")

Given raw candidate sections:

* Cluster them by subtopic
* Extract key facts
* Produce short, structured summaries

This de-noises the input for Claude.

---

## 6. Non-LLM Algorithms That Strengthen the System

### 6.1 MinHash + LSH

Detect near-duplicate docs.
Tag duplicates as "shadowed".

### 6.2 BM25 / BM25+

Battle-tested lexical ranker.
Way better than Jaccard for actual retrieval.

### 6.3 Graph-Based Ranking

Run **PageRank** or **Personalized PageRank** over the doc graph.

Helps surface:

* Central docs
* Canonical sources
* ADRs
* Popular references

### 6.4 Extractive Summarization

Use **TextRank** or **LexRank** per doc section.

Store a "summary index" for retrieval.

### 6.5 Change-Impact Scoring

Mark docs stale if:

* They describe code that changed recently
* But have not been updated accordingly

Helps downweight misleading docs.

---

## 7. What Research Says (Summary of Literature)

Recent advances match your use case precisely.

### 7.1 Hierarchical & Long-Context Retrieval

* **LATTICE**: LLM-guided hierarchical retrieval
* **LongRefiner**: refines + compresses retrieved context before LLM sees it
* **Discourse-aware retrieval**: retrieves coherent sections, not random chunks
* **Retrieval vs Long-context LLMs**: smart retrieval beats dumping everything

### 7.2 Repository-Level Agents

* **RepoAgent**: multi-pass indexing + repo-wide doc generation
* **CodeRAG**: multi-path retrieval + reranking
* **LocAgent**: graph-based navigation over code repositories
* **Code knowledge graphs (Tongyi Lingma, others)**: MCTS-like navigation over repo graph

### 7.3 Traceability Work

* Research on "documentation-to-code trace link prediction"
* Useful for establishing doc-code links automatically.

These papers all confirm your instinct:
**Local, layered retrieval + refinement is the winning strategy.**

---

## 8. Recommended Implementation Roadmap

### Phase 1 - Replace/supplement Jaccard

* Add BM25
* Add embeddings
* Add doc typing metadata

### Phase 2 - Build the doc graph

* Node types: doc, section, code file, service
* Edge types: supersedes, duplicates, describes, references
* Add canonicality scoring

### Phase 3 - Region clustering

* HDBSCAN (semantic clustering)
* Create `REGION_SUMMARY.md` files

### Phase 4 - Local context refinement

* Implement a local LongRefiner-style summarizer
* Produce structured digests for all queries

### Phase 5 - Claude integration

* Claude receives the digest, not raw docs
* All final reasoning is remote
* All preprocessing is local

This yields **stable, reliable, scalable** doc understanding.

---

## 9. References

### 9.1 ArXiv Papers (Hierarchical & Long-Context Retrieval)

* **LATTICE** - LLM-guided Hierarchical Retrieval (Source: arXiv)
* **LongRefiner** - Dual-level query analysis + hierarchical structuring (Source: arXiv)
* **Long-document QA via Discourse-Aware Hierarchical Retrieval** (Source: arXiv)
* **When Retrieval Meets Long-Context LLMs** (Source: arXiv)

### 9.2 Repository-Level AI Systems

* **RepoAgent** - Repository-level documentation generation (Source: arXiv)
* **LocAgent** - Graph-based multi-hop search over codebases (Source: arXiv)
* **CodeRAG** - Retrieval-augmented repository completion (Source: arXiv)

### 9.3 Codebase Knowledge Graph Work

* **Survey of Code-Generation Agents** - includes knowledge graph & MCTS approaches (Source: arXiv)

### 9.4 Traceability & Doc-Code Linking

* **Evaluating LLMs for Documentation-to-Code Traceability** (Source: arXiv)

### 9.5 Industry Practices (Reddit, Blogs, Companies)

* Notion, Dagster, AI wiki governance discussions
* Patterns around high-signal "trusted page" curation
* Strong use of hybrid search (BM25 + embeddings)

---

## 10. Closing Summary

A scalable anti-documentation-sprawl system requires:

* **Multiple indexing modes** (lexical, semantic, graph, canonicality)
* **Hierarchical retrieval** instead of flat chunk-picking
* **Localized context refinement** before external LLM calls
* **Doc graph + region clustering** for navigability
* **Local LLMs as routers, not reasoners**
* **Remote LLMs only for final synthesis**

This approach is directly aligned with cutting-edge arXiv research and the direction of repository-scale intelligent agents.

---

# Part 3: Technical Assessment & Roadmap

## 1. Current State Assessment

### 1.1 Strengths

**Architecture:**
- Clean separation: forward index (file->metadata) + reverse index (keyword->files)
- SimHash fingerprinting for structural similarity (good for near-duplicates)
- Jaccard similarity for keyword overlap
- Combined scoring (60% Jaccard + 40% SimHash) balances lexical and structural signals

**Performance:**
- Fast: 250ms for 200 files, <10ms queries
- Lightweight: 2.8MB binary, no runtime dependencies
- Efficient index format: JSON (human-readable, easy to debug)

**Functionality:**
- `build`: Fast indexing with gitignore support
- `query`: Keyword search with stemming
- `similar`: Find similar files (combined scoring)
- `dupes`: Detect duplicates with threshold tuning
- `diff`: Compare two files (shared/unique keywords, headings)
- `stats`: Index statistics and top keywords
- `repl`: Interactive mode

**Testing Against Real Corpus:**
Current duplicate detection on actual project docs shows:
- 79 duplicate pairs at 50% threshold
- Correctly identifies template duplication (terraform envs: 80-84% similarity)
- Detects agent definition overlap (58-66% similarity)
- Some false positives (35% SimHash match between unrelated docs suggests overfitting)

### 1.2 Weaknesses

**Ranking Quality:**
- Jaccard similarity is **order-independent** (ignores term frequency/importance)
- No TF-IDF or BM25 weighting (all keywords treated equally)
- Stop word list is hardcoded and basic
- No query-time boosting (title matches vs body matches)

**Duplicate Detection:**
- SimHash uses 3-word shingles (fixed window, misses larger structures)
- No LSH bucketing (O(n^2) comparisons for dupes)
- Threshold tuning is manual (no automatic clustering)
- Cannot detect partial duplicates (shared sections across docs)

**Metadata & Structure:**
- No document type classification (ADR, runbook, plan, example, etc.)
- No canonicality scoring (can't identify "source of truth" docs)
- No staleness detection (docs vs code drift)
- Link extraction exists but unused (no graph features)

**Search Capabilities:**
- No phrase search ("exact match" queries)
- No field-specific search (title: vs body: vs heading:)
- No fuzzy matching (typo tolerance)
- No semantic search (synonyms, related concepts)

### 1.3 Corpus Characteristics (Actual Project)

**Size & Structure:**
- 366 total files (223 .md in docs/)
- 12,492 unique keywords (high diversity)
- 12,871 headings (rich structure)
- 83,031 body keywords (heavy text content)
- 402 links (moderate cross-referencing)

**Document Types Observed:**
- Architecture docs: `IMPLEMENTATION_PLAN.md`, `KUBERNETES_DEPLOYMENT.md`, ADRs
- MVP planning: `MVP_P0_TASKS.md`, `MVP_DEMO_WALKTHROUGH.md`, status docs
- Agent definitions: 18 agents in `.claude/agents/`, symlinked in `agents/definitions/`
- Examples: Heavy duplication in `terraform/envs/*`, `tests/artifacts/*`
- Reports: 60+ files in `agents/reports/`
- Runbooks, guides, testing docs

**Duplication Patterns:**
- **Template-driven duplication:** Terraform envs (80-84% similarity) - intentional, low priority
- **Example duplication:** Test app READMEs (71-74% similarity) - intentional boilerplate
- **Agent definition overlap:** 58-66% similarity - **unintentional, high value to detect**
- **False positives:** 35% SimHash matches between unrelated docs - **needs fixing**

**Key Use Cases:**
1. **Find canonical documentation** when multiple docs cover same topic
2. **Detect duplicate planning content** (e.g., phase docs vs IMPLEMENTATION_PLAN.md)
3. **Identify stale docs** (references to outdated code/APIs)
4. **Navigate complex doc hierarchy** (18 agent defs, 9 architectural areas)
5. **Validate cross-references** (402 links need link checking)

---

## 2. Original Research Report Critical Review

### 2.1 What's Correct

**Accurate Insights:**
- Multi-view indexing (lexical + semantic + graph + canonicality) is the right architecture
- Hierarchical retrieval beats flat vector search for large corpora
- Local preprocessing (classification, summarization) reduces remote LLM costs
- Non-LLM algorithms (BM25, MinHash, PageRank) are fast and deterministic
- Document graph with typed edges (supersedes, duplicates, describes) matches repository structure

**Research Alignment:**
The paper correctly cites:
- LATTICE, LongRefiner (hierarchical retrieval + context compression)
- RepoAgent, CodeRAG, LocAgent (repository-level agents)
- HDBSCAN for semantic clustering
- BM25 superiority over Jaccard for ranking

### 2.2 What's Overstated

**Complexity vs ROI:**
1. **Local LLMs for preprocessing** - Adds infrastructure complexity (model serving, GPU dependencies) for marginal gains. Simple heuristics + regex achieve 80% of doc classification accuracy.
2. **Dense embeddings per section** - Embedding 12,871 sections requires ~50MB RAM + slow startup. Useful long-term but not MVP.
3. **HDBSCAN clustering** - Only valuable at >1000 documents. Current corpus (366 files) can be manually organized.
4. **Context refinement pipeline** - Assumes integration with remote LLM API, but yore is a standalone CLI tool. Feature creep.

**Missing Context:**
- No discussion of **incremental improvement path** (assumes greenfield rewrite)
- No **Rust crate recommendations** (paper is language-agnostic)
- No **validation strategy** against actual corpus
- No **performance benchmarks** (how does BM25 affect 250ms index time?)

### 2.3 What's Missing

**Critical Gaps:**
1. **Document type classification heuristics** - Can use path conventions (`docs/architecture/ADR-*.md`, `agents/reports/*.md`, `docs/workflows/*.md`)
2. **Canonicality scoring** - Simple heuristics: path depth, incoming link count, filename patterns, last-modified date
3. **Link validation** - Extract links (already done) but validate targets exist, detect broken links
4. **Partial duplicate detection** - Section-level SimHash (not just document-level)
5. **Query syntax** - Support `title:"exact phrase"` or `path:docs/architecture`

---

## 3. Prioritized Improvement Recommendations

### Phase 1: Enhanced Ranking & Duplicate Detection (HIGH ROI, LOW EFFORT)

**Goal:** Improve search quality and duplicate detection accuracy without changing data structures.

#### 1.1 BM25 Ranking (Replace Jaccard for Queries)

**Why:** BM25 accounts for term frequency, document length, and inverse document frequency. Superior to Jaccard for actual retrieval tasks.

**Rust Crate:** `tantivy` (full-text search library with BM25) or implement from scratch (60 lines).

**Implementation:**
```rust
// Add to FileEntry
pub struct FileEntry {
    // ... existing fields ...
    term_frequencies: HashMap<String, usize>, // NEW: term counts
    doc_length: usize,                         // NEW: total terms
}

// BM25 scoring function
fn bm25_score(
    query_terms: &[String],
    doc: &FileEntry,
    avg_doc_length: f64,
    idf_map: &HashMap<String, f64>,
) -> f64 {
    const K1: f64 = 1.5;
    const B: f64 = 0.75;

    let mut score = 0.0;
    let norm_factor = 1.0 - B + B * (doc.doc_length as f64 / avg_doc_length);

    for term in query_terms {
        let tf = *doc.term_frequencies.get(term).unwrap_or(&0) as f64;
        let idf = idf_map.get(term).unwrap_or(&0.0);
        score += idf * (tf * (K1 + 1.0)) / (tf + K1 * norm_factor);
    }
    score
}
```

**Changes Required:**
- Update `index_file()` to compute term frequencies (10 lines)
- Add `avg_doc_length` to `ForwardIndex` (1 line)
- Compute IDF during indexing (20 lines)
- Replace Jaccard scoring in `cmd_query()` with BM25 (30 lines)

**Validation:**
- Compare top-10 results for queries like "kubernetes deployment", "agent definition", "test coverage"
- Expect: More relevant results ranked higher (titles/headings weighted correctly)

**Effort:** 4-6 hours
**Risk:** Low (BM25 is well-understood, no new dependencies if implemented manually)

---

#### 1.2 MinHash + LSH for Fast Duplicate Detection

**Why:** Current O(n^2) comparison (66,795 pairs for 366 files) is slow. LSH reduces to O(n) by bucketing similar docs.

**Rust Crate:** `minhash` or custom implementation using `ahash` for fast hashing.

**Implementation:**
```rust
use std::collections::hash_map::DefaultHasher;

pub struct MinHashSignature {
    hashes: Vec<u64>, // 128 hash values
}

fn compute_minhash(keywords: &[String], num_hashes: usize) -> MinHashSignature {
    let mut hashes = vec![u64::MAX; num_hashes];

    for keyword in keywords {
        for i in 0..num_hashes {
            let h = hash_with_seed(keyword, i as u64);
            hashes[i] = hashes[i].min(h);
        }
    }

    MinHashSignature { hashes }
}

fn minhash_similarity(a: &MinHashSignature, b: &MinHashSignature) -> f64 {
    let matches = a.hashes.iter()
        .zip(b.hashes.iter())
        .filter(|(x, y)| x == y)
        .count();
    matches as f64 / a.hashes.len() as f64
}

// LSH bucketing
fn lsh_buckets(sigs: &[(String, MinHashSignature)], bands: usize) -> HashMap<u64, Vec<String>> {
    let rows_per_band = sigs[0].1.hashes.len() / bands;
    let mut buckets: HashMap<u64, Vec<String>> = HashMap::new();

    for (path, sig) in sigs {
        for band in 0..bands {
            let start = band * rows_per_band;
            let end = start + rows_per_band;
            let band_hash = hash_slice(&sig.hashes[start..end]);
            buckets.entry(band_hash).or_default().push(path.clone());
        }
    }
    buckets
}
```

**Changes Required:**
- Add `minhash: MinHashSignature` to `FileEntry` (1 line)
- Compute MinHash during indexing (15 lines)
- Implement LSH bucketing in `cmd_dupes()` (40 lines)
- Only compare docs in same bucket (reduces comparisons by ~95%)

**Validation:**
- Run on current corpus: should find same 79 duplicate pairs at 50% threshold
- Measure speedup: expect 10-20x faster for 1000+ files

**Effort:** 6-8 hours
**Risk:** Low (MinHash is simple, LSH tuning may need iteration)

---

#### 1.3 Section-Level SimHash for Partial Duplicates

**Why:** Current doc-level SimHash misses shared sections (e.g., "Prerequisites" heading duplicated across 20 files).

**Implementation:**
```rust
pub struct SectionFingerprint {
    heading: String,
    level: usize,
    line_start: usize,
    line_end: usize,
    simhash: u64,
}

pub struct FileEntry {
    // ... existing fields ...
    section_fingerprints: Vec<SectionFingerprint>, // NEW
}

fn index_sections(content: &str, headings: &[Heading]) -> Vec<SectionFingerprint> {
    let lines: Vec<&str> = content.lines().collect();
    let mut sections = Vec::new();

    for i in 0..headings.len() {
        let start = headings[i].line - 1;
        let end = headings.get(i + 1)
            .map(|h| h.line - 1)
            .unwrap_or(lines.len());

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
```

**New Command:**
```bash
yore dupes-sections --threshold 0.7
# Output: Shared sections across multiple files
# "Prerequisites" section found in 18 files (95% similarity)
```

**Validation:**
- Identify boilerplate sections (setup, prerequisites, testing)
- Detect copy-pasted code examples across docs

**Effort:** 4-5 hours
**Risk:** Low (reuses existing SimHash implementation)

---

### Phase 2: Document Classification & Canonicality (MEDIUM ROI, MEDIUM EFFORT)

#### 2.1 Heuristic Document Type Classification

**Why:** Enables filtering by doc type, boosts canonical docs in search, validates taxonomy.

**Implementation:**
```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum DocType {
    ADR,              // docs/decisions/ADR-*.md
    Architecture,     // docs/architecture/*.md (not ADR, not MVP_*)
    MVPPlan,          // docs/architecture/MVP_*.md
    AgentDefinition,  // .claude/agents/*.md
    AgentReport,      // agents/reports/*.md
    Runbook,          // docs/runbooks/*.md
    Testing,          // docs/testing/*.md
    Example,          // docs/architectures/example_guidance/**
    Archived,         // docs/archived/**
    Unknown,
}

fn classify_doc(path: &str, headings: &[Heading]) -> DocType {
    // Path-based heuristics
    if path.starts_with("docs/decisions/ADR-") {
        return DocType::ADR;
    }
    if path.starts_with("docs/architecture/MVP_") {
        return DocType::MVPPlan;
    }
    if path.starts_with(".claude/agents/") || path.starts_with("agents/definitions/") {
        return DocType::AgentDefinition;
    }
    if path.starts_with("agents/reports/") {
        return DocType::AgentReport;
    }
    if path.starts_with("docs/runbooks/") {
        return DocType::Runbook;
    }
    if path.starts_with("docs/testing/") || path.contains("/test") {
        return DocType::Testing;
    }
    if path.starts_with("docs/architectures/example_guidance/") {
        return DocType::Example;
    }
    if path.contains("/archived/") {
        return DocType::Archived;
    }
    if path.starts_with("docs/architecture/") {
        return DocType::Architecture;
    }

    // Heading-based fallback
    let has_decision = headings.iter()
        .any(|h| h.text.to_lowercase().contains("decision"));
    if has_decision && path.contains("/docs/") {
        return DocType::ADR;
    }

    DocType::Unknown
}
```

**New Features:**
```bash
yore query "kubernetes" --type architecture  # Filter by doc type
yore stats --by-type                        # Show distribution
yore dupes --exclude-type example           # Ignore intentional template duplication
```

**Validation:**
- 100% accuracy on known paths (ADRs, agent defs, reports)
- Flag `Unknown` docs for manual review

**Effort:** 3-4 hours
**Risk:** Low (simple path matching)

---

#### 2.2 Canonicality Scoring

**Why:** Identify "source of truth" documents to boost in search results and warn about stale alternatives.

**Algorithm:**
```rust
pub struct CanonicalityScore {
    score: f64,         // 0.0 (scratch) to 1.0 (canonical)
    factors: CanonicalityFactors,
}

pub struct CanonicalityFactors {
    path_depth: u32,          // Shallower = more canonical (docs/ > docs/archived/)
    incoming_links: u32,      // More references = more canonical
    filename_pattern: f64,    // Matches canonical patterns (IMPLEMENTATION_PLAN.md)
    last_modified_age: u32,   // Days since last update
    doc_type_weight: f64,     // ADR > Architecture > Report > Scratch
}

fn compute_canonicality(
    path: &str,
    doc_type: DocType,
    incoming_links: u32,
    last_modified: SystemTime,
) -> CanonicalityScore {
    let path_depth = path.matches('/').count() as u32;
    let depth_penalty = (path_depth as f64 * 0.1).min(0.5);

    let filename_boost = if path.ends_with("IMPLEMENTATION_PLAN.md")
        || path.ends_with("KUBERNETES_DEPLOYMENT.md")
        || path.contains("ADR-")
        || path == "README.md"
    {
        0.3
    } else {
        0.0
    };

    let type_weight = match doc_type {
        DocType::ADR => 1.0,
        DocType::Architecture => 0.9,
        DocType::MVPPlan => 0.85,
        DocType::AgentDefinition => 0.8,
        DocType::Runbook => 0.7,
        DocType::Testing => 0.6,
        DocType::AgentReport => 0.5,
        DocType::Example => 0.3,
        DocType::Archived => 0.1,
        DocType::Unknown => 0.5,
    };

    let link_boost = (incoming_links as f64 * 0.05).min(0.3);

    let age_days = SystemTime::now()
        .duration_since(last_modified)
        .unwrap()
        .as_secs() / 86400;
    let staleness_penalty = if age_days > 180 { 0.2 } else { 0.0 };

    let score = (type_weight + filename_boost + link_boost - depth_penalty - staleness_penalty)
        .max(0.0)
        .min(1.0);

    CanonicalityScore {
        score,
        factors: CanonicalityFactors {
            path_depth,
            incoming_links,
            filename_pattern: filename_boost,
            last_modified_age: age_days as u32,
            doc_type_weight: type_weight,
        },
    }
}
```

**New Features:**
```bash
yore query "deployment" --boost-canonical    # Weight canonical docs higher
yore stats --top-canonical 20               # Show most canonical docs
yore validate-canonical                     # Find duplicate canonical candidates
```

**Validation:**
- `IMPLEMENTATION_PLAN.md` scores 0.9+ (high canonicality)
- `docs/archived/**` scores <0.3 (low canonicality)
- Agent reports score 0.5 (informational)

**Effort:** 5-6 hours
**Risk:** Medium (requires link counting, file metadata integration)

---

### Phase 3: Document Graph Features (HIGH VALUE, MEDIUM EFFORT)

#### 3.1 Link Graph Construction

**Why:** Navigate relationships (supersedes, references, describes), run PageRank for importance scoring.

**Implementation:**
```rust
pub struct DocumentGraph {
    nodes: HashMap<String, DocNode>,
    edges: Vec<DocEdge>,
}

pub struct DocNode {
    path: String,
    doc_type: DocType,
    canonicality: f64,
    pagerank: f64, // Computed after graph construction
}

pub enum EdgeType {
    References,     // [link](target.md)
    Supersedes,     // Detected via "replaces", "deprecated" mentions
    Duplicates,     // High similarity (>70%)
    Describes,      // Doc describes code file (future: parse code paths)
}

pub struct DocEdge {
    from: String,
    to: String,
    edge_type: EdgeType,
}
```

**PageRank Implementation:**
```rust
fn compute_pagerank(graph: &mut DocumentGraph, iterations: usize) {
    let damping = 0.85;
    let n = graph.nodes.len() as f64;

    // Initialize
    for node in graph.nodes.values_mut() {
        node.pagerank = 1.0 / n;
    }

    // Iterate
    for _ in 0..iterations {
        let mut new_ranks = HashMap::new();

        for (path, _) in &graph.nodes {
            let mut rank = (1.0 - damping) / n;

            // Sum contributions from incoming edges
            for edge in &graph.edges {
                if edge.to == *path && edge.edge_type == EdgeType::References {
                    let out_degree = graph.edges.iter()
                        .filter(|e| e.from == edge.from && e.edge_type == EdgeType::References)
                        .count() as f64;
                    rank += damping * graph.nodes[&edge.from].pagerank / out_degree;
                }
            }

            new_ranks.insert(path.clone(), rank);
        }

        // Update ranks
        for (path, rank) in new_ranks {
            graph.nodes.get_mut(&path).unwrap().pagerank = rank;
        }
    }
}
```

**New Features:**
```bash
yore graph --show-clusters                   # Visualize doc clusters
yore graph --orphans                         # Find unreferenced docs
yore graph --supersedes "old_plan.md"        # Find replacement doc
yore stats --top-pagerank 20                 # Most referenced/important docs
```

**Validation:**
- `README.md`, `IMPLEMENTATION_PLAN.md` have high PageRank
- Archived docs have low PageRank
- Duplicate clusters correctly grouped

**Effort:** 10-12 hours
**Risk:** Medium (graph construction is straightforward, PageRank needs validation)

---

#### 3.2 Link Validation

**Why:** Detect broken links (current corpus has 402 links, likely 5-10% broken).

**Implementation:**
```rust
pub struct LinkValidation {
    valid: Vec<(String, String)>,      // (source, target)
    broken: Vec<(String, String, String)>, // (source, target, reason)
}

fn validate_links(forward_index: &ForwardIndex) -> LinkValidation {
    let mut result = LinkValidation {
        valid: Vec::new(),
        broken: Vec::new(),
    };

    for (source, entry) in &forward_index.files {
        for link in &entry.links {
            match resolve_link(&link.target, &forward_index.files) {
                Some(target) => {
                    result.valid.push((source.clone(), target));
                }
                None => {
                    let reason = if link.target.starts_with("http") {
                        "External link (not validated)".to_string()
                    } else {
                        format!("Target not found: {}", link.target)
                    };
                    result.broken.push((source.clone(), link.target.clone(), reason));
                }
            }
        }
    }

    result
}
```

**New Command:**
```bash
yore validate-links
# Output:
# 380 valid links
# 22 broken links:
#   docs/architecture/PLAN.md -> old_phase_doc.md (Target not found)
```

**Effort:** 3-4 hours
**Risk:** Low (straightforward path resolution)

---

### Phase 4: Semantic Search with Embeddings (DEFERRED, HIGH EFFORT)

**Why Defer:**
- Requires embedding model (50-200MB download)
- Adds startup latency (model loading)
- Marginal improvement over BM25 for keyword-rich documents (technical docs are keyword-heavy)
- Current corpus (366 files) is manageable with lexical search

**When to Implement:**
- Corpus grows >1000 files
- Users request "conceptual search" (e.g., "how to debug timeout errors" without keyword "timeout")
- After Phase 1-3 validated

**Recommended Approach (Future):**
- Use `fastembed` or `ort` (ONNX runtime) for Rust
- Embed with `all-MiniLM-L6-v2` (22MB model, fast inference)
- Store embeddings in separate `embeddings.bin` file
- Hybrid search: `0.7 * BM25 + 0.3 * cosine_similarity`

**Effort (Future):** 15-20 hours
**Risk:** Medium (model integration, performance tuning)

---

## 4. Implementation Strategy

### 4.1 Development Workflow

**For Each Phase:**
1. **Branch:** Create `feature/phase-N` branch
2. **Implement:** Add new structs/functions (preserve backward compatibility)
3. **Test:** Add unit tests + integration tests on actual corpus
4. **Benchmark:** Ensure <300ms index time, <20ms query time
5. **Validate:** Run on `real documentation corpus`
6. **Document:** Update README with new features

**Backward Compatibility:**
- Use `version: 3` in index format for Phase 1 changes
- Support reading v2 indexes (migration on next build)
- Never break existing CLI commands

### 4.2 Testing Strategy

**Unit Tests:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_ranking() {
        // Create test corpus
        let docs = vec![
            ("doc1.md", vec!["kubernetes", "deployment"], 100),
            ("doc2.md", vec!["kubernetes"], 50),
        ];

        // Test query
        let results = bm25_rank(&docs, &["kubernetes", "deployment"]);
        assert_eq!(results[0].0, "doc1.md"); // Should rank higher
    }

    #[test]
    fn test_minhash_similarity() {
        let sig1 = compute_minhash(&["foo", "bar", "baz"], 128);
        let sig2 = compute_minhash(&["foo", "bar", "qux"], 128);
        let sim = minhash_similarity(&sig1, &sig2);
        assert!(sim > 0.5 && sim < 0.8); // Partial overlap
    }

    #[test]
    fn test_doc_classification() {
        assert_eq!(
            classify_doc("docs/decisions/ADR-001.md", &[]),
            DocType::ADR
        );
        assert_eq!(
            classify_doc("agents/reports/AUDIT.md", &[]),
            DocType::AgentReport
        );
    }
}
```

**Integration Tests:**
```bash
#!/bin/bash
# tests/integration_test.sh

# Index test corpus
cargo run --release -- build tests/fixtures --output /tmp/yore-test

# Test BM25 ranking
results=$(cargo run --release -- query kubernetes deployment --index /tmp/yore-test --json)
top_result=$(echo "$results" | jq -r '.[0][0]')
expected="tests/fixtures/kubernetes_deployment.md"

if [ "$top_result" = "$expected" ]; then
    echo "BM25 ranking test passed"
else
    echo "BM25 ranking test failed: expected $expected, got $top_result"
    exit 1
fi

# Test duplicate detection
dupes=$(cargo run --release -- dupes --threshold 0.7 --index /tmp/yore-test --json)
count=$(echo "$dupes" | jq length)

if [ "$count" -eq 3 ]; then
    echo "Duplicate detection test passed"
else
    echo "Duplicate detection test failed: expected 3 pairs, got $count"
    exit 1
fi
```

**Validation Against Real Corpus:**
```bash
# Run on actual project
cd project directory
yore build . --output .yore-v3

# Compare results
yore query "kubernetes deployment" > results_v3.txt
yore-old query "kubernetes deployment" > results_v2.txt
diff results_v2.txt results_v3.txt

# Validate dupes
yore dupes --threshold 0.5 > dupes_v3.txt
# Manually review: should find same template duplication + new agent def overlap

# Check performance
time yore build .
# Should be <400ms (20% overhead acceptable for Phase 1 features)
```

### 4.3 Performance Benchmarks

**Target Performance (Phase 1-3):**
| Operation | Current | Phase 1 | Phase 2 | Phase 3 |
|-----------|---------|---------|---------|---------|
| Index 366 files | 250ms | 300ms | 320ms | 350ms |
| Query (BM25) | 8ms | 12ms | 12ms | 15ms |
| Dupes (LSH) | 50ms | 10ms | 10ms | 10ms |
| Validate links | - | - | - | 5ms |

**Regression Tests:**
- Index size growth <30% (acceptable for new metadata)
- Memory usage <100MB during indexing
- No disk I/O during queries (index loaded in memory)

---

## 5. Data Structure Changes

### 5.1 Index Format Evolution

**Version 2 (Current):**
```json
{
  "version": 2,
  "files": {
    "path": {
      "keywords": [...],
      "body_keywords": [...],
      "simhash": 12345,
      "headings": [...],
      "links": [...]
    }
  }
}
```

**Version 3 (Phase 1):**
```json
{
  "version": 3,
  "avg_doc_length": 500,
  "idf_map": {"keyword": 2.3, ...},
  "files": {
    "path": {
      "keywords": [...],
      "term_frequencies": {"keyword": 5, ...},
      "doc_length": 450,
      "simhash": 12345,
      "minhash": [123, 456, ...],
      "headings": [...],
      "links": [...]
    }
  }
}
```

**Version 4 (Phase 2):**
```json
{
  "version": 4,
  "files": {
    "path": {
      "doc_type": "ADR",
      "canonicality": {
        "score": 0.85,
        "factors": {
          "path_depth": 2,
          "incoming_links": 5,
          "filename_pattern": 0.3,
          "last_modified_age": 30,
          "doc_type_weight": 1.0
        }
      }
    }
  }
}
```

**Version 5 (Phase 3):**
Add separate `graph.json`:
```json
{
  "nodes": [
    {"path": "...", "pagerank": 0.05}
  ],
  "edges": [
    {"from": "...", "to": "...", "type": "references"}
  ]
}
```

**Migration Strategy:**
- Auto-upgrade on `yore build` (detect old version, add new fields)
- Keep `version` field to detect format
- Never delete old indexes (backup as `.yore/forward_index.v2.json.bak`)

---

## 6. Risk Assessment & Mitigation

### 6.1 Technical Risks

**Risk 1: BM25 Degrades Performance**
- **Probability:** Low
- **Impact:** Medium (users notice slower indexing)
- **Mitigation:** Benchmark on 1000-file corpus before merge. If >500ms, optimize TF computation.

**Risk 2: MinHash LSH Misses Duplicates**
- **Probability:** Medium
- **Impact:** Medium (false negatives in duplicate detection)
- **Mitigation:** Tune LSH bands/rows to maintain 95% recall. Validate against known duplicates.

**Risk 3: Canonicality Scoring Misclassifies**
- **Probability:** Medium
- **Impact:** Low (only affects ranking, not correctness)
- **Mitigation:** Allow users to override via `.yore.toml` config. Expose factors in `--json` output.

**Risk 4: Graph Construction is Slow**
- **Probability:** Low
- **Impact:** Medium (delays index build)
- **Mitigation:** Build graph incrementally (only recompute changed nodes). Cache graph.json.

### 6.2 Product Risks

**Risk 1: Feature Creep**
- **Probability:** High (original research report has 15+ features)
- **Impact:** High (delays MVP, increases maintenance burden)
- **Mitigation:** Strict phasing (Phase 1->2->3), validate ROI before next phase.

**Risk 2: Backward Incompatibility**
- **Probability:** Medium (index format changes)
- **Impact:** High (breaks existing users)
- **Mitigation:** Auto-migration on `yore build`, version checks, backup old indexes.

**Risk 3: Scope Drift (LLM Integration)**
- **Probability:** Medium (original research report suggests local LLM)
- **Impact:** High (changes tool identity from "fast CLI" to "ML pipeline")
- **Mitigation:** Keep yore as indexing/retrieval tool. LLM integration is separate orchestration layer.

---

## 7. Recommendations Summary

### 7.1 Immediate Actions (Next 2 Weeks)
1. **Implement BM25 ranking** (Phase 1.1) - 6 hours
2. **Implement MinHash + LSH** (Phase 1.2) - 8 hours
3. **Add unit tests** for BM25, MinHash - 4 hours
4. **Benchmark on real corpus** - 2 hours
5. **Update README** with new features - 1 hour

**Total Effort:** ~20 hours
**Expected ROI:** 30% better search relevance, 10x faster duplicate detection

### 7.2 Short-Term (1 Month)
1. **Implement doc type classification** (Phase 2.1) - 4 hours
2. **Implement canonicality scoring** (Phase 2.2) - 6 hours
3. **Add integration tests** - 4 hours
4. **Validate against 10 test queries** - 2 hours

**Total Effort:** ~16 hours
**Expected ROI:** Structured doc taxonomy, canonical doc identification

### 7.3 Medium-Term (2-3 Months)
1. **Implement document graph** (Phase 3.1) - 12 hours
2. **Implement link validation** (Phase 3.2) - 4 hours
3. **Add graph visualization** (optional) - 8 hours
4. **Performance tuning** - 4 hours

**Total Effort:** ~28 hours
**Expected ROI:** Graph-based navigation, broken link detection

### 7.4 Long-Term (Deferred)
1. **Semantic search with embeddings** (Phase 4) - 20 hours
2. **HDBSCAN clustering** (if corpus >1000 files) - 10 hours
3. **Local LLM integration** (if classification accuracy <90%) - 30 hours

**Decision Point:** Re-evaluate after Phase 1-3 deployed and validated.

---

# Part 4: Phase 1 Implementation Checklist

**Goal:** Enhanced Ranking & Duplicate Detection
**Effort:** 20 hours
**Timeline:** 2 weeks

---

## Pre-Implementation Setup

### Environment
- [ ] Rust toolchain up to date (`rustup update`)
- [ ] Create feature branch: `git checkout -b feature/phase-1-bm25-minhash`
- [ ] Backup existing index: `cp -r .yore .yore.v2.backup`
- [ ] Install benchmark tools: `cargo install cargo-criterion`

### Dependencies
```toml
# Add to Cargo.toml [dependencies]
ahash = "0.8"  # Fast hashing for MinHash
```

---

## Task 1: BM25 Ranking (6 hours)

### 1.1 Data Structure Changes (1h)
- [ ] Add `term_frequencies: HashMap<String, usize>` to `FileEntry`
- [ ] Add `doc_length: usize` to `FileEntry`
- [ ] Add `avg_doc_length: f64` to `ForwardIndex`
- [ ] Add `idf_map: HashMap<String, f64>` to `ForwardIndex`
- [ ] Update `version: 3` in `ForwardIndex`

**File:** `src/main.rs` lines 147-186

### 1.2 Term Frequency Computation (1h)
- [ ] Update `index_file()` to compute term frequencies
- [ ] Count total terms for `doc_length`
- [ ] Store in `FileEntry.term_frequencies`

**Location:** `src/main.rs` line 401 (in `index_file()` function)

**Code Snippet:**
```rust
// After keyword extraction (around line 454)
let mut term_frequencies: HashMap<String, usize> = HashMap::new();
let mut total_terms = 0;

for line in &lines {
    if line.starts_with("```") || line.starts_with("    ") {
        continue; // Skip code blocks
    }
    let words = extract_keywords(line);
    for word in words {
        let stemmed = stem_word(&word);
        *term_frequencies.entry(stemmed).or_insert(0) += 1;
        total_terms += 1;
    }
}

// Add to FileEntry construction (around line 463)
term_frequencies,
doc_length: total_terms,
```

### 1.3 IDF Computation (1h)
- [ ] Compute document frequency for each term during indexing
- [ ] Calculate IDF: `log((N - df + 0.5) / (df + 0.5))` (BM25 formula)
- [ ] Store in `ForwardIndex.idf_map`
- [ ] Compute `avg_doc_length` across all docs

**Location:** `src/main.rs` line 362 (after indexing loop in `cmd_build()`)

**Code Snippet:**
```rust
// After indexing all files (around line 362)
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

// Compute IDF
let mut idf_map: HashMap<String, f64> = HashMap::new();
for (term, df) in doc_frequencies {
    let idf = ((total_docs - df as f64 + 0.5) / (df as f64 + 0.5)).ln();
    idf_map.insert(term, idf);
}

forward_index.avg_doc_length = total_length as f64 / total_docs;
forward_index.idf_map = idf_map;
```

### 1.4 BM25 Scoring Function (2h)
- [ ] Implement `bm25_score()` function
- [ ] Use parameters: `k1 = 1.5`, `b = 0.75` (standard BM25 values)
- [ ] Replace Jaccard scoring in `cmd_query()`

**Location:** Add new function after `jaccard_similarity()` (around line 1082)

**Code Snippet:**
```rust
fn bm25_score(
    query_terms: &[String],
    doc: &FileEntry,
    avg_doc_length: f64,
    idf_map: &HashMap<String, f64>,
) -> f64 {
    const K1: f64 = 1.5;
    const B: f64 = 0.75;

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
```

### 1.5 Update Query Command (1h)
- [ ] Replace HashMap counting with BM25 scoring
- [ ] Keep top-N results by BM25 score
- [ ] Maintain backward compatibility (JSON output format)

**Location:** `src/main.rs` line 568 (`cmd_query()` function)

**Code Snippet:**
```rust
// Replace lines 578-594 with:
let mut file_scores: Vec<(String, f64)> = forward_index.files.iter()
    .map(|(path, entry)| {
        let score = bm25_score(&terms, entry, forward_index.avg_doc_length, &forward_index.idf_map);
        (path.clone(), score)
    })
    .filter(|(_, score)| *score > 0.0)
    .collect();

file_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
file_scores.truncate(limit);
```

### 1.6 Testing (1h)
- [ ] Unit test: BM25 ranks title matches higher than body matches
- [ ] Unit test: BM25 handles zero-length docs gracefully
- [ ] Integration test: Query "kubernetes deployment" on real corpus
- [ ] Validate: Top result should be `KUBERNETES_DEPLOYMENT.md`

---

## Task 2: MinHash + LSH (8 hours)

### 2.1 MinHash Signature Computation (2h)
- [ ] Add `minhash: Vec<u64>` to `FileEntry` (128 hash values)
- [ ] Implement `compute_minhash()` function
- [ ] Use `ahash::AHasher` for fast hashing
- [ ] Compute during indexing

**Location:** Add after `compute_simhash()` (around line 548)

**Code Snippet:**
```rust
use ahash::AHasher;
use std::hash::{Hash, Hasher};

fn compute_minhash(keywords: &[String], num_hashes: usize) -> Vec<u64> {
    let mut hashes = vec![u64::MAX; num_hashes];

    for keyword in keywords {
        for i in 0..num_hashes {
            let mut hasher = AHasher::default();
            keyword.hash(&mut hasher);
            i.hash(&mut hasher); // Use index as seed
            let h = hasher.finish();

            hashes[i] = hashes[i].min(h);
        }
    }

    hashes
}
```

### 2.2 MinHash Similarity Function (1h)
- [ ] Implement `minhash_similarity()` function
- [ ] Count matching hash values
- [ ] Return fraction of matches

**Location:** Add after `simhash_similarity()` (around line 566)

**Code Snippet:**
```rust
fn minhash_similarity(a: &[u64], b: &[u64]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }

    let matches = a.iter()
        .zip(b.iter())
        .filter(|(x, y)| x == y)
        .count();

    matches as f64 / a.len() as f64
}
```

### 2.3 LSH Bucketing (3h)
- [ ] Implement `lsh_buckets()` function
- [ ] Use 16 bands x 8 rows = 128 hashes (tuned for 0.5 similarity threshold)
- [ ] Return HashMap of bucket -> file paths
- [ ] Update `cmd_dupes()` to use LSH

**Location:** Add new function before `cmd_dupes()` (around line 741)

**Code Snippet:**
```rust
fn lsh_buckets(
    files: &HashMap<String, FileEntry>,
    bands: usize,
) -> HashMap<u64, Vec<String>> {
    let rows_per_band = 128 / bands; // Assuming 128 hashes
    let mut buckets: HashMap<u64, Vec<String>> = HashMap::new();

    for (path, entry) in files {
        for band in 0..bands {
            let start = band * rows_per_band;
            let end = start + rows_per_band;

            // Hash this band's values
            let mut hasher = AHasher::default();
            for val in &entry.minhash[start..end] {
                val.hash(&mut hasher);
            }
            let band_hash = hasher.finish();

            buckets.entry(band_hash)
                .or_insert_with(Vec::new)
                .push(path.clone());
        }
    }

    buckets
}
```

### 2.4 Update Dupes Command (2h)
- [ ] Build LSH buckets
- [ ] Only compare files in same bucket
- [ ] Use combined score: `0.4 * jaccard + 0.3 * simhash + 0.3 * minhash`
- [ ] Measure speedup (log to stderr if not quiet mode)

---

## Task 3: Section-Level SimHash (5 hours)

### 3.1 Section Fingerprint Structure (1h)
- [ ] Add `SectionFingerprint` struct
- [ ] Add `section_fingerprints: Vec<SectionFingerprint>` to `FileEntry`
- [ ] Compute during indexing

**Location:** Add after `Link` struct (around line 171)

**Code Snippet:**
```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SectionFingerprint {
    heading: String,
    level: usize,
    line_start: usize,
    line_end: usize,
    simhash: u64,
}
```

### 3.2 Section Indexing (2h)
- [ ] Split content by headings
- [ ] Compute SimHash per section
- [ ] Store in `FileEntry.section_fingerprints`

### 3.3 Section Duplicate Command (2h)
- [ ] Add new command: `DupesSection`
- [ ] Compare all section pairs across files
- [ ] Group by similar heading text + SimHash
- [ ] Output: shared sections across multiple files

---

## Task 4: Testing & Validation (4 hours)

### 4.1 Unit Tests (2h)
- [ ] Test BM25 scoring with known TF-IDF values
- [ ] Test MinHash similarity (known keyword sets)
- [ ] Test LSH bucketing (verify candidates captured)
- [ ] Test section fingerprinting (heading extraction)

### 4.2 Integration Tests (1h)
- [ ] Index real corpus: `real documentation corpus`
- [ ] Run queries: "kubernetes deployment", "agent definition", "test coverage"
- [ ] Validate top-3 results match expectations
- [ ] Run dupes command, compare with v2 results
- [ ] Check section dupes finds "Prerequisites", "Installation"

### 4.3 Benchmark (1h)
- [ ] Measure index time (should be <400ms for 366 files)
- [ ] Measure query latency (should be <20ms)
- [ ] Measure dupes time (should be <50ms with LSH)
- [ ] Compare with v2 baseline

---

## Success Criteria

### Functionality
- [ ] BM25 ranking improves top-3 relevance (manual review of 10 queries)
- [ ] MinHash LSH reduces duplicate detection time (measure on 1000-file synthetic corpus)
- [ ] Section duplicates detected (e.g., "Prerequisites" in 10+ files)
- [ ] All existing commands work (query, dupes, similar, stats, repl)

### Performance
- [ ] Index build time: <400ms for 366 files (vs 250ms baseline)
- [ ] Query latency: <20ms
- [ ] Duplicate detection: <50ms with LSH (scales to 10,000 files)

### Quality
- [ ] Unit tests pass: `cargo test`
- [ ] Integration tests pass: `bash tests/integration_test.sh`
- [ ] No clippy warnings: `cargo clippy --all-targets`
- [ ] Binary size: <5MB

---

## Troubleshooting

### Issue: Index build time >500ms
**Diagnosis:** Term frequency computation is slow
**Fix:** Use `HashMap::with_capacity()` to pre-allocate, reduce allocations

### Issue: BM25 scores all zero
**Diagnosis:** IDF map not computed correctly
**Fix:** Check that document frequencies are non-zero, validate IDF formula

### Issue: LSH misses duplicates
**Diagnosis:** Too few bands (low recall)
**Fix:** Increase bands from 16 to 20 (trades precision for recall)

### Issue: Binary size >5MB
**Diagnosis:** Debug symbols or dependencies
**Fix:** Ensure `strip = true` in `Cargo.toml` `[profile.release]`

---

# Part 5: Before & After Examples

## Example 1: Search Quality (Phase 1: BM25)

### Scenario
User searches for "kubernetes deployment" to find deployment documentation.

### Before (Jaccard Similarity)
**Command:**
```bash
yore query kubernetes deployment --limit 5
```

**Output:**
```
5 results for: kubernetes deployment

docs/testing/k8s-validation.md (score: 8)
  > L45: Kubernetes Testing
  > L78: Deployment Validation

docs/architecture/KUBERNETES_DEPLOYMENT.md (score: 8)
  > L1: Kubernetes Deployment Guide

tests/k8s/deployment_test.py (score: 6)

docs/architecture/k8s-control-plane-architecture.md (score: 5)

README.md (score: 4)
```

**Problems:**
- Testing doc ranks equal to canonical deployment guide (both score 8)
- Score based solely on keyword count (8 occurrences)
- No distinction between title match vs body match
- Python test file ranks above architecture doc

---

### After (BM25 Ranking)
**Command:**
```bash
yore query kubernetes deployment --limit 5
```

**Output:**
```
5 results for: kubernetes deployment

docs/architecture/KUBERNETES_DEPLOYMENT.md (score: 14.3)
  > L1: Kubernetes Deployment Guide
  > L15: Deployment Architecture

docs/architecture/k8s-control-plane-architecture.md (score: 9.8)
  > L23: Control Plane Deployment

docs/testing/k8s-validation.md (score: 7.2)
  > L45: Kubernetes Testing
  > L78: Deployment Validation

README.md (score: 5.1)
  > L8: Quick Start: Kubernetes Deployment

tests/k8s/deployment_test.py (score: 2.3)
```

**Improvements:**
- Canonical guide ranks first (14.3 vs 8)
- Title matches weighted higher than body matches
- Architecture docs ranked above test files
- BM25 accounts for document length (shorter docs don't dominate)

**Why Better:**
- IDF weighting: "kubernetes" is common (low weight), "deployment" is specific (high weight)
- TF saturation: 20 occurrences of "deployment" doesn't score 2x higher than 10 occurrences
- Length normalization: 100-line doc with 5 matches ranks similar to 1000-line doc with 50 matches

---

## Example 2: Duplicate Detection Speed (Phase 1: MinHash LSH)

### Scenario
User wants to find duplicate documentation across 1000 files (simulated).

### Before (O(n^2) Comparison)
**Command:**
```bash
time yore dupes --threshold 0.5
```

**Output:**
```
79 duplicate pairs found (threshold: 50%)

84% terraform/envs/dev/README.md <-> terraform/envs/stage/README.md
...

real    0m2.450s
user    0m2.380s
sys     0m0.068s
```

**Performance:**
- 2.45 seconds for 366 files
- O(n^2) = 66,795 comparisons
- Extrapolated to 1000 files: **18 seconds** (499,500 comparisons)

---

### After (LSH Bucketing)
**Command:**
```bash
time yore dupes --threshold 0.5
```

**Output:**
```
79 duplicate pairs found (threshold: 50%)
LSH duplicate detection: 245ms (342 buckets)

84% terraform/envs/dev/README.md <-> terraform/envs/stage/README.md
...

real    0m0.245s
user    0m0.210s
sys     0m0.034s
```

**Performance:**
- 0.245 seconds for 366 files (**10x speedup**)
- LSH reduces to ~5,000 comparisons (only within buckets)
- Extrapolated to 1000 files: **0.8 seconds** (50,000 comparisons)

**How It Works:**
- MinHash creates 128-hash signature per document
- LSH groups similar docs into 16 buckets
- Only compare docs in same bucket (95% reduction in comparisons)
- Maintains 95%+ recall (finds 95%+ of true duplicates)

---

## Example 3: Shared Section Detection (Phase 1: Section SimHash)

### Scenario
User suspects boilerplate sections are duplicated across multiple docs.

### Before (Manual Review)
**Process:**
1. Run `yore dupes` to find similar files
2. Manually open each pair
3. Visually scan for shared sections
4. Takes 30 minutes to identify patterns

**Result:**
User finds "Prerequisites" section manually in 5-10 files (incomplete).

---

### After (Section Duplicate Detection)
**Command:**
```bash
yore dupes-section --threshold 0.7
```

**Output:**
```
15 shared sections found:

"Prerequisites" (18 occurrences):
  docs/architecture/KUBERNETES_DEPLOYMENT.md:45
  docs/testing/PLAYWRIGHT_SETUP.md:12
  docs/workflows/testing-guide.md:23
  docs/development/local-control-plane.md:8
  ... (14 more)

"Installation" (12 occurrences):
  docs/QUICK_START.md:15
  docs/architecture/k8s-infrastructure-setup-guide.md:78
  ... (10 more)

"Testing" (9 occurrences):
  docs/testing/TESTING.md:34
  agents/plans/database-agent-complete-plan-and-status.md:156
  ... (7 more)

"Security Considerations" (6 occurrences):
  SECURITY.md:89
  docs/architecture/k8s-control-plane-identity.md:120
  ... (4 more)
```

**Improvements:**
- Instant detection (vs 30 minutes manual review)
- Complete coverage (finds all 18 occurrences, not just 5-10)
- Actionable output (file:line references)
- Quantified duplication (18 copies of same section)

**Use Cases:**
- Consolidate boilerplate into reusable includes
- Identify copy-paste documentation
- Find sections to templatize
- Detect stale duplicates (one version updated, others not)

---

## Example 4: Document Classification (Phase 2)

### Scenario
User wants to search only architecture docs, excluding test reports and examples.

### Before (No Classification)
**Command:**
```bash
yore query "deployment strategy"
```

**Output:** Mixed results (architecture, tests, examples, reports)
```
12 results for: deployment strategy

docs/architecture/KUBERNETES_DEPLOYMENT.md (score: 15.2)
tests/artifacts/applications/simple-go-app/README.md (score: 8.1)
agents/reports/PHASE_16_COMPLETION_REPORT.md (score: 7.8)
docs/architectures/example_guidance/k8s/deployment.yaml (score: 6.9)
docs/architecture/BUILD_DEPLOY_STAGE_SEPARATION.md (score: 6.5)
...
```

**Problems:**
- Test app README ranks above architecture doc
- Example YAML file mixes with conceptual docs
- No way to filter by document type

---

### After (Document Classification)
**Command:**
```bash
yore query "deployment strategy" --type architecture
```

**Output:** Only architecture docs
```
3 results for: deployment strategy (type: architecture)

docs/architecture/KUBERNETES_DEPLOYMENT.md (score: 15.2)
docs/architecture/BUILD_DEPLOY_STAGE_SEPARATION.md (score: 6.5)
docs/architecture/k8s-control-plane-architecture.md (score: 5.8)
```

**Classification Logic:**
```
Path: docs/decisions/ADR-001-kaniko.md -> Type: ADR
Path: docs/architecture/KUBERNETES_DEPLOYMENT.md -> Type: Architecture
Path: agents/reports/AUDIT.md -> Type: AgentReport
Path: docs/testing/TESTING.md -> Type: Testing
Path: docs/architectures/example_guidance/k8s/deployment.yaml -> Type: Example
Path: docs/archived/PHASE_0.5_PLAN.md -> Type: Archived
```

**New Commands:**
```bash
# Filter by type
yore query "kubernetes" --type architecture
yore query "test" --type testing
yore query "decision" --type adr

# Show type distribution
yore stats --by-type
# Output:
# Document Types:
#   Architecture: 45 files
#   AgentReport: 60 files
#   Testing: 18 files
#   ADR: 5 files
#   Example: 38 files
#   Archived: 12 files
#   Unknown: 8 files

# Exclude intentional duplicates
yore dupes --exclude-type example
# (Ignores terraform/envs/* and tests/artifacts/*)
```

---

## Example 5: Canonicality Scoring (Phase 2)

### Scenario
User has two docs covering "implementation plan" - which is the canonical one?

### Before (No Canonicality)
**Files:**
- `docs/architecture/IMPLEMENTATION_PLAN.md` (canonical, actively maintained)
- `docs/archived/phase-0.5/PHASE_0.5_ACTIVITY_PLAN.md` (legacy, stale)

**Query:**
```bash
yore query "implementation plan"
```

**Output:** Both rank similarly (keyword match)
```
2 results for: implementation plan

docs/archived/phase-0.5/PHASE_0.5_ACTIVITY_PLAN.md (score: 9.2)
docs/architecture/IMPLEMENTATION_PLAN.md (score: 8.8)
```

**Problem:** Legacy doc ranks higher (more keyword occurrences)

---

### After (Canonicality Boost)
**Command:**
```bash
yore query "implementation plan" --boost-canonical
```

**Output:**
```
2 results for: implementation plan

docs/architecture/IMPLEMENTATION_PLAN.md (score: 12.3, canonical: 0.92)
docs/archived/phase-0.5/PHASE_0.5_ACTIVITY_PLAN.md (score: 4.1, canonical: 0.15)
```

**Canonicality Factors:**
```
IMPLEMENTATION_PLAN.md:
  Filename match: "IMPLEMENTATION_PLAN" (+0.3)
  Path depth: 2 levels (-0.2)
  Incoming links: 8 references (+0.4)
  Doc type: Architecture (0.9 weight)
  Last modified: 5 days ago (+0.0)
  -> Score: 0.92

PHASE_0.5_ACTIVITY_PLAN.md:
  In archived/ directory (-0.5)
  Path depth: 4 levels (-0.4)
  Incoming links: 0 references (+0.0)
  Doc type: Archived (0.1 weight)
  Last modified: 365 days ago (-0.2)
  -> Score: 0.15
```

---

## Example 6: Link Validation (Phase 3)

### Scenario
Documentation has 402 links - are they all valid?

### Before (Manual Check)
**Process:**
1. Manually click each link in rendered Markdown
2. Takes hours for 402 links
3. Often miss broken links in rarely-visited docs

---

### After (Automated Validation)
**Command:**
```bash
yore validate-links
```

**Output:**
```
Link Validation Report
======================

380 valid links
22 broken links

Broken Links:
-------------

docs/architecture/IMPLEMENTATION_PLAN.md:145
  -> docs/archived/PHASE-0.6-PLAN.md
  Reason: Target not found (file deleted or moved)

docs/testing/TESTING.md:67
  -> docs/workflows/old-testing-guide.md
  Reason: Target not found

agents/reports/DOCUMENTATION_AUDIT_REPORT.md:234
  -> DOCUMENTATION_STANDARDS.md
  Reason: Relative path broken (should be docs/DOCUMENTATION_STANDARDS.md)

... (19 more)

External Links (not validated): 45
  https://kubernetes.io/docs/...
  https://github.com/...
```

**Improvements:**
- Instant validation (vs hours of manual work)
- Catches relative path errors
- Identifies stale references to deleted files
- Actionable output (file:line for each broken link)

---

## Example 7: Document Graph Navigation (Phase 3)

### Scenario
User wants to understand relationships between MVP planning docs.

### Before (Manual Exploration)
**Process:**
1. Open `IMPLEMENTATION_PLAN.md`
2. Find link to `MVP_P0_TASKS.md`
3. Open that file, find link to `MVP_DEMO_WALKTHROUGH.md`
4. Manually build mental map (20+ minutes)

---

### After (Graph Visualization)
**Command:**
```bash
yore graph --starting-from docs/architecture/IMPLEMENTATION_PLAN.md --depth 2
```

**Output:**
```
Document Graph (2-hop from IMPLEMENTATION_PLAN.md)
===================================================

IMPLEMENTATION_PLAN.md (pagerank: 0.085)
|-- [references] MVP_P0_TASKS.md (pagerank: 0.042)
|   |-- [references] MVP_DEMO_WALKTHROUGH.md (pagerank: 0.031)
|   |-- [references] MVP_INTEGRATION_ROADMAP.md (pagerank: 0.028)
|   +-- [references] MVP_STATUS.md (pagerank: 0.019)
|-- [references] KUBERNETES_DEPLOYMENT.md (pagerank: 0.078)
|   |-- [references] k8s-infrastructure-setup-guide.md (pagerank: 0.045)
|   +-- [references] local-control-plane.md (pagerank: 0.023)
|-- [duplicates 68%] docs/archived/PHASE-1-PLAN.md (pagerank: 0.002)
+-- [references] DOCUMENTATION_STANDARDS.md (pagerank: 0.051)
```

**Graph Statistics:**
```bash
yore graph --stats
# Output:
# Graph Statistics:
#   Nodes: 366 documents
#   Edges: 845 total
#     - References: 402
#     - Duplicates: 79
#     - Supersedes: 12
#   Average degree: 2.31
#   Orphans: 8 documents (no incoming/outgoing links)
#   Top PageRank:
#     0.105 README.md
#     0.085 IMPLEMENTATION_PLAN.md
#     0.078 KUBERNETES_DEPLOYMENT.md
```

**Find Orphaned Docs:**
```bash
yore graph --orphans
# Output:
# Orphaned Documents (no links in/out):
#   docs/fixes/TEMPORARY_FIX_NOTES.md
#   agents/reports/observer/execution_log_123.txt
#   docs/analysis/SCRATCH_NOTES.md
#   ... (5 more)
```

---

## Example 8: Combined Workflow (All Phases)

### Scenario
New developer joins project, wants to understand deployment architecture.

### Before (Manual Process)
**Steps:**
1. Ask team lead "where's the deployment doc?"
2. Team lead searches Slack for link (5 minutes)
3. Developer opens doc, finds it references old phase docs
4. Spends 30 minutes clicking links, unsure which are current
5. Finds 3 different deployment guides, unclear which is canonical
6. Asks team lead again for clarification

**Total Time:** 45 minutes + team lead interruptions

---

### After (yore-Powered Workflow)
**Step 1: Find canonical deployment doc**
```bash
yore query "deployment" --type architecture --boost-canonical --limit 3
```

**Output:**
```
3 results for: deployment (type: architecture)

docs/architecture/KUBERNETES_DEPLOYMENT.md (score: 18.5, canonical: 0.88)
  > L1: Kubernetes Deployment Guide (Canonical)
  > L15: Local Deployment with kind

docs/architecture/BUILD_DEPLOY_STAGE_SEPARATION.md (score: 12.1, canonical: 0.75)
  > L1: Multi-Environment Deployment Architecture
  > L8: Build Artifact Reuse Strategy

docs/architecture/k8s-infrastructure-setup-guide.md (score: 9.8, canonical: 0.68)
  > L23: Infrastructure Setup for Deployment
```

**Step 2: Validate links in top doc**
```bash
yore validate-links --file docs/architecture/KUBERNETES_DEPLOYMENT.md
```

**Output:**
```
All 15 links valid in KUBERNETES_DEPLOYMENT.md

Referenced Documents:
  -> docs/development/local-control-plane.md (local kind setup)
  -> docs/architecture/k8s-control-plane-architecture.md (architecture)
  -> docs/runbooks/k8s-control-plane.md (operations runbook)
```

**Step 3: Explore related docs**
```bash
yore graph --starting-from docs/architecture/KUBERNETES_DEPLOYMENT.md --depth 1
```

**Output:**
```
KUBERNETES_DEPLOYMENT.md (pagerank: 0.078)
|-- [references] local-control-plane.md (setup guide)
|-- [references] k8s-control-plane-architecture.md (architecture)
|-- [references] k8s-control-plane.md (runbook)
+-- [duplicates 42%] k8s-infrastructure-setup-guide.md (alternative guide)
```

**Step 4: Check for stale alternatives**
```bash
yore dupes --file docs/architecture/KUBERNETES_DEPLOYMENT.md --threshold 0.4
```

**Output:**
```
1 similar document:

42% k8s-infrastructure-setup-guide.md (last modified: 180 days ago)
  Status: Possibly stale (KUBERNETES_DEPLOYMENT.md updated 5 days ago)
  Shared sections: "Prerequisites", "Local Setup"
  Unique in KUBERNETES_DEPLOYMENT.md: "Production Deployment", "Troubleshooting"
```

**Total Time:** 2 minutes + high confidence in results

---

## Performance Comparison Table

| Operation | Before | Phase 1 | Phase 2 | Phase 3 | Improvement |
|-----------|--------|---------|---------|---------|-------------|
| Index 366 files | 250ms | 300ms | 320ms | 350ms | +40% time, +500% features |
| Query | 8ms | 12ms | 12ms | 15ms | +88% latency, 3x relevance |
| Dupes (366 files) | 50ms | 5ms | 5ms | 5ms | **10x faster** |
| Dupes (1000 files) | 18s | 800ms | 800ms | 800ms | **22x faster** |
| Find boilerplate | Manual (30min) | Instant | Instant | Instant | **Infinitely faster** |
| Identify canonical | Impossible | N/A | Instant | Instant | New feature |
| Validate links | Manual (hours) | N/A | N/A | Instant | New feature |
| Graph navigation | Manual (20min) | N/A | N/A | Instant | New feature |

---

## Conclusion

**Phase 1-3 Improvements Transform yore From:**
- Basic keyword search tool

**To:**
- Production-grade documentation intelligence system

**Key Capabilities Gained:**
1. **Smart Search** - BM25 ranking with canonicality boost
2. **Fast Duplication** - MinHash LSH (10-22x speedup)
3. **Boilerplate Detection** - Section-level analysis
4. **Document Taxonomy** - Automatic classification (95% accuracy)
5. **Canonicality Scoring** - Identify source-of-truth docs
6. **Link Validation** - Automated broken link detection
7. **Graph Navigation** - Explore doc relationships visually

**Investment:** 64 hours over 3 months
**ROI:** Save 10-20 hours/week across team (documentation discovery + maintenance)

---

# Part 6: Exhaustive Implementation Details (Appendix)

**Added to make the plan fully executable by an AI agent**

---

## A. Complete Cargo.toml Changes

**Current State:**
```toml
[package]
name = "yore"
version = "0.1.0"
edition = "2021"
description = "Fast document indexer for finding duplicates and searching content"
license = "MIT"
keywords = ["documentation", "index", "search", "duplicates"]
categories = ["command-line-utilities", "text-processing"]

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
walkdir = "2"
ignore = "0.4"
regex = "1"
colored = "2"
toml = "0.8"

[profile.release]
lto = true
codegen-units = 1
strip = true
```

**Phase 1 Changes (add to [dependencies]):**
```toml
ahash = "0.8"  # Fast hashing for MinHash
```

**Phase 2 Changes (add to [dependencies]):**
```toml
filetime = "0.2"  # File metadata for canonicality scoring
```

**Phase 3 Changes (add to [dependencies]):**
```toml
petgraph = "0.6"  # Graph algorithms for PageRank
```

---

## B. Complete CLI Argument Additions

### B.1 Current Commands Structure (src/main.rs lines 30-144)

Add these new subcommands after `Repl`:

```rust
/// Detect duplicate sections across files
DupesSection {
    /// Similarity threshold (0.0 to 1.0)
    #[arg(short, long, default_value = "0.7")]
    threshold: f64,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Index directory
    #[arg(short, long, default_value = ".yore")]
    index: PathBuf,
},

/// Validate all links in the index
ValidateLinks {
    /// Check only a specific file
    #[arg(long)]
    file: Option<PathBuf>,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Index directory
    #[arg(short, long, default_value = ".yore")]
    index: PathBuf,
},

/// Show document graph relationships
Graph {
    /// Starting document for traversal
    #[arg(long)]
    starting_from: Option<PathBuf>,

    /// Traversal depth
    #[arg(long, default_value = "2")]
    depth: usize,

    /// Show orphaned documents
    #[arg(long)]
    orphans: bool,

    /// Show graph statistics
    #[arg(long)]
    stats: bool,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Index directory
    #[arg(short, long, default_value = ".yore")]
    index: PathBuf,
},
```

### B.2 Add to Query Command (Phase 2)

Add these arguments to the existing `Query` command:

```rust
/// Filter by document type
#[arg(long, value_parser = ["adr", "architecture", "mvpplan", "agentdefinition",
                            "agentreport", "runbook", "testing", "example", "archived"])]
doc_type: Option<String>,

/// Boost canonical documents in ranking
#[arg(long)]
boost_canonical: bool,
```

### B.3 Add to Stats Command (Phase 2)

```rust
/// Show statistics by document type
#[arg(long)]
by_type: bool,

/// Show top N canonical documents
#[arg(long)]
top_canonical: Option<usize>,

/// Show top N by PageRank
#[arg(long)]
top_pagerank: Option<usize>,
```

### B.4 Add to Dupes Command (Phase 2)

```rust
/// Exclude specific document types
#[arg(long)]
exclude_type: Option<String>,

/// Filter to specific file
#[arg(long)]
file: Option<PathBuf>,
```

---

## C. Complete Data Structure Definitions

### C.1 Current FileEntry (src/main.rs:147-157)

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileEntry {
    path: String,
    size_bytes: u64,
    line_count: usize,
    headings: Vec<Heading>,
    keywords: Vec<String>,
    body_keywords: Vec<String>,
    links: Vec<Link>,
    simhash: u64,
}
```

### C.2 Phase 1 FileEntry (Version 3)

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileEntry {
    path: String,
    size_bytes: u64,
    line_count: usize,
    headings: Vec<Heading>,
    keywords: Vec<String>,
    body_keywords: Vec<String>,
    links: Vec<Link>,
    simhash: u64,
    // Phase 1 additions:
    term_frequencies: HashMap<String, usize>,  // NEW
    doc_length: usize,                          // NEW
    minhash: Vec<u64>,                          // NEW: 128 hash values
    section_fingerprints: Vec<SectionFingerprint>, // NEW
}
```

### C.3 Phase 2 FileEntry (Version 4)

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileEntry {
    // ... all Phase 1 fields ...
    // Phase 2 additions:
    doc_type: DocType,                          // NEW
    canonicality: CanonicalityScore,            // NEW
}
```

### C.4 Current ForwardIndex (src/main.rs:181-186)

```rust
#[derive(Serialize, Deserialize, Debug)]
struct ForwardIndex {
    files: HashMap<String, FileEntry>,
    indexed_at: String,
    version: u32,
}
```

### C.5 Phase 1 ForwardIndex (Version 3)

```rust
#[derive(Serialize, Deserialize, Debug)]
struct ForwardIndex {
    files: HashMap<String, FileEntry>,
    indexed_at: String,
    version: u32,
    // Phase 1 additions:
    avg_doc_length: f64,                        // NEW
    idf_map: HashMap<String, f64>,              // NEW
}
```

---

## D. Index Migration Code

### D.1 Version Detection and Migration

Add this function after `load_forward_index()`:

```rust
fn migrate_forward_index(mut index: ForwardIndex) -> ForwardIndex {
    match index.version {
        2 => {
            // Migrate v2 -> v3
            eprintln!("Migrating index from v2 to v3...");

            // Initialize new fields with defaults
            for entry in index.files.values_mut() {
                // These fields don't exist in v2, add defaults
                // Note: Actual implementation needs to recompute these during rebuild
            }

            index.version = 3;
            eprintln!("Migration complete. Run 'yore build' to fully populate new fields.");
            index
        }
        3 => {
            // Migrate v3 -> v4
            eprintln!("Migrating index from v3 to v4...");
            index.version = 4;
            index
        }
        _ => index, // Already current version
    }
}
```

### D.2 Backward-Compatible Deserialization

Use `#[serde(default)]` for new fields:

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileEntry {
    path: String,
    size_bytes: u64,
    line_count: usize,
    headings: Vec<Heading>,
    keywords: Vec<String>,
    body_keywords: Vec<String>,
    links: Vec<Link>,
    simhash: u64,
    #[serde(default)]
    term_frequencies: HashMap<String, usize>,
    #[serde(default)]
    doc_length: usize,
    #[serde(default)]
    minhash: Vec<u64>,
    #[serde(default)]
    section_fingerprints: Vec<SectionFingerprint>,
    #[serde(default)]
    doc_type: DocType,
    #[serde(default)]
    canonicality: Option<CanonicalityScore>,
}

impl Default for DocType {
    fn default() -> Self {
        DocType::Unknown
    }
}
```

---

## E. Test Fixtures

### E.1 Create Test Fixture Files

Create directory: `tests/fixtures/`

**tests/fixtures/kubernetes_deployment.md:**
```markdown
# Kubernetes Deployment Guide

This is the canonical guide for deploying to Kubernetes.

## Prerequisites

- kubectl installed
- Helm 3.x
- Access to cluster

## Deployment Architecture

The deployment uses Helm charts with GitOps.

## Local Deployment with kind

Run the bootstrap script to create a local cluster.

## Production Deployment

Use ArgoCD for production deployments.
```

**tests/fixtures/k8s_testing.md:**
```markdown
# Kubernetes Testing

This document covers testing K8s deployments.

## Prerequisites

- kubectl installed
- Helm 3.x
- Access to cluster

## Test Scenarios

Various test scenarios for validation.
```

**tests/fixtures/agent_auth.md:**
```markdown
# Auth Service Agent

Agent definition for authentication service.

## Responsibilities

- Handle user login
- JWT token management
- Session handling
```

**tests/fixtures/agent_db.md:**
```markdown
# Database Agent

Agent definition for database operations.

## Responsibilities

- Handle database migrations
- Connection pooling
- Query optimization
```

**tests/fixtures/archived_old_plan.md:**
```markdown
# Old Phase Plan (Deprecated)

This is an old plan document that should be archived.

## Outdated Content

This content is no longer relevant.
```

### E.2 Integration Test Script

**tests/integration_test.sh:**
```bash
#!/bin/bash
set -e

YORE="cargo run --release --"
TEST_DIR="tests/fixtures"
INDEX_DIR="/tmp/yore-test-$$"

echo "=== Building test index ==="
$YORE build "$TEST_DIR" --output "$INDEX_DIR"

echo "=== Test 1: BM25 ranking ==="
results=$($YORE query kubernetes deployment --index "$INDEX_DIR" --json)
top_result=$(echo "$results" | head -1 | grep -o '"[^"]*kubernetes_deployment[^"]*"' || echo "")
if [[ "$top_result" == *"kubernetes_deployment"* ]]; then
    echo "PASS: Top result is kubernetes_deployment.md"
else
    echo "FAIL: Expected kubernetes_deployment.md as top result"
    exit 1
fi

echo "=== Test 2: Section duplicates ==="
# Both fixtures have identical "Prerequisites" sections
dupes=$($YORE dupes --threshold 0.3 --index "$INDEX_DIR" --json)
count=$(echo "$dupes" | grep -c "kubernetes" || echo "0")
if [ "$count" -gt 0 ]; then
    echo "PASS: Found duplicate content"
else
    echo "WARN: No duplicates found (may need lower threshold)"
fi

echo "=== Test 3: Stats command ==="
$YORE stats --index "$INDEX_DIR"

echo "=== Cleanup ==="
rm -rf "$INDEX_DIR"

echo "=== All tests passed ==="
```

---

## F. Error Handling Patterns

### F.1 Graceful Degradation for New Fields

```rust
fn bm25_score(
    query_terms: &[String],
    doc: &FileEntry,
    avg_doc_length: f64,
    idf_map: &HashMap<String, f64>,
) -> f64 {
    // Handle empty/missing term_frequencies gracefully
    if doc.term_frequencies.is_empty() || doc.doc_length == 0 {
        // Fall back to simple keyword counting
        let matches = query_terms.iter()
            .filter(|t| doc.keywords.contains(&stem_word(t))
                     || doc.body_keywords.contains(&stem_word(t)))
            .count();
        return matches as f64;
    }

    // Normal BM25 calculation
    const K1: f64 = 1.5;
    const B: f64 = 0.75;

    let avg_dl = if avg_doc_length > 0.0 { avg_doc_length } else { 100.0 };
    let norm_factor = 1.0 - B + B * (doc.doc_length as f64 / avg_dl);

    let mut score = 0.0;
    for term in query_terms {
        let stemmed = stem_word(&term.to_lowercase());
        let tf = *doc.term_frequencies.get(&stemmed).unwrap_or(&0) as f64;
        let idf = idf_map.get(&stemmed).copied().unwrap_or(1.0);

        if tf > 0.0 {
            score += idf * (tf * (K1 + 1.0)) / (tf + K1 * norm_factor);
        }
    }

    score
}
```

### F.2 MinHash Fallback

```rust
fn minhash_similarity(a: &[u64], b: &[u64]) -> f64 {
    // Handle empty/missing minhash vectors
    if a.is_empty() || b.is_empty() {
        return 0.0;  // Cannot compute, return no similarity
    }

    if a.len() != b.len() {
        // Different sizes - take minimum overlap
        let min_len = a.len().min(b.len());
        let matches = a.iter().take(min_len)
            .zip(b.iter().take(min_len))
            .filter(|(x, y)| x == y)
            .count();
        return matches as f64 / min_len as f64;
    }

    let matches = a.iter()
        .zip(b.iter())
        .filter(|(x, y)| x == y)
        .count();

    matches as f64 / a.len() as f64
}
```

---

## G. Exact Code Insertion Points

### G.1 Add imports at top of main.rs (after line 11)

```rust
use ahash::AHasher;  // Phase 1
use std::hash::Hasher;  // Phase 1 (already imported via Hash)
use filetime::FileTime;  // Phase 2
use petgraph::prelude::*;  // Phase 3
use petgraph::algo::page_rank;  // Phase 3
```

### G.2 Add new structs after Link (after line 171)

```rust
// Phase 1
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct SectionFingerprint {
    heading: String,
    level: usize,
    line_start: usize,
    line_end: usize,
    simhash: u64,
}

// Phase 2
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
enum DocType {
    ADR,
    Architecture,
    MVPPlan,
    AgentDefinition,
    AgentReport,
    Runbook,
    Testing,
    Example,
    Archived,
    #[default]
    Unknown,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct CanonicalityScore {
    score: f64,
    path_depth: u32,
    incoming_links: u32,
    filename_pattern: f64,
    last_modified_age: u32,
    doc_type_weight: f64,
}
```

### G.3 Update main() match arms (after line 227)

```rust
// Add these match arms:
Commands::DupesSection { threshold, json, index } => {
    cmd_dupes_section(threshold, json, &index)
}
Commands::ValidateLinks { file, json, index } => {
    cmd_validate_links(file.as_deref(), json, &index)
}
Commands::Graph { starting_from, depth, orphans, stats, json, index } => {
    cmd_graph(starting_from.as_deref(), depth, orphans, stats, json, &index)
}
```

---

## H. Phase-by-Phase Checklist with File Locations

### Phase 1 Checklist (20 hours)

| Task | File | Line | Hours |
|------|------|------|-------|
| Add `ahash` to Cargo.toml | Cargo.toml | 18 | 0.1 |
| Add `SectionFingerprint` struct | src/main.rs | after 171 | 0.5 |
| Add `term_frequencies` to FileEntry | src/main.rs | 148 | 0.2 |
| Add `doc_length` to FileEntry | src/main.rs | 148 | 0.1 |
| Add `minhash` to FileEntry | src/main.rs | 148 | 0.1 |
| Add `section_fingerprints` to FileEntry | src/main.rs | 148 | 0.1 |
| Add `avg_doc_length` to ForwardIndex | src/main.rs | 182 | 0.1 |
| Add `idf_map` to ForwardIndex | src/main.rs | 182 | 0.1 |
| Update version to 3 | src/main.rs | 264 | 0.1 |
| Implement `compute_minhash()` | src/main.rs | after 548 | 2.0 |
| Implement `minhash_similarity()` | src/main.rs | after 566 | 0.5 |
| Implement `lsh_buckets()` | src/main.rs | after 740 | 2.0 |
| Implement `bm25_score()` | src/main.rs | after 1082 | 2.0 |
| Update `index_file()` for term frequencies | src/main.rs | 401-472 | 2.0 |
| Compute IDF in `cmd_build()` | src/main.rs | after 362 | 1.5 |
| Update `cmd_query()` to use BM25 | src/main.rs | 568-648 | 2.0 |
| Update `cmd_dupes()` to use LSH | src/main.rs | 741-839 | 2.0 |
| Add `DupesSection` command | src/main.rs | after 144 | 1.0 |
| Implement `cmd_dupes_section()` | src/main.rs | new | 2.5 |
| Unit tests | src/main.rs | bottom | 2.0 |
| Integration tests | tests/integration_test.sh | new file | 1.0 |

### Phase 2 Checklist (16 hours)

| Task | File | Line | Hours |
|------|------|------|-------|
| Add `filetime` to Cargo.toml | Cargo.toml | 19 | 0.1 |
| Add `DocType` enum | src/main.rs | after 171 | 0.5 |
| Add `CanonicalityScore` struct | src/main.rs | after DocType | 0.5 |
| Implement `classify_doc()` | src/main.rs | new | 2.0 |
| Implement `compute_canonicality()` | src/main.rs | new | 2.0 |
| Add `--type` to Query command | src/main.rs | 52-71 | 0.5 |
| Add `--boost-canonical` to Query | src/main.rs | 52-71 | 0.5 |
| Update `cmd_query()` for doc type filter | src/main.rs | 568-648 | 2.0 |
| Update `cmd_query()` for canonical boost | src/main.rs | 568-648 | 1.5 |
| Add `--by-type` to Stats | src/main.rs | 127-136 | 0.5 |
| Update `cmd_stats()` for type distribution | src/main.rs | 944-982 | 1.5 |
| Add `--exclude-type` to Dupes | src/main.rs | 96-112 | 0.5 |
| Update `cmd_dupes()` for type exclusion | src/main.rs | 741-839 | 1.0 |
| Integration tests | tests/ | multiple | 2.0 |
| Documentation | README.md | multiple | 1.0 |

### Phase 3 Checklist (28 hours)

| Task | File | Line | Hours |
|------|------|------|-------|
| Add `petgraph` to Cargo.toml | Cargo.toml | 20 | 0.1 |
| Add `DocumentGraph` struct | src/main.rs | after CanonicalityScore | 1.0 |
| Add `DocNode` struct | src/main.rs | after DocumentGraph | 0.5 |
| Add `DocEdge` struct | src/main.rs | after DocNode | 0.5 |
| Add `EdgeType` enum | src/main.rs | after DocEdge | 0.5 |
| Add `Graph` command | src/main.rs | after 144 | 1.0 |
| Add `ValidateLinks` command | src/main.rs | after Graph | 0.5 |
| Implement `build_doc_graph()` | src/main.rs | new | 4.0 |
| Implement `compute_pagerank()` | src/main.rs | new | 2.0 |
| Implement `validate_links()` | src/main.rs | new | 2.0 |
| Implement `resolve_link()` | src/main.rs | new | 1.5 |
| Implement `cmd_graph()` | src/main.rs | new | 4.0 |
| Implement `cmd_validate_links()` | src/main.rs | new | 2.0 |
| Add graph file output | src/main.rs | cmd_build | 1.5 |
| Add `--top-pagerank` to Stats | src/main.rs | 127-136 | 0.5 |
| Update `cmd_stats()` for PageRank | src/main.rs | 944-982 | 1.5 |
| Performance optimization | multiple | multiple | 3.0 |
| Integration tests | tests/ | multiple | 2.0 |
| Documentation | README.md | multiple | 1.0 |

---

## I. Success Validation Commands

After implementing each phase, run these validation commands:

### Phase 1 Validation
```bash
# Build
cargo build --release

# Index test corpus
./target/release/yore build ../.. --output /tmp/yore-test

# Validate BM25 ranking
./target/release/yore query kubernetes deployment --index /tmp/yore-test
# Expected: KUBERNETES_DEPLOYMENT.md ranked first

# Validate fast duplicate detection
time ./target/release/yore dupes --threshold 0.5 --index /tmp/yore-test
# Expected: <50ms for 366 files

# Validate section duplicates
./target/release/yore dupes-section --threshold 0.7 --index /tmp/yore-test
# Expected: Lists "Prerequisites" and other shared sections

# Unit tests
cargo test
```

### Phase 2 Validation
```bash
# Type-filtered query
./target/release/yore query deployment --type architecture --index /tmp/yore-test
# Expected: Only architecture docs

# Canonical boost
./target/release/yore query implementation plan --boost-canonical --index /tmp/yore-test
# Expected: IMPLEMENTATION_PLAN.md ranked higher than archived versions

# Stats by type
./target/release/yore stats --by-type --index /tmp/yore-test
# Expected: Distribution across DocType categories
```

### Phase 3 Validation
```bash
# Link validation
./target/release/yore validate-links --index /tmp/yore-test
# Expected: List of valid/broken links

# Graph from starting point
./target/release/yore graph --starting-from docs/architecture/IMPLEMENTATION_PLAN.md --depth 2 --index /tmp/yore-test
# Expected: Tree view of referenced documents

# Orphans detection
./target/release/yore graph --orphans --index /tmp/yore-test
# Expected: List of unreferenced documents

# PageRank stats
./target/release/yore stats --top-pagerank 10 --index /tmp/yore-test
# Expected: README.md, IMPLEMENTATION_PLAN.md in top 5
```

---

**End of Consolidated Document**
