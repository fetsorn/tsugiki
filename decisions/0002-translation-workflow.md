---
status: accepted
date: 2026-04-11
---

# Translation workflow

## Context and Problem Statement

ADR-0001 defines the three-tree data model: source, structure, and target. This decision describes the workflow that populates those trees — the sequence of phases a translator walks through, what happens in each, and how the artifacts change.

The workflow must work in two modes: an AI-assisted session where the translator and an AI interlocutor walk through the text together, and a standalone CLI where the translator works alone. Both modes must produce the same artifacts.

## The three phases

### Phase 1: Glean

The name "decompose" is commonly used for this step, but the important act is not breaking the text down — it is gleaning the rhetorical skeleton that the translator commits to uphold through the entire translation. This is the first act of translation.

The translator provides a source text as a well-formed markdown file. Markdown input is required, not optional — the parser needs unambiguous structure (headings for sections, paragraphs for blocks, sentences within paragraphs) and unambiguous footnotes (`[^N]` references and `[^N]:` definitions). Plain text input is not supported because recovering structure from unformatted prose is unreliable, and recovering footnotes from it is impossible.

The parser reads the markdown, breaks it into a tree of structural units, and produces both the source tree and the structure tree skeleton simultaneously. Because of the structural fidelity invariant (ADR-0001), every source node gets a shadow structure node at the same depth, and the bridge tablets (`source-structure.csv`, `source-child.csv`, `structure-child.csv`) are fully populated at this step.

The structure skeleton in `structure.fountain` comes out with headings, UUIDs, and empty content. Action lines have no text; heading lines have the heading marker and UUID but no title. The annotate phase fills in the real annotations.

Footnotes are extracted from the markdown, placed in a `## Footnotes` section at the bottom of `source.fountain`, and their reference edges are written to `source-footnote.csv`.

**Artifacts produced:**
- `prose/source.fountain` — the full source text, annotated with UUIDs and section markers.
- `prose/structure.fountain` — the structure skeleton with placeholder annotations.
- `prose/source.md` — a clean rendered copy of the source text.
- `csvs/source-child.csv` — source containment edges.
- `csvs/structure-child.csv` — structure depth edges.
- `csvs/source-structure.csv` — the 1:1 bridge between source and structure.
- `csvs/source-footnote.csv` — sentence-to-footnote reference edges.

**Sentence boundary decisions.** Most prose has obvious sentence breaks. Some texts — legal documents, liturgical texts, stream-of-consciousness prose — have sentences that run for paragraphs. The parser uses heuristics and flags ambiguous splits as warnings. The translator reviews and approves or corrects. The tool does not force a granularity.

### Phase 2: Annotate

The translator walks the structure tree node by node and writes a short annotation for each — naming what that piece of the text is doing, not what it says.

The annotations are written in whatever language the translator thinks in. They are informal, often blunt. "pharaoh addressing pharaoh" or "you rock!" — the point is to name the rhetorical move so precisely that someone who does not speak the source language could reconstruct the intent from the structure tree alone.

The CLI presents each node by scanning `structure.fountain` for the first node that still has empty content, showing the corresponding source sentence alongside it. The translator writes the annotation, and the CLI writes it into `structure.fountain`. No CSV writes happen during this phase — the bridge and containment tablets are already populated from glean.

**The pass is idempotent.** If the translator restarts, the CLI scans from the top and skips nodes that already have annotations (non-empty content). There is no cursor file. The position is derived from the file every time. For troper's 180-node structure file, scanning for the first unannotated node is instantaneous.

The translator also records notes — observations about nuance, tone, cultural load, or translation pitfalls — as `[[text]]` lines after the annotation. These are working notes consulted again during regrow.

**Artifacts modified:**
- `prose/structure.fountain` — placeholder annotations replaced with real ones, notes added.

**In AI-assisted mode**, this is where the AI is most useful. The AI reads the source sentence, asks the translator what it is doing, and the translator's answer becomes the annotation. The AI can probe: "is the list of government bodies important, or is the point that graduates are everywhere?" The translator answers, and the answer sharpens the annotation.

### Phase 3: Regrow

The translator walks the structure tree and, for each structure node, writes target text that expresses that meaning in the target language. Each invocation of the regrow command creates exactly one target node. If a single structure node needs multiple target sentences (1:N in `structure-target.csv`), the translator calls regrow N times against the same structure node. The target tree may differ from the source tree in sequence, in the number of sentences per block, and in block structure — but every target leaf traces back to a structure node through `structure-target.csv`.

This is where the translator's craft lives. The AI does not write target-language text. In AI-assisted mode, the AI can ask about choices: "the source packs two ideas into one sentence — are you keeping that or splitting?" But the words are the translator's.

During regrow, the structure tree is a living document. The translator adds `[[| text]]` notes (pipe prefix) about why they made a particular choice, or refines the annotation now that they see how it lands in the target language. The structure tree grows across phases 2 and 3.

**Footnote handling.** When the CLI presents a structure node whose source sentence has a footnote (detected by traversing source-structure → source → source-footnote), it prompts: the source footnote text is shown, and the translator is asked whether to attach a footnote to this target sentence. If yes, the translator writes the footnote translation, and the CLI places it in the target's `## Footnotes` section and writes the reference edge to `target-footnote.csv`. If the source sentence was split into multiple target sentences, the translator chooses which one gets the footnote.

Footnotes are translated directly — no annotation pass, no structure nodes. They are stray ideas, and the translation is usually straightforward (citations, references, bibliographic entries).

**Artifacts produced:**
- `prose/target.fountain` — the translated text with UUIDs and section markers.
- `prose/target.md` — a clean rendered copy of the translation.
- `prose/structure.fountain` — updated with `[[| text]]` notes from regrow.
- `csvs/target-child.csv` — target containment edges.
- `csvs/structure-target.csv` — structure-to-target bridge.
- `csvs/target-footnote.csv` — target sentence-to-footnote reference edges.

## Addressing

Line numbers are the natural way to point into a Fountain file from the terminal. `tsugiki next` prints a line number alongside each node; `tsugiki annotate 104 "text"` writes to the node at that line. UUIDs and short hex IDs also work as addresses.

Line numbers are ephemeral — they shift on every write — and are never persisted in CSVS. They are a runtime convenience, derived fresh from the file each time the CLI runs. UUIDs are the stable identity in CSVS; line numbers are the human interface in the terminal.

## Reset

The phases are sequential: glean, then annotate, then regrow. Once a phase is complete, the translator moves forward. If the translator wants to correct a mistake in a completed phase, they edit the Fountain file directly — the files are human-readable and human-writable by design.

If the translator wants to redo a phase from scratch, the CLI provides reset commands:

- **`tsugiki reset annotate`**: archive `structure.fountain` as `structure.{ISO-timestamp}.fountain` in place (preserving the `.fountain` extension so tools can read it), then regenerate the skeleton from the source tree. All annotations are lost; the archived file is the safety net.
- **`tsugiki reset regrow`**: archive `target.fountain`, `target-child.csv`, `structure-target.csv`, and `target-footnote.csv` with ISO timestamps. Clear the target artifacts. The translator starts regrow from the beginning.

Archives are never overwritten. If the translator resets twice in one day, both archives coexist with different timestamps (e.g., `structure.2026-04-11T14:30:00Z.fountain` and `structure.2026-04-11T16:45:00Z.fountain`). Moving or deleting archives is the translator's concern.

## Rendering

`tsugiki render source` and `tsugiki render target` produce clean markdown from the Fountain files by stripping UUIDs, section markers, and translator notes, then reassembling the prose with paragraph breaks.

The renderer handles footnotes: it walks the main text, and for each sentence that has a reference edge in `source-footnote.csv` (or `target-footnote.csv`), it assigns the next footnote number, inserts `[^N]` into the markdown output at the sentence boundary, and collects the footnote body for the bottom of the document. Footnote numbers are derived from order of first reference — they exist only in the rendered markdown, never in the Fountain files.

## Workflow summary

| Phase    | Translator does                            | Artifacts written                                                            | Structure tree status                |
|----------|--------------------------------------------|------------------------------------------------------------------------------|--------------------------------------|
| Glean    | Provides markdown source, approves parse   | source.fountain, structure.fountain (skeleton), source.md, 4 CSV tablets     | Skeleton with placeholders           |
| Annotate | Names the rhetorical move of each node     | structure.fountain (annotations filled in, notes added)                      | Populated with annotations and notes |
| Regrow   | Writes target text for each structure node | target.fountain, target.md, structure.fountain (regrow notes), 3 CSV tablets | Updated with regrow-phase notes      |

## Consequences

- The structural fidelity invariant means glean produces both trees and the bridge in one pass. Annotate writes to one file. The complexity of tree construction is front-loaded into glean where the parser can validate it.
- The annotate phase is a streaming read-write on one file — no CSV operations, no parent resolution, no insertion logic beyond overwriting placeholders.
- No cursor file is needed. The CLI derives position from the file state. This eliminates a class of synchronization bugs (stale cursor, cursor pointing at a deleted node, cursor from a different phase).
- The sequential phase model is simple but firm. Corrections to completed phases happen through direct file editing, not through the CLI. This keeps the tool's state machine trivial.
- Reset is archive-and-regenerate, not rollback. The archived file is always available. The regeneration is deterministic from the previous phase's output.
- Footnotes are handled as a lightweight sidecar during regrow, not as a parallel workflow with its own phases.
