# Yore Maintenance Workflows (Graph + Consolidation)

This document shows how to use Yore's graph export and consolidation suggestions to plan documentation cleanups and structural changes.

## 1. Export the documentation link graph

First, build or refresh the docs index:

```bash
yore --profile docs build
```

Then export the link graph:

```bash
# JSON graph for downstream tools
yore export-graph --index docs/.index --format json > docs/.index/yore-graph.json

# Graphviz DOT graph for visualization
yore export-graph --index docs/.index --format dot > docs/.index/yore-graph.dot
```

Typical uses:

- Visualize clusters of tightly connected docs (for example in Graphviz, Gephi, or a small script).
- Identify hubs (heavily linked “index” docs) and peripheries (leaf docs).
- Spot dead ends or overly cross-linked areas.

## 2. Suggest consolidation candidates

Use the consolidation command to find likely duplicate or overlapping documents:

```bash
yore suggest-consolidation --index docs/.index --threshold 0.7 --json > docs/.index/yore-consolidation.json
```

The output contains groups like:

```json
{
  "total_groups": 1,
  "groups": [
    {
      "canonical": "docs/architecture/MVP_STATUS.md",
      "merge_into": ["docs/MVP_STATUS.md"],
      "canonical_score": 0.82,
      "avg_similarity": 0.87,
      "note": "Merge 1 file(s) into canonical docs/architecture/MVP_STATUS.md"
    }
  ]
}
```

Each group identifies:

- `canonical` — the document Yore considers the best candidate to keep (based on canonicality scoring).
- `merge_into[]` — documents that are strong duplication candidates.
- `canonical_score` — how “authoritative” the canonical doc appears.
- `avg_similarity` — similarity between canonical and its merge targets.

You can also run it in human-readable form:

```bash
yore suggest-consolidation --index docs/.index --threshold 0.7
```

## 3. Using graph + consolidation together

A recommended workflow:

1. **Export the graph** (JSON or DOT).
2. **Run consolidation suggestions** to get canonical ↔ duplicate pairs.
3. **Overlay consolidation candidates on the graph**, for example:
   - Mark canonical docs in one color.
   - Mark `merge_into` docs in another.
4. **Decide actions**:
   - Merge content from `merge_into` into `canonical`, then archive the merged docs.
   - Update links in the graph, using:
     - `yore fix-links` for safe mechanical fixes.
     - `yore fix-references --mapping mappings.yaml` for bulk rewrites.

## 4. Example mapping file for bulk rewrites

After you've decided on consolidations, you can codify them in a mapping:

```yaml
# mappings.yaml
mappings:
  - from: docs/MVP_STATUS.md
    to: docs/architecture/MVP_STATUS.md
  - from: docs/database/old-summary.md
    to: docs/database/control-plane-consolidation-summary.md
```

Apply them with:

```bash
# Dry-run to preview changed files
yore fix-references --mapping mappings.yaml --index docs/.index --dry-run

# Apply when ready
yore fix-references --mapping mappings.yaml --index docs/.index --apply
```

This combination — `export-graph`, `suggest-consolidation`, and `fix-references` — provides a repeatable, mechanical backbone for documentation cleanup, while still leaving final decision-making and prose edits to humans.

