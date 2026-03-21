# IMPLEMENTATION_PLAN

## Project: Yore MCP for bounded documentation context

## Session status
- The repository is currently at `b970943` on `master`.
- No MCP implementation exists in the codebase yet.
- This session validated the target product shape from the `gptqueue` message sent by `amoebum-codex-shell`.
- Root planning docs have been updated to reflect the MCP effort as the next active track.

## Validated objective
Build Yore into a bounded documentation/context MCP for agent workflows, especially Amoebum, without turning it into a broad file-dumping tool.

Yore should:
- expose a narrow, deterministic retrieval surface;
- return compact previews before full expansion;
- enforce strict budget limits;
- deduplicate overlapping context;
- keep large artifacts off-transcript behind opaque handles;
- preserve selection-first behavior instead of defaulting to workspace-wide ingestion.

## Current baseline to leverage
Existing functionality already provides useful building blocks:
- deterministic indexing and profile support;
- ranked section retrieval through `query`;
- token-budgeted context assembly through `assemble`;
- explicit file selection through `assemble --from-files`;
- related structural tools such as `similar`, `backlinks`, `canonicality`, and graph export.

Important constraints from the current code:
- `assemble` currently emits markdown only;
- the public surface is CLI-first, not MCP-first;
- current examples still assume writing assembled context directly into prompt text;
- there is no opaque handle store, byte-budget contract, or explicit preview/fetch split yet.

## Priority tranches

### Tranche M1 - MCP contract and response schema
**Priority:** high  
**Status:** not started

Define the initial MCP-facing surface and data contracts.

Deliverables:
- Decide whether the first implementation lives as:
  - an in-process MCP server binary in this repo, or
  - a thin server wrapper over existing Yore library/CLI functionality.
- Define a minimal tool set, likely:
  - `search_context`
  - `preview_context`
  - `expand_context` or `fetch_context`
- Define stable response fields for:
  - result summaries,
  - source references,
  - truncation indicators,
  - budget usage,
  - opaque artifact handles.

Acceptance criteria:
- A short design note or code-level contract makes the search/preview versus expand/fetch split explicit.
- The contract forbids raw large-document dumps by default.

### Tranche M2 - Search and preview path
**Priority:** high  
**Status:** not started

Implement the bounded first-step retrieval flow.

Deliverables:
- Add a preview-oriented retrieval path that returns:
  - top matches,
  - concise snippets or section summaries,
  - source metadata,
  - budget and truncation metadata.
- Enforce hard defaults for top-k, token caps, and byte caps.
- Deduplicate overlapping hits before formatting the response.

Acceptance criteria:
- A caller can discover relevant docs without receiving a full assembled digest.
- Results remain deterministic for unchanged index state.

### Tranche M3 - Explicit expansion and opaque handles
**Priority:** high  
**Status:** not started

Add the second-step expansion path for callers that explicitly ask for more detail.

Deliverables:
- Introduce opaque handles for preview results or stored artifacts.
- Implement fetch/expand behavior keyed by handle instead of repeating full payloads inline.
- Keep large artifacts off-transcript and return compact summaries plus handles.

Acceptance criteria:
- Full-context expansion only happens on explicit follow-up.
- The transport contract makes it easy for clients to keep bulky material outside the chat transcript.

### Tranche M4 - Pressure signals and diff-aware selection
**Priority:** medium  
**Status:** not started

Make the interface safe for IDE and status-bar usage.

Deliverables:
- Add explicit pressure indicators such as:
  - token budget used,
  - byte budget used,
  - number of hits omitted,
  - truncation reason.
- Add a selection-first path that can operate on explicit files, lists, or changed files rather than defaulting to whole-workspace ingestion.
- If diff-aware support is added, keep it opt-in and bounded.

Acceptance criteria:
- A client can tell when Yore truncated or suppressed material.
- The default behavior stays narrow and predictable.

### Tranche M5 - Tests, docs, and Amoebum integration notes
**Priority:** medium  
**Status:** not started

Document and verify the MCP surface.

Deliverables:
- Add integration tests for preview/fetch budgeting and dedupe behavior.
- Update `README.md` to stop implying that direct prompt stuffing is the only agent workflow.
- Add a short integration note for Amoebum-style clients and status-bar modes.

Acceptance criteria:
- Tests cover the two-step retrieval contract.
- Docs explain bounded usage and transcript-discipline expectations.

## Suggested execution order
1. M1 contract and schema
2. M2 search and preview path
3. M3 explicit expansion and opaque handles
4. M4 pressure signals and diff-aware selection
5. M5 tests and docs

## Risks and open questions
- The current code is concentrated in `src/main.rs`, so the MCP work may be easier if we first extract reusable retrieval helpers rather than layering more logic directly into command handlers.
- We need to choose whether opaque handles are purely in-memory session identifiers or persisted artifacts with cleanup rules.
- We should decide early whether byte budgeting is approximate or exact at the transport boundary.
- README examples currently promote direct `context.md` prompt injection and should be revised once the MCP flow exists.

## First commands for the next shell
- `git status --short`
- `rg -n "Assemble|cmd_assemble|query|from_files" src/main.rs README.md`
- `sed -n '469,520p' src/main.rs`
- `sed -n '6128,6260p' src/main.rs`
