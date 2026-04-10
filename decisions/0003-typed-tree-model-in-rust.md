---
status: proposed
date: 2026-04-10
---

# Typed tree model and streaming CLI in Rust

## Context and Problem Statement

The three-tree data model (ADR-0001) and the translation workflow (ADR-0002) describe a structure of three trees — source, structure, target — connected by cross-tree bridges, stored as Fountain files and CSVS tablets. The current implementation is a set of Python scripts that load entire files into memory, build dictionaries, and perform lookups. This works for a prototype but creates three problems that grow worse with scale and tool diversity.

### Problem 1: Implicit invariants

The three trees have structural invariants that are never enforced by code:

- Each tree has a depth ordering where a child's depth is exactly one level deeper than its parent's. The number of levels varies by intent — a letter might have four (document, section, paragraph, sentence), a monograph chapter five (adding footnotes), a book seven (adding parts and subchapters). The system enforces the relative invariant (child = parent + 1), not a fixed set of named levels.
- Cross-tree bridges (source→structure, structure→target) must connect nodes of the same depth. A sentence-level source node maps to a sentence-level structure node, never to a section-level one.
- Source and target trees have ordered siblings (file order matters). The structure tree has unordered siblings (file order is convenience).
- Every source leaf should eventually have exactly one structure bridge. Every structure leaf should have zero or more target bridges.

These invariants live in ADR prose and in the human's mental model. When an AI session or a script violates them — misnumbering a depth level, creating a bridge between mismatched depths — nothing catches it. The error propagates silently into the CSVS tablets and Fountain files, where it becomes difficult to diagnose later.

### Problem 2: Session continuity

An AI-assisted understanding session (Phase 2) walks the source tree node by node. When a session ends, the next session — whether with the same AI, a different AI, or a standalone CLI — needs to pick up exactly where the last one stopped. This requires knowing:

- Which source node was last annotated (a cursor position in the tree).
- Which source nodes still lack structure annotations (a coverage query).
- The parent chain of the current node (paragraph, section, document) for context.
- The text of the next source node to present to the translator.

Currently, the AI reads entire Fountain files and CSVs to reconstruct this state. This is expensive in tokens and fragile across sessions. A new session that reads differently, or an AI that interprets the data model differently, may lose the thread.

### Problem 3: Source text parsing

Source texts arrive in many formats — docx, pdf, markdown, plain text. Each format has its own parsing challenges. The current decompose script handles Russian academic prose with a heuristic sentence splitter that protects abbreviations and initials. But different source languages, genres, and formats need different parsers.

The deeper issue is that parsing errors are currently silent. If the sentence splitter incorrectly breaks "Дж. Тарелло" into two sentences, the resulting source tree has a malformed node. Nothing in the system catches this because the tree invariants are not checked. The parse error propagates into the structure tree (creating a bridge to a non-sentence) and eventually into the target tree.

What is needed is not a perfect parser — parsing natural language will always be approximate — but a system that knows what guarantees it needs from a parser, can validate those guarantees against declared types, asks the human for approval when guarantees cannot be met, and catches violations early rather than letting them propagate through three trees and five CSVS tablets.

## Decision Drivers

- **Invariants must be enforced, not documented.** If a depth mismatch can be caught by the type system or a smart constructor, it should be. If it can only be caught at runtime, it should be caught immediately on construction, not during a later traversal.
- **Streaming over loading.** Fountain files and CSVS tablets are line-oriented. Operations on them — find next sibling, read node content, check if mapped, append an annotation — should be line-oriented too. No operation in the normal workflow requires loading an entire file.
- **Session state is a cursor, not a snapshot.** Resuming work means knowing one UUID and being able to walk from there. The tool should store and restore a cursor, not require re-reading the entire tree.
- **Parsers are fallible and pluggable.** The system must define what a valid parse result looks like (a well-formed tree with correct depth ordering), accept parse results that meet this definition, reject or flag those that don't, and allow different parsers for different source formats.
- **Testable with property-based tests.** The invariants should be expressible as properties that a test framework (proptest in Rust) can check against randomly generated trees and bridges.
- **Readable by a human implementor.** This decision must contain enough detail for someone — human or AI — to scaffold the Rust crate without access to the conversation that produced it.

## Decision

### Core types

The tree model is expressed as Rust types with enforcement at construction time.

**Depth** is a newtype over `u8`. The number of depth levels is a property of the intent, not the system. A letter might have four levels (document, section, paragraph, sentence). A monograph chapter might have five (adding footnotes). A book with parts and subchapters could have seven. The invariant is relative: a child is exactly one level deeper than its parent.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Depth(pub u8);

impl Depth {
    pub const MAX: Depth = Depth(3);

    /// The depth of a child node, if this depth can have children.
    /// Depth 3 (sentence) is always a leaf — returns None.
    pub fn child(&self) -> Option<Depth> {
        if self.0 < 3 { Some(Depth(self.0 + 1)) } else { None }
    }

    /// Whether this depth can contain the given child depth.
    pub fn can_contain(&self, child: &Depth) -> bool {
        child.0 == self.0 + 1 && child.0 <= 3
    }

    /// Whether this is the root depth.
    pub fn is_root(&self) -> bool {
        self.0 == 0
    }

    /// Whether this is the leaf (sentence) depth.
    pub fn is_leaf(&self) -> bool {
        self.0 == 3
    }

    /// Returns the Fountain section marker for this depth, if any.
    /// Depth 0 → `#`, Depth 1 → `##`, Depth 2 → `###`.
    /// Depth 3 (sentence) has no marker — it is an action block.
    pub fn fountain_marker(&self) -> Option<&'static str> {
        match self.0 {
            0 => Some("#"),
            1 => Some("##"),
            2 => Some("###"),
            _ => None,
        }
    }
}
```

Fountain has exactly four levels: `#` (Depth 0), `##` (Depth 1), `###` (Depth 2), and unmarked action blocks (Depth 3). This is a fundamental limit, not a workaround. The deepest level (Depth 3, no hash) is always the sentence — the leaf unit of translation. The shallowest level (Depth 0, `#`) is always the intent root. The two intermediate levels (`##`, `###`) accommodate the text's natural structure: for a letter, section and paragraph; for a monograph chapter, section and paragraph; for a poem, stanza and line.

If a text requires more than four levels of structural depth — a book with parts, chapters, sections, paragraphs, and sentences — the translator splits it into separate intents. A book becomes one intent per chapter (or per part, depending on the translator's judgment). This is not a limitation but a discipline: if the rhetorical structure cannot be held in four levels, the scope of the translation unit is too large for one pass.

The system enforces: max depth is 3, child depth = parent depth + 1, and Depth 3 nodes are always leaves.

**Texts with fewer than four natural levels.** A text with only two natural levels (document + sentences) still produces a four-level tree. The intermediate levels (Depth 1 and Depth 2) are synthetic single-child nodes. Synthetic nodes appear in CSVS containment tablets (so the depth invariant holds everywhere) but are omitted from the Fountain file to avoid clutter. The Fountain parser infers synthetic nodes when it encounters a depth gap — e.g., `#` (Depth 0) followed directly by an action block (Depth 3). Synthetic UUIDs are derived deterministically via UUID v5 from the parent UUID and the depth level, so they are stable across re-parses and do not require explicit tracking.

The length of the short ID prefix used in Fountain files is configurable. The default is 8 hex characters (4 bytes). For large intents (hundreds of nodes), a longer prefix reduces collision probability. The length is set once, when `tsugiki decompose` creates the Fountain file (`--short-len N` flag, default 8). All subsequent operations auto-detect the length from the existing Fountain file by reading the first `.` line. No configuration file is needed — the Fountain file itself is the record of the chosen length.

Internally, all data structures and CSVS tablets use full v4 UUIDs. Short IDs are a Fountain serialization concern only, not a core type. The `NodeId` type wraps a full UUID. Truncation happens at the Fountain render boundary; lookup by prefix happens at the Fountain parse boundary.

**Tree kind** is encoded as zero-sized types (phantom types) so the compiler distinguishes source, structure, and target at the type level:

```rust
pub struct Source;
pub struct Structure;
pub struct Target;
```

**Sibling ordering** is encoded as a trait bound. Source and target trees have ordered siblings; structure trees do not:

```rust
pub trait SiblingOrder {}
pub struct Ordered;
pub struct Unordered;
impl SiblingOrder for Ordered {}
impl SiblingOrder for Unordered {}

// Source and Target are Ordered; Structure is Unordered.
// This is expressed through the TreeKind trait:
pub trait TreeKind {
    type Order: SiblingOrder;
}
impl TreeKind for Source    { type Order = Ordered; }
impl TreeKind for Structure { type Order = Unordered; }
impl TreeKind for Target    { type Order = Ordered; }
```

**Note** is a translator annotation attached to a structure node. Notes from the understand phase and regrow phase are distinguished by a leading `|` inside the parenthetical:

```
(this node introduces the standard description)
(| split into two target sentences, second carries the footnote)
```

The first form (no prefix) is an understand-phase note. The second form (`| ` prefix) is a regrow-phase note. The pipe character at position 0 inside a parenthetical is reserved; translator input containing a leading pipe is escaped or rejected.

```rust
pub enum NotePhase {
    Understand,
    Regrow,
}

pub struct Note {
    pub phase: NotePhase,
    pub text: String,
}
```

**No-collapse rule for single-sentence blocks.** When a block contains only one sentence, the structure tree still has two nodes: one at the block depth and one at the sentence depth. The sentence-level annotation uses the `=` prefix to indicate "same intent as parent":

```
.f3a4b5c6
## polite supplication

.d7e8f9a0
= polite supplication
```

The `=` prefix at position 0 of a structure annotation means "this sentence's rhetorical intent is identical to the containing block's intent." This preserves the bridge depth-matching invariant: the source sentence (Depth 3) maps to a structure sentence (Depth 3), not to a structure block (Depth 2). The `=` prefix is a third reserved character in annotation space, alongside `|` for regrow-phase notes and `(` for parentheticals.

This rule makes every bridge depth-consistent and every intent explicit. The cost is one extra node per single-sentence block. The benefit is that the type system never needs to handle depth mismatches in bridges.

**Open question: consecutive parentheticals in Fountain.** The Fountain spec defines parentheticals as `(text)` on their own line between dialogue lines. Tsugiki uses parentheticals outside dialogue context (attached to structure nodes in action blocks). It is unclear whether Fountain parsers treat two consecutive `(...)` lines as a single parenthetical or two separate ones. This must be tested against the Rust Fountain crates (`fountain-rs`, `rustwell`, `fountain-parser-rs`, `lottie/fount`) during Layer 2 implementation. If parsers merge consecutive parentheticals, we may need a different encoding — possibly a single parenthetical with `|` as an internal phase separator between understand and regrow text.

**Node** carries its tree kind as a phantom type parameter and its depth as a runtime value:

```rust
pub struct Node<T: TreeKind> {
    uuid: Uuid,
    depth: Depth,
    notes: Vec<Note>,
    _tree: PhantomData<T>,
}
```

**Containment edge** (parent-child within one tree) is constructed through a function that validates the depth invariant:

```rust
pub struct ContainmentEdge<T: TreeKind> {
    parent: Uuid,
    child: Uuid,
    _tree: PhantomData<T>,
}

impl<T: TreeKind> ContainmentEdge<T> {
    pub fn new(
        parent: &Node<T>,
        child: &Node<T>,
    ) -> Result<Self, TreeError> {
        if parent.depth.can_contain(&child.depth) {
            Ok(ContainmentEdge {
                parent: parent.uuid,
                child: child.uuid,
                _tree: PhantomData,
            })
            }
        } else {
            Err(TreeError::InvalidDepth {
                parent_depth: parent.depth,
                child_depth: child.depth,
            })
        }
    }
}
```

**Bridge edge** (cross-tree) is similarly constructed with a depth-match check:

```rust
pub struct BridgeEdge<From: TreeKind, To: TreeKind> {
    from: Uuid,
    to: Uuid,
    _marker: PhantomData<(From, To)>,
}

impl<F: TreeKind, T: TreeKind> BridgeEdge<F, T> {
    pub fn new(
        from: &Node<F>,
        to: &Node<T>,
    ) -> Result<Self, TreeError> {
        if from.depth != to.depth {
            return Err(TreeError::DepthMismatch {
                from_depth: from.depth,
                to_depth: to.depth,
            });
        }
        Ok(BridgeEdge {
            from: from.uuid,
            to: to.uuid,
            _marker: PhantomData,
        })
    }
}
```

Note: the type system prevents constructing `BridgeEdge<Source, Target>` if no such type alias is defined. Only `BridgeEdge<Source, Structure>` and `BridgeEdge<Structure, Target>` should be aliased:

```rust
pub type SourceStructureBridge = BridgeEdge<Source, Structure>;
pub type StructureTargetBridge = BridgeEdge<Structure, Target>;
```

### The TreeWalk trait

All tree traversal goes through a single trait, implementable by both in-memory trees (for tests) and streaming file walkers (for CLI):

```rust
pub trait TreeWalk<T: TreeKind> {
    fn root(&self) -> Uuid;
    fn children(&self, node: Uuid) -> Result<Vec<Uuid>, TreeError>;
    fn parent(&self, node: Uuid) -> Result<Option<Uuid>, TreeError>;
    fn depth(&self, node: Uuid) -> Result<Depth, TreeError>;
    fn content(&self, node: Uuid) -> Result<Option<String>, TreeError>;
    fn next_sibling(&self, node: Uuid) -> Result<Option<Uuid>, TreeError>;
    fn leaves(&self) -> Result<Vec<Uuid>, TreeError>;
}
```

**In-memory implementation** (`MemTree<T>`) holds `HashMap`s of nodes, children, and content. Used in tests and for small trees.

**Streaming implementation** (`FountainWalk<T>`) holds file paths to a Fountain file and a CSVS containment tablet. Each method issues a targeted read:

- `children(uuid)`: grep the CSV for lines starting with the UUID.
- `parent(uuid)`: grep the CSV for lines ending with the UUID.
- `content(uuid)`: grep the Fountain file for `.{short_uuid}`, read the next non-blank line.
- `depth(uuid)`: grep the Fountain file for `.{short_uuid}`, check if the next line starts with `#`, `##`, `###`, or none.
- `next_sibling(uuid)`: call `parent(uuid)`, then `children(parent)`, find position, return next.

No method loads the entire file. Each is O(file_size) in the worst case for a grep, but operates on a single file and reads only the matched lines.

### Bridge operations

```rust
pub trait BridgeWalk<From: TreeKind, To: TreeKind> {
    fn bridge_target(&self, from: Uuid) -> Result<Option<Uuid>, TreeError>;
    fn bridge_source(&self, to: Uuid) -> Result<Option<Uuid>, TreeError>;
    fn is_mapped(&self, from: Uuid) -> Result<bool, TreeError>;
    fn unmapped_leaves(
        &self,
        tree: &dyn TreeWalk<From>,
    ) -> Result<Vec<Uuid>, TreeError>;
}
```

**Streaming implementation** (`CsvBridgeWalk<From, To>`) holds the path to the bridge CSV (e.g., `source-structure.csv`). Each method is a grep.

### Session cursor

A session's position is a single UUID plus the tree it belongs to:

```rust
pub struct Cursor<T: TreeKind> {
    pub current: Uuid,
    _tree: PhantomData<T>,
}

impl<T: TreeKind> Cursor<T> {
    /// Advance to the next node in depth-first order.
    /// Returns None if the tree is exhausted.
    pub fn advance(&self, tree: &dyn TreeWalk<T>) -> Result<Option<Cursor<T>>, TreeError> {
        // Try first child
        let children = tree.children(self.current)?;
        if let Some(first) = children.first() {
            return Ok(Some(Cursor { current: *first, _tree: PhantomData }));
        }
        // Try next sibling
        if let Some(sib) = tree.next_sibling(self.current)? {
            return Ok(Some(Cursor { current: sib, _tree: PhantomData }));
        }
        // Walk up and try uncle
        let mut node = self.current;
        while let Some(parent) = tree.parent(node)? {
            if let Some(uncle) = tree.next_sibling(parent)? {
                return Ok(Some(Cursor { current: uncle, _tree: PhantomData }));
            }
            node = parent;
        }
        Ok(None) // exhausted
    }
}
```

The cursor is serializable as a single UUID. A new session reads it from a file (e.g., `.cursor`) and resumes.

### Parser contract

A parser converts a raw source document into a source tree. Different parsers handle different formats (docx, pdf, markdown, plain text) and languages. All parsers must satisfy the same output contract:

```rust
pub struct ParseResult {
    pub nodes: Vec<Node<Source>>,
    pub edges: Vec<ContainmentEdge<Source>>,
    pub content: HashMap<Uuid, String>,
    pub warnings: Vec<ParseWarning>,
}

pub enum ParseWarning {
    /// The parser was uncertain about a sentence boundary.
    AmbiguousSplit {
        before: String,
        after: String,
        position: usize,
    },
    /// A node's content was empty after cleaning.
    EmptyContent { uuid: Uuid },
    /// The parser encountered a structure it could not classify into a depth level.
    UnknownDepth { raw_text: String },
}
```

The `ParseResult` is validated before it becomes a source tree:

```rust
pub fn validate_parse(result: &ParseResult) -> Result<(), Vec<TreeError>> {
    let mut errors = vec![];

    // Check: exactly one root (Document depth, no parent)
    // Check: every edge satisfies depth ordering
    // Check: every node has exactly one parent (except root)
    // Check: no cycles
    // Check: all UUIDs in edges appear in nodes

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}
```

Warnings do not block tree construction — they are presented to the human for review. The human may approve the parse as-is, edit the source text to resolve ambiguities, or reject and re-parse. The tool records which warnings were reviewed and approved, so a later session does not re-raise them:

```rust
pub struct ParseApproval {
    pub warning_hash: String,  // hash of the warning content
    pub approved_by: String,   // "human" or session identifier
    pub date: String,
}
```

### Property-based tests

The following properties are tested with `proptest` using randomly generated trees:

1. **Depth monotonicity**: for every containment edge, `parent.depth.can_contain(&child.depth)` is true (i.e., child depth = parent depth + 1).
2. **Single parent**: every non-root node has exactly one parent.
3. **Bridge depth consistency**: for every bridge edge, `from.depth == to.depth`.
4. **Leaf coverage** (after Phase 2): every source leaf has a bridge to a structure node.
5. **Fountain roundtrip**: `parse(render(tree)) ≅ tree` (structural equality, ignoring UUIDs).
6. **CSV roundtrip**: `parse_csv(render_csv(edges)) == edges`.
7. **Streaming equivalence**: for any tree, `MemTree` and `FountainWalk` return the same results for all `TreeWalk` methods.

Random tree generation respects the depth ordering:

```rust
fn arb_tree<T: TreeKind>(max_depth: u8) -> impl Strategy<Value = MemTree<T>> {
    // Generate a root at Depth(0).
    // For each depth level 1..max_depth, generate children (1..8 branching factor).
    // Leaf nodes are at depth max_depth, optionally with children at max_depth+1.
    // Content is arbitrary non-empty strings.
    // max_depth is itself a parameter (3..7) to test varying tree shapes.
}
```

### CLI commands

The CLI is a thin wrapper around the trait methods:

| Command                          | Implementation                           | Reads                        | Writes    |
|----------------------------------|------------------------------------------|------------------------------|-----------|
| `tsugiki next <uuid>`            | `cursor.advance()`                       | grep CSV                     | nothing   |
| `tsugiki read <uuid>`            | `tree.content()`                         | grep fountain                | nothing   |
| `tsugiki mapped <uuid>`          | `bridge.is_mapped()`                     | grep bridge CSV              | nothing   |
| `tsugiki context <uuid>`         | `tree.parent()` × N                      | grep CSV + fountain          | nothing   |
| `tsugiki annotate <uuid> <text>` | construct node + edges, validate, append | nothing                      | append ×3 |
| `tsugiki status`                 | `bridge.unmapped_leaves()`               | grep bridge CSV + source CSV | nothing   |
| `tsugiki validate`               | `validate_parse()` on all trees          | read all CSVs                | nothing   |
| `tsugiki resume`                 | read cursor file                         | 1 file                       | nothing   |

The `annotate` command is the only write operation. It constructs a `Node<Structure>`, a `ContainmentEdge<Structure>`, and a `BridgeEdge<Source, Structure>` through the smart constructors, which validate depth matching. Then it appends one line to `structure.fountain` (after the paragraph heading — this is the one non-append operation, implemented as a streaming insertion), one line to `structure-child.csv`, and one line to `source-structure.csv`.

### Fountain insertion as streaming transformation

The `annotate` command's fountain insertion is expressed as an iterator adapter:

```rust
fn insert_after_paragraph(
    lines: impl Iterator<Item = String>,
    target_uuid: &str,
    new_lines: Vec<String>,
) -> impl Iterator<Item = String> {
    // Emit all lines until `.{target_uuid}` is found.
    // Emit the UUID line, the heading line, the blank line.
    // Emit new_lines.
    // Emit remaining lines.
}
```

This reads the fountain file line by line, writes to a temporary file, then replaces the original. Memory usage is O(new_lines), not O(file_size).

## Consequences

- The Rust crate becomes the canonical implementation of the three-tree model. Python scripts may continue to exist for prototyping but are not authoritative.
- Every tree operation goes through a validated constructor. Depth mismatches, malformed edges, and invalid bridges become compile-time or construction-time errors, not silent data corruption.
- The `TreeWalk` trait abstracts over storage. Tests use `MemTree`; the CLI uses `FountainWalk`. Both satisfy the same properties, verified by proptest.
- Parsers become pluggable modules that produce a `ParseResult`. The system validates the result, flags warnings, and requires human approval for ambiguous cases. Parse errors are caught before they enter the tree, not after they propagate through three trees and five tablets.
- Session continuity reduces to persisting a single UUID. A cursor file (or even a command-line argument) is sufficient to resume work.
- The streaming design means the CLI never loads a full file. Operations are proportional to the number of matched lines, not the total file size. This matters for book-length translations with thousands of nodes.
- Property-based tests provide confidence that the invariants hold for arbitrary trees, not just the specific trees we have tested manually. Regressions in parser or tree construction code are caught automatically.
- The type system prevents a class of errors that are currently possible: passing a source UUID to a function expecting a target UUID, constructing a bridge between source and target directly (bypassing structure), creating a containment edge between nodes of incompatible depths.
- Haskell-style dependent types (proving depth equality at the type level) are not available in Rust. The smart-constructor pattern with proptest backing is the chosen alternative. If a future version of Rust adds const generics expressive enough for this, the design can be tightened.

## Related work and scope

### Translation memory and CAT tools

Trados, MemoQ, OmegaT and similar tools segment source text into translation units (usually sentences), align them flat with target segments, and store the pairs in a database. They have no structure tree — alignment is source-sentence → target-sentence with no layer capturing what the sentence is doing. Segmentation is mechanical (SRX rules), not translator-guided. They cannot express that two source sentences map to one target sentence through a shared rhetorical intent. The demand in this space is enormous but the product is commodity. Tsugiki is not competing here; it addresses a layer these tools skip entirely.

### Rhetorical Structure Theory (RST)

RST is the academic formalism closest to the structure tree. RST parsers (Stede's work, the RST Discourse Treebank) and annotation tools (RSTTool, rstWeb) annotate text with rhetorical relations — elaboration, contrast, cause, concession. The overlap is that they build a tree of rhetorical intent over a text. The differences: RST is monolingual analysis (nobody uses RST as a bridge between source and target in translation); RST trees are relation-typed (each edge has a label like "elaboration"), while tsugiki's structure tree currently has untyped edges with intent captured in prose annotations and parentheticals. The demand is academic — RST is well-cited but has few practitioners and no commercial tools. The relevant insight: the structure tree's shape (depth = document/section/paragraph/sentence with "expressed through" semantics) is independently motivated by 40 years of discourse analysis research. Tsugiki rediscovered this shape from translation practice; RST arrived at it from linguistics.

### Interlinear glossed text (IGT)

Field linguists decompose source text into morphemes, gloss each morpheme, then produce a free translation. This gives three aligned layers — source, gloss, target — but the gloss layer is mechanical (morpheme-by-morpheme), not rhetorical. Alignment is at the word/morpheme level, not the sentence/paragraph level. Tools like SIL FLEx and Toolbox serve a real community. The overlap is the three-layer architecture. The difference is that tsugiki's middle layer captures intent, not morphology.

### Screenwriting and the Fountain ecosystem

Fountain format is used by Highland, Fade In, and open-source tools. Tsugiki reuses Fountain's tokens (forced scene headings for UUIDs, section markers for depth, parentheticals for translator notes, action blocks for prose). There is an interesting resonance beyond format: screenwriters think about "what is this beat doing" in exactly the way tsugiki's structure tree does. The beat sheet (Save the Cat, Snyder's structure) is a rhetorical structure tree for narrative. Nobody has formalized it as typed data, but the practice is there. Several Rust crates exist for Fountain parsing: `fountain-rs`, `rustwell`, `fountain-parser-rs`, `lottie/fount`. These may be usable for reading Fountain files, though tsugiki's use of Fountain tokens (forced scene headings as UUID markers, section markers as depth indicators) is non-standard and may require a custom parser or adapter layer on top of an existing crate.

### Parallel corpus alignment

Computational linguistics aligns sentences across parallel corpora for machine translation training (Europarl, OPUS project). Tools like hunalign and bleualign perform automatic sentence alignment. The overlap is source-target alignment. The differences: fully automatic, flat (no hierarchy), no rhetorical layer.

### Legal annotation and hermeneutics

Legal informatics has tools for annotating statutory text with interpretive layers — Akoma Ntoso for legislative XML, ELI for legal identifiers. The overlap is that they layer meaning over structured text. The difference: they annotate one text with metadata, not three aligned trees. The demand signal is real — legal translation is a high-value domain where "what is this clause doing" has contractual consequences, and the structure tree serves as an audit trail of translation decisions.

### Literate programming

Knuth's WEB, noweb, and org-mode tangling maintain prose and code as two views of the same structure. The overlap is the dual-tree idea — one tree for the human narrative, one for the machine-executable structure. The difference: literate programming has two trees, not three, and the "structure" tree is implicit in the code's AST.

### What is novel

Three things in tsugiki appear to have no public software precedent:

1. **The three-tree bridge.** Using a rhetorical intent tree as the alignment layer between source and target in translation. The theoretical pieces (RST, translation studies, CAT tools) exist independently but have not been combined in a tool.

2. **The AI constraint.** AI contributes structure (questions, annotations, faithful recording) but never target-language text. This inverts the current industry direction where AI generates translations and humans post-edit.

3. **Prose-readable rhetorical structure.** Storing the intent tree as Fountain prose with parenthetical annotations, so the rhetorical structure is readable as a document rather than locked in XML or a database.

### Sustainability and demand

Professional translators are conservative about tools. The learning curve of "decompose, understand, regrow" is steep compared to "type the translation in the right box." Demand is more likely to come from: translation studies (academic, where the three-tree model has theoretical value), literary translators (artisanal, where the translator's voice must be uncontaminated), and legal translators (where "why did you translate it this way" has contractual consequences and the structure tree is an audit trail). The typed Rust implementation lowers the barrier by making the workflow available as a CLI that catches errors early, rather than requiring an AI session to maintain the invariants.

### Build vs reuse

- **CSVS and panrec**: already exist, handle all tablet operations. Panrec also handles rich text to markdown conversion, which means the tsugiki parser only needs to deal with markup and plain text, not docx/pdf/etc.
- **Fountain parsing**: existing Rust crates (`fountain-rs`, `rustwell`, `fountain-parser-rs`, `lottie/fount`) may provide a base, though tsugiki's non-standard use of Fountain tokens likely requires a custom layer.
- **RST**: theoretical grounding for the structure tree, but no code to reuse.
- **Tree types, streaming CLI, proptest suite, cursor model, parser contract**: novel, built from scratch.
