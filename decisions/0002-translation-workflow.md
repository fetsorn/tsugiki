---
status: accepted
date: 2026-04-11
---

# Translation workflow

## Context and Problem Statement

ADR-0001 defines the three-tree data model: source, structure, and target. This decision describes the workflow that populates those trees — the sequence of phases a translator walks through, what happens in each, and how the artifacts change.

The workflow must work in two modes: an AI-assisted session where the translator and an AI interlocutor walk through the text together, and a standalone CLI where the translator works alone. Both modes must produce the same artifacts.

## The four phases

### Phase 1: Init

The translator provides a source text as a well-formed markdown file. Markdown input is required — the parser needs unambiguous structure (headings for sections, paragraphs for blocks) and unambiguous footnotes (`[^N]` references and `[^N]:` definitions). Plain text input is not supported because recovering structure from unformatted prose is unreliable, and recovering footnotes from it is impossible.

The parser reads the markdown, extracts the heading hierarchy and paragraph blocks, and produces the source tree and the structure tree skeleton simultaneously. Headings become inner nodes at the appropriate depth. Each paragraph — a block of text between blank lines — becomes a leaf node. No sentence splitting is performed. The paragraph is the initial leaf granularity.

Because of the structural fidelity invariant (ADR-0001), every source node gets a shadow structure node with matching tree position, and the bridge tablets (`source-structure.csv`, `source-child.csv`, `structure-child.csv`) are fully populated at this step.

The structure skeleton in `structure.fountain` comes out with headings that carry UUIDs but no titles, and action blocks that carry UUIDs but no text. The annotate phase fills in the annotations. The heading-level structure nodes will receive their annotations during annotate, the same as leaf nodes.

Footnotes are extracted from the markdown, placed in a `## Footnotes` section at the bottom of `source.fountain`, and their reference edges are written to `source-footnote.csv`. Each footnote becomes a single leaf node — footnote bodies are not split.

**Artifacts produced:**
- `prose/source.fountain` — the full source text, annotated with UUIDs and section markers.
- `prose/structure.fountain` — the structure skeleton with empty annotations.
- `prose/source.md` — a clean rendered copy of the source text.
- `csvs/source-child.csv` — source containment edges.
- `csvs/structure-child.csv` — structure depth edges.
- `csvs/source-structure.csv` — the 1:1 bridge between source and structure.
- `csvs/source-footnote.csv` — node-to-footnote reference edges (if footnotes exist).

If the intent directory already contains artifacts, `init` refuses to overwrite.

### Phase 2: Split

The translator scans each paragraph leaf and breaks it into smaller working units. This is a perceptual act, not an interpretive one — the translator is finding the joints in the text, not yet naming the bones. What counts as an atomic unit is the translator's judgment: it might follow grammatical sentence boundaries, or it might not.

Split operates on a single leaf at a time. It has two modes:

**Get mode.** The translator points at a leaf node. The CLI prints its text. Nothing is written.

**Put mode.** The translator provides the same text back, broken into multiple lines. Each line becomes a new leaf node. The CLI validates that the concatenation of the input lines matches the original text (modulo whitespace) — split is a scalpel, not a pen.

If the leaf is above maximum depth, the original leaf becomes an inner node and the new leaves are its children at depth+1. If the leaf is already at maximum depth (depth 4), the original is replaced by N sibling leaves at the same depth under the same parent — flattening, because there is nowhere deeper to go. New structure skeleton nodes are created for each new leaf, and bridge edges connect them.

If the translator provides a single line (or the same text unsplit), nothing happens.

Split is the recommended second phase, but it is available anytime before a leaf is annotated. Once a leaf has a structure annotation, splitting it would orphan that annotation, so the CLI refuses to split annotated leaves. This flexibility allows the translator to discover during annotate that a leaf is too coarse and go back to split it before continuing.

**Artifacts modified:**
- `prose/source.fountain` — leaf replaced by multiple finer leaves (as children if depth < 4, as siblings if depth = 4).
- `prose/structure.fountain` — matching skeleton nodes added or replaced accordingly.
- `csvs/source-child.csv` — new containment edges (parent→children if deepened, or replacement edges if flattened).
- `csvs/structure-child.csv` — same pattern.
- `csvs/source-structure.csv` — new bridge edges for the new leaves.

**In AI-assisted mode**, the AI calls split in get mode, reads the paragraph text, asks the translator "where are the joints?", and constructs the put-mode input from the translator's answer. The AI never modifies the source text — it only cuts where the translator indicates.

### Phase 3: Annotate

The translator walks the structure tree node by node and writes a short annotation for each — naming what that piece of the text is doing, not what it says.

The annotations are written in whatever language the translator thinks in. They are informal, often blunt. "pharaoh addressing pharaoh" or "you rock!" — the point is to name the rhetorical move so precisely that someone who does not speak the source language could reconstruct the intent from the structure tree alone.

The CLI presents each node by scanning `structure.fountain` for the first node that still has empty content, showing the corresponding source text alongside it. The translator writes the annotation, and the CLI writes it into `structure.fountain`. No CSV writes happen during this phase — the bridge and containment tablets are already populated from init and split.

**The pass is idempotent.** If the translator restarts, the CLI scans from the top and skips nodes that already have annotations (non-empty content). There is no cursor file. The position is derived from the file every time.

The translator also records notes — observations about nuance, tone, cultural load, or translation pitfalls — as `[[text]]` lines after the annotation. These are working notes consulted again during regrow.

Both inner nodes (headings) and leaf nodes receive annotations. A heading-level annotation names the rhetorical function of the entire section; a leaf-level annotation names the function of that specific working unit.

**Artifacts modified:**
- `prose/structure.fountain` — empty annotations replaced with real ones, notes added.

**In AI-assisted mode**, this is where the AI is most useful. The AI reads the source text, asks the translator what it is doing, and the translator's answer becomes the annotation. The AI can probe: "is the list of government bodies important, or is the point that graduates are everywhere?" The translator answers, and the answer sharpens the annotation.

### Phase 4: Regrow

The translator walks the structure tree and, for each structure leaf, writes target text that expresses that meaning in the target language. Each invocation of the regrow command creates exactly one target leaf node. If a single structure node needs multiple target leaves (1:N in `structure-target.csv`), the translator calls regrow multiple times against the same structure node. The target tree may differ from the source tree in sequence, in the number of leaves per block, and in block structure — but every target leaf traces back to a structure node.

This is where the translator's craft lives. The AI does not write target-language text. In AI-assisted mode, the AI can ask about choices: "the source packs two ideas into one unit — are you keeping that or splitting?" But the words are the translator's.

During regrow, the structure tree is a living document. The translator adds `[[| text]]` notes (pipe prefix) about why they made a particular choice, or refines the annotation now that they see how it lands in the target language. The structure tree grows across phases 3 and 4.

**Footnote handling.** When the CLI presents a structure node whose source has a footnote (detected by traversing source-structure → source → source-footnote), it prompts: the source footnote text is shown, and the translator is asked whether to attach a footnote to this target node. If yes, the translator writes the footnote translation, and the CLI places it in the target's `## Footnotes` section and writes the reference edge to `target-footnote.csv`. If the source node was split into multiple target nodes, the translator chooses which one gets the footnote.

Footnotes are translated directly — no annotation pass, no structure nodes. They are stray ideas, and the translation is usually straightforward.

**Artifacts produced:**
- `prose/target.fountain` — the translated text with UUIDs and section markers.
- `prose/target.md` — a clean rendered copy of the translation.
- `prose/structure.fountain` — updated with `[[| text]]` notes from regrow.
- `csvs/target-child.csv` — target containment edges.
- `csvs/structure-target.csv` — structure-to-target bridge.
- `csvs/target-footnote.csv` — target node-to-footnote reference edges.

## Addressing

Line numbers are the natural way to point into a Fountain file from the terminal. `tsugiki next` prints a line number alongside each node; `tsugiki annotate 104 "text"` writes to the node at that line. UUIDs and short hex IDs also work as addresses.

Line numbers are ephemeral — they shift on every write — and are never persisted in CSVS. They are a runtime convenience, derived fresh from the file each time the CLI runs. UUIDs are the stable identity in CSVS; line numbers are the human interface in the terminal.

## Reset

The phases are sequential in recommendation: init, then split, then annotate, then regrow. Split is available on unannotated leaves at any time. Corrections to completed phases happen through direct file editing — the files are human-readable and human-writable by design.

If the translator wants to redo a phase from scratch, the CLI provides reset commands:

- **`tsugiki reset annotate`**: archive `structure.fountain` as `structure.{ISO-timestamp}.fountain` in place, then regenerate the skeleton from the source tree. All annotations are lost; the archived file is the safety net.
- **`tsugiki reset regrow`**: archive `target.fountain`, `target-child.csv`, `structure-target.csv`, and `target-footnote.csv` with ISO timestamps. Clear the target artifacts. The translator starts regrow from the beginning.

Archives are never overwritten. If the translator resets twice in one day, both archives coexist with different timestamps. Moving or deleting archives is the translator's concern.

## Rendering

`tsugiki render source` and `tsugiki render target` produce clean markdown from the Fountain files by stripping UUIDs, section markers, and translator notes, then reassembling the prose with paragraph breaks.

The renderer handles footnotes: it walks the main text, and for each node that has a reference edge in `source-footnote.csv` (or `target-footnote.csv`), it assigns the next footnote number, inserts `[^N]` into the markdown output at the node boundary, and collects the footnote body for the bottom of the document. Footnote numbers are derived from order of first reference — they exist only in the rendered markdown, never in the Fountain files.

## Workflow summary

| Phase    | Translator does                            | Artifacts written                                                              | Structure tree status                |
|----------|--------------------------------------------|--------------------------------------------------------------------------------|--------------------------------------|
| Init     | Provides markdown source                   | source.fountain, structure.fountain (skeleton), source.md, 4 CSV tablets       | Skeleton with empty annotations      |
| Split    | Breaks paragraph leaves into working units | source.fountain, structure.fountain (skeleton extended), 3 CSV tablets updated | Skeleton extended for new leaves     |
| Annotate | Names the rhetorical move of each node     | structure.fountain (annotations filled in, notes added)                        | Populated with annotations and notes |
| Regrow   | Writes target text for each structure leaf | target.fountain, target.md, structure.fountain (regrow notes), 3 CSV tablets   | Updated with regrow-phase notes      |

## Consequences

- Init is fully mechanical. It takes what markdown can reliably tell us about structure — headings and paragraph blocks — and builds the tree skeleton. No language-specific heuristics, no sentence detection, no ambiguity.
- Split separates the human judgment (where are the joints?) from the mechanical work (UUID assignment, edge creation, skeleton extension). The translator controls granularity entirely.
- The get/put interface for split means the translator never retypes text. They copy the CLI output, add line breaks, and pipe it back. The concatenation check ensures no accidental edits.
- Split is available on unannotated leaves at any time, giving the translator flexibility to refine granularity during annotate without restarting the workflow.
- The annotate phase is a streaming read-write on one file — no CSV operations, no insertion logic beyond overwriting empty annotations.
- No cursor file is needed. The CLI derives position from the file state. This eliminates a class of synchronization bugs.
- Reset is archive-and-regenerate, not rollback. The archived file is always available. The regeneration is deterministic from the previous phase's output.
- Footnotes are handled as a lightweight sidecar during regrow, not as a parallel workflow with its own phases.
