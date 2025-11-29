# yore

Fast document indexer for finding duplicates and searching content.

## Features

- **Fast**: Indexes 200+ files in ~250ms
- **Portable**: Single 2.8MB binary, no runtime dependencies
- **Query**: Search by keywords across all documents
- **Similar**: Find documents similar to a reference file
- **Dupes**: Detect duplicate/overlapping content
- **REPL**: Interactive query mode

## Installation

```bash
# Build from source
cargo build --release

# Binary is at target/release/yore
cp target/release/yore ~/.local/bin/
```

## Usage

### Build Index

```bash
# Index current directory
yore build .

# Index specific directory with custom output
yore build /path/to/docs --output .yore

# Specify file types
yore build . --types md,txt,rst
```

### Search

```bash
# Search for keywords
yore query kubernetes deployment

# Limit results
yore query auth jwt --limit 5

# Files only (for scripting)
yore query api --files-only

# JSON output
yore query database --json
```

### Find Similar Files

```bash
# Find files similar to a reference
yore similar README.md

# Adjust threshold and limit
yore similar docs/ARCHITECTURE.md --threshold 0.4 --limit 10
```

### Find Duplicates

```bash
# Find duplicates with default threshold (35%)
yore dupes

# Stricter threshold
yore dupes --threshold 0.5

# Group duplicates
yore dupes --group
```

### Statistics

```bash
# Show index statistics
yore stats

# Show top keywords
yore stats --top-keywords 50
```

### Interactive Mode

```bash
yore repl
> kubernetes deployment
> similar docs/README.md
> dupes
> stats
> quit
```

## Index Format

Indexes are stored in `.yore/` directory:

- `forward_index.json` - File → metadata mapping
- `reverse_index.json` - Keyword → file references
- `stats.json` - Index statistics

## Configuration

Create `.yore.toml` in your project:

```toml
[index]
output = ".yore"
types = ["md", "txt", "rst"]

[exclude]
patterns = ["node_modules", "target", "vendor"]

[similarity]
default_threshold = 0.35
```

## Performance

| Operation | Time |
|-----------|------|
| Index 200 files | ~250ms |
| Query | <10ms |
| Dupes (200 files) | ~50ms |

## License

MIT
