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

### `Depth` enum (depth.rs)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Depth {
    Document,  // #
    Section,   // ##
    Paragraph, // ###
    Leaf,      // no marker
}
```

- Derive `Ord` so that `Document < Section < Paragraph < Leaf`.
- `Depth::from_fountain(s: &str) -> Option<Depth>` — parses `"#"`, `"##"`, `"###"`, bare.
- `Depth::to_fountain(&self) -> Option<&'static str>` — inverse. `Leaf` returns `None`.
- `Depth::can_contain(&self, child: &Depth) -> bool` — each depth can contain only the next level. `Leaf` contains nothing.

### Tree kind markers (tree_kind.rs)

```rust
pub struct Source;
pub struct Structure;
pub struct Target;
```

Zero-sized types used as phantom type parameters.

### `NodeId` and `ShortId` (node.rs)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(pub uuid::Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShortId([u8; 4]); // first 4 bytes = 8 hex chars
```

- `NodeId::short(&self) -> ShortId`
- `ShortId::to_hex(&self) -> String` — 8 lowercase hex chars.
- `NodeId::from_short_hex(s: &str) -> Result<ShortId>` — parse 8 hex chars.
- `NodeId::new() -> NodeId` — generates v4 UUID.

### `Node<T>` (node.rs)

```rust
pub struct Node<T> {
    pub id: NodeId,
    pub depth: Depth,
    pub text: String,
    pub note: Option<String>, // parenthetical
    _tree: PhantomData<T>,
}
```

Smart constructor:

```rust
impl<T> Node<T> {
    pub fn new(id: NodeId, depth: Depth, text: String) -> Self { ... }
    pub fn leaf(id: NodeId, text: String) -> Self { ... }
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
    DuplicateId(ShortId),
    MissingBridge { source: NodeId },
    ParseWarning(String),
}
```

## Tests (unit, no proptest yet)

1. `Depth` ordering: `Document < Section < Paragraph < Leaf`.
2. `can_contain`: `Document` can contain `Section`, not `Paragraph`. `Leaf` contains nothing.
3. `ShortId` roundtrip: `NodeId::new().short().to_hex()` is 8 chars, parse back succeeds.
4. `Node` phantom type prevents mixing: `Node<Source>` cannot be assigned to `Node<Structure>` (compile-time, not runtime test — a doc-test showing the compiler error).
5. `BridgeEdge` direction: `BridgeEdge<Source, Structure>` compiles, reversed order is a different type.

## Done when

- `cargo check` passes with no warnings.
- `cargo test` passes all unit tests.
- Types are `pub` and re-exported from `lib.rs`.
- No I/O, no file system, no async anywhere in this layer.
