# Yore Assemble - Context Assembly for LLMs

**Status:** Phase 2 - Implementation in Progress
**Goal:** Provide LLMs with optimal, distilled context for answering questions about the codebase
**Priority:** HIGH - Direct LLM operation improvement

---

## Contract

### CLI Interface

```bash
yore assemble "How do I deploy a new service?" \
  --max-tokens 8000 \
  --format markdown \
  --output context.md
```

**Required Arguments:**
- Query string (natural language question/task)

**Optional Arguments:**
- `--max-tokens N` - Token budget for output (default: 8000)
- `--format {markdown|json}` - Output format (default: markdown)
- `--output PATH` - Output file (default: stdout)
- `--depth N` - Cross-reference expansion depth (default: 1, max: 2)
- `--sections N` - Max sections to include (default: 20)
- `--index PATH` - Index directory (default: .yore)

**Exit Codes:**
- 0: Success
- 1: Error (no index, query failed, etc.)

---

## Output Format (Markdown)

```markdown
# Context Digest for: "How do I deploy a new service?"

**Generated:** 2025-11-29T15:23:00Z
**Token Budget:** 8000
**Actual Tokens:** ~7,850
**Primary Areas:** deployment, services, operations
**Documents Scanned:** 221
**Sections Selected:** 18

---

## Top Relevant Documents

1. **docs/deployment/NEW_SERVICE_GUIDE.md** (score: 0.92, canonical: 0.85)
   - Last updated: 2025-11-15
   - Status: canonical
   - Sections included: 3

2. **docs/adr/ADR-0013-service-onboarding.md** (score: 0.87, canonical: 0.90)
   - Last updated: 2025-10-01
   - Status: canonical
   - Sections included: 2

3. **docs/operations/RUNBOOK.md** (score: 0.75, canonical: 0.60)
   - Last updated: 2025-11-01
   - Status: secondary
   - Sections included: 1

---

## Distilled Content

### Required Steps (from docs/deployment/NEW_SERVICE_GUIDE.md)

**Source:** docs/deployment/NEW_SERVICE_GUIDE.md:45-89 (canonical: 0.85)

Create a new service skeleton using the script:

```bash
./scripts/new_service.sh <service-name>
```

Register the service in `services.yaml` with required fields:
- `owner` - Team email
- `team` - Team name
- `runtime` - nodejs | python | rust
- `deployment_env` - staging | prod

---

### Deployment Process (from docs/deployment/NEW_SERVICE_GUIDE.md)

**Source:** docs/deployment/NEW_SERVICE_GUIDE.md:120-145 (canonical: 0.85)

First-time deployment to staging:

```bash
yarn services deploy <service-name> --env=staging
```

After staging validation, deploy to production:

```bash
yarn services deploy <service-name> --env=prod
```

---

### Cross-Reference: Service Registry (from docs/adr/ADR-0013-service-onboarding.md)

**Source:** docs/adr/ADR-0013-service-onboarding.md:78-95 (canonical: 0.90)
**Referenced by:** docs/deployment/NEW_SERVICE_GUIDE.md:67

All services must be registered in the central `services.yaml` registry.
This enables automated:
- CI/CD pipeline generation
- Service discovery
- Dependency tracking

The registry is the source of truth for service metadata.

---

## Metadata

**Canonicality Scores:**
- 0.90+: Authoritative source, prefer over other docs
- 0.70-0.89: Reliable, current documentation
- 0.50-0.69: Secondary or supporting documentation
- <0.50: Potentially stale, use with caution

**Section Selection:**
- Total candidate sections: 127
- BM25 relevance threshold: 0.15
- Selected after filtering: 18
- Token allocation per section: ~400 tokens avg

---

## Usage with LLM

Paste this digest into your LLM conversation, then ask:

> Using only the information in the context above, answer: "How do I deploy a new service?"
> Be explicit when something is not documented in the context.

This ensures the LLM:
- Uses only vetted, canonical information
- Doesn't hallucinate paths or commands
- Acknowledges gaps in documentation
```

---

## Implementation Architecture

### Phase 1: Core Functions

#### 1. `search_relevant_sections(query: &str, index: &ForwardIndex) -> Vec<SectionMatch>`

**Purpose:** Find top-N relevant sections using BM25

**Algorithm:**
1. Use existing BM25 scoring to rank documents
2. For each top document (e.g., top 20):
   - Split into sections by headings
   - Compute BM25 score for each section
3. Merge and sort all sections by score
4. Return top M sections (e.g., 50-100)

**Output:**
```rust
struct SectionMatch {
    doc_path: String,
    heading: String,
    line_start: usize,
    line_end: usize,
    bm25_score: f64,
    content: String,
}
```

#### 2. `score_canonicality(doc_path: &str, metadata: &FileEntry) -> f64`

**Purpose:** Score document authority/trustworthiness

**Heuristic (v1):**
```
score = 0.5  // baseline

// Path-based boosts
if path.contains("docs/adr/") || path.contains("docs/architecture/"):
    score += 0.2
if path.contains("docs/index/"):
    score += 0.15
if path.contains("scratch") || path.contains("archive") || path.contains("old"):
    score -= 0.3

// Filename patterns
if filename matches "README|INDEX|GUIDE|RUNBOOK|PLAN":
    score += 0.1

// Recency
days_since_update = (now - last_modified).days()
if days_since_update < 90:
    score += 0.1
else if days_since_update > 365:
    score -= 0.1

// Clamp to [0.0, 1.0]
return score.max(0.0).min(1.0)
```

**Later improvements:**
- Use link graph (PageRank)
- Parse frontmatter for explicit `status: canonical`
- Factor in cross-reference count

#### 3. `expand_crossrefs(sections: &[SectionMatch], index: &ForwardIndex, depth: usize) -> Vec<SectionMatch>`

**Purpose:** Resolve cross-references to include linked content

**Algorithm:**
1. For each section in input:
   - Scan content for markdown links: `[text](path.md)`
   - Scan for ADR references: `ADR-00XX`
2. Resolve links to file paths
3. For each referenced doc:
   - If anchor present (`#heading`): fetch that section
   - Else: fetch introduction (first 2-3 paragraphs)
4. Recursively expand (up to `depth` levels)
5. Deduplicate by `(doc_path, line_start, line_end)`
6. Limit total expansions (e.g., +20 sections max)

**Link resolution regex:**
```rust
// Markdown links
\[([^\]]+)\]\(([^)]+\.md(?:#[^)]+)?)\)

// ADR references
ADR-(\d{3,4})
```

#### 4. `distill_to_markdown(sections: &[SectionMatch], canonicality: &HashMap<String, f64>, query: &str, max_tokens: usize) -> String`

**Purpose:** Build final markdown digest within token budget

**Algorithm:**
1. Group sections by `doc_path`
2. Sort docs by combined score: `bm25_score * 0.7 + canonicality * 0.3`
3. For each doc (top 10-15):
   - Add header with metadata
   - For each section:
     - Include heading
     - Include content (up to ~400 tokens per section)
     - Add source attribution
4. Track running token count (~4 chars per token)
5. Stop when approaching `max_tokens`
6. Add metadata footer with stats

**Output structure:**
```
# Context Digest for: "<query>"
[metadata block]

## Top Relevant Documents
[ranked list with scores]

## Distilled Content
### Section 1 (from doc A)
[content]
### Section 2 (from doc B)
[content]
...

## Metadata
[stats, canonicality legend, usage instructions]
```

---

## Testing Strategy

### Manual Testing

**Test Query 1: Deployment**
```bash
yore assemble "How do I deploy a new service?" --max-tokens 8000 > test1.md
```

**Expected:**
- Includes NEW_SERVICE_GUIDE.md (if exists)
- Includes deployment scripts/commands
- References ADRs about service onboarding
- Stays under 8000 tokens

**Test Query 2: Cross-Document**
```bash
yore assemble "What are the retry semantics for failed API calls?" > test2.md
```

**Expected:**
- Spans multiple docs (architecture, runbooks, ADRs)
- Resolves cross-references
- Provides complete picture

**Test Query 3: Operational**
```bash
yore assemble "How do I troubleshoot agent coordinator failures?" > test3.md
```

**Expected:**
- Prioritizes runbooks over design docs
- Includes diagnostic commands
- References related monitoring docs

### Automated Evaluation

**Minimal Harness:** `evaluation/questions.jsonl`

```jsonl
{"id": 1, "question": "How do I restart the agent coordinator?", "expected_contains": ["kubectl", "rollout restart", "agent-coordinator"]}
{"id": 2, "question": "Which ADR defines retry semantics?", "expected_contains": ["ADR", "retry", "backoff"]}
{"id": 3, "question": "How do I onboard a new service?", "expected_contains": ["new_service.sh", "services.yaml", "register"]}
```

**Evaluation script:**
```bash
#!/bin/bash
# evaluation/run_eval.sh

total=0
passed=0

while IFS= read -r line; do
    id=$(echo "$line" | jq -r '.id')
    question=$(echo "$line" | jq -r '.question')
    expected=$(echo "$line" | jq -r '.expected_contains[]')

    # Run assemble
    context=$(yore assemble "$question" --max-tokens 8000)

    # Check each expected phrase
    match_count=0
    expected_count=$(echo "$line" | jq -r '.expected_contains | length')

    for phrase in $expected; do
        if echo "$context" | grep -qi "$phrase"; then
            ((match_count++))
        fi
    done

    coverage=$(echo "scale=2; $match_count / $expected_count" | bc)
    echo "Q$id: $coverage ($match_count/$expected_count matches)"

    if (( $(echo "$coverage >= 0.8" | bc -l) )); then
        ((passed++))
    fi
    ((total++))

done < evaluation/questions.jsonl

echo "Passed: $passed/$total"
```

---

## Implementation Phases

### Phase 2.1: Core Assembly (3 hours)
- [x] Create spec document
- [ ] Implement `search_relevant_sections()`
- [ ] Implement `score_canonicality()`
- [ ] Add `assemble` subcommand to CLI
- [ ] Basic markdown output format

### Phase 2.2: Cross-Reference Expansion (1 hour)
- [ ] Implement `expand_crossrefs()`
- [ ] Link resolution regex
- [ ] Deduplication logic

### Phase 2.3: Distillation & Token Budget (1 hour)
- [ ] Implement `distill_to_markdown()`
- [ ] Token counting (approximate)
- [ ] Section truncation/selection
- [ ] Metadata generation

### Phase 2.4: Testing & Refinement (1 hour)
- [ ] Manual testing with 5-10 real queries
- [ ] Create `evaluation/questions.jsonl`
- [ ] Implement eval harness script
- [ ] Tune canonicality weights based on results

**Total Estimated Effort:** 6 hours

---

## Success Criteria

1. **Functional:**
   - `yore assemble` produces markdown digest for any query
   - Stays within token budget (±10%)
   - Includes relevant sections from top docs
   - Resolves at least one level of cross-references

2. **Quality:**
   - Evaluation harness: ≥80% coverage on test questions
   - Manual review: digest is coherent and useful for LLM
   - No duplicate sections in output
   - Canonicality scores make intuitive sense

3. **Performance:**
   - Assembly takes <5s for typical query on 200-file corpus
   - Scales to 1000-file corpus in <15s

---

## Future Enhancements (Phase 3+)

**Not in scope for v1:**
- LLM-based summarization (extractive only for now)
- Graph-based PageRank canonicality
- Semantic clustering of sections
- Multi-repo support
- Interactive refinement ("add more about X")

These can wait until we validate that basic assembly helps LLMs.

---

## Usage Examples

### With Claude Code

```bash
# Generate context
yore assemble "How do I add a new endpoint to the API?" > context.md

# In Claude Code conversation:
# 1. Paste context.md
# 2. Ask: "Using the context above, how do I add a new endpoint?"
```

### With ChatGPT

```bash
# Generate context
yore assemble "Explain the authentication flow" --max-tokens 6000

# Copy output and paste into ChatGPT with:
# "Using only the information below, explain the authentication flow.
#  Acknowledge gaps if anything is missing."
```

### As Agent Tool

Future: Claude Code agent calls `yore assemble` automatically before answering doc questions.

---

## Appendix: Token Estimation

**Rough approximation:**
- 1 token ≈ 4 characters
- Average section: ~1000 chars = ~250 tokens
- 8000 token budget = ~32,000 chars
- Target: 15-20 sections with metadata overhead

**Accurate counting (future):**
- Use `tiktoken` library (Python)
- Or approximate with char count × 0.25

For v1, char-based estimation is sufficient.
