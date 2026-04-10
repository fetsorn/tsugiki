---
title: "Layer 3: Streaming FountainWalk"
status: active
layer: 3
adr: decisions/0003-typed-tree-model-in-rust.md
depends: [001-layer0-types, 002-layer1-memtree-proptest, 003-layer2-serialization]
---

# Layer 3: Streaming FountainWalk

## Goal

Implement `FountainWalk<T>` — a streaming tree walker that operates directly on Fountain files and CSVS tablets without loading everything into memory. This is the production path for the CLI. Un-ignore proptest property 7 (streaming equivalence).

## New files

```
tsugiki-core/src/
  fountain_walk/
    mod.rs
    reader.rs     — line-by-line Fountain reader
    inserter.rs   — streaming Fountain insertion
    appender.rs   — CSVS tablet append
```

## Design

### `FountainWalk<T>` (mod.rs)

Implements `TreeWalk<T>` by reading files on demand rather than building a full in-memory tree.

```rust
pub struct FountainWalk<T> {
    fountain_path: PathBuf,
    csvs_dir: PathBuf,
    _tree: PhantomData<T>,
}
```

### TreeWalk implementation

Each method is a targeted file operation:

- `root()` — read first `.` line from Fountain file. Cache after first call.
- `node(id)` — grep Fountain file for `.{id}`, read the following lines to extract depth/text/note.
- `children(id)` — grep containment CSV for lines starting with the full UUID (looked up from short id). This requires a short→full index; build lazily from CSV on first call, or grep for the 8-char prefix.
- `parent(id)` — grep containment CSV for lines where second column starts with the UUID prefix.

### Streaming Fountain reader (`reader.rs`)

```rust
pub struct FountainReader {
    path: PathBuf,
}

impl FountainReader {
    pub fn find_node(&self, short_id: &ShortId) -> Result<Option<RawNode>>
    pub fn nodes_after(&self, short_id: &ShortId) -> Result<Vec<RawNode>>
    pub fn all_short_ids(&self) -> Result<Vec<ShortId>>
}
```

`RawNode` is a lightweight struct: `{ short_id, depth_marker: Option<String>, text: String, note: Option<String>, line_number: usize }`.

Each method does a single pass or targeted seek. No full parse into `MemTree`.

### Streaming Fountain inserter (`inserter.rs`)

The key non-append operation: inserting sentence annotations after a paragraph heading.

```rust
pub fn insert_after(
    fountain_path: &Path,
    target_id: &ShortId,
    new_lines: &[String],
) -> Result<()>
```

Algorithm:
1. Read file line by line.
2. When `.{target_id}` is found, copy through the heading + blank line.
3. Insert `new_lines`.
4. Copy remainder.
5. Write back (atomic rename).

This matches what `insert_structure.py` does today but with typed IDs and error handling.

### CSVS tablet appender (`appender.rs`)

```rust
pub fn append_edge(csv_path: &Path, col1: &str, col2: &str) -> Result<()>
```

Opens file in append mode, writes one line. No full file read.

For bulk:
```rust
pub fn append_edges(csv_path: &Path, edges: &[(String, String)]) -> Result<()>
```

### Equivalence to MemTree

The critical invariant: for any sequence of operations, `FountainWalk` produces the same observable state as `MemTree`. "Observable" means: same nodes, same parent-child relationships, same text content.

Differences that are acceptable:
- File-level whitespace variations.
- UUID ordering within unordered trees (Structure).
- Intermediate file states during multi-step operations.

## Proptest property 7: streaming equivalence

Un-ignore the stub from Layer 1.

Strategy:
1. Generate a random `MemTree<Source>`.
2. Render to Fountain file in a temp directory.
3. Write containment edges to CSV.
4. Create `FountainWalk<Source>` over those files.
5. For each node, compare `MemTree.node(id)` with `FountainWalk.node(id)`.
6. For each node, compare children lists.
7. Assert all match.

## Tests

- Unit: `FountainReader::find_node` on a known Fountain file.
- Unit: `insert_after` on a small Fountain file, verify output.
- Unit: `append_edge` on an existing CSV.
- Integration: build tree via MemTree, serialize to files, walk via FountainWalk, compare.
- Proptest (un-ignored): property 7.

## Performance notes

- `find_node` is O(n) per call where n = file lines. For interactive CLI this is fine (files are <1000 lines for a chapter).
- For repeated access within a session, `FountainWalk` can use an index tablet (`source-line.csv`, `structure-line.csv`, `target-line.csv`) that maps short IDs to line numbers. The index is built on first scan, stored in CSVS, and invalidated when the Fountain file is modified. This gives O(1) lookup after the first pass. See Layer 4 plan for details.
- CSVS grep is O(m) per call where m = CSV rows. Again fine for translation-scale data.

## Done when

- `FountainWalk` implements `TreeWalk`.
- Streaming insertion works correctly.
- Property 7 passes proptest.
- No operation loads a full Fountain or CSV into a `MemTree`.
