---
title: "Layer 0: Core types and depth invariants"
status: active
layer: 0
adr: decisions/0003-typed-tree-model-in-rust.md
depends: []
---

# Layer 0: Core types and depth invariants

## Goal

Define the foundational types that make illegal tree states unrepresentable. No I/O, no serialization — pure types, constructors, and compile-time guarantees.

## Crate setup

Create `tsugiki-core` as a library crate (workspace member or standalone). No dependencies beyond `std` and `uuid`.

```
tsugiki-core/
  src/
    lib.rs
    depth.rs
    node.rs
    tree_kind.rs
    edge.rs
    error.rs
  Cargo.toml
```

## Types to implement

### `Depth` newtype (depth.rs)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Depth(pub u8);
```

Depth counts upward from the sentence level. Depth 0 is always the sentence — the leaf unit of translation. Grouping levels above increment: Depth 1 is the first grouping level (paragraph or chapter, depending on the text), Depth 2 the next, etc. The number of levels is intent-specific. Fountain supports at most 4 levels (3 heading levels + action blocks). Texts requiring more are split into separate intents.

- `Depth::child(&self) -> Option<Depth>` — returns `Some(Depth(self.0 - 1))` if `self.0 > 0`, else `None`. Depth 0 nodes (sentences) cannot have children.
- `Depth::can_contain(&self, child: &Depth) -> bool` — true when `self.0 > 0 && child.0 == self.0 - 1`.
- `Depth::is_sentence(&self) -> bool` — true when `self.0 == 0`.
- No `is_root` — the root depth depends on the intent's tree height, not a fixed constant.
- No `Depth::MAX` constant — the maximum depth is determined by the Fountain file's heading levels (at most 3 for `#`/`##`/`###`).
- `Depth::from_fountain_level(heading_hashes: usize, max_heading_hashes: usize) -> Depth` — maps a Fountain heading level to a depth. `max_heading_hashes` is the number of `#` in the shallowest heading in the file. `heading_hashes == 0` (action block) always maps to Depth 0. `heading_hashes == max_heading_hashes` maps to the root depth.

### Tree kind markers (tree_kind.rs)

```rust
pub struct Source;
pub struct Structure;
pub struct Target;
```

Zero-sized types used as phantom type parameters.

### `NodeId` (node.rs)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(pub uuid::Uuid);
```

- `NodeId::new() -> NodeId` — generates v4 UUID.
- `NodeId::prefix(&self, len: usize) -> String` — first `len` hex chars of the UUID. Used only by the Fountain serialization layer.
- `NodeId::matches_prefix(&self, prefix: &str) -> bool` — true if the UUID's hex starts with the given prefix. Used for lookup by short ID.

Short IDs are not a core type. They are a Fountain display format. All internal data structures and CSVS tablets use full `NodeId`. The truncation length is determined at Fountain write time and auto-detected at Fountain read time.

### `Node<T>` (node.rs)

```rust
pub struct Node<T> {
    pub id: NodeId,
    pub depth: Depth,
    pub text: String,
    pub notes: Vec<Note>,
    _tree: PhantomData<T>,
}
```

Notes distinguish understand vs regrow phase via leading `|`:

```rust
pub enum NotePhase { Understand, Regrow }
pub struct Note { pub phase: NotePhase, pub text: String }
```

Smart constructor:

```rust
impl<T> Node<T> {
    pub fn new(id: NodeId, depth: Depth, text: String) -> Self { ... }
}
```

No public field mutation for `depth` — once created, depth is fixed.

### `Edge` types (edge.rs)

```rust
/// Within-tree containment edge (parent → child).
pub struct ContainmentEdge<T> {
    pub parent: NodeId,
    pub child: NodeId,
    _tree: PhantomData<T>,
}

/// Cross-tree bridge edge.
pub struct BridgeEdge<From, To> {
    pub from: NodeId,
    pub to: NodeId,
    _from: PhantomData<From>,
    _to: PhantomData<To>,
}
```

Bridge edges are typed: `BridgeEdge<Source, Structure>` and `BridgeEdge<Structure, Target>`.

### Error type (error.rs)

```rust
pub enum TreeError {
    DepthViolation { parent: Depth, child: Depth },
    OrphanNode(NodeId),
    DuplicateId(NodeId),
    MissingBridge { source: NodeId },
    ParseWarning(String),
}
```

## Tests (unit, no proptest yet)

1. `Depth` ordering: `Depth(0) < Depth(1) < Depth(5)`.
2. `can_contain`: `Depth(2)` can contain `Depth(1)`, not `Depth(0)` or `Depth(2)`. `Depth(0)` cannot contain anything.
3. `NodeId::prefix(8)` returns 8 hex chars, `matches_prefix` roundtrips.
4. `Node` phantom type prevents mixing: `Node<Source>` cannot be assigned to `Node<Structure>` (compile-time, not runtime test — a doc-test showing the compiler error).
5. `BridgeEdge` direction: `BridgeEdge<Source, Structure>` compiles, reversed order is a different type.

## Done when

- `cargo check` passes with no warnings.
- `cargo test` passes all unit tests.
- Types are `pub` and re-exported from `lib.rs`.
- No I/O, no file system, no async anywhere in this layer.
