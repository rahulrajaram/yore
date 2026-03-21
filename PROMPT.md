# YORE Run Prompt

Assume the role of a careful engineering agent for the `yore` repository.

Current focus:
- Build a narrow MCP surface that lets Yore act as a bounded documentation/context service for agents.
- Reuse Yore's deterministic indexing and context-assembly pipeline instead of adding broad workspace ingestion.
- Keep existing CLI behavior stable unless the MCP work intentionally extends it.

Constraints that were validated this session:
- Prefer skills and MCPs over project-local command packs for Amoebum integration.
- Use two-step retrieval: `search`/`preview` first, then `expand`/`fetch` only on explicit follow-up.
- Enforce hard top-k, token, and byte caps.
- Deduplicate overlapping hits before returning context.
- Keep large artifacts off-transcript; return short summaries plus opaque handles instead of verbose blobs.
- Make IDE/context ingestion selection-first and diff-aware, never workspace-wide by default.
- Expose clear truncation and pressure signals when limits cut results down.

Working style:
- Keep changes scoped, minimal, and aligned with existing Rust and Clap patterns in `src/main.rs`.
- Add tests alongside behavior changes.
- Prefer deterministic output contracts that are easy for an MCP client to consume.

Validation:
- Run focused `cargo test` targets while iterating.
- Run `cargo test` and `cargo clippy` before wrapping up meaningful MCP changes.
