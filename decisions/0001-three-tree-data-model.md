---
status: accepted
date: 2026-04-11
---

# Three-tree data model for translation

## Context and Problem Statement

Translation is not word-for-word substitution. A human translator studies the shape and life of the original, understands what it is doing and why, and grows a new organism in the target language that carries the same life in a native form. A good translation does not read like a translation.

Tsugiki needs a data model that captures this process faithfully. The model must satisfy a hard commercial and principled constraint: not a single word in the target language may come from AI, because contamination risks loss of clients and corrupts the translator's voice. AI is, however, exceptionally good at asking questions, discussing nuance, and helping a translator articulate why a sentence works the way it does. The model should support that dialogue while also working as a standalone CLI tool — a translator working alone, without AI assistance.

## Decision Drivers

- **No AI-generated target language.** AI contributes questions and structure, never prose in the target language.
- **Fidelity to the translator's cognitive process.** Translators break text into a mental tree, understand the intent behind each piece, and rebuild that intent in a new structure.
- **Scale.** A book has thousands of sentences. The storage format must handle this without degrading.
- **Prose in context.** A sentence is meaningful inside its paragraph inside its chapter. Storing sentences in isolation strips that context.
- **Streamability.** The format should be processable line-by-line, so a CLI can walk through it interactively without loading the entire file into memory.
- **Independence from AI.** The full workflow must be achievable through a CLI tool, with or without an AI interlocutor.

## Decision

### Three trees, structural fidelity

Translation is modeled as three trees that share a single abstract root: the communicative intent of the text.

**Source tree.** A structural decomposition of the original text. Parent-child means containment: a chapter contains paragraphs, a paragraph contains sentences. The order of siblings is the author's ordering and is significant.

**Structure tree.** A rhetorical decomposition of communicative intent. It has the same depth levels as the source tree — document, section, paragraph, sentence — but parent-child means "is expressed through these sub-meanings." The order of siblings is not significant; what matters is which structure-nodes are children of which. Each structure-node carries a short annotation in whatever language the translator thinks in, naming the rhetorical move precisely enough that someone who does not speak the source language could reconstruct the intent of the text from the structure tree alone.

**Target tree.** A structural decomposition of the translated text. Same containment logic as the source tree. The order of siblings is the translator's ordering and may differ from the source ordering.

**Structural fidelity.** The source tree and structure tree are isomorphic: every source node maps to exactly one structure node, and vice versa, at every depth level. This isomorphism is generated at intake time — when the translator breaks the source text into structural pieces, the corresponding structure skeleton is produced simultaneously. The 1:1 mapping is what makes the output a translation rather than an adaptation or a work inspired by the source. Breaking this constraint (splitting one meaning into two, or merging two into one) is a decision to leave the tsugiki workflow. The tool does not forbid it — it stops being the right tool.

The target tree is where the isomorphism breaks. A single structure node can produce multiple target sentences (1:N in `structure-target.csv`). The translator may reorder, split, or merge at the sentence level in the target, but the decomposition of meaning is fixed.

### Prose lives in Fountain files

Each tree is stored as a single Fountain file: `source.fountain`, `structure.fountain`, `target.fountain`.

Fountain tokens encode tree structure:

- **Section markers** (`#`, `##`, `###`) encode depth levels. The number of levels follows the natural shape of the text, not a fixed hierarchy.
- **Notes** (`[[hex-id]]`) at the end of a heading or action line identify the node's UUID. Standalone `[[text]]` lines hold translator annotations. `[[| text]]` lines (pipe prefix) hold annotations added during the regrow phase.
- **Action blocks** hold the prose content — the actual sentences of the source text, the structure annotations, or the translated sentences.
- **File order** is the sibling sequence for source and target trees. For the structure tree, file order is a convenience, not a semantic relationship.

Each Fountain file reads as prose in structural context: you can open `source.fountain` and read the original text with its natural hierarchy visible. You can open `structure.fountain` and read the rhetorical script. You can open `target.fountain` and read the translation as a standalone document.

### Four-level ceiling

Fountain section markers allow `#`, `##`, `###`, and unmarked action blocks — four depth levels. A text with sentences grouped only into sections has two levels. Sentences in paragraphs in sections has three. Sections in chapters with paragraphs and sentences has four.

If a text requires more than four levels of structural depth, the translator splits it into separate intents. A book becomes one intent per chapter or per part, depending on the translator's judgment. This is not a limitation but a discipline: if the rhetorical structure cannot be held in four levels, the scope of the translation unit is too large for one pass. Fountain could theoretically allow six heading levels but chose not to; tsugiki follows the same instinct.

### Footnotes are stray ideas

Footnotes — citations, hedges, tangential remarks, digressions the author could not resist — are outside the communicative intent of the text. If a footnote contained relevant evidence, that evidence would be in the main text. Footnotes do not participate in the rhetorical arc.

Source footnotes live in a `## Footnotes` section at the bottom of `source.fountain`, as regular nodes at the same depth levels as the main text. Target footnotes live in the same position in `target.fountain`. No structure nodes are generated for footnotes. They are not walked during the annotate phase and are translated directly during regrow.

Footnote numbers do not appear in Fountain files. Numbers are rendering artifacts, assigned at render time by order of first reference in the main text. If the translator adds a footnote earlier in the target text, all subsequent footnotes renumber automatically.

The relationship between a sentence and its footnote is recorded as a reference edge: `source-footnote.csv` connects a source sentence to its footnote block, `target-footnote.csv` does the same for the target. During regrow, the CLI traverses the graph (source sentence → source-footnote → source sentence → source-structure → structure → structure-target → target sentence) to prompt: "this target sentence's source has a footnote — attach it here?"

The source-to-target footnote correspondence is derived through graph traversal, not stored directly. Each source footnote maps to exactly one target footnote (the same structural fidelity that holds for the main text). The translator may also add footnotes that exist only in the target, with no source counterpart.

### Relationships live in CSVS

CSVS tablets store only UUID-to-UUID relationships, never arbitrary text:

| Tablet                 | Meaning                                                |
|------------------------|--------------------------------------------------------|
| `source-child.csv`    | Structural containment within the source tree          |
| `target-child.csv`    | Structural containment within the target tree          |
| `structure-child.csv` | Depth relationships within the structure tree          |
| `source-structure.csv`| Which structure node corresponds to each source node   |
| `structure-target.csv`| Which target node(s) express each structure node       |
| `source-footnote.csv` | Which source sentence anchors which source footnote    |
| `target-footnote.csv` | Which target sentence anchors which target footnote    |

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
    source-footnote.csv  — source sentence-to-footnote reference
    target-footnote.csv  — target sentence-to-footnote reference
```

## Consequences

- Fountain files serve as the single source of truth for all prose. They are human-readable, preserve structural context, and stream line-by-line.
- CSVS handles what it handles well: typed relationships between identifiers. No tablet contains arbitrary text.
- The structure tree is the shared ground between source and target. It is where AI and translator meet to discuss options — by referencing structure nodes, not by generating target-language text.
- Structural fidelity (source-structure isomorphism) is the defining constraint. It makes the three-tree workflow a tool for translation, not for adaptation or creative rewriting.
- Footnotes stay outside the rhetorical structure. Their numbering is a render-time artifact, their correspondence is derived through graph traversal, and their translation is direct — no annotation pass needed.
- The four-level ceiling is an architectural choice that keeps individual translation units scoped to what a translator can hold in their head.
