---
status: proposed
date: 2026-04-09
---

# Three-tree data model for translation

## Context and Problem Statement

Translation is not word-for-word substitution. When a human translator works, they do something closer to regrowing a plant: they study the shape and life of the original, understand what it is doing and why, and then grow a new organism in the target language that carries the same life in a native form. A good translation does not read like a translation.

This project — tsugiki, named after the Japanese art of tree grafting — needs a data model that captures this process faithfully. The model must satisfy a hard commercial constraint: not a single word in the target language may come from AI, because contamination risks the loss of clients. AI is, however, exceptionally good at asking open questions, discussing nuances, and helping a translator articulate why a sentence works the way it does. The model should support that dialogue while also working as a standalone tool — a CLI where the human translator is their own witness, without AI assistance.

## Decision Drivers

- **No AI-generated target language.** AI contributes questions and structure, never prose in the target language.
- **Fidelity to the translator's cognitive process.** Translators decompose text into a mental tree, understand the intent behind each piece, and rebuild that intent in a new structure. The data model should mirror these phases.
- **Scale.** A book has thousands of sentences. The storage format must handle this without degrading.
- **Prose in context.** A sentence is meaningful inside its paragraph inside its chapter. Storing each sentence in isolation (one file per node) strips that context. Storing all sentences in a flat database field is worse.
- **Streamability.** The format should be processable line-by-line, so a CLI can walk through it interactively without loading the entire file into memory.
- **Independence from AI.** The full workflow — decompose, understand, regrow — must be achievable through a CLI tool backed by CSVS, with or without an AI interlocutor.

## How We Got Here

The design emerged through a conversation that explored several tensions:

**Where does prose live?** CSVS (Comma-Separated Value Store) is elegant for relationships between identifiers, but storing arbitrary-length human prose in CSV fields scales poorly — you cannot predict or control file size, and the data resists diffing. Individual files per sentence (`prose/uuid.md`) preserve the text but strip it from its structural context: a sentence marooned in its own file loses the paragraph that gives it meaning. We needed a format where the file structure itself encodes the tree, so that reading the file means reading the text in context.

**Why Fountain?** The conversation considered markdown with HTML comment anchors (viable but not streamable, heading depth capped at six levels), content-addressed prose (clean but fragile — any whitespace edit breaks the link), and CSVS sidecar files (two files per tree that must stay in sync). Fountain — the screenwriting markup language — emerged as the best fit for reasons that go beyond syntax. Fountain is not a document format; it is a format for performances. A screenplay is a sequence of beats, each responding to the last. Parentheticals are not metadata — they are stage directions that say how a beat lands. The structure layer of translation has exactly this character: a rhetorical script where each beat builds on the previous one. Fountain is also already in use in adjacent projects for spatial scene markup, its tokens are human-readable without tooling, and it streams naturally line-by-line.

**The shape of the structure tree.** The initial idea was a simple chain: each structure-node's parent is the previous structure-node, tracing the communicative arc. But this does not match how translators actually think. A translator says: "this text's idea is generally about X; it has chapters each meaning Y; each chapter's idea is expressed in paragraphs meaning Z." The last paragraph of one chapter rarely connects to the first paragraph of the next — what connects is the chapter-level idea. The structure tree turned out to have the same depth as the structural trees (document, chapter, paragraph, sentence), but where parent-child means "is expressed through" rather than "contains." Sequence of siblings in the structure tree does not matter — what matters is the depth relationship. This structure has been stable across multiple languages and genres in the designer's translation practice, and corresponds closely to Rhetorical Structure Theory (RST), which is already used in a sibling project.

**Typed edges.** An early draft used a single `node-child.csv` tablet for containment relationships in both source and target trees, relying on UUID uniqueness to disambiguate. This was rejected in favor of separate tablets per tree — `source-child.csv`, `target-child.csv`, `structure-child.csv` — because typed edges are clearer to query, easier to reason about, and do not require traversing cross-tree tablets to determine which tree a node belongs to.

## Decision

### Three trees, one root

Translation is modeled as three trees that share a single abstract root: the communicative intent of the text.

**Source tree.** A structural decomposition of the original text. Parent-child means containment: a chapter contains paragraphs, a paragraph contains sentences. The order of siblings is the author's ordering and is significant.

**Structure tree.** A rhetorical decomposition of communicative intent. It has the same depth levels as the source and target trees — document, chapter, paragraph, sentence — but parent-child means "is expressed through these sub-meanings." The order of siblings is not significant; what matters is which structure-nodes are children of which. Each structure-node at the sentence level is linked to exactly one source-node and (eventually) one target-node.

**Target tree.** A structural decomposition of the translated text. Same containment logic as the source tree. The order of siblings is the translator's ordering and may differ from the source ordering, even when both trees conform to the same meaning structure.

A target sentence has two parents: its structural parent in the target tree, and the structure-node it expresses. A source sentence also has two parents: its structural parent in the source tree, and the structure-node it grounds.

### Prose lives in Fountain files

Each tree is stored as a single Fountain file: `source.fountain`, `structure.fountain`, `target.fountain`.

Fountain tokens encode tree structure:

- **Section markers** (`#`, `##`, `###`) encode depth levels (document, chapter, paragraph, sentence).
- **Forced scene headings** (`.NODE uuid`) mark individual nodes with their identifiers.
- **Action blocks** hold the prose content of each node — the actual sentences of the source text, the shortform structure annotations, or the translated sentences.
- **Parentheticals** annotate how a node functions, especially in the structure tree (e.g., rhetorical role, tone, register).
- **File order** is the sibling sequence for source and target trees. For the structure tree, file order is a convenience, not a semantic relationship.

Each Fountain file reads as prose in structural context: you can open `source.fountain` and read the original text with its natural hierarchy visible. You can open `structure.fountain` and read the rhetorical script of the text. You can open `target.fountain` and read the translation as a standalone document.

### Relationships live in CSVS

CSVS tablets store only UUID-to-UUID relationships, never arbitrary text:

| Tablet               | Meaning                                             |
|----------------------|-----------------------------------------------------|
| `source-child.csv`   | Structural containment within the source tree       |
| `target-child.csv`   | Structural containment within the target tree       |
| `structure-child.csv`  | Depth relationships within the structure tree         |
| `source-structure.csv` | Which structure-node grounds each source-node         |
| `structure-target.csv` | Which structure-node is expressed by each target-node |

Schema (`_-_.csv`):
```
source,child
target,child
structure,child
source,structure
structure,target
```

### Three-phase workflow

1. **Decompose.** Read the source text. Break it into structural pieces — chapters, paragraphs, sentences. Write `source.fountain` with section markers and node headings. Populate `source-child.csv` with the containment relationships.

2. **Understand.** For each source node, articulate what it does — not what it says, but what it accomplishes in the communicative arc. Write `structure.fountain` with shortform annotations at each depth level. Populate `structure-child.csv` with depth relationships and `source-structure.csv` with the links from source nodes to their structure nodes. This phase is where AI dialogue is most valuable: the AI asks what a sentence is doing, the translator answers, and the answer becomes the structure annotation.

3. **Regrow.** For each structure node, write a target sentence that expresses that meaning within the target tree's structure. Write `target.fountain`. Populate `target-child.csv` and `structure-target.csv`. The translator may reorder, split, or merge structural elements — the target tree's shape may differ from the source tree's shape — but every target leaf traces back to a structure node.

## Consequences

- Fountain files serve as the single source of truth for all prose. They are human-readable, preserve structural context, and stream line-by-line.
- CSVS handles what it handles well: typed relationships between identifiers. No tablet contains arbitrary text.
- The structure tree is the shared ground between source and target. It is where AI and translator meet to discuss options — by referencing structure nodes, not by generating target-language text.
- The three-phase workflow can be driven by a CLI that walks the Fountain files beat-by-beat, or by an AI session that asks questions about each node. Both paths produce the same artifacts.
- Fountain's richer tokens — dialogue, transitions, dual dialogue — remain available for future use. Rhetorical relation types (elaboration, contrast, cause) can be added as parentheticals or as additional CSVS tablets, aligning with RST when ready.
- TTL annotation of the CSVS schema and formal mapping to a public ontology are deferred to a future decision.
