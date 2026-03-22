# IMPLEMENTATION_PLAN

## Project: Yore MCP for bounded documentation context

## Session status
- The repository is on `master` with the hardened bounded preview/fetch CLI committed.
- `yore mcp search-context` and `yore mcp fetch-context` are implemented in the CLI and covered by integration tests.
- This session hardened cwd-independent document resolution, read-only-safe MCP handle storage, preview truncation reporting, and opt-in hook installation.
- The next priority is a thin MCP server or wrapper layer so Amoebum can call the existing preview/fetch contract without shell-specific glue.
- Remaining roadmap gaps after that are diff-aware selection and deeper client integration notes.

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
- bounded preview/fetch retrieval through `yore mcp search-context` and `yore mcp fetch-context`;
- source-root metadata for portable index reads across working directories;
- filesystem-backed MCP handle persistence with read-only-safe fallback storage;
- related structural tools such as `similar`, `backlinks`, `canonicality`, and graph export.

Important constraints from the current code:
- `assemble` currently emits markdown only;
- the public surface is still CLI-first, not a standalone MCP server;
- byte and token budgeting remain approximate at the transport boundary;
- current MCP handle storage is local-filesystem-based rather than session/service managed;
- the code is still concentrated in `src/main.rs`.

## Priority tranches

### Tranche M1 - MCP contract and response schema
**Priority:** high  
**Status:** implemented in CLI

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
**Status:** implemented and hardened

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
**Status:** implemented and hardened

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
**Status:** partially implemented

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
**Status:** partially implemented

Document and verify the MCP surface.

Deliverables:
- Add integration tests for preview/fetch budgeting and dedupe behavior.
- Update `README.md` to stop implying that direct prompt stuffing is the only agent workflow.
- Add a short integration note for Amoebum-style clients and status-bar modes.

Acceptance criteria:
- Tests cover the two-step retrieval contract.
- Docs explain bounded usage and transcript-discipline expectations.

### Tranche M6 - Thin MCP server or wrapper layer
**Priority:** high  
**Status:** next

Expose the existing bounded preview/fetch contract through an actual MCP-facing process.

Deliverables:
- Decide whether the first server shape is:
  - a dedicated `yore-mcp` binary, or
  - a `yore mcp serve` subcommand wrapping existing retrieval helpers.
- Reuse the existing `search_context` and `fetch_context` schemas instead of inventing a second contract.
- Implement MCP tool registration and request/response handling for the bounded preview/fetch flow.
- Keep the server layer thin: delegate retrieval, truncation, dedupe, and handle storage to existing Rust helpers.

Acceptance criteria:
- Amoebum can invoke Yore over MCP transport without scraping CLI stdout conventions.
- The wrapper preserves the existing bounded preview/fetch semantics and budget defaults.
- CLI usage remains stable for current users.

## Suggested execution order
1. M1 contract and schema
2. M2 search and preview path
3. M3 explicit expansion and opaque handles
4. M6 thin MCP server or wrapper layer
5. M4 pressure signals and diff-aware selection
6. M5 tests and docs

## Risks and open questions
- The current code is concentrated in `src/main.rs`, so the MCP work may be easier if we first extract reusable retrieval helpers rather than layering more logic directly into command handlers.
- We still need to decide whether long-term opaque handles should stay local-filesystem-based or move to a more explicit session model.
- We should decide early whether byte budgeting is approximate or exact at the transport boundary.
- We still need to define the server packaging choice: dedicated binary versus `yore mcp serve`.

## First commands for the next shell
- `git status --short`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `rg -n "search_context|fetch_context|source_root|artifact_store_unavailable|RefinedSection|YORE_INSTALL_GIT_HOOKS" src/main.rs build.rs tests README.md`
## Next Work Tranches

1. NXT-008 `Implement a thin MCP server or wrapper layer over yore search-context and fetch-context for Amoebum.`: incomplete. tranche_group=next-todos
    Scope:
    1. Choose the first server shape: dedicated binary or `yore mcp serve`.
    2. Reuse the existing bounded preview/fetch contract instead of defining a second schema.
    3. Keep CLI behavior stable while exposing the same tools over MCP transport.
    Exit criteria:
    1. Amoebum can call Yore through MCP transport for preview and fetch.
    2. Focused tests cover the server or wrapper flow without regressing the current CLI path.

Operator policy while queue is non-empty:
- Keep IMPLEMENTATION_PLAN.md and .yarli/tranches.toml synchronized after each planning change.
