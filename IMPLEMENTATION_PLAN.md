# IMPLEMENTATION_PLAN

## Project: yore vocabulary feature (for faster-whisper prompting)

This plan is split into **small, YARLI-friendly tranches** so we can execute work in parallel where safe.

## Global objective
Add a `yore vocabulary` command that:
- derives vocabulary from index coverage (no full-file re-index),
- returns unstemmed, domain-relevant terms with stable ranking,
- defaults to one term per line,
- supports `--format json`,
- supports `--format prompt` (comma-separated for Whisper),
- supports `--limit N` (default 100),
- resolves `--index`,
- filters stop/common words (default dictionary stop set + optional custom set).

## Tranche model
- **Priority:** earlier tranches unblock later tranches.
- **Parallelism:** tasks without hard dependency can run concurrently in separate YARLI runs.
- **Execution guard:** keep each tranche focused to avoid token churn.

### Tranche T1 — Contract and dispatch
**Priority:** high  
**Parallelizable:** yes (with T2 once option names are agreed)  
**Status:** ✅ Implemented (2026-02-21 run completed for `YORE-CORE-01`; command wiring, argument surface, and dispatch stub are now present in `src/main.rs`; runtime behavior remains no-op placeholder by design until later tranches)  
**Goal:** Make the feature discoverable and runnable.

- Add `Commands::Vocabulary` to `src/main.rs` with:
  - `--index <path>` (default `.yore`)
  - `--limit <usize>` (default `100`)
  - `--format [lines|json|prompt]` (default `lines`)
  - `--json` alias for `--format json`
  - optional `--stopwords <path>` (custom stop-list file)
  - optional `--include-stemming` (if we decide to retain stem-only terms as fallback)
- Add dispatch branch in `run()` to call `cmd_vocabulary`.
- Add `cmd_vocabulary` signature and output/result structs (lightweight).
- Add help text + examples.

**Acceptance criteria**
- `yore help` shows `vocabulary` command and flags.
- Build continues to compile with existing features untouched.

Evidence:
- Added `Commands::Vocabulary` to `src/main.rs` with `--index`, `--limit`, `--format [lines|json|prompt]`, `--json`, `--stopwords`, and `--include-stemming`.
- Added `Commands::Vocabulary` dispatch branch in `run()` and a `cmd_vocabulary` stub that currently emits empty results in the requested format.
- Added `VocabularyResult` output struct for staged JSON path and format validation in `cmd_vocabulary` (`lines`, `json`, `prompt`).
- Verification for this tranche run:
  - `cargo check -q`
  - `cargo run --quiet -- vocabulary --help`
  - `cargo run --quiet -- vocabulary --format json --limit 3`

### Tranche T2 — Candidate extraction (reverse-index intake)
**Status:** ✅ complete
**Priority:** high  
**Parallelizable:** **only** with T1’s type definitions finalized  
**Goal:** Harvest candidate terms from reverse-index-backed content.

- Read existing reverse-index postings (`stem -> doc_ids`) and supporting index metadata.
- Keep stem keys stable and deterministic during extraction.
- Track term and document frequency maps with stable ordering by first-seen and frequency.

**Acceptance criteria**
- Terms missing from index gracefully degrade to empty list.
- No stem-only output leaks into the default CLI path.

**Status update**
- `YORE-CORE-02` completed in `src/main.rs` by loading `reverse_index.json` in `cmd_vocabulary`, aggregating candidate terms across postings, tracking doc/term frequencies, and producing deterministic ordering by frequency plus deterministic first-seen tuple.
- The command now returns extracted terms instead of an empty result set, and missing `reverse_index.json` is handled as an empty candidate set.
- Evidence command run: `cargo test --quiet`

### Tranche T2A — Stem-to-surface mapping
**Priority:** high
**Parallelizable:** no (depends on T2)
**Goal:** Resolve stems to representative surface forms.

- Map stemmed candidates back to source surface forms using reverse/forward evidence.
- Preserve deterministic casing behavior for equal candidates.
- Document first-seen precedence and tie-break semantics.

**Status:** ✅ complete

Status update:
- `YORE-CORE-02A` completed in `src/main.rs` by adding a deterministic stem-to-surface resolver inside `cmd_vocabulary`.
- Surface candidates are sourced from heading evidence in `ReverseEntry` first, then forward index term evidence as fallback; ties are resolved deterministically by source rank, path, line, token order, and lexical casing.
- When no surface match is available, output remains stem-only only when `--include-stemming` is enabled; otherwise that candidate is skipped.

Verification for this tranche run:
- `cargo run --quiet -- vocabulary --index .yore --format lines --limit 8`
- `cargo run --quiet -- vocabulary --index .yore --format json --limit 8`
- `cargo run --quiet -- vocabulary --index .yore --format prompt --include-stemming --limit 8`
  - observed on `2026-02-21`:
    - lines output: `yore`, `output`, `command`, `similar`, `json`, `indexer`, `build`, `files`
    - json output: total=`873`, terms=`["yore","output","command","similar","json","indexer","build","files"]`
    - prompt output: `yore, output, command, similar, json, indexer, build, files`

**Acceptance criteria**
- Surface output is stable across runs with identical index data.

### Tranche T2B — Ranking and truncation
**Priority:** high  
**Parallelizable:** no (depends on T2A)  
**Goal:** Enforce deterministic ranking and limit behavior.

- Apply stable ranking and tie-break rules after frequency scoring.
- Enforce `--limit` against deterministic ordered candidates.

**Status:** ✅ complete

Status update:
- `YORE-CORE-03B` completed in `src/main.rs` by adding an explicit first-heading tie-break into `cmd_vocabulary` ranking.
- Candidate order now resolves ties using first-seen file, line, heading, and lexical term after frequency scores.

**Acceptance criteria**
- Ranked output remains stable and deterministic.

Validation for this tranche run:
- `cargo test --quiet`

### Tranche T3 — Filtering and scoring
**Priority:** high  
**Depends on:** T1, T2  
**Goal:** Deliver only domain-relevant vocabulary.

- ✅ **Implemented in current tranche run (2026-02-21 run):**
  - Added default English/common stop-word filtering in `cmd_vocabulary`.
  - Added optional `--stopwords` file merging with default stop words.
  - Added hygiene filtering for term length, numeric-heavy tokens, and punctuation-only tokens.
  - Enforced these filters before `--limit` truncation while preserving deterministic ranking.
  - Fixed `cmd_vocabulary` output construction to preserve custom stop-word path metadata correctly (avoid accidental `HashSet::map` call).

Validation evidence:
- `cargo run --quiet -- vocabulary --index .yore --format lines --limit 12`
- `cargo run --quiet -- vocabulary --index .yore --format json --limit 12`
- `cargo run --quiet -- vocabulary --index .yore --stopwords /tmp/yore-vocabulary-stopwords.txt --limit 12`

**Acceptance criteria**
- Common-language terms are filtered by default.
- Output ordering is stable and deterministic.

### Tranche T4A — JSON output contract
**Priority:** medium  
**Depends on:** T2B, T3  
**Goal:** Finalize structured JSON output schema.

- Define a stable JSON payload including term, score, and count fields.
- Ensure schema remains constant for unchanged index state.

**Status:** ✅ complete

Implemented in this tranche:
- Changed `VocabularyResult` JSON payload to emit term objects with `term`, `score`, and `count` fields.
- Kept `lines` and `prompt` output renderers compatible while sourcing rendered values from the same structured vocabulary model.

Validation evidence:
- `cargo run --quiet -- vocabulary --index .yore --format json --limit 12`

**Acceptance criteria**
- `--format json` emits schema-compatible, deterministic payloads.

### Tranche T4 — Renderers and formatting
**Priority:** medium  
**Depends on:** T2B, T3  
**Goal:** Produce required output modes.

**Status:** ✅ complete

- Implement formatter path:
  - `lines` (default): one term per line.
  - `json`: list of terms or objects (`{ term, score, count }`) as decided.
  - `prompt`: comma-separated string (no trailing separator).
- Validate `--json` alias behavior and format validation.

Status update:
- `YORE-CORE-04` completed in `src/main.rs` by adding dedicated `lines` and `prompt` renderers, plus prompt-term normalization for whitespace/control-character cleanup.
- Verification for this tranche run:
  - `cargo run --quiet -- vocabulary --index .yore --format lines --limit 12`
  - `cargo run --quiet -- vocabulary --index .yore --format prompt --limit 12`
  - `cargo run --quiet -- vocabulary --index .yore --format json --limit 12`

**Acceptance criteria**
- `--format json` emits valid JSON.
- `--format prompt` is whitespace-normalized and LLM-safe.

### Tranche T5A — CLI integration tests
**Priority:** medium  
**Depends on:** T1, T2, T2A, T2B, T3, T4  
**Status:** ✅ complete  

Status update:
- Implemented in this tranche in `tests/vocabulary_cli_integration.rs`:
  - Added command-level wiring checks for `yore vocabulary --help`.
  - Added end-to-end format coverage for `lines`, `json`, and `prompt` output using the same fixture index.
  - Added `--limit` and default filtering assertions.
  - Added custom stop-word flow and `--json` alias coverage.
  
Validation command for this tranche:
- `cargo test --test vocabulary_cli_integration`

**Goal:** Validate command-level behavior for all formats.

- Add integration coverage for:
- `yore vocabulary` wiring and flags,
  - JSON prompt and line output consistency,
  - `--limit` and custom stopword flows.

**Acceptance criteria**
- `cargo test --test` level checks for CLI command behavior pass.

### Tranche T5 — Unit tests
**Priority:** medium  
**Depends on:** T1, T2, T2A, T2B, T3, T4  
**Goal:** Prevent regressions in core data transforms.

- Add/extend unit tests:
  - stem-to-surface mapping,
  - stopword/common-word filtering,
  - output formatting for each mode,
  - `--limit` bounds.
- Add/refresh integration tests:
  - command wiring for `yore vocabulary ...`
  - JSON schema shape check.

**Acceptance criteria**
- `cargo test` passes for new and existing vocabulary-related paths.
- Command returns expected structures on fixture corpus.

**Status:** ✅ complete (2026-02-21 run)

Status update:
- Implemented in this tranche:
  - Added dedicated vocabulary helpers in `src/main.rs`:
    - `apply_vocabulary_limit` (preserves total candidate count)
    - `render_vocabulary_lines`
    - `resolve_vocabulary_surface`
  - Added unit coverage for:
    - `apply_vocabulary_limit` bounds and total-count behavior
    - `render_vocabulary_lines` output
    - `render_vocabulary_prompt` normalization
    - `VocabularyResult` JSON schema shape
    - deterministic `resolve_vocabulary_surface` heading-vs-forward and fallback behavior
- Refactored vocabulary internals so helpers are separately testable without changing CLI output semantics.
- Verification command run:
  - `cargo test`

### Tranche T6 — Docs + release notes
**Priority:** low  
**Depends on:** T1..T5, T5A  
**Goal:** Publish feature with clear user guidance.

**Status:** ✅ complete

- Update `README.md` command reference and examples.
- Add changelog entry for the release.
- Update any hook/task docs that reference command coverage.

Status update:
- `YORE-DOCS-01` completed by adding query diagnostics usage in `README.md` (text + JSON payload explanation), adding a full `yore vocabulary` command reference and examples, and updating `CHANGELOG.md`.
- `yore query --explain` documentation now references JSON diagnostics fields (`tokens`, `stems`, `missing_terms`, `idf`, `bm25`, `index_path`, `doc_count`).
- `yore vocabulary` is now documented with format modes (`lines`, `json`, `prompt`), stop-word controls, and example prompts.

**Acceptance criteria**
- New command discoverable in docs and examples.
- Release note explicitly states what changed.

Validation command for this tranche:
- `rg -q "vocabulary|query diagnostics" README.md CHANGELOG.md`

### Tranche YORE-RELEASE-01 — Release workflow and version semantics
**Priority:** medium  
**Depends on:** T6, YARLI onboarding  
**Goal:** Align release workflow and version semantics.

**Status:** ✅ complete

Implemented in this tranche:
- Changed publish workflow trigger to explicit semver-tag patterns (`v*.*.*`) instead of wildcard tags.
- Added an early secret/token guard for `CARGO_REGISTRY_TOKEN` using repository secret context.
- Tightened tag/version validation to require explicit `v`-prefixed SemVer-like tags and exact match to `Cargo.toml` version.

Validation command for this tranche:
- `bash -n .github/workflows/publish-crates.yml`

### Tranche Hook-01 — Sensitive-content scan hardening
**Priority:** high
**Depends on:** baseline hook wiring
**Status:** ✅ complete (2026-02-21 run)

- Expanded pre-commit sensitive scan patterns for additional high-risk secrets:
  - AWS session/access keys (`ASIA...`, `A3T...`)
  - GitHub token variants (`ghr_...`)
  - Slack, OpenAI, Google, Stripe, and Hugging Face API-style tokens
  - Encrypted/private key variant coverage (`BEGIN ENCRYPTED PRIVATE KEY`)
- Kept existing bypass and failure behavior unchanged; only detection coverage changed.

Validation for this tranche:
- `bash scripts/abs-path-leak-scan.sh`

### Tranche Hook-01A — Absolute-path disclosure scan hardening
**Priority:** high
**Depends on:** Hook-01
**Status:** ✅ complete (2026-02-21 run)

- Expanded absolute-path detection in `scripts/abs-path-leak-scan.sh` to include:
  - explicit `file://` URI paths,
  - UNC path forms,
  - expanded POSIX absolute-location coverage for `/tmp`, `/opt`, `/var`, `/etc`, `/mnt`, and `/Users`/`/home` home-style paths.
- Added clearer scanner guidance with optional debug visibility via `YORE_ABS_PATH_MASK=0` while still masking by default.
- Preserved existing skip behavior (`YORE_SKIP_ABSOLUTE_PATH_CHECK=1`) and kept scan strictness non-regressive for home/repo path cases.
- Added masking coverage in diagnostics for `/tmp`, `/opt`, `/var`, `/etc`, and `/mnt`.

Validation for this tranche:
- `bash scripts/abs-path-leak-scan.sh`

### Tranche Hook-02 — Finalize squash-scope reporting UX
**Priority:** medium
**Depends on:** Hook-01A
**Status:** ✅ complete (2026-02-21 run)

- Added richer heuristic candidate rendering in `scripts/squash-scope-review.sh` by including commit subjects for each flagged pair.
- Added explicit rewrite guidance and next-action recommendation (interactive rebase command and `squash`/`fixup` direction) when candidates are found.
- Preserved existing `SQUASH_SCOPE_NEEDS_REWRITE` machine-readable signal and heuristic disclaimer.

Validation for this tranche:
- `bash scripts/squash-scope-review.sh`

### Tranche Hook-02A — Optional AI-backed squash recommendations
**Priority:** medium  
**Depends on:** Hook-02  
**Status:** ✅ complete (2026-02-21 run)

- Added AI-assisted mode to `scripts/squash-scope-review.sh` gated by `YORE_SQUASH_AI_ENABLED=1`.
- Added required helper configuration via `YORE_AI_HELPER`, optional `YORE_AI_ARGS`, and `YORE_AI_INPUT_MODE` support.
- Captured structured heuristic candidate payload (`score`, `overlap`, `reason`) and passed it to the AI helper for actionable recommendations.
- Added graceful degradation:
  - helper unavailable or command missing -> `AI review skipped` message
  - helper failures or empty output -> fallback to heuristic report only

Validation for this tranche:
- `bash scripts/squash-scope-review.sh`

### Tranche YARLI-01 — Finalize YARLI execution onboarding map
**Priority:** medium  
**Status:** ✅ complete (2026-02-21 run)

- Confirmed `yarli-orchestration` is represented as its own onboarding tranche in the execution map.
- Finalized and validated onboarding metadata in `.yarl/tranches.toml`.

Validation for this tranche:
- `yarli plan validate`

## YARLI execution map
- **Wave 1:** T1 + T2 (parallel if safe within dependencies).
- **Wave 2:** T2A + T2B + T3 (depends on Wave 1).
- **Wave 3:** T4 + T4A + T5.
- **Wave 4:** T5A + T6 + final PR/docs sign-off.

## Onboarding: active YARLI tranches
These work packages are mirrored in `.yarli/tranches.toml` for machine execution:
- `vocabulary-feature` (`YORE-CORE-01`, `YORE-CORE-02*`, `YORE-CORE-03*`, `YORE-CORE-04*`, `YORE-CORE-05*`)
- `commit-hardening` (`YORE-HOOK-01`, `YORE-HOOK-01A`, `YORE-HOOK-02`, `YORE-HOOK-02A`)
- `yarli-orchestration` (YORE-YARLI-01)
- `release-and-docs` (YORE-DOCS-01..YORE-RELEASE-01)

Use `yarli plan validate` to verify syntax and `yarli plan tranche list` to inspect state.

## Exit criteria
- New command works: `yore vocabulary --index .yore --limit 200 --format prompt`.
- Existing CLI behavior remains stable.
- Change is ready for review, squashing review, and PR merge.

## Next tranche map draft (post-release hardening)
This is the next execution plan, split so each YARLI task stays narrow.

### Wave A: hook hardening cleanup
- **YORE-MAINT-01 — Cross-project hook onboarding**
  - Validate and document `.githooks` versus shared `/home/<user>/Documents/commithooks` behavior for mixed clone layouts.
  - Add clarifying examples for `YORE_SKIP_*` and `YORE_ABS_PATH_MASK` in a shared hook setup note.
- **YORE-MAINT-02 — Scanner coverage hardening**
  - Add/refresh regression fixtures for absolute-path detection and masking outputs.
  - Keep false positives low while still catching home/repo paths, file:// URIs, and mount-style locations.

### Wave B: docs and operational checks
- **YORE-MAINT-03 — Workflow behavior**
  - Verify publish workflow still requires explicit tags and tag/version matching before `cargo publish`.
- **YORE-MAINT-04 — Runbook update**
  - Capture the exact PR-to-publish flow (feature branch -> PR -> merge -> tag -> publish) in docs for operators.

### Wave C: release readiness
- **YORE-MAINT-05 — Reconcile commit scope**
  - Produce a concise commit-scope report for the above waves and confirm whether any work should be squashed before merge.

Planned tranches are tracked as draft work items and can be materialized into `.yarl/tranches.toml` in the next cycle when you want YARLI to execute them.
