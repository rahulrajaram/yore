# Yore Proposals

This document tracks proposed features and enhancements for yore.

---

## 0. Policy Enhancements — Section Length + Required Links

**Status:** Implemented
**Priority:** Medium

Adds section-level length limits and required link targets to policy rules so
summary documents can be enforced as short sections that link to canonical
details. Also adds reporting for canonical documents that lack inbound links.

---

## 1. `yore health` — Structural Document Health Detection

**Status:** Proposed
**Priority:** High
**Source:** [striation_customer_platform agents/reports/platform-documentation-steward/2026-01-01_yore-structural-analysis-proposal.md](https://github.com/anthropics/striation_customer_platform)

### Problem

Documentation follows a predictable decay pattern:

```
Day 1:    Clean plan document (~200 lines)
Week 2:   Implementation details added (~400 lines)
Week 4:   Debugging session notes appended (~600 lines)
Week 8:   Completed phases retained, new phases added (~1000 lines)
Week 12:  Changelog grows, mixed concerns accumulate (~1500 lines)
Week 16:  Document becomes "the junk drawer" (~2000+ lines)
```

This creates maintenance burden, wastes agent tokens, and makes information hard to find.

### Proposed Solution

A `yore health` command to detect structural anti-patterns:

| Detection | Description | Severity |
|-----------|-------------|----------|
| `bloated-file` | Lines exceed threshold (default: 500) | ERROR |
| `accumulator-pattern` | "Part N" sections growing unbounded | ERROR |
| `stale-completed` | Sections marked DONE but >50 lines retained | WARNING |
| `changelog-bloat` | Changelog entries exceed threshold (default: 15) | WARNING |
| `mixed-concerns` | Plan + guide + debugging in same file | WARNING |

### Example Usage

```bash
# Check single file
yore health docs/plans/BUILD_PLAN.md

# Check all documents
yore health --all --index .yore

# JSON output for CI
yore health --all --format json --index .yore

# With custom thresholds
yore health --max-lines 800 --max-changelog 25 docs/
```

### Example Output

```
docs/plans/BUILD_PLAN.md: UNHEALTHY (3 issues)

  bloated-file       ERROR   2771 lines (threshold: 500)
  accumulator        ERROR   16 "Part N" sections [confidence: high]
  stale-completed    WARN    6 sections marked DONE, 500 lines total

Suggested actions:
  - Archive Parts 13-15 (debugging notes) → agents/reports/archive/
  - Extract Part 11 (CLI guide) → docs/guides/BUILD_CLI.md
  - Truncate changelog to 10 entries
```

### Implementation Phases

**Phase 1: Core Metrics**
- Add `lines`, `h2_count`, `changelog_entries` to index during `yore build`
- Basic `yore health <file>` with default thresholds
- Human-readable and JSON output

**Phase 2: Pattern Detection**
- Accumulator pattern detection (`Part N` regex + high H2 count)
- Stale-completed detection (DONE/COMPLETE markers + line count)
- `yore health --all` summary

**Phase 3 (RFC gated):** Section classification and recommendations
- Mixed-concerns keyword detection
- Per-section classification
- Actionable recommendation output

### Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Compute during build or separate? | During `yore build` | Metrics are cheap, avoids stale data |
| Confidence scores? | Yes, coarse buckets (high/medium/low) | Helps triage fuzzy detections |
| Auto-fix capability? | No | Too risky; output suggestions instead |
| Config file? | Defer to Phase 3 | CLI flags for v1, config file later |

### Open Questions

1. Should health metrics be stored in the index or computed on-demand?
2. How to handle documents that legitimately need to be large (e.g., API references)?
3. Integration with `yore check` unified command?

---

## 2. `yore structure` — Document Structure Validation and Scaffolding

**Status:** Proposed
**Priority:** Medium
**Depends on:** `yore health` (shares schema definitions)

### Problem

While `yore health` detects problems, it doesn't help fix them or prevent them. New documents start unstructured and drift over time.

### Revised Scope

After reviewing real agent-generated reports, the original "infer structure from free text" use case is less relevant. Agent-generated documents are already well-structured. The value is:

1. **Preventing decay** — detect when good structure degrades
2. **Enforcing consistency** — ensure all reports follow expected schemas
3. **Bootstrapping** — help new reports start correctly

### Proposed Solution

A `yore structure` command focused on validation and scaffolding:

```bash
# Validate structure against schema
yore structure --validate report.md --schema debugging-report

# Generate scaffold for new document
yore structure --new --template plan > new-plan.md

# Audit all reports for consistency
yore structure --audit agents/reports/
```

### Example: Structure Validation

```bash
$ yore structure --validate agents/reports/2025-12-30_report.md --schema report

✓ Has metadata header (Date, Author)
✓ Has Executive Summary
✓ Has Conclusion
✗ Missing "Next Steps" section (recommended for reports)
✗ Has 16 H2 sections (threshold: 8) — accumulator pattern risk
```

### Example: Template Scaffolding

```bash
$ yore structure --new --template debugging-report

# Debugging Report: [TITLE]

**Date:** 2026-01-01
**Author:** [AGENT]
**Status:** In Progress

---

## Executive Summary
<!-- Brief overview of the issue and resolution -->

## Problem Description
<!-- Symptoms and error messages -->

## Investigation
<!-- Steps taken to diagnose -->

## Root Cause
<!-- What was actually wrong -->

## Solution
<!-- Changes made -->

## Verification
<!-- How you confirmed the fix -->

## Conclusion
<!-- Lessons learned -->
```

### Example: Cross-Report Audit

```bash
$ yore structure --audit agents/reports/

Report Structure Audit
======================

platform-documentation-steward/ (8 reports)
  ✓ All have Executive Summary
  ✓ All have metadata headers
  ⚠ 2 reports missing "Next Steps"
  ✗ 1 report has 16 H2 sections (sprawl risk)

platform-developer/ (2 reports)
  ✓ Consistent structure

Recommended actions:
  - Add "Next Steps" to: 2025-12-30_quick-wins-implementation.md
  - Review for accumulator pattern: 2026-01-01_yore-structural-analysis-proposal.md
```

### Schema Definition

Schemas would be defined in `.yore.toml` or a separate `.yore/schemas.yml`:

```yaml
schemas:
  debugging-report:
    required_sections:
      - Executive Summary
      - Problem Description
      - Root Cause
      - Solution
      - Conclusion
    recommended_sections:
      - Verification
      - Next Steps
    max_h2_sections: 10

  plan:
    required_sections:
      - Summary
      - Current State
      - Open Work
    forbidden_sections:
      - Debugging
      - Session Notes
    max_lines: 600

  guide:
    required_sections:
      - Overview
      - Usage
      - Examples
    forbidden_sections:
      - Changelog
      - TODO
    max_lines: 400
```

### Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Modify files directly? | No | Output patches or suggestions only |
| LLM for inference? | Optional (`--llm` flag) | Keep deterministic by default |
| Relationship to `yore health`? | Complementary | Health detects, structure validates/scaffolds |

### Implementation Phases

**Phase 1: Template Scaffolding**
- `yore structure --new --template <name>`
- Built-in templates: plan, guide, report, debugging-report, adr
- Custom templates via `.yore/templates/`

**Phase 2: Structure Validation**
- `yore structure --validate <file> --schema <name>`
- Check required/forbidden sections
- Check section count thresholds

**Phase 3: Audit Mode**
- `yore structure --audit <dir>`
- Cross-report consistency checking
- Aggregate statistics

### Open Questions

1. Should schemas be per-project or ship with yore as defaults?
2. How to handle documents that don't match any schema?
3. Should `--validate` be integrated into `yore check`?

---

## Relationship Between Proposals

```
┌─────────────────────────────────────────────────────────────┐
│                    Document Lifecycle                        │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  CREATE          MAINTAIN           DETECT           FIX    │
│                                                              │
│  yore structure  yore structure     yore health      Manual │
│  --new           --validate                          or     │
│  --template      --audit                             Agent  │
│                                                              │
│  "Start with     "Ensure it        "Find problems   "Act on │
│   good structure" stays healthy"    when they occur" suggestions"│
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Contributing

To propose a new feature:

1. Open an issue describing the problem and proposed solution
2. If accepted, add an entry to this file
3. Implementation PRs should reference the proposal

---

*Last updated: 2026-01-01*
