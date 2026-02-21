# IMPLEMENTATION_PLAN

## Project: yore vocabulary feature (for faster-whisper prompting)

This plan is split into **granular YARLI tranches** to keep per-tranche context low and support parallel execution where possible.

## Global objective
Add `yore vocabulary` command that extracts domain-relevant, unstemmed terms from indexed content and returns ranked candidates in formats suitable for LLM prompting:
- `lines`
- `csv`
- `json`
- `prompt` (comma-separated for Whisper `initial_prompt`)

## Tranche model
- **Parallelism rule:** tranches with no hard dependency can run at the same time.
- **Priority:** earlier tranches unblock later ones.
- **Scope guard:** each tranche is intentionally bounded to avoid a single huge token run.

### Tranche T1 — API + command wiring (high priority)
**Goal:** Add CLI/API shell so the feature is discoverable and callable.

- Add `Vocabulary` variant to `Commands` with options:
  `--limit`, `--min-occurrences`, `--max-occurrences`, `--stopwords`, `--no-default-stopwords`, `--format`, `--index`, `--json`, `--quiet`.
- Add dispatch path in `run()` to call `cmd_vocabulary(...)` with resolved index path.
- Add `VocabularyResult`, `VocabularyEntry`, and command result shape structs near `cmd_stats` models.

**Acceptance criteria:**
- `yore help` shows new command and options.
- Existing `cargo check/clippy/tests` continue to pass (no behavior change yet to results).

**Dependencies:** none

### Tranche T2 — Vocabulary extraction core engine (parallelizable)
**Goal:** Implement extraction of candidate raw surface terms from indexed files.

- Implement token extraction over file contents (re-read indexed files from `ForwardIndex.files` paths).
- Preserve raw casing for candidates (no stemming).
- Candidate pattern coverage:
  - `PascalCase`
  - `ALL_CAPS`
  - mixed alpha+digits
  - hyphenated terms
  - dotted identifiers
- Build term frequency and document frequency maps with optional custom stopword exclusion and default-stopword fallback.
- Add casing heuristic utility + filtering helpers.

**Acceptance criteria:**
- Unit-level helper tests cover each token pattern.
- Candidate map excludes empty/very-short terms and handles missing files robustly.

**Dependencies:** T1

### Tranche T3 — Ranking + dedup + formatters (depends on T1/T2)
**Goal:** Score candidates, deduplicate, and render outputs.

- Implement YAKE-inspired score with:
  - tf
  - df moderation
  - casing bonus
  - common-word penalty
- Deduplicate case-insensitively, retaining canonical surface form (e.g., `PyQt6` over `pyqt6`).
- Implement formatters:
  - `lines`
  - `csv`
  - `json` (including score + occurrence fields)
  - `prompt`
- Support `--json` as alias for `--format json`.

**Acceptance criteria:**
- Sorting stable and deterministic.
- `--format prompt` returns a single comma-delimited string without trailing comma/quotes.

**Dependencies:** T1, T2

### Tranche T4 — Command integration test coverage (depends on T1/T3)
**Goal:** Verify CLI behavior and integration surfaces.

- Add integration tests in `tests/`:
  - `yore vocabulary --format lines`
  - `--format json` schema fields
  - `--format prompt` output style
  - stopword filtering and occurrence bounds

**Acceptance criteria:**
- New tests pass on clean temp fixture corpus.

**Dependencies:** T1, T3

### Tranche T5 — UX/docs + changelog updates (depends on T1..T4)
**Goal:** Document the feature and close operationally.

- Update README command surface section for `yore vocabulary` with examples.
- Update `CHANGELOG.md` for the new command and output modes.
- Optional: add a small sample of suggested whisper integration usage in docs.

**Acceptance criteria:**
- Changelog entry present and readable.
- No markdown/render issues.

**Dependencies:** T1, T2, T3, T4

## Suggested execution order with parallel runs
1. Start T1 and T2 in parallel only if desired (T2 assumes command interface names from T1; coordinate accordingly).
2. Run T3 after T2.
3. Run T4 after T3.
4. Run T5 once functional and tests are stable.

Each tranche should be assigned one YARLI run with output artifacts:
- test output file path
- commit hash
- known open risks
- any follow-up tasks

## Exit criteria
- All existing tests still pass.
- New command usable as:
  `yore vocabulary --index .yore --limit 200 --format prompt > vocabulary.txt`
