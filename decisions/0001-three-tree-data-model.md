---
status: accepted
date: 2026-04-11
---

# Three-tree data model for translation

## Context and Problem Statement

Translation is not word-for-word substitution. A human translator studies the shape and life of the original, understands what it is doing and why, and grows a new organism in the target language that carries the same life in a native form. A good translation does not read like a translation.

Tsugiki needs a data model that captures this process faithfully. The model must satisfy a hard commercial and principled constraint: not a single word in the target language may come from AI, because contamination risks loss of clients and corrupts the translator's voice. AI is exceptionally good at asking questions, discussing nuance, and helping a translator articulate why a piece of text works the way it does. The model should support that dialogue while also working as a standalone CLI tool — a translator working alone, without AI assistance.

## Decision Drivers

- **No AI-generated target language.** AI contributes questions and structure, never prose in the target language.
- **Fidelity to the translator's cognitive process.** Translators break text into a mental tree, understand the intent behind each piece, and rebuild that intent in a new structure.
- **Scale.** A book has thousands of working units. The storage format must handle this without degrading.
- **Prose in context.** A working unit is meaningful inside its parent block inside its chapter. Storing units in isolation strips that context.
- **Streamability.** The format should be processable line-by-line, so a CLI can walk through it interactively without loading the entire file into memory.
- **Independence from AI.** The full workflow must be achievable through a CLI tool, with or without an AI interlocutor.

## Decision

### Three trees, structural fidelity

Translation is modeled as three trees that share a single abstract root: the communicative intent of the text.

**Source tree.** A structural decomposition of the original text. Parent-child means containment: a chapter contains paragraphs, a paragraph contains its working units. The order of siblings is the author's ordering and is significant.

**Structure tree.** A rhetorical decomposition of communicative intent. It mirrors the shape of the source tree — the same nesting, the same branching — but parent-child means "is expressed through these sub-meanings." The order of siblings is not significant; what matters is which structure nodes are children of which. Each structure node carries a short annotation in whatever language the translator thinks in, naming the rhetorical move precisely enough that someone who does not speak the source language could reconstruct the intent of the text from the structure tree alone.

**Target tree.** A structural decomposition of the translated text. Same containment logic as the source tree. The order of siblings is the translator's ordering and may differ from the source ordering.

**Structural fidelity.** The source tree and structure tree are isomorphic in shape: for every source node there is exactly one structure node, and vice versa, and the parent-child relationships match. This isomorphism is what makes the output a translation rather than an adaptation or a work inspired by the source. Breaking this constraint — adding a structure node with no source counterpart, or merging two source nodes into one structure node — is a decision to leave the tsugiki workflow. The tool does not forbid it; it stops being the right tool.

The target tree is where the isomorphism breaks. A single structure node can produce multiple target leaves (1:N in `structure-target.csv`). The translator may reorder, split, or merge at the leaf level in the target, but the decomposition of meaning is fixed.

### Leaves are the working unit

The translator works leaf by leaf. A leaf is any node with no children — the smallest unit the translator chose to work with in this particular decomposition. What a leaf contains is the translator's judgment: it might be one grammatical sentence, three short sentences that function as a single rhetorical move, or half a sentence that the translator decided to treat separately.

No depth level is privileged. The system does not have a concept of "sentence level" — it has leaves and inner nodes. Leaves are the units of annotation (each leaf gets one structure annotation) and the units of regrow (each structure leaf maps to one or more target leaves). Inner nodes provide grouping and context.

Leaves may exist at different depths within the same tree. A translator might split one paragraph into five leaves while leaving another paragraph as a single leaf. Both are valid decompositions.

### Prose lives in Fountain files

Each tree is stored as a single Fountain file: `source.fountain`, `structure.fountain`, `target.fountain`.

Fountain tokens encode tree structure:

- **Section markers** (`#`, `##`, `###`) encode depth levels. The number of levels follows the natural shape of the text, not a fixed hierarchy.
- **Notes** (`[[hex-id]]`) at the end of a heading or action line identify the node's UUID. Standalone `[[text]]` lines hold translator annotations from the annotate phase. `[[| text]]` lines (pipe prefix) hold annotations added during the regrow phase.
- **Action blocks** hold the prose content — the actual text of the source, the structure annotations, or the translated text.
- **File order** is the sibling sequence for source and target trees. For the structure tree, file order is a convenience, not a semantic relationship.

Each Fountain file reads as prose in structural context: you can open `source.fountain` and read the original text with its natural hierarchy visible. You can open `structure.fountain` and read the rhetorical script. You can open `target.fountain` and read the translation as a standalone document.

### Depth from the root

Depth counts downward from the document root. The root node is at the shallowest depth. Each child is one level deeper than its parent. The exact number of levels depends on the text — a letter might have three, a monograph chapter four. The system enforces the relative invariant (child depth = parent depth + 1), not a fixed set of levels.

The maximum is four levels (`#` + `##` + `###` + action blocks). If a text requires more depth, the translator splits it into separate intents. This is not a limitation but a discipline: if the rhetorical structure cannot be held in four levels, the scope of the translation unit is too large for one pass.

Depth values are internal to the tree. In user-facing output, the CLI displays depth as level numbers (L1, L2, L3, L4) counting from the root. The mapping between Fountain heading markers and depth is determined by counting the distinct heading levels actually present in the file.

### Footnotes are stray ideas

Footnotes — citations, hedges, tangential remarks, digressions the author could not resist — are outside the communicative intent of the text. If a footnote contained relevant evidence, that evidence would be in the main text. Footnotes do not participate in the rhetorical arc.

Source footnotes live in a `## Footnotes` section at the bottom of `source.fountain`, as regular leaf nodes. Target footnotes live in the same position in `target.fountain`. No structure nodes are generated for footnotes. They are not walked during annotate and are translated directly during regrow. Footnote bodies are always single leaves — they are not split further.

Footnote numbers do not appear in Fountain files. Numbers are rendering artifacts, assigned at render time by order of first reference in the main text. If the translator reorders the target text, footnotes renumber automatically.

The relationship between a text node and its footnote is recorded as a reference edge: `source-footnote.csv` connects a source node to its footnote, `target-footnote.csv` does the same for the target. During regrow, the CLI traverses the graph (source node → source-footnote → footnote → source-structure → structure) to prompt: "this node's source has a footnote — attach it here?"

The source-to-target footnote correspondence is derived through graph traversal, not stored directly. Each source footnote maps to exactly one target footnote. The translator may also add footnotes that exist only in the target, with no source counterpart.

### Relationships live in CSVS

CSVS tablets store only UUID-to-UUID relationships, never arbitrary text:

| Tablet                 | Meaning                                               |
|------------------------|-------------------------------------------------------|
| `source-child.csv`     | Structural containment within the source tree         |
| `target-child.csv`     | Structural containment within the target tree         |
| `structure-child.csv`  | Depth relationships within the structure tree         |
| `source-structure.csv` | Which structure node corresponds to each source node  |
| `structure-target.csv` | Which target node(s) express each structure node      |
| `source-footnote.csv`  | Which source node anchors which source footnote       |
| `target-footnote.csv`  | Which target node anchors which target footnote       |

Schema (`_-_.csv`):
```
source,child
target,child
structure,child
source,structure
structure,target
source,footnote
target,footnote
```

### Project layout

Each translation intent is its own directory with its own version control:

```
{intent}/
  prose/
    source.fountain      — annotated source tree
    structure.fountain   — rhetorical structure with annotations
    target.fountain      — annotated target tree
    source.md            — clean source document (rendered)
    target.md            — clean target document (rendered)
  csvs/
    .csvs.csv            — dataset identity
    _-_.csv              — schema
    source-child.csv     — source tree containment
    structure-child.csv  — structure tree depth
    target-child.csv     — target tree containment
    source-structure.csv — source-to-structure bridge
    structure-target.csv — structure-to-target bridge
    source-footnote.csv  — source node-to-footnote reference
    target-footnote.csv  — target node-to-footnote reference
```

## Consequences

- Fountain files serve as the single source of truth for all prose. They are human-readable, preserve structural context, and stream line-by-line.
- CSVS handles what it handles well: typed relationships between identifiers. No tablet contains arbitrary text.
- The structure tree is the shared ground between source and target. It is where AI and translator meet to discuss options — by referencing structure nodes, not by generating target-language text.
- Structural fidelity (source-structure isomorphism) is the defining constraint. It makes the three-tree workflow a tool for translation, not for adaptation or creative rewriting.
- Leaves are the working unit. No depth level is privileged. The translator controls the granularity of decomposition, and the system supports uneven leaf depths within a single tree.
- Footnotes stay outside the rhetorical structure. Their numbering is a render-time artifact, their correspondence is derived through graph traversal, and their translation is direct — no annotation pass needed.
