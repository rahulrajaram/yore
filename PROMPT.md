# YORE Run Prompt

Assume the role of a careful engineering agent for the `yore` repository.

Current focus:
- Implement a thin MCP server or wrapper layer over Yore's existing bounded preview/fetch contract.
- Reuse Yore's deterministic indexing and context-assembly pipeline instead of adding broad workspace ingestion.
- Keep existing CLI behavior stable; the server layer should wrap the current contract rather than replace it.

Constraints that were validated this session:
- Prefer skills and MCPs over project-local command packs for Amoebum integration.
- Use two-step retrieval: `search`/`preview` first, then `expand`/`fetch` only on explicit follow-up.
- Enforce hard top-k, token, and byte caps.
- Deduplicate overlapping hits before returning context.
- Keep large artifacts off-transcript; return short summaries plus opaque handles instead of verbose blobs.
- Make IDE/context ingestion selection-first and diff-aware, never workspace-wide by default.
- Expose clear truncation and pressure signals when limits cut results down.
- Preserve the existing `search_context` and `fetch_context` response shapes when adding the server layer.
- Treat the server or wrapper as thin glue over the hardened Rust helpers now in `src/main.rs`.

Working style:
- Keep changes scoped, minimal, and aligned with existing Rust and Clap patterns in `src/main.rs`.
- Prefer extracting reusable helpers only where needed to keep the server wrapper clean.
- Add tests alongside behavior changes.
- Prefer deterministic output contracts that are easy for an MCP client to consume.

Validation:
- Run focused `cargo test` targets while iterating.
- Run `cargo test` and `cargo clippy` before wrapping up meaningful MCP changes.
- Verify the MCP server or wrapper can exercise preview and fetch end-to-end for Amoebum's expected flow.
