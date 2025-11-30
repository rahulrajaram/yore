# Cross-Reference Expansion for `yore assemble`

## 0. Scope & Goals

**Goal:**
Extend `yore assemble` so that, in addition to the "best local documents" for a query, it also pulls in **critical referenced docs** (especially ADRs and design docs) in a **bounded, deterministic, token-aware** way.

**Non-goals (for this phase):**

* No recursive multi-hop graph navigation beyond a small fixed depth.
* No heavy refactoring of existing BM25/section-fingerprint logic.
* No governance or lifecycle metadata beyond what already exists.

We're adding **one new phase** in the existing assembly pipeline:

> After selecting primary sections → **expand cross-references** → merge into digest → render markdown (respecting global token budget).

---

## 1. Parsing Logic for Markdown Links & ADR References

### 1.1 Data Model

Introduce a small struct (or dict) to represent cross-references:

```rust
struct CrossRef {
    ref_type: RefType,           // MarkdownLink or AdrId
    origin_section_id: String,    // section fingerprint or internal ID
    origin_doc_path: String,
    target_doc_path: String,      // normalized repo-relative path
    target_anchor: Option<String>,// e.g. "retry-semantics" from "#retry-semantics"
    raw_text: String,             // original link text or ADR-013 token
}

enum RefType {
    MarkdownLink,
    AdrId,
}
```

### 1.2 Markdown Link Parsing

We care only about **internal markdown links** (docs within the repo).

**Regex:**

* Generic markdown link: `\[(?P<label>[^\]]+)\]\((?P<target>[^)]+)\)`
* Ignore:
  * Image links: start with `![`
  * External links: targets starting with `http://`, `https://`, `mailto:`, etc.

**Rules:**

1. Scan each primary section's text for matches.
2. For each link:
   * Skip if image: text begins with `![`.
   * Skip if `target` starts with `http://`, `https://`, `mailto:`.
3. Parse `target`:
   ```text
   ../docs/adr/ADR-013-retries.md#retry-semantics
   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^
          path_part                  anchor
   ```
   * Split on `#`:
     * `path_part` → relative or absolute path within repo.
     * `anchor`   → slug or heading fragment (optional).
4. Normalize `target_doc_path`:
   * Resolve relative to `origin_doc_path`'s directory.
   * Normalize (`..`, `.`).
   * Map to your canonical index key (whatever you already use for docs).
5. Emit `CrossRef`

### 1.3 ADR ID Parsing (Plain Text, Non-Link)

We also want to catch things like:

* `As described in ADR-013, ...`
* `See ADR 13 for details`
* `Refer to ADR_0013 ...`

**Regex:**

```rust
ADR_REGEX = r"\bADR[-_ ]?(?P<num>\d{2,4})\b"
```

**Assumptions:**

* ADR docs live under `docs/adr/` with filenames like:
  * `ADR-013-*.md`
  * `ADR_0013*.md`
  * `ADR-13*.md`
* We can build an **ADR index** once at startup:

```rust
adr_index: HashMap<String, String>  // "013" -> "docs/adr/ADR-013-retries.md"
```

Build this by scanning `docs/adr/` and extracting digit sequences from filenames.

**Algorithm per section:**

1. Run `ADR_REGEX.finditer(section_text)`.
2. For each match:
   * Normalize ADR number: zero-pad to 3 digits (or 4 if that's your convention).
   * Lookup in `adr_index`; if not found, skip.
3. Emit `CrossRef`

### 1.4 Self-links & Same-Doc Anchors

For now:

* If `target_doc_path` resolves to the **same file** as `origin_doc_path`:
  * Treat as **non-expanding** (we already have sections from this doc).
  * Optionally log but do not create a cross-ref that consumes tokens.

---

## 2. Resolution Strategy: Which Sections to Pull from Linked Docs

We want:

* **High signal, low noise**.
* Priority for **ADRs** and **architecture/design docs**.
* Bounded per-doc and global contribution.

### 2.1 Inputs & Outputs

**Inputs:**

* `primary_sections`: list of selected section objects (from current `yore assemble` phase).
* `crossrefs`: list of `CrossRef` (deduplicated per origin section).
* `xref_token_budget`: integer token cap for cross-ref content.

**Output:**

* `xref_sections`: list of additional section objects to include.

### 2.2 Doc Priority / Type Heuristic

We'll classify target docs into priority bands:

```rust
fn classify_target_doc(path: &str) -> DocType {
    let path_lower = path.to_lowercase();
    if path_lower.contains("/adr/") {
        DocType::Adr
    } else if path_lower.contains("architecture") || path_lower.contains("design") {
        DocType::Design
    } else if path_lower.contains("runbook") || path_lower.contains("operations") {
        DocType::Ops
    } else {
        DocType::Other
    }
}

enum DocType {
    Adr,     // Priority 1
    Design,  // Priority 2
    Ops,     // Priority 3
    Other,   // Priority 4
}
```

Priority ordering for expansion:

1. `Adr`
2. `Design`
3. `Ops`
4. `Other`

Within same doc type, we can bias by:

* Canonicality score (from existing `score_canonicality(doc_path)`).
* Number of `CrossRef`s pointing to that doc.

Define a simple **xref doc score**:

```rust
xref_doc_score = 0.5 * canonicality(doc) + 0.5 * log(1 + num_refs_to_doc)
```

### 2.3 Which Sections to Pull per Doc

For each **target doc** we decide **what to extract**:

Let `sections(doc)` be your existing section list.

#### 2.3.1 If a doc has an anchor (`target_anchor`)

We try to resolve to a specific section:

* Normalize anchor (strip leading `#`, lowercase, replace `-`/`_` with space).
* For each section:
  * Compute a slug from section title (e.g. `Retry Semantics` → `retry-semantics`).
  * If slug matches anchor → select that section.
* If no title match, fallback: use the **first section** whose text contains the anchor token (or part of it).

If all fails → fallback to the **doc-level default** (see below).

#### 2.3.2 ADR docs

We want the **decision context** and summary, not the entire ADR.

Heuristic:

1. Always include the **top-level "Context" and "Decision" sections** if present (titles contain `context`, `decision`, case-insensitive).
2. Otherwise, include:
   * The first section (intro).
   * Plus up to N more sections whose titles contain `motivation`, `rationale`, `consequences`, `summary`.

Cap per ADR doc: `MAX_SECTIONS_PER_ADR` (e.g. 3).

#### 2.3.3 Design / Architecture docs

We want the relevant subsystem context.

Heuristic:

1. If anchor was provided → resolve to that section.
2. Otherwise:
   * Compute a simple similarity score between section text and the original query and/or origin section text (e.g. BM25 restricted to this doc).
   * Pick top `MAX_SECTIONS_PER_DESIGN` (e.g. 2–3).

#### 2.3.4 Ops / Runbook docs

Heuristic:

* Prefer sections whose titles mention:
  * `deploy`, `restart`, `rollback`, `monitor`, etc. (configurable keywords).
* Again, top `MAX_SECTIONS_PER_OPS` (e.g. 2).

#### 2.3.5 Other docs

Fallback:

* Include only the **first section** (overview) or
* The first section whose title or body includes overlap with query tokens.

### 2.4 Deduplication & Depth

We want **no explosion**:

* Maintain `visited_doc_paths` for this `assemble` run.
* We only expand cross-refs from **primary_sections**, not from xref sections (depth 1).
* Ignore cross-refs whose `target_doc_path` is already in:
  * `primary_docs`
  * `xref_docs` (already added)

---

## 3. Token Budget Management for Cross-Refs

We must keep `yore assemble` within the global `--max-tokens` budget.

### 3.1 Budget Split Strategy

Let:

* `T_total` = global max tokens (`--max-tokens`)
* `T_primary` = tokens consumed by primary sections (pre-expansion)
* `T_remaining` = `T_total - T_primary`

We will allocate **up to**:

```rust
const XREF_TOKEN_FRACTION: f64 = 0.3;         // configurable
const XREF_TOKEN_ABS_MAX: usize = 2000;

xref_cap = min(
    (T_total as f64 * XREF_TOKEN_FRACTION) as usize,
    XREF_TOKEN_ABS_MAX
);
xref_token_budget = min(T_remaining, xref_cap);
```

If `xref_token_budget <= 0`, we skip expansion completely.

### 3.2 Per-Doc Cap

To avoid a single ADR eating everything:

```rust
const MAX_TOKENS_PER_XREF_DOC: usize = 600;  // e.g. ~2-3 medium sections
```

When adding sections from a given `target_doc`:

* Keep a running `tokens_for_doc`.
* Stop adding from that doc when `tokens_for_doc >= MAX_TOKENS_PER_XREF_DOC`.
* Stop adding globally when `xref_token_budget` is exhausted.

### 3.3 Token Estimation

Use the **same estimator** you already use for `yore assemble`:

```rust
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4  // rough heuristic
}
```

When selecting sections for cross-ref:

1. Sort `target docs` by:
   * doc_type priority (`Adr > Design > Ops > Other`)
   * `xref_doc_score` (canonicality + number of refs)
2. For each doc in that order:
   * Select sections according to per-doc rules.
   * For each section:
     * `t = estimate_tokens(section_text)`
     * If `t > xref_token_budget` → skip (or truncate section).
     * Else:
       * Add section to `xref_sections`.
       * Decrement `xref_token_budget` and `tokens_for_doc`.

We don't need to be perfect; we just need a **hard cap**.

---

## 4. Implementation Approach & Hook Points

### 4.1 Existing Pipeline

Right now, `yore assemble` roughly does:

```rust
fn cmd_assemble(...) {
    let forward_index = load_forward_index(index_dir)?;
    let primary_sections = search_relevant_sections(query, &forward_index, max_sections);
    let digest = distill_to_markdown(&primary_sections, query, max_tokens);
    println!("{}", digest);
}
```

We will insert cross-ref expansion between section selection and rendering.

### 4.2 New Functions

#### 4.2.1 Parse cross-refs from sections

```rust
fn collect_crossrefs(sections: &[SectionMatch]) -> Vec<CrossRef> {
    let mut refs = Vec::new();
    for sec in sections {
        refs.extend(parse_markdown_links(sec));
        refs.extend(parse_adr_ids(sec));
    }
    dedupe_crossrefs(refs)
}
```

Where:

* `parse_markdown_links(sec: &SectionMatch) -> Vec<CrossRef>`
* `parse_adr_ids(sec: &SectionMatch) -> Vec<CrossRef>`
* `dedupe_crossrefs` reduces duplicates by `(origin_section_id, target_doc_path, target_anchor)`.

#### 4.2.2 Resolve cross-refs into sections

```rust
fn resolve_crossrefs(
    crossrefs: &[CrossRef],
    query: &str,
    primary_sections: &[SectionMatch],
    index: &ForwardIndex,
    xref_token_budget: usize,
) -> Vec<SectionMatch> {
    // 1. Group crossrefs by target_doc_path
    // 2. Score docs, sort by priority
    // 3. For each doc, select sections as per rules
    // 4. Respect per-doc and global token caps
    vec![] // returns xref_sections
}
```

Helper functions:

* `classify_target_doc(path: &str) -> DocType`
* `select_sections_for_adr(doc_path: &str, index: &ForwardIndex) -> Vec<SectionMatch>`
* `select_sections_for_design(doc_path: &str, anchor: Option<&str>, query: &str, index: &ForwardIndex) -> Vec<SectionMatch>`
* `select_sections_for_ops(doc_path: &str, index: &ForwardIndex) -> Vec<SectionMatch>`
* `select_sections_for_other(doc_path: &str, index: &ForwardIndex) -> Vec<SectionMatch>`

### 4.3 New Pipeline

Revised `cmd_assemble`:

```rust
fn cmd_assemble(...) -> Result<(), Box<dyn std::error::Error>> {
    let forward_index = load_forward_index(index_dir)?;

    // Phase 1: Primary section selection
    let primary_sections = search_relevant_sections(query, &forward_index, max_sections);
    let primary_tokens = primary_sections.iter()
        .map(|s| estimate_tokens(&s.content))
        .sum::<usize>();

    // Phase 2: Cross-reference expansion
    let crossrefs = collect_crossrefs(&primary_sections);
    let xref_token_budget = calculate_xref_budget(max_tokens, primary_tokens);
    let xref_sections = resolve_crossrefs(
        &crossrefs,
        query,
        &primary_sections,
        &forward_index,
        xref_token_budget,
    );

    // Phase 3: Merge and render
    let mut all_sections = primary_sections.clone();
    all_sections.extend(xref_sections);

    let digest = distill_to_markdown(&all_sections, query, max_tokens);
    println!("{}", digest);
    Ok(())
}
```

---

## 5. Test Cases

You want both **unit-level parsing tests** and **end-to-end expansion tests**.

### 5.1 Parsing Tests

#### 5.1.1 Markdown link parsing

Input section text:

```markdown
See [Service Onboarding](../architecture/SERVICE_ONBOARDING.md#retry-semantics)
and [External Docs](https://example.com/ext).

![Diagram](../images/arch.png)
```

Expected:

* `collect_crossrefs` returns **one** `CrossRef`:
  * `ref_type = MarkdownLink`
  * `target_doc_path` resolves to `docs/architecture/SERVICE_ONBOARDING.md`
  * `target_anchor = "retry-semantics"`
* External link and image link are ignored.

#### 5.1.2 ADR ID parsing

Text:

```markdown
As described in ADR-013 and ADR 14, we changed the retry semantics.
```

With `adr_index = {"013": "docs/adr/ADR-013-retries.md", "014": "docs/adr/ADR-014-timeouts.md"}`

Expected:

* Two `CrossRef` objects with:
  * `target_doc_path` = respective ADR paths.
  * `ref_type = AdrId`.

### 5.2 Resolution Tests

#### 5.2.1 ADR expansion

Setup:

* Primary section mentions `ADR-013`.
* `ADR-013` doc has sections: `Context`, `Decision`, `Consequences`, `Appendix`.
* `MAX_SECTIONS_PER_ADR = 3`.
* Sufficient token budget.

Expected:

* `resolve_crossrefs` returns sections from `ADR-013`:
  * Includes `Context` and `Decision` always.
  * Optionally `Consequences`, not `Appendix`.

#### 5.2.2 Anchor resolution in design doc

Setup:

* Primary section contains:
  `See [Service Onboarding](../architecture/SERVICE_ONBOARDING.md#retry-semantics)`
* `SERVICE_ONBOARDING.md` has sections:
  * `Overview`
  * `Retry Semantics`
  * `Logging`

Expected:

* Selected cross-ref section = `Retry Semantics` only (or first if you choose to add more).
* No extra unrelated sections pulled.

#### 5.2.3 Token budget cap

Setup:

* `max_tokens = 1000`
* `primary_sections` estimated at `800` tokens.
* `XREF_TOKEN_FRACTION = 0.3` → xref cap = `min(300, XREF_TOKEN_ABS_MAX)`
* `T_remaining = 200`, `xref_token_budget = min(200, 300) = 200`.
* Potential crossref sections sum to 500 tokens.

Expected:

* `resolve_crossrefs` only returns sections whose combined estimated tokens ≤ 200.
* Extra sections are dropped.

#### 5.2.4 No-budget scenario

Setup:

* `primary_sections` already consume ≥ `max_tokens`.

Expected:

* `xref_sections` empty; cross-ref expansion skipped.

### 5.3 Integration Tests (End-to-End `yore assemble`)

#### 5.3.1 "retry semantics" query

* Query: `"retry semantics"`
* Primary doc: `KUBERNETES_TEST_EXECUTION.md` has section:
  ```markdown
  Retry behavior is defined in ADR-013.
  ```
* `ADR-013` exists and describes retry semantics.

Expected `context.md`:

* Contains primary section from `KUBERNETES_TEST_EXECUTION.md`.
* Also contains sections from `ADR-013` (Context/Decision, etc).
* ADR parts appear under a clearly labeled section, e.g.:
  ```markdown
  ## Cross-Referenced Documents

  ### ADR-013 (docs/adr/ADR-013-retries.md)
  ...
  ```

#### 5.3.2 "authentication system" query

* Primary doc: `SESSION_ISOLATION_INVESTIGATION.md`
* It contains link:
  ```markdown
  See [Auth Architecture](../architecture/AUTH_SYSTEM_DESIGN.md#session-model)
  for the full rationale.
  ```

Expected:

* Primary sections + cross-ref section `session-model` from `AUTH_SYSTEM_DESIGN.md`.

---

This spec keeps cross-reference expansion:

* **Simple** (depth 1, no crazy graph),
* **Deterministic** (fixed heuristics, token caps),
* And **squarely focused** on improving LLM performance by giving it the missing "why" (ADRs/design docs) without blowing the context window.

You can implement `collect_crossrefs` + `resolve_crossrefs` exactly as specified and plug them into the existing `yore assemble` pipeline with minimal surgery.
