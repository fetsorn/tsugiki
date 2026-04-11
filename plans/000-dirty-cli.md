---
title: "Layer -1: Dirty but working CLI"
status: active
layer: -1
depends: []
---

# Layer -1: Dirty but working CLI

## Goal

Get a working `tsugiki` CLI that a translator can sit down and use on a real intent. No type safety, no proptest, no streaming abstraction — just string manipulation on Fountain files and CSV appends. This is the prototype rewritten in Rust with `clap`, proving the workflow end-to-end before investing in the typed layer.

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
- **Action blocks** with trailing UUID note: `Sentence text. [[hex-id]]`
- **Multi-line action blocks**: UUID note on the first line, continuation lines follow.
- **Translator notes**: standalone `[[text]]` lines (annotate phase) or `[[| text]]` lines (regrow phase).
- **Blank lines** separate nodes.

The UUID is always the last `[[...]]` on a line that contains non-note text. Standalone `[[...]]` lines (no preceding text on the same line) are always translator notes.

### Parsing rules

A Fountain file is parsed line by line:

1. Line with `[[hex]]` where hex matches `[0-9a-f]{N}` → node boundary. If line starts with `#` → heading node, depth from `#` count. Otherwise → action node, depth 0.
2. Subsequent lines until blank: if `[[...]]` standalone → note. If plain text → continuation of multi-line action block.
3. Blank line → finalize current node.

Depth assignment: count distinct heading levels in file on first pass. Map shallowest `#` to highest depth, action blocks to depth 0.

## Commands

Five commands. That is the API for Layer -1.

### `tsugiki glean <source.md> [--dir <dir>]`

Parse a well-formed markdown file into source and structure trees.

1. Read the markdown file. Extract headings (depth levels), paragraphs, sentences, and footnotes (`[^N]` references and `[^N]:` definitions).
2. Assign UUIDs to every node. Generate the source tree (containment edges) and the structure skeleton (matching containment edges, empty content).
3. Write artifacts:
   - `prose/source.fountain` — the full source text with `[[hex-id]]` on each heading and action line.
   - `prose/structure.fountain` — the structure skeleton: headings with `[[hex-id]]` and no titles, action lines with `[[hex-id]]` and no text.
   - `prose/source.md` — a clean rendered copy of the source.
   - `csvs/source-child.csv` — source containment edges.
   - `csvs/structure-child.csv` — structure containment edges.
   - `csvs/source-structure.csv` — the 1:1 bridge (every source node to its structure shadow, same depth).
   - `csvs/source-footnote.csv` — sentence-to-footnote reference edges (if footnotes exist).
   - `csvs/.csvs.csv` and `csvs/_-_.csv` — dataset identity and schema.

4. Print a summary: number of nodes per depth level, number of footnotes, any warnings (ambiguous sentence splits).

Sentence splitting uses heuristics (period + space + uppercase, etc.) and prints warnings for ambiguous cases. The translator reviews and re-runs with corrections if needed.

If the intent directory already contains artifacts, `glean` refuses to overwrite. The translator must `reset` first or start in an empty directory.

### `tsugiki next [--dir <dir>]`

Determine the current phase by checking which artifacts exist:
- If `structure.fountain` has placeholder text → annotate phase.
- If `structure.fountain` is fully annotated and `target.fountain` is absent or incomplete → regrow phase.

Scan the relevant Fountain file for the first node that needs attention:
- In annotate phase: first structure node with placeholder annotation text.
- In regrow phase: first structure leaf node whose UUID is not in `structure-target.csv`.

Print the node with its line number, source context, and parent:

```
annotate phase
  L47 [c7f3f522] Уважаемая Алла Эдуардовна!
       parent: Обращение [aaec663b]
       structure placeholder: "Обращение"
```

In regrow phase, also check for footnotes:

```
regrow phase
  L12 [c0a28c91] anniversary marks a summit of growth
       source: За прошедшие десятилетия Институт права...
       ⚠ source has footnote: "The Institute was founded in 1988..."
```

### `tsugiki annotate <addr> "<text>" [--note "<note>"] [--dir <dir>]`

1. Resolve `<addr>` to a node in `structure.fountain`. The address can be a line number, a short hex ID, or a full UUID.
2. Overwrite the placeholder text for that node with the provided text. If `--note`, add a `[[note text]]` line after the content.
3. Print confirmation with the node's line number (which may have shifted from the write).

The write is a streaming transformation: read the file line by line, write to a temporary file, replace the original. When the target node is found, replace its content line(s) with the new text. Memory usage is O(new text), not O(file size).

No CSV writes. The bridge and containment tablets were populated at glean time.

### `tsugiki regrow <addr> "<text>" [--footnote "<footnote-text>"] [--dir <dir>]`

1. Resolve `<addr>` to a structure node.
2. Generate a new UUID for the target node.
3. Determine the target's parent: find the structure node's parent in `structure-child.csv`, then find whether that parent structure node already has a target child heading in `target.fountain`. If not, create the target heading node too (auto-generating the block-level target node, same pattern as the structure skeleton auto-generation at glean time).
4. Append to `target.fountain`: the target sentence as an action block with `[[new-uuid]]`, under the appropriate heading.
5. Append to `csvs/target-child.csv`: the containment edge.
6. Append to `csvs/structure-target.csv`: the bridge edge.
7. If `--footnote` is provided: generate a UUID for the target footnote node, append to target's `## Footnotes` section, append to `csvs/target-footnote.csv`.
8. Print confirmation.

When presenting a node for regrow (via `next`), the CLI checks `source-footnote.csv` to see if the corresponding source sentence has a footnote. The translator can provide the footnote text inline with `--footnote` or come back to it later.

### `tsugiki render <tree> [--dir <dir>]`

Render a Fountain file to clean markdown. `<tree>` is `source` or `target`.

1. Read the Fountain file line by line. Strip `[[hex-id]]` notes, `[[annotation]]` notes, and `[[| regrow notes]]`.
2. Reassemble prose with paragraph breaks. Heading markers become markdown headings.
3. Handle footnotes: walk the main text, and for each sentence that has a reference edge in `source-footnote.csv` (or `target-footnote.csv`), assign the next footnote number by order of first reference. Insert `[^N]` at the sentence boundary. Collect footnote bodies for the bottom of the document.
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
- No `status`, `validate`, `show`, `diff` — these belong in later layers.

## What this layer validates

- Does `tsugiki glean` produce well-formed source and structure trees from markdown?
- Is the `[[note]]` Fountain convention readable and writable?
- Does `tsugiki next` → `tsugiki annotate` → `tsugiki next` feel like a natural loop for the annotate phase?
- Does `tsugiki next` → `tsugiki regrow` → `tsugiki next` feel natural for the regrow phase?
- Does the footnote prompt during regrow work in practice?
- Does the streaming write (temp file + replace) work reliably?
- Does `tsugiki render` produce clean markdown that matches hand-written output?
- Can a translator make progress on real intents using only this CLI?

## Done when

- The full loop works on cong: `glean` from `source.md`, `annotate` all nodes, `regrow` all nodes, `render` both trees. Compare the CLI-produced artifacts against the existing hand-built cong intent.
- The annotate loop works on troper at scale (180+ nodes).
- The translator has used it for at least one real session and reported what felt wrong.

## Feeds into the typed layers

Findings from this layer become constraints for the typed implementation:

- Which invariants actually got violated during use → these become proptest properties.
- Which commands were awkward → these get redesigned before typing.
- Whether the markdown parser's sentence splitting heuristics work → edge cases become test fixtures.
- Whether the Fountain format has parsing edge cases → these become test fixtures.
- Whether the stateless scanning (no cursor) works at troper's scale.
- Whether footnote handling during regrow adds friction or flows naturally.
- Whether the glean → annotate → regrow → render pipeline produces artifacts consistent with hand-built intents.
