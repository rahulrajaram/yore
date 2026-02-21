# Implementation Plan: Query UX, Diagnostics, and Assemble From Files

## Summary
Round out regression coverage and release notes for recent query behavior changes.

## Goals
- Multi-word queries behave predictably and match indexed tokens.
- Users get actionable feedback when queries return no results.
- `assemble` supports explicit file inputs in addition to natural-language queries.
- Documentation reflects real query behavior and supported flags.

## Non-goals
- Full semantic search or embeddings.
- Large-scale rearchitecture of indexing or retrieval.

## Current Focus
Workstream F: Tests and Regression Coverage

## Workstream F: Tests and Regression Coverage
TODOs
- [x] Unit tests for query parsing: punctuation, quotes, hyphens, stopwords, mixed case.
- [x] Unit tests for phrase parsing and required adjacency behavior.
- [x] Integration tests: `yore query` with multiple terms returns results where single-term does.
- [x] Integration tests: empty results emit diagnostics and non-zero exit in machine mode (if added).
- [x] Tests for `assemble --from-files` including invalid and unindexed paths.

## Workstream G: Backward Compatibility and Release
TODOs
- [ ] Ensure `yore query term1 term2` output remains stable unless diagnostics are requested.
- [ ] If index schema changes, bump version and add a clear rebuild message.
- [ ] Decide whether empty-results diagnostics are on by default or gated by a flag.
- [ ] Add release notes and migration guidance in `CHANGELOG.md`.

## Optional Enhancements (Nice-to-have)
TODOs
- [ ] Add boolean operators (`AND`, `OR`, `-term`) for power users.
- [ ] Expose field weighting (headings vs body) as a config option.
- [ ] Add query suggestions based on top keywords from the index.
- [ ] Add `assemble --from-glob` for file selection via patterns.

## Open Questions
- Do we want to support a minimal query language now (operators), or defer?
