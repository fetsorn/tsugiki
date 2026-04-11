---
status: accepted
date: 2026-04-11
---

# Typed tree model and streaming CLI in Rust

## Context and Problem Statement

The three-tree data model (ADR-0001) and the translation workflow (ADR-0002) describe source, structure, and target trees connected by bridges, stored as Fountain files and CSVS tablets. A prototype exists as Python scripts. Three problems grow worse with scale and tool diversity.

### Implicit invariants

The trees have structural invariants that are never enforced by code:

- Each tree has a depth ordering where a child's depth is exactly one level deeper than its parent's. The number of levels varies by intent — a letter might have three, a monograph chapter four. The system enforces the relative invariant (child = parent - 1 in depth), not a fixed set of named levels.
- The source and structure trees are isomorphic: every source node has exactly one structure node at the same depth, and vice versa (structural fidelity, ADR-0001).
- Cross-tree bridges (source→structure, structure→target) connect nodes of the same depth. A sentence-level source node maps to a sentence-level structure node, never to a section-level one.
- Source and target trees have ordered siblings (file order matters). The structure tree has unordered siblings.

When an AI session or a script violates these invariants, nothing catches it. The error propagates silently into the CSVS tablets and Fountain files.

### Session continuity

An annotate session walks the structure tree node by node. When a session ends, the next session needs to pick up where the last one stopped. The position is derived from the file: the first structure node that still has placeholder text. No cursor file is needed, and storing one introduces synchronization problems.

### Source text parsing

Source texts arrive as markdown. The parser must extract structure (headings, paragraphs, sentences), footnotes (`[^N]` references and definitions), and produce well-formed trees. Parsing errors must be caught before they enter the tree, not after they propagate through three trees and seven tablets.

Plain text input is not supported. Recovering footnotes from unformatted prose is impossible, and recovering structure is unreliable. The parser's input contract is well-formed markdown.

## Decision

### Core types

**Depth** is a newtype over `u8`. Depth counts upward from the sentence level: sentences are always Depth 0, and grouping levels above them increment. A text with sentences grouped only into sections has two levels (0 and 1). A text with sentences in paragraphs in sections has three (0, 1, 2). The invariant is relative: a child's depth is exactly one less than its parent's.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Depth(pub u8);

impl Depth {
    pub fn is_sentence(&self) -> bool { self.0 == 0 }

    pub fn child(&self) -> Option<Depth> {
        if self.0 > 0 { Some(Depth(self.0 - 1)) } else { None }
    }

    pub fn can_contain(&self, child: &Depth) -> bool {
        self.0 > 0 && child.0 == self.0 - 1
    }
}
```

**Depth from Fountain markers.** The parser reads the Fountain file and assigns depth based on the heading levels actually present. If the file uses `#`, `##`, and action blocks, it assigns `#` → Depth 2, `##` → Depth 1, action → Depth 0. If the file uses `#`, `##`, `###`, and action blocks: `#` → Depth 3, `##` → Depth 2, `###` → Depth 1, action → Depth 0. The mapping is determined by counting the distinct heading levels in the file and assigning them top-down.

The maximum is four levels (`#` + `##` + `###` + action). If a text requires more depth, the translator splits it into separate intents (ADR-0001).

**Short ID length.** The length of the hex prefix used in Fountain `[[hex]]` notes is configurable. The default is 8 hex characters. For large intents, a longer prefix reduces collision probability. The length is set at glean time (`--short-len N` flag, default 8) and auto-detected by subsequent operations from the first `[[hex]]` note in the file. Internally, all data structures and CSVS tablets use full v4 UUIDs. Short IDs are a Fountain serialization concern only.

**Tree kind** is encoded as zero-sized phantom types so the compiler distinguishes source, structure, and target:

```rust
pub struct Source;
pub struct Structure;
pub struct Target;

pub trait TreeKind {
    type Order: SiblingOrder;
}
impl TreeKind for Source    { type Order = Ordered; }
impl TreeKind for Structure { type Order = Unordered; }
impl TreeKind for Target    { type Order = Ordered; }
```

**Node** carries its tree kind as a phantom type parameter and its depth as a runtime value:

```rust
pub struct Node<T: TreeKind> {
    uuid: Uuid,
    depth: Depth,
    notes: Vec<Note>,
    _tree: PhantomData<T>,
}
```

**Note** is a translator annotation attached to a structure node:

```rust
pub enum NotePhase { Annotate, Regrow }

pub struct Note {
    pub phase: NotePhase,
    pub text: String,
}
```

In Fountain, annotate-phase notes are `[[text]]`. Regrow-phase notes are `[[| text]]` (pipe prefix). Both are Fountain notes, invisible in standard renderers.

**Containment edge** validates the depth invariant at construction:

```rust
pub struct ContainmentEdge<T: TreeKind> {
    parent: Uuid,
    child: Uuid,
    _tree: PhantomData<T>,
}

impl<T: TreeKind> ContainmentEdge<T> {
    pub fn new(parent: &Node<T>, child: &Node<T>) -> Result<Self, TreeError> {
        if parent.depth.can_contain(&child.depth) {
            Ok(ContainmentEdge { parent: parent.uuid, child: child.uuid, _tree: PhantomData })
        } else {
            Err(TreeError::InvalidDepth { parent_depth: parent.depth, child_depth: child.depth })
        }
    }
}
```

**Bridge edge** validates depth matching at construction:

```rust
pub struct BridgeEdge<From: TreeKind, To: TreeKind> {
    from: Uuid,
    to: Uuid,
    _marker: PhantomData<(From, To)>,
}

impl<F: TreeKind, T: TreeKind> BridgeEdge<F, T> {
    pub fn new(from: &Node<F>, to: &Node<T>) -> Result<Self, TreeError> {
        if from.depth != to.depth {
            return Err(TreeError::DepthMismatch { from_depth: from.depth, to_depth: to.depth });
        }
        Ok(BridgeEdge { from: from.uuid, to: to.uuid, _marker: PhantomData })
    }
}

// Only these two bridge types are defined:
pub type SourceStructureBridge = BridgeEdge<Source, Structure>;
pub type StructureTargetBridge = BridgeEdge<Structure, Target>;
```

**Footnote reference edge** connects a sentence to its footnote block within the same tree. No depth constraint — the sentence and the footnote block live in different sections of the Fountain file.

```rust
pub struct FootnoteEdge<T: TreeKind> {
    sentence: Uuid,
    footnote: Uuid,
    _tree: PhantomData<T>,
}
```

### The TreeWalk trait

All tree traversal goes through a single trait, implementable by both in-memory trees (for tests) and streaming file walkers (for CLI):

```rust
pub trait TreeWalk<T: TreeKind> {
    fn root(&self) -> Uuid;
    fn children(&self, node: Uuid) -> Result<Box<dyn Iterator<Item = Uuid> + '_>, TreeError>;
    fn parent(&self, node: Uuid) -> Result<Option<Uuid>, TreeError>;
    fn depth(&self, node: Uuid) -> Result<Depth, TreeError>;
    fn content(&self, node: Uuid) -> Result<Option<String>, TreeError>;
    fn next_sibling(&self, node: Uuid) -> Result<Option<Uuid>, TreeError>;
    fn leaves(&self) -> Result<Box<dyn Iterator<Item = Uuid> + '_>, TreeError>;
}
```

Boxed iterators are the pragmatic choice for trait-object compatibility. The allocation is negligible at translation scale.

**In-memory implementation** (`MemTree<T>`) holds `HashMap`s of nodes, children, and content. Used in tests and for small trees.

**Streaming implementation** (`FountainWalk<T>`) holds file paths to a Fountain file and a CSVS containment tablet. Each method issues a targeted grep:

- `children(uuid)`: grep the CSV for lines starting with the UUID.
- `parent(uuid)`: grep the CSV for lines ending with the UUID.
- `content(uuid)`: grep the Fountain file for `[[{short_uuid}]]`, extract the text.
- `depth(uuid)`: grep the Fountain file for `[[{short_uuid}]]`, check heading level.
- `next_sibling(uuid)`: call `parent(uuid)`, then `children(parent)`, find position, return next.

No method loads the entire file. No line index is cached — every method is a stateless grep, deriving position fresh from the file each time. This eliminates synchronization bugs when files are modified between calls.

### Bridge operations

```rust
pub trait BridgeWalk<From: TreeKind, To: TreeKind> {
    fn bridge_targets(&self, from: Uuid) -> Result<Vec<Uuid>, TreeError>;
    fn bridge_sources(&self, to: Uuid) -> Result<Vec<Uuid>, TreeError>;
    fn is_mapped(&self, from: Uuid) -> Result<bool, TreeError>;
    fn unmapped_leaves(
        &self, tree: &dyn TreeWalk<From>,
    ) -> Result<Box<dyn Iterator<Item = Uuid> + '_>, TreeError>;
}
```

Bridge methods return `Vec` because a single structure node can produce multiple target sentences (1:N in `structure-target.csv`).

**Streaming implementation** (`CsvBridgeWalk<From, To>`) holds the path to the bridge CSV. Each method is a grep.

### Parser contract

A parser converts a markdown source document into a source tree and a structure skeleton. The input contract is well-formed markdown.

```rust
pub struct ParseResult {
    pub source_nodes: Vec<Node<Source>>,
    pub source_edges: Vec<ContainmentEdge<Source>>,
    pub structure_nodes: Vec<Node<Structure>>,
    pub structure_edges: Vec<ContainmentEdge<Structure>>,
    pub bridges: Vec<SourceStructureBridge>,
    pub footnote_edges: Vec<FootnoteEdge<Source>>,
    pub content: HashMap<Uuid, String>,
    pub warnings: Vec<ParseWarning>,
}

pub enum ParseWarning {
    AmbiguousSplit { before: String, after: String, position: usize },
    EmptyContent { uuid: Uuid },
}
```

The `ParseResult` is validated before it becomes a pair of trees:

```rust
pub fn validate_parse(result: &ParseResult) -> Result<(), Vec<TreeError>> {
    // Check: exactly one root per tree
    // Check: every edge satisfies depth ordering
    // Check: every non-root node has exactly one parent
    // Check: no cycles
    // Check: all UUIDs in edges appear in nodes
    // Check: source-structure bridge is 1:1 (structural fidelity)
    // Check: bridge edges connect nodes of matching depth
    // Check: every footnote reference and definition are paired
}
```

Warnings do not block tree construction — they are presented to the translator for review. Errors do.

### Property-based tests

The following properties are tested with `proptest` using randomly generated trees:

1. **Depth monotonicity**: for every containment edge, child depth = parent depth - 1.
2. **Single parent**: every non-root node has exactly one parent.
3. **Bridge depth consistency**: for every bridge edge, from.depth == to.depth.
4. **Structural fidelity**: source-structure bridge is 1:1 at every depth level.
5. **Fountain roundtrip**: `parse(render(tree)) ≅ tree` (structural equality, ignoring whitespace).
6. **CSV roundtrip**: `parse_csv(render_csv(edges)) == edges`.
7. **Streaming equivalence**: for any tree, `MemTree` and `FountainWalk` return the same results for all `TreeWalk` methods.

Random tree generation respects the depth ordering:

```rust
fn arb_tree<T: TreeKind>(height: u8) -> impl Strategy<Value = MemTree<T>> {
    // Generate a root at Depth(height - 1).
    // For each depth going down to 0, generate children (1..8 branching factor).
    // Leaves are at Depth(0). Content is arbitrary non-empty strings.
    // height parameter ranges 2..5 to test varying tree shapes.
}
```

### CLI commands

| Command                            | Implementation                          | Reads                    | Writes                   |
|------------------------------------|-----------------------------------------|--------------------------|--------------------------|
| `tsugiki glean <source.md>`        | parse markdown into source + structure  | markdown file            | source.fountain + structure.fountain + 4 CSVs |
| `tsugiki next [--dir <dir>]`       | scan for first unannotated/untranslated | structure.fountain       | nothing                  |
| `tsugiki annotate <addr> "<text>"` | write annotation into structure         | structure.fountain       | structure.fountain       |
| `tsugiki regrow <addr> "<text>"`   | create target node, write bridge        | structure + bridge CSVs  | target.fountain + 2 CSVs |
| `tsugiki reset annotate`           | archive and regenerate skeleton         | source tree              | structure.fountain       |
| `tsugiki reset regrow`             | archive and clear target artifacts      | nothing                  | target.fountain + 3 CSVs |
| `tsugiki render <tree>`            | strip Fountain to clean markdown        | fountain + footnote CSV  | .md file                 |
| `tsugiki status`                   | count mapped vs unmapped nodes          | bridge CSVs + fountain   | nothing                  |
| `tsugiki check`                    | check all invariants                    | all CSVs + all fountains | nothing                  |

The `<addr>` parameter accepts a line number, a short hex ID, or a full UUID. Line numbers are resolved against the current file state and never persisted.

## Consequences

- Every tree operation goes through a validated constructor. Depth mismatches, malformed edges, and invalid bridges become construction-time errors, not silent data corruption.
- The `TreeWalk` trait abstracts over storage. Tests use `MemTree`; the CLI uses `FountainWalk`. Both satisfy the same properties, verified by proptest.
- Parsers are pluggable modules that produce a `ParseResult` containing both source and structure trees (structural fidelity enforced at parse time). The system validates the result, flags warnings, and requires human approval for ambiguous cases.
- The streaming design means the CLI never loads a full file. Every operation is a stateless grep — no cached line index, no stale state.
- Property-based tests provide confidence that invariants hold for arbitrary trees. Regressions in parser or tree construction code are caught automatically.
- The type system prevents passing a source UUID to a function expecting a target UUID, constructing a bridge between source and target directly (bypassing structure), or creating a containment edge between nodes of incompatible depths.
