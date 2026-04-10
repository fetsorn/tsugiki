---
title: "Layer 2: Fountain and CSVS serialization"
status: active
layer: 2
adr: decisions/0003-typed-tree-model-in-rust.md
depends: [001-layer0-types, 002-layer1-memtree-proptest]
csvs-rs: ~/mm/codes/csvs-rs/
---

# Layer 2: Fountain and CSVS serialization

## Goal

Roundtrip `MemTree` ↔ Fountain files and CSVS tablets. Un-ignore proptest properties 5 and 6. Integrate with csvs-rs for tablet I/O.

## New files

```
tsugiki-core/src/
  fountain/
    mod.rs
    render.rs
    parse.rs
  csvs/
    mod.rs
    containment.rs   — source-child, structure-child, target-child
    bridge.rs        — source-structure, structure-target
```

Add csvs-rs as a dependency (path or git).

## Fountain format

### Render (`fountain/render.rs`)

`render_fountain<T>(tree: &MemTree<T>) -> String`

Walk the tree DFS. For each node:
- Emit `.{node_id.prefix(short_len)}` on its own line. `short_len` is a parameter to the render function (default 8).
- If `depth.fountain_marker()` returns `Some(marker)` (depths 0–2), emit marker + space + text on next line.
- Else emit text as action block (plain line). Depth is implicit from containment.
- For each note: emit `({note.text})` if understand, `(| {note.text})` if regrow.
- Blank line after each node.

### Parse (`fountain/parse.rs`)

`parse_fountain<T>(input: &str) -> Result<(MemTree<T>, Vec<ParseWarning>)>`

State machine:
1. Line starting with `.` followed by hex chars → begin new node, capture prefix. Length is auto-detected from the first `.` line in the file.
2. Next non-blank line: if starts with `#` → count `#` characters to get depth (0, 1, or 2). Extract text after marker. Else → action block, depth is determined from containment (see below).
3. Next line(s): if `(...)` → parenthetical note. Check for leading `| ` to distinguish regrow from understand. Multiple consecutive parentheticals may occur. Else → part of text (multi-line action).
4. Blank line → finalize node.

Depth determines parent: maintain a stack of `(Depth, NodeId)`. For nodes with a heading marker, depth is known from the marker. For action-block nodes (Depth 3), depth is the sentence level — always a leaf. New node pops stack until top has a strictly lower depth, then becomes child of top.

**Depth gaps.** When the parser sees a depth gap (e.g., `#` at Depth 0 followed by an action block at Depth 3), it infers synthetic single-child nodes at the skipped depths. Synthetic UUIDs are derived deterministically via UUID v5 from the parent UUID and the depth level, so they are stable across re-parses. Synthetic nodes appear in CSVS containment tablets but not in Fountain.

**Open question**: how Fountain parsers handle consecutive `(...)` lines — merged or separate? Must be tested against Rust Fountain crates during this layer. If merged, fall back to single parenthetical with `|` as internal separator.

Warnings:
- `ParseWarning::AmbiguousDepth` — heading marker count doesn't match expected nesting.
- `ParseWarning::OrphanNode` — node couldn't find parent in stack.
- `ParseWarning::DuplicateId` — short id collision.

### Roundtrip property

Un-ignore proptest property 5: `parse(render(tree)) == tree` for arbitrary `MemTree<Source>`.

Equality: same nodes (by ShortId), same depth, same text, same parent-child relationships, same sibling order.

## CSVS integration

### Containment tablets (`csvs/containment.rs`)

Each containment CSV (`source-child.csv`, `structure-child.csv`, `target-child.csv`) is a two-column tablet where column names come from the filename.

Read:
```rust
pub fn read_containment<T>(dataset: &Dataset, base: &str, leaf: &str)
    -> Result<Vec<ContainmentEdge<T>>>
```

Uses csvs-rs `Dataset::select()` or direct tablet reading. Each row becomes a `ContainmentEdge<T>` with full UUIDs.

Write:
```rust
pub fn write_containment<T>(dataset: &Dataset, edges: &[ContainmentEdge<T>], base: &str, leaf: &str)
    -> Result<()>
```

Uses csvs-rs `Dataset::insert()` to append edges.

### Bridge tablets (`csvs/bridge.rs`)

Same pattern for `source-structure.csv` and `structure-target.csv`.

```rust
pub fn read_bridge<F, T>(dataset: &Dataset, from_col: &str, to_col: &str)
    -> Result<BridgeSet<F, T>>

pub fn write_bridge<F, T>(dataset: &Dataset, bridges: &BridgeSet<F, T>, from_col: &str, to_col: &str)
    -> Result<()>
```

### Prefix-to-full UUID mapping

CSVS stores full UUIDs. Fountain uses short prefixes (length auto-detected from file, default 8 hex chars). The serialization layer maintains a `HashMap<String, NodeId>` mapping prefixes to full UUIDs, built from the CSVS tablets on first access.

When a new node is created (e.g., during understand phase), generate a full v4 UUID. The Fountain renderer truncates it to the file's prefix length. If the prefix collides with an existing node, report an error — the intent needs a longer prefix length (re-run `decompose` with `--short-len`).

### Roundtrip property

Un-ignore proptest property 6: write containment edges to a temp csvs dataset, read them back, compare to original.

## csvs-rs integration notes

csvs-rs key types for this layer:
- `Dataset::open(dir)` — opens an existing dataset directory
- `Dataset::select(query)` — streams entries matching a query
- `Dataset::insert(entries)` — appends entries
- `Entry { base, base_value, leader_value, leaves }` — a record
- `Grain { base, base_value, leaf, leaf_value }` — a flat fact (two columns)
- `Schema` — parsed from `_-_.csv`, has `Branch` with `Trunks` and `Leaves`

The containment and bridge tablets are simple two-column CSVs. We may read them directly (line-by-line) or via `Grain` for efficiency. If csvs-rs lacks a simple "read all rows from tablet X" helper, this is a feature request to flag.

## Potential csvs-rs feature requests

- **Tablet-level read**: `Dataset::read_tablet(name) -> Vec<Grain>` for bulk two-column reads.
- **Streaming tablet append**: `Dataset::append_tablet(name, grains)` without rewriting the file.
- **Schema validation**: verify that `source-child` matches the `_-_.csv` schema before reading.

Flag these as issues if encountered during implementation.

## Tests

- Unit: render a hand-built MemTree, check exact Fountain string.
- Unit: parse a known Fountain string, check tree matches.
- Unit: roundtrip small tree through Fountain.
- Unit: write/read containment edges through csvs-rs.
- Unit: short-to-full UUID mapping consistency.
- Proptest (un-ignored): property 5 (Fountain roundtrip).
- Proptest (un-ignored): property 6 (CSV roundtrip).

## Done when

- Fountain render + parse roundtrips pass proptest.
- CSVS read/write works against real dataset directory layout.
- Properties 5 and 6 are no longer `#[ignore]`.
- Any csvs-rs limitations are documented as issues.
