---
status: proposed
date: 2026-04-11
---

### Footnotes are stray ideas

Footnotes — citations, hedges, tangential remarks, digressions the author could not resist — are outside the communicative intent of the text. If a footnote contained relevant evidence, that evidence would be in the main text. Footnotes do not participate in the rhetorical arc.

Source footnotes live in a `## Footnotes` section at the bottom of `source.fountain`, as regular leaf nodes. Target footnotes live in the same position in `target.fountain`. No structure nodes are generated for footnotes. They are not walked during annotate and are translated directly during regrow. Footnote bodies are always single leaves — they are not split further.

Footnote numbers do not appear in Fountain files. Numbers are rendering artifacts, assigned at render time by order of first reference in the main text. If the translator reorders the target text, footnotes renumber automatically.

The relationship between a text node and its footnote is recorded as a reference edge: `source-footnote.csv` connects a source node to its footnote, `target-footnote.csv` does the same for the target. During regrow, the CLI traverses the graph (source node → source-footnote → footnote → source-structure → structure) to prompt: "this node's source has a footnote — attach it here?"

The source-to-target footnote correspondence is derived through graph traversal, not stored directly. Each source footnote maps to exactly one target footnote. The translator may also add footnotes that exist only in the target, with no source counterpart.

Footnotes stay outside the rhetorical structure. Their numbering is a render-time artifact, their correspondence is derived through graph traversal, and their translation is direct — no annotation pass needed.

Footnotes are extracted from the markdown, placed in a `## Footnotes` section at the bottom of `source.fountain`, and their reference edges are written to `source-footnote.csv`. Each footnote becomes a single leaf node — footnote bodies are not split.

**Footnote handling.** When the CLI presents a structure node whose source has a footnote (detected by traversing source-structure → source → source-footnote), it prompts: the source footnote text is shown, and the translator is asked whether to attach a footnote to this target node. If yes, the translator writes the footnote translation, and the CLI places it in the target's `## Footnotes` section and writes the reference edge to `target-footnote.csv`. If the source node was split into multiple target nodes, the translator chooses which one gets the footnote.

Footnotes are translated directly — no annotation pass, no structure nodes. They are stray ideas, and the translation is usually straightforward.

init reads the markdown file. Extract headings (depth levels), paragraph blocks (text between blank lines under a heading), and footnotes (`[^N]` references and `[^N]:` definitions).

next shows footnote if one exists

regrow if `--footnote` is provided: generate a UUID for the target footnote node, append to target's `## Footnotes` section, append to `csvs/target-footnote.csv`.

The renderer handles footnotes: it walks the main text, and for each node that has a reference edge in `source-footnote.csv` (or `target-footnote.csv`), it assigns the next footnote number, inserts `[^N]` into the markdown output at the node boundary, and collects the footnote body for the bottom of the document. Footnote numbers are derived from order of first reference — they exist only in the rendered markdown, never in the Fountain files.

**Footnote reference edge** connects a node to its footnote block within the same tree:

```rust
pub struct FootnoteEdge<T: TreeKind> {
    node: Uuid,
    footnote: Uuid,
    _tree: PhantomData<T>,
}
```

// Check: every footnote reference and definition are paired
