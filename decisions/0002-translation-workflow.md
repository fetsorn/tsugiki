---
status: proposed
date: 2026-04-09
---

# Translation workflow

## Context and Problem Statement

ADR-0001 defines the three-tree data model: source, structure, and target. This decision describes the workflow that populates those trees — the sequence of phases a translator walks through, what happens in each, and how the artifacts change.

The workflow must work in two modes: an AI-assisted session where the translator and an AI interlocutor walk through the text together, and a standalone CLI where the translator works alone, using the tool as a structured mirror for their own process. Both modes must produce the same artifacts.

## The three phases

### Phase 1: Decompose

The translator reads the source text and breaks it into a tree of structural units. The depth of the tree follows the natural shape of the text — not a fixed hierarchy. A formal letter might decompose into salutation, body blocks, wishes, and signature. A poem might decompose into stanzas and lines. A legal brief might decompose into sections, clauses, and sub-clauses. The translator names each block in the source language, choosing names that capture function rather than content ("address" rather than "dear so-and-so").

**Artifacts produced:**
- `prose/source.fountain` — the full source text, annotated with UUIDs and section markers at each depth level.
- `csvs/source-child.csv` — containment edges: which nodes are children of which.
- `prose/source.md` — a clean copy of the source text with no annotations, for reference and delivery.

**Example.** A formal congratulatory letter decomposes into six blocks: address, congratulation, merit, confidence, wishes, signature. The merit and confidence blocks each contain two sentences; the others contain one. The section markers in `source.fountain` look like:

```
.a1b2c3d4
# Congratulatory Letter

.e5f6a7b8
## Address

.c9d0e1f2
Dear Colleague,
```

**Edge cases and open questions:**
- *Where to draw sentence boundaries.* Most prose has obvious sentence breaks. But some texts — legal documents, liturgical texts, stream-of-consciousness prose — have sentences that run for paragraphs. The translator decides; the tool should not force a granularity.
- *How deep to go.* A sentence could be further decomposed into clauses. For now, sentences are the leaf level. If clause-level decomposition becomes useful, the model supports it — just add another depth level.
- *Multiline source nodes.* A signature block or a poem stanza is a single node spanning multiple lines. In Fountain, everything between one scene heading and the next is the content of that node. This works, but a CLI walker needs to know that "node content" is not always one line.

### Phase 2: Understand

The translator articulates what each piece of the source text is *doing* — not what it says, but what it accomplishes in the communicative arc. This produces the structure tree, which has the same depth as the source tree but where parent-child means "is expressed through" rather than "contains."

The structure annotations are written in whatever language the translator thinks in. They are informal, often blunt. The point is to name the rhetorical move so precisely that someone who does not speak the source language could reconstruct the *intent* of the text from the structure tree alone.

During this phase, the translator also records parenthetical notes — observations about nuance, tone, cultural load, or translation pitfalls. These are the translator's working notes and will be consulted again during Phase 3.

**Artifacts produced:**
- `prose/structure.fountain` — structure annotations at each depth level, with parenthetical notes.
- `csvs/structure-child.csv` — depth edges within the structure tree.
- `csvs/source-structure.csv` — links from each source node to its structure node.

**Example.** The congratulatory letter's merit block ("you rock!") decomposes into two sentence-level meanings:

```
.f3a4b5c6
## you rock!

.d7e8f9a0
anniversary marks a summit of growth
(key is the authority acquired, not the metaphor of the path)

.b1c2d3e4
institution became flesh of the country
(the source says "happiness" but the culture has "luck", not "happiness" — translator's choice whether to carry that)
```

The parenthetical on the second sentence captures a cultural nuance that will matter in Phase 3: the source language makes a claim the translator knows is culturally loaded. The structure annotation ("institution became flesh of the country") is neutral; the parenthetical flags the tension.

**In AI-assisted mode**, this is where the AI is most useful. The AI reads the source sentence, asks the translator what it is doing, and the translator's answer becomes the structure annotation. The AI can probe: "is the list of government bodies important, or is the point that graduates are everywhere that matters?" The translator answers, and the answer sharpens the structure node.

**Edge cases and open questions:**
- *Structure nodes for single-sentence blocks.* When a block contains only one sentence, the block-level meaning and the sentence-level meaning are the same. We collapsed these into a single node in the prototype. This feels right but means the structure tree has variable depth — some branches go root → block → sentence, others go root → block (which is also the sentence). A CLI walker must handle both.
- *Structure nodes with no source.* The root structure node ("to please the recipient") has no single source sentence — it is the intent behind the entire document. It links to the document-level source node, which is the structural root. This is fine but worth noting: not every structure node corresponds to a piece of text you can point at.
- *Revising structure during Phase 3.* The structure tree is not frozen after Phase 2. During regrowth, the translator often discovers that their initial structure annotation was imprecise — the act of writing the target sentence reveals what the source sentence was *really* doing. The `structure.fountain` file gets parenthetical additions during Phase 3. This is expected, not a failure of the process.

### Phase 3: Regrow

The translator walks the structure tree and, for each structure node, writes a target sentence that expresses that meaning within the target language's natural structure. The target tree may differ from the source tree in sequence, in the number of sentences per block, even in the number of blocks — but every target leaf traces back to a structure node.

This is where the translator's craft lives. The AI does not write target-language text. In AI-assisted mode, the AI can ask about choices: "the source packs two ideas into one sentence — are you keeping that or splitting?" But the words are the translator's.

During regrowth, the structure tree often gets updated. The translator adds parenthetical notes about *why* they made a particular choice, or refines the structure annotation now that they see how it lands in the target language. The structure tree is a living document across Phases 2 and 3.

**Artifacts produced:**
- `prose/target.fountain` — the translated text, annotated with UUIDs and section markers.
- `csvs/target-child.csv` — containment edges within the target tree.
- `csvs/structure-target.csv` — links from each structure node to its target node(s).
- `prose/target.md` — a clean copy of the translation for delivery.
- `prose/structure.fountain` — updated with new parentheticals from the regrowth process.

**Example.** The "pharaoh addressing pharaoh" structure node had one source sentence but produced two target sentences:

```
.g5h6i7j8
## Congratulation

.k9l0m1n2
Today we celebrate ten years since the founding of the Institute.

.o3p4q5r6
From the bottom of our hearts, we congratulate you and everyone at the Institute!
```

The source language can sustain a single sentence that names the sender, the recipient, the occasion, and the congratulation. English cannot — or rather, the result would read as translated. The translator split it. Both target nodes link to the same structure node in `structure-target.csv`.

Meanwhile, `structure.fountain` gained a parenthetical during this phase:

```
.s7t8u9v0
## pharaoh addressing pharaoh
(institution-to-institution through human faces)
(english can't sustain the single-sentence form — split into occasion + congratulation)
```

**Edge cases and open questions:**
- *One structure, many targets.* A single structure node can produce multiple target sentences (as above). The reverse — multiple structure nodes collapsing into one target sentence — is also possible but did not arise in the prototype. The CSVS model supports both via many-to-many in `structure-target.csv`.
- *Zero target for a structure node.* Some structure nodes may have no target equivalent. A culturally specific honorific, a rhetorical flourish that does not translate, a formal convention that the target culture does not have. The structure node documents that it existed in the source; the absence of a target edge documents the translator's decision to omit it.
- *Target block structure.* The target tree needs its own block structure, which may differ from the source. In the prototype, the blocks mapped roughly one-to-one (address → address, merit → merit). But a translator might merge two source blocks into one target block, or split one into three. The CLI must allow the translator to define the target tree's structure independently, not just mirror the source.
- *Generating clean documents.* `source.md` and `target.md` are generated by stripping UUIDs, scene headings, and section markers from the Fountain files, then reassembling the prose with paragraph breaks where blocks were. This is mechanical and should be a CLI command: `tsugiki render source` or `tsugiki render target`. The generation must respect multiline nodes (signature blocks, stanzas) and handle the fact that block-level headings are structural annotations, not part of the delivered text.
- *Preserving the translator's exact text.* The clean document must contain exactly the words the translator wrote, including any intentional unconventional punctuation, capitalization, or phrasing. The render step strips structure, not content.

## Workflow summary

| Phase      | Human does                                 | Artifacts written                                             | Structure tree status                            |
|------------|--------------------------------------------|---------------------------------------------------------------|--------------------------------------------------|
| Decompose  | Breaks source into structural tree         | source.fountain, source-child.csv, source.md                  | Does not exist yet                               |
| Understand | Names the rhetorical move of each piece    | structure.fountain, structure-child.csv, source-structure.csv | Created with annotations and parentheticals      |
| Regrow     | Writes target text for each structure node | target.fountain, target-child, structure-target, target.md    | Updated with new parentheticals from translation |

## Consequences

- The structure tree is a living document that grows across two phases. Tools must not treat it as immutable after Phase 2.
- The workflow produces five prose files and five CSVS tablets per translation. Each translation lives in its own directory under `intents/`.
- The AI-assisted and CLI-only modes differ only in who asks the questions — the AI or the tool's prompts. The artifacts are identical.
- The render step (Fountain → clean markdown) is mechanical and must be a CLI command, not a manual process.
- The translator's cultural and linguistic judgment is captured in parentheticals, making the structure tree a record of *why* the translation reads the way it does — not just what was translated, but what was considered and decided.
