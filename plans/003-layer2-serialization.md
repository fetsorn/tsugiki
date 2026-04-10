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
- Emit `.{short_id}` on its own line.
- If depth is not Leaf, emit depth marker (`#`, `##`, `###`) followed by space and text on next line.
- Else emit text as action block (plain line).
- If node has a note, emit `({note})` on next line.
- Blank line after each node.

### Parse (`fountain/parse.rs`)

`parse_fountain<T>(input: &str) -> Result<(MemTree<T>, Vec<ParseWarning>)>`

State machine:
1. Line starting with `.` and exactly 8 hex chars → begin new node, capture ShortId.
2. Next non-blank line: if starts with `#` → extract depth and text. Else → Leaf, text is this line.
3. Next line: if `(...)` → parenthetical note. Else → part of text (multi-line action).
4. Blank line → finalize node.

Depth determines parent: maintain a stack of `(Depth, ShortId)`. New node pops stack until top has lower depth, then becomes child of top.

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

### Short-to-full UUID mapping

CSVS stores full UUIDs. Fountain uses 8-char short ids. The serialization layer must maintain a `HashMap<ShortId, NodeId>` built from the CSVS tablets, and use it when going Fountain → CSVS.

When a new node is created (e.g., during understand phase), generate a full v4 UUID and register both short and full forms.

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
