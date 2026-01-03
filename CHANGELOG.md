# Changelog

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
