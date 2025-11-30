# yore - Fast Document Indexer and Retrieval Engine

**yore** is a deterministic, multi-stage retrieval engine for documentation that provides:
- BM25-based search ranking
- Duplicate detection (documents and sections)
- Cross-reference expansion
- Extractive refinement for signal density
- Context assembly for LLM consumption

## Installation

Install yore to `~/.local/bin`:

```bash
cd tools/yore
./install.sh
```

Verify installation:

```bash
yore --version
```

## Quick Start

### 1. Build an Index

```bash
yore build docs --output docs/.index --types md
```

### 2. Search

```bash
yore query kubernetes deployment --index docs/.index
```

### 3. Find Duplicates

```bash
# Duplicate sections across documents
yore dupes-sections --index docs/.index --threshold 0.7

# Duplicate documents
yore dupes --index docs/.index --threshold 0.35
```

### 4. Assemble Context for LLMs

```bash
yore assemble "How does authentication work?" --index docs/.index
```

### 5. Evaluate Retrieval Quality

```bash
yore eval --questions evaluation/questions.jsonl --index docs/.index
```

### 6. Check Links

```bash
# Find broken links and anchors
yore check-links --index docs/.index

# JSON output for agents
yore check-links --index docs/.index --json
```

### 7. Find Backlinks

```bash
# See what documents link to a specific file
yore backlinks docs/architecture/DEPLOYMENT-GUIDE.md --index docs/.index
```

### 8. Find Orphaned Files

```bash
# Find files with no inbound links
yore orphans --index docs/.index --exclude README
```

### 9. Show Canonicality Scores

```bash
# Score documents by authority/trustworthiness
yore canonicality --index docs/.index --threshold 0.7
```

## Commands

### `yore build`

Build forward and reverse indexes over a document corpus.

```bash
yore build <path> --output <index-dir> --types <extensions>
```

**Options:**
- `--output, -o` - Index directory (default: `.yore`)
- `--types, -t` - File extensions to index (default: `md,txt,rst`)
- `--exclude, -e` - Patterns to exclude (repeatable)

**Example:**
```bash
yore build docs --output docs/.index --types md,txt
```

### `yore query`

Search the index using BM25 ranking.

```bash
yore query <terms...> --index <index-dir>
```

**Options:**
- `--limit, -n` - Max results (default: 10)
- `--files-only, -l` - Show only file paths
- `--json` - Output as JSON

**Example:**
```bash
yore query kubernetes deployment --limit 5 --index docs/.index
```

### `yore dupes`

Find duplicate or highly similar documents.

```bash
yore dupes --index <index-dir>
```

**Options:**
- `--threshold, -t` - Similarity threshold (default: 0.35)
- `--group` - Group duplicates together
- `--json` - Output as JSON

**Scoring:** Combined metric using 40% Jaccard + 30% SimHash + 30% MinHash

**Example:**
```bash
yore dupes --threshold 0.4 --json --index docs/.index
```

### `yore dupes-sections`

Find duplicate sections across documents.

```bash
yore dupes-sections --index <index-dir>
```

**Options:**
- `--threshold, -t` - SimHash similarity threshold (default: 0.7)
- `--min-files, -n` - Minimum files sharing a section (default: 2)
- `--json` - Output as JSON

**Example:**
```bash
# Find sections appearing in 5+ files with 85%+ similarity
yore dupes-sections --threshold 0.85 --min-files 5 --json
```

### `yore assemble`

Assemble context digest for LLM consumption.

```bash
yore assemble <query> --index <index-dir>
```

**Pipeline:**
1. BM25 primary section selection
2. Cross-reference expansion (ADR + markdown links)
3. Extractive refinement (signal density)
4. Markdown digest generation

**Options:**
- `--max-tokens, -t` - Token budget (default: 8000)
- `--max-sections, -s` - Max sections to include (default: 20)
- `--depth, -d` - Cross-ref expansion depth (default: 1, max: 2)
- `--format, -f` - Output format (default: markdown)

**Example:**
```bash
yore assemble "How does the authentication system work?" \
  --max-tokens 6000 \
  --depth 1 \
  --index docs/.index > context.md
```

### `yore eval`

Evaluate retrieval pipeline against test questions.

```bash
yore eval --questions <jsonl-file> --index <index-dir>
```

**Questions format (JSONL):**
```json
{"id": 1, "q": "How does auth work?", "expect": ["session", "token"], "min_hits": 2}
```

**Example:**
```bash
yore eval --questions evaluation/questions.jsonl --index docs/.index
```

### `yore check-links`

Validate all markdown links and anchors in the documentation.

```bash
yore check-links --index <index-dir>
```

**Options:**
- `--json` - Output as JSON
- `--root, -r` - Root directory for resolving relative paths

**Output:** Reports broken links, missing files, and invalid anchors with source locations.

**Example:**
```bash
yore check-links --index docs/.index --json
```

### `yore backlinks`

Find all files that link to a specific file.

```bash
yore backlinks <file> --index <index-dir>
```

**Options:**
- `--json` - Output as JSON

**Use case:** Determine safe deletion - see what needs updating before removing a file.

**Example:**
```bash
yore backlinks docs/architecture/DEPLOYMENT-GUIDE.md --index docs/.index
```

### `yore orphans`

Find files with no inbound links (potential cleanup candidates).

```bash
yore orphans --index <index-dir>
```

**Options:**
- `--json` - Output as JSON
- `--exclude, -e` - Exclude files matching pattern (repeatable)

**Output:** Lists orphaned files with size and line count.

**Example:**
```bash
# Find orphans excluding README and INDEX files
yore orphans --index docs/.index --exclude README --exclude INDEX
```

### `yore canonicality`

Score documents by authority and trustworthiness based on location and naming.

```bash
yore canonicality --index <index-dir>
```

**Options:**
- `--json` - Output as JSON
- `--threshold, -t` - Minimum score threshold (0.0 to 1.0, default: 0.0)

**Scoring factors:**
- Architecture/ADR documents (+0.2)
- Index documents (+0.15)
- README/Guide/Runbook (+0.1)
- Scratch/Archive/Old (-0.3)
- Deprecated/Backup (-0.25)

**Use case:** Resolve conflicting information by trusting higher-canonicality sources.

**Example:**
```bash
# Show only high-authority documents
yore canonicality --index docs/.index --threshold 0.7
```

## Use Cases

### Documentation Cleanup

Find and consolidate duplicate sections:

```bash
# Find duplicate sections
./scripts/docs/find-duplicates.sh | jq .
```

### LLM Context Assembly

Generate precise context for LLM queries:

```bash
yore assemble "How do I deploy a new service?" > context.md
```

### Agent Integration

Full integration with documentation-steward agent:

**Available commands:**
- `yore check-links` - Validate all documentation links and anchors
- `yore backlinks <file>` - Find inbound references before deletion
- `yore orphans` - Identify unreferenced documents for cleanup
- `yore canonicality` - Score document authority for conflict resolution
- `yore dupes-sections` - Find duplicate boilerplate sections
- `yore dupes` - Find highly similar documents

**Wrapper script:** `scripts/docs/find-duplicates.sh` provides automated duplication detection.

## Architecture

### Determinism

**Guaranteed deterministic:**
- Same query + same index → identical context
- No vector embeddings, no approximate search, no sampling

**Benefits:**
- Debuggable failures
- Regression detection
- Cacheable results
- Auditable decisions

### Performance

**Indexing:** 221 files in ~2 seconds
**Query:** BM25 search <10ms
**Eval:** 5 questions in 2-3 seconds

## License

MIT
