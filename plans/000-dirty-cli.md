---
title: "Layer -1: Dirty but working CLI"
status: active
layer: -1
depends: []
---

# Layer -1: Dirty but working CLI

## Goal

Get a working `tsugiki` CLI that a translator can sit down and use on a real intent. No type safety, no proptest, no streaming abstraction — just string manipulation on Fountain files and CSV appends. This is the prototype in Rust with `clap`, proving the workflow end-to-end before investing in the typed layer.

## Why this comes first

The layered plan (types → memtree → serialization → streaming → parser) builds correctness from the bottom up. But correctness of a tool nobody has used yet is speculative. This layer builds usability first: does the workflow feel right? Are the CLI commands the right granularity? Does the Fountain format work in practice? Answers feed back into the typed layers, possibly changing them.

## Crate setup

Single binary crate. No library separation yet.

```
tsugiki/
  src/
    main.rs        — clap dispatch + all logic
  Cargo.toml
```

Dependencies: `clap` (derive), `uuid`, `regex` (for Fountain parsing).

## Fountain conventions

This layer implements the Fountain format from ADR-0001 and ADR-0003:

- **Section headings** with inline UUID note: `## Title [[hex-id]]`
- **Action blocks** with trailing UUID note: `Text content. [[hex-id]]`
- **Multi-line action blocks**: UUID note on the first line, continuation lines follow.
- **Translator notes**: standalone `[[text]]` lines (annotate phase) or `[[| text]]` lines (regrow phase).
- **Blank lines** separate nodes.

The UUID is always the last `[[...]]` on a line that contains non-note text. Standalone `[[...]]` lines (no preceding text on the same line) are always translator notes.

### Parsing rules

A Fountain file is parsed line by line:

1. Line with `[[hex]]` where hex matches `[0-9a-f]{N}` → node boundary. If line starts with `#` → heading node, depth from `#` count. Otherwise → action node at the deepest level.
2. Subsequent lines until blank: if `[[...]]` standalone → note. If plain text → continuation of multi-line action block.
3. Blank line → finalize current node.

Depth assignment: count distinct heading levels in file on first pass. Map shallowest `#` to Depth 1, next `##` to Depth 2, and so on. Action blocks get the next depth after the deepest heading level. The implicit root at Depth 0 has no Fountain marker.

## Commands

Seven commands. That is the API for Layer -1.

### `tsugiki init <source.md> [--dir <dir>]`

Parse a well-formed markdown file into source and structure trees.

1. Read the markdown file. Extract headings (depth levels), paragraph blocks (text between blank lines under a heading), and footnotes (`[^N]` references and `[^N]:` definitions).
2. Assign UUIDs to every node. Headings become inner nodes, paragraphs become leaves. Generate the source tree (containment edges) and the structure skeleton (matching containment edges, empty content).
3. Write artifacts:
   - `prose/source.fountain` — the full source text with `[[hex-id]]` on each heading and action line.
   - `prose/structure.fountain` — the structure skeleton: headings with `[[hex-id]]` and no titles, action lines with `[[hex-id]]` and no text.
   - `prose/source.md` — a clean rendered copy of the source.
   - `csvs/source-child.csv` — source containment edges.
   - `csvs/structure-child.csv` — structure containment edges.
   - `csvs/source-structure.csv` — the 1:1 bridge (every source node to its structure shadow).
   - `csvs/source-footnote.csv` — node-to-footnote reference edges (if footnotes exist).
   - `csvs/.csvs.csv` and `csvs/_-_.csv` — dataset identity and schema.
4. Print a summary: number of nodes per depth level, number of footnotes.

No sentence splitting. Paragraphs are the initial leaves. The translator refines granularity with `split`.

If the intent directory already contains artifacts, `init` refuses to overwrite. The translator must `reset` first or start in an empty directory.

### `tsugiki split <addr> [--dir <dir>]`

Split has two modes based on the presence of stdin.

**Get mode** (no stdin): resolve `<addr>` to a leaf node in `source.fountain`. Print its text to stdout. Nothing is written.

**Put mode** (stdin present): read lines from stdin. Each line becomes a new leaf node.

1. Concatenate input lines (stripping whitespace) and compare against the original leaf text. If they don't match, error — split is cutting, not editing.
2. If input is a single line matching the original, no-op.
3. The original leaf becomes an inner node. Each input line becomes a child leaf at depth = original depth + 1.
4. Generate UUIDs for each new leaf. Create matching structure skeleton nodes with empty content. Create bridge edges.
5. Update `prose/source.fountain`: replace the original leaf's content with the new child nodes under a heading (if the original was an action block, it becomes a section grouping its children).
6. Update `prose/structure.fountain`: add matching skeleton nodes.
7. Append to `csvs/source-child.csv`, `csvs/structure-child.csv`, `csvs/source-structure.csv`.
8. Print confirmation with new leaf count and their line numbers.

**Guard:** refuse to split a leaf whose corresponding structure node has a non-empty annotation. This prevents orphaning annotations. The translator must be in the split or pre-annotate stage for that leaf.

### `tsugiki next [--dir <dir>]`

Determine the current phase by checking which artifacts exist:
- If `structure.fountain` has nodes with empty content → annotate phase (or split phase — the translator decides).
- If `structure.fountain` is fully annotated and `target.fountain` is absent or incomplete → regrow phase.

Scan the relevant Fountain file for the first node that needs attention:
- In annotate phase: first structure node with empty annotation text.
- In regrow phase: first structure leaf whose UUID is not in `structure-target.csv`.

Print the node with its line number, source context, and parent:

```
annotate phase
  L47 [c7f3f522] Уважаемая Алла Эдуардовна!
       parent: Обращение [aaec663b]
```

In regrow phase, also check for footnotes:

```
regrow phase
  L12 [c0a28c91] anniversary marks a summit of growth
       source: За прошедшие десятилетия Институт права...
       ⚠ source has footnote: "The Institute was founded in 1988..."
```

### `tsugiki show <addr> [--dir <dir>]`

Display a single node with its full context. Resolve `<addr>` to a node in any tree.

Print:
- The node's text, line number, and UUID.
- Its parent node (text and UUID).
- Its children (if any).
- Its bridge counterpart(s): for a source node, the corresponding structure annotation; for a structure node, the source text and any target text; for a target node, the structure annotation it expresses.
- Any translator notes attached.
- Footnote reference if one exists.

This is the "look around" command for orienting yourself at any point in the workflow.

### `tsugiki annotate <addr> "<text>" [--note "<note>"] [--dir <dir>]`

1. Resolve `<addr>` to a node in `structure.fountain`. The address can be a line number, a short hex ID, or a full UUID.
2. Overwrite the empty content for that node with the provided text. If `--note`, add a `[[note text]]` line after the content.
3. Print confirmation with the node's line number (which may have shifted from the write).

The write is a streaming transformation: read the file line by line, write to a temporary file, replace the original. When the target node is found, replace its content with the new text. Memory usage is proportional to the new text, not the file size.

No CSV writes. The bridge and containment tablets were populated at init and split time.

### `tsugiki regrow <addr> "<text>" [--footnote "<footnote-text>"] [--dir <dir>]`

1. Resolve `<addr>` to a structure leaf node.
2. Generate a new UUID for the target node.
3. Determine the target's parent: find the structure node's parent in `structure-child.csv`, then find whether that parent structure node already has target children via `structure-target.csv` and `target-child.csv`. If the parent-level target heading does not exist, create it (auto-generating the block-level target node, same pattern as the structure skeleton auto-generation at init time).
4. Append to `target.fountain`: the target text as an action block with `[[new-uuid]]`, under the appropriate heading.
5. Append to `csvs/target-child.csv`: the containment edge.
6. Append to `csvs/structure-target.csv`: the bridge edge.
7. If `--footnote` is provided: generate a UUID for the target footnote node, append to target's `## Footnotes` section, append to `csvs/target-footnote.csv`.
8. Print confirmation.

When presenting a node for regrow (via `next`), the CLI checks `source-footnote.csv` to see if the corresponding source node has a footnote. The translator can provide the footnote text inline with `--footnote` or come back to it later.

### `tsugiki render <tree> [--dir <dir>]`

Render a Fountain file to clean markdown. `<tree>` is `source` or `target`.

1. Read the Fountain file line by line. Strip `[[hex-id]]` notes, `[[annotation]]` notes, and `[[| regrow notes]]`.
2. Reassemble prose with paragraph breaks. Heading markers become markdown headings.
3. Handle footnotes: walk the main text, and for each node that has a reference edge in `source-footnote.csv` (or `target-footnote.csv`), assign the next footnote number by order of first reference. Insert `[^N]` at the node boundary. Collect footnote bodies for the bottom of the document.
4. Write the output to `prose/source.md` or `prose/target.md`.

### Addressing

All commands that take `<addr>` accept three forms:

- **Line number**: `tsugiki annotate 47 "polite supplication"` — resolved against the current file state.
- **Short hex ID**: `tsugiki annotate c7f3f522 "polite supplication"` — grepped from the Fountain file.
- **Full UUID**: for scripting and programmatic use.

Line numbers are the most natural for interactive use. They are never stored — the next `tsugiki next` will print fresh line numbers.

## What this layer does NOT do

- No `Depth` newtype, no phantom types, no smart constructors — depths are `u8`, tree kinds are strings.
- No `MemTree` — all operations work directly on files.
- No proptest — correctness comes from running on real intents and eyeballing.
- No streaming abstraction — each command reads what it needs via grep/regex.
- No `TreeWalk` trait — each command knows which files to read.
- No `reset` — the reset mechanism (archive + regenerate) belongs in later layers.
- No `status`, `check` — these belong in later layers.

## What this layer validates

- Does `tsugiki init` produce well-formed source and structure trees from markdown?
- Does the `[[note]]` Fountain convention read and write cleanly?
- Does `tsugiki split` (get/put) feel natural for decomposing paragraphs into working units?
- Does `tsugiki next` → `tsugiki annotate` → `tsugiki next` feel like a natural loop for the annotate phase?
- Does `tsugiki next` → `tsugiki regrow` → `tsugiki next` feel natural for the regrow phase?
- Does the footnote prompt during regrow work in practice?
- Does the streaming write (temp file + replace) work reliably?
- Does `tsugiki render` produce clean markdown that matches hand-written output?
- Can a translator make progress on real intents using only this CLI?
- Does splitting mid-annotate work smoothly, or does it create friction?
- Does `tsugiki show` provide enough context for orientation?

## Done when

- The full loop works on a real intent: `init` from a markdown source, `split` paragraph leaves into working units, `annotate` all nodes, `regrow` all nodes, `render` both trees.
- The split/annotate loop works at scale (100+ nodes).
- The translator has used it for at least one real session and reported what felt wrong.

## Feeds into the typed layers

Findings from this layer become constraints for the typed implementation:

- Which invariants actually got violated during use → these become proptest properties.
- Which commands were awkward → these get redesigned before typing.
- Whether the Fountain format has parsing edge cases → these become test fixtures.
- Whether the stateless scanning (no cursor) works at scale.
- Whether footnote handling during regrow adds friction or flows naturally.
- Whether split's get/put interface works for both standalone and AI-assisted modes.
- Whether leaves at different depths cause any practical problems.
- Whether the init → split → annotate → regrow → render pipeline produces artifacts consistent with hand-built intents.
