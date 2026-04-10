---
title: "Layer 1: In-memory tree and property-based tests"
status: active
layer: 1
adr: decisions/0003-typed-tree-model-in-rust.md
depends: [001-layer0-types]
---

# Layer 1: In-memory tree and property-based tests

## Goal

Build `MemTree<T>` — an in-memory tree that enforces all invariants at insertion time. Use proptest to verify 7 structural properties from ADR-0003. This is the reference implementation that streaming (Layer 3) must match.

## New files

```
tsugiki-core/src/
  mem_tree.rs
  bridge.rs
  properties.rs   (proptest strategies and properties)
```

Add `proptest` as a dev-dependency.

## `MemTree<T>` (mem_tree.rs)

```rust
pub struct MemTree<T> {
    nodes: HashMap<ShortId, Node<T>>,
    children: HashMap<ShortId, Vec<ShortId>>,  // ordered for Source/Target
    parent: HashMap<ShortId, ShortId>,
    root: Option<ShortId>,
}
```

### Operations

- `new() -> Self`
- `add_root(node: Node<T>) -> Result<()>` — must be `Depth::Document`, tree must be empty.
- `add_child(parent: ShortId, node: Node<T>) -> Result<()>` — checks `parent.depth.can_contain(node.depth)`, checks parent exists, checks no duplicate id.
- `get(&self, id: &ShortId) -> Option<&Node<T>>`
- `children(&self, id: &ShortId) -> &[ShortId]`
- `parent(&self, id: &ShortId) -> Option<&ShortId>`
- `walk_depth_first(&self) -> impl Iterator<Item = &Node<T>>` — pre-order DFS.
- `leaves(&self) -> impl Iterator<Item = &Node<T>>` — all `Depth::Leaf` nodes.

### `TreeWalk` trait

```rust
pub trait TreeWalk<T> {
    fn root(&self) -> Option<ShortId>;
    fn node(&self, id: &ShortId) -> Option<&Node<T>>;
    fn children(&self, id: &ShortId) -> Vec<ShortId>;
    fn parent(&self, id: &ShortId) -> Option<ShortId>;
}
```

`MemTree<T>` implements `TreeWalk<T>`.

## `BridgeSet` (bridge.rs)

```rust
pub struct BridgeSet<From, To> {
    forward: HashMap<ShortId, ShortId>,  // from → to
    reverse: HashMap<ShortId, ShortId>,  // to → from
}
```

- `add(from: ShortId, to: ShortId) -> Result<()>` — checks no duplicate mapping.
- `get_target(&self, from: &ShortId) -> Option<&ShortId>`
- `get_source(&self, to: &ShortId) -> Option<&ShortId>`

## Proptest strategies (properties.rs)

### Arbitrary tree generator

Strategy that builds a valid `MemTree<Source>` by:
1. Generate root with `Depth::Document`.
2. For each existing node, optionally add children with valid depth (one level deeper).
3. Recurse until `Depth::Leaf` or random stop.

Parameters: max nodes (10–100), branching factor (1–5).

### 7 properties

1. **Depth monotonicity**: for every parent-child pair, `parent.depth < child.depth`.
2. **Single parent**: every non-root node has exactly one parent.
3. **Bridge consistency**: if `source → structure` bridge exists, both sides exist in their trees.
4. **Leaf coverage**: every leaf in source tree has at most one bridge edge.
5. **Fountain roundtrip**: `parse(render(tree)) == tree` (needs Layer 2, stub with `#[ignore]` for now).
6. **CSV roundtrip**: `parse(render(edges)) == edges` (same, stub).
7. **Streaming equivalence**: `MemTree` operations produce same result as `FountainWalk` (stub, needs Layer 3).

Properties 5–7 are written as `#[ignore]` stubs that will be un-ignored when their layer is ready.

## Tests

- Unit test: build a small tree manually, verify walk order.
- Unit test: attempt invalid depth (Section under Leaf), expect error.
- Unit test: bridge set rejects duplicate.
- Proptest: properties 1–4 with generated trees (100 cases each).
- Proptest: two generated trees with random bridge, verify consistency.

## Done when

- `cargo test` passes all non-ignored tests.
- `cargo test -- --ignored` shows stubs for properties 5–7.
- `MemTree` is the authoritative reference for all invariants.
