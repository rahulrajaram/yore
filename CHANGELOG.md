# Changelog

## 0.7.0
- Add deterministic relation extraction at build time (`relations.json`).
- Emit three edge types: `links_to`, `section_links_to`, `adr_reference`.
- Persist stable, sorted, deduplicated edges for graph-aware retrieval.
- Display relation count in build summary output.
- Extract ADR references (`ADR-001`, `ADR 13`, etc.) during indexing.
- Add backward-compatible `load_relation_index()` for downstream commands.
- Add `yore paths` subcommand for BFS relation traversal with
  `--depth`, `--kind` filter, and `--json` output.
- Add `--use-relations` flag to `assemble` for graph-aware
  cross-reference expansion from persisted relation edges.
- Add ranked retrieval metrics to `yore eval`: precision@k, recall@k,
  MRR, and nDCG@k over initial BM25 retrieval ranking.
- Add `--k` flag to `eval` for configurable k values (default: 5,10).
- Add `relevant_docs` field to eval JSONL format for per-question
  relevance judgments (backward compatible).
- Fix `found`/`missing` in eval JSON output to match against the
  assembled digest instead of the query text.

## 0.6.0
- Add `query` field to each result object in `--json` output so callers
  can correlate results with the query that produced them.
- Add `query` field to the top-level `--explain --json` wrapper alongside
  `results` and `diagnostics`.

## 0.5.0
- Add bounded MCP retrieval flow with `yore mcp search-context` and
  `yore mcp fetch-context` for preview-first, explicit expansion.
- Add `yore mcp serve` to expose the bounded contract over MCP stdio
  for editor and agent clients.
- Harden MCP behavior with cwd-independent source resolution, read-only-safe
  handle storage fallback, and explicit truncation metadata.

## 0.4.0
- Add missing command coverage in the help and README command reference (`similar`, `diff`, `stats`, `repl`, `policy`) for full CLI parity.
- Add missing option documentation for `eval`, `fix-links`, `mv`, and `fix-references` to keep CLI guidance congruent with behavior.
- Fix command-line validation by removing `eval`'s conflicting `-q` shorthand with global `-q/--quiet`.
- Wire `check --stale-days` through to runtime stale checks and keep help output aligned.
- Improve duplicate-policy wording in `check` help and behavior documentation to avoid misleading user expectations.

## 0.3.1
- Add `--query`, `--phrase`, `--explain`, and `--no-stopwords` for better query control and diagnostics.
- Add query diagnostics (tokens, stems, missing terms, index stats) and JSON diagnostics payloads.
- Add `yore vocabulary` with `lines`, `json`, and `prompt` formats for deterministic term extraction from index coverage.
- Add vocabulary default stop-word filtering, optional `--stopwords`, and `--include-stemming` behavior.
- Add `--from-files` to `assemble` with `@list.txt` list expansion support.
- Add local commit hooks for squash-scope review and optional LLM-assisted analysis.
- Add a staged sensitive-content scan to pre-commit checks, plus hook install tooling.
- Add a GitHub workflow to post squash-scope analysis on PRs for team visibility.
- Align query parsing behavior with indexing tokenization and stemmer assumptions.

## 0.3.0
- Add `--json` flag to `build` and `eval` commands for structured output
- Add `--track-renames` to `build` for git rename history extraction
- Add `--use-git-history` to `fix-links` for rename-aware suggestions
- Add `[external]` config section for cross-repo link validation
- Add propose/apply pattern to `fix-links` for agent-friendly disambiguation
- Extend `.yore.toml` config with `[link-check]` and `[policy]` sections
- Add section-length and required-link policy rules for targeted doc enforcement
- Add `canonical-orphans` to report high-canonicality docs with no inbound links
- Update OUTPUT FORMATS help section with complete command list

## 0.2.0

- Add maintenance workflows documentation for graph and consolidation
- Add policy checks, link fixes, move and stale commands
- Expand policy, consolidation, and graph tooling

## 0.1.0

- Initial release with core indexing and search
- BM25 ranking with MinHash similarity detection
- Duplicate and near-duplicate detection
- Link validation and broken link checking
- Context assembly for LLMs
