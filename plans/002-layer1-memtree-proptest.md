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
    nodes: HashMap<NodeId, Node<T>>,
    children: HashMap<NodeId, Vec<NodeId>>,  // ordered for Source/Target
    parent: HashMap<NodeId, NodeId>,
    root: Option<NodeId>,
}
```

### Operations

- `new() -> Self`
- `add_root(node: Node<T>) -> Result<()>` — must be `Depth(0)`, tree must be empty.
- `add_child(parent: NodeId, node: Node<T>) -> Result<()>` — checks `parent.depth.can_contain(node.depth)`, checks parent exists, checks no duplicate id.
- `get(&self, id: &NodeId) -> Option<&Node<T>>`
- `children(&self, id: &NodeId) -> &[NodeId]`
- `parent(&self, id: &NodeId) -> Option<&NodeId>`
- `walk_depth_first(&self) -> impl Iterator<Item = &Node<T>>` — pre-order DFS.
- `leaves(&self) -> impl Iterator<Item = &Node<T>>` — all nodes with no children.

### `TreeWalk` trait

```rust
pub trait TreeWalk<T> {
    fn root(&self) -> Option<NodeId>;
    fn node(&self, id: &NodeId) -> Option<&Node<T>>;
    fn children(&self, id: &NodeId) -> Box<dyn Iterator<Item = NodeId> + '_>;
    fn parent(&self, id: &NodeId) -> Option<NodeId>;
}
```

`MemTree<T>` implements `TreeWalk<T>`.

## `BridgeSet` (bridge.rs)

```rust
pub struct BridgeSet<From, To> {
    forward: HashMap<NodeId, Vec<NodeId>>,  // from → [to, ...]
    reverse: HashMap<NodeId, Vec<NodeId>>,  // to → [from, ...]
}
```

- `add(from: NodeId, to: NodeId) -> Result<()>` — checks no duplicate pair.
- `get_targets(&self, from: &NodeId) -> &[NodeId]` — returns all target nodes for a source node (1:N for structure→target).
- `get_sources(&self, to: &NodeId) -> &[NodeId]` — returns all source nodes for a target node (N:1 for source→structure).

## Proptest strategies (properties.rs)

### Arbitrary tree generator

Strategy that builds a valid `MemTree<Source>` by:
1. Generate `height: u8` in range `2..5` (varying tree shapes — 2 is chapter+sentences, 4 is max Fountain).
2. Generate root with `Depth(height - 1)`.
3. For each existing node, optionally add children at depth - 1.
4. Recurse until Depth 0 (sentence level) reached.

Parameters: height (2–4), max nodes (10–100), branching factor (1–5).

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
- Unit test: attempt invalid depth (Depth(0) under Depth(3)), expect error.
- Unit test: bridge set rejects duplicate.
- Proptest: properties 1–4 with generated trees (100 cases each).
- Proptest: two generated trees with random bridge, verify consistency.

## Done when

- `cargo test` passes all non-ignored tests.
- `cargo test -- --ignored` shows stubs for properties 5–7.
- `MemTree` is the authoritative reference for all invariants.
