---
title: "Layer -1: Dirty but working CLI"
status: active
layer: -1
adr: decisions/0003-typed-tree-model-in-rust.md
depends: []
---

# Layer -1: Dirty but working CLI

## Goal

Get a working `tsugiki` CLI that a translator can sit down and use on a real intent (troper). No type safety, no proptest, no streaming abstraction — just string manipulation on Fountain files and CSV appends. This is the Python scripts rewritten in Rust with `clap`, proving the workflow end-to-end before investing in the typed layer.

## Why this comes first

The 6-layer plan (types → memtree → serialization → streaming → cursor → parser) builds correctness from the bottom up. But correctness of a tool nobody has used yet is speculative. This layer builds *usability* first: does the workflow feel right? Are the CLI commands the right granularity? Does the Fountain format work in practice with `[[note]]` conventions? Answers to these questions will feed back into Layers 0–5, possibly changing them.

## Crate setup

Single binary crate. No library separation yet.

```
tsugiki/
  src/
    main.rs        — clap dispatch + all logic
  Cargo.toml
```

Dependencies: `clap` (derive), `uuid`, `regex` (for Fountain parsing).

## Fountain conventions (new)

This layer implements the updated Fountain format from ADR-0003:

- **Section headings** with inline UUID: `## Title [[hex-id]]`
- **Action blocks** with trailing UUID: `Sentence text. [[hex-id]]`
- **Multi-line action blocks**: UUID on the first line: `First line [[hex-id]]`
- **Translator notes**: standalone `[[text]]` lines (understand phase) or `[[| text]]` (regrow phase)
- **Blank lines** separate nodes

The UUID is always the last `[[...]]` on a line that contains non-note text. Standalone `[[...]]` lines (no preceding text) are always translator notes.

### Parsing rules

A Fountain file is parsed line by line. State machine:

1. Line with `[[hex]]` where hex matches `[0-9a-f]{N}` → node boundary. If line starts with `#` → heading node, depth from `#` count. Otherwise → action node, depth 0.
2. Subsequent lines until blank: if `[[...]]` standalone → note. If plain text → continuation of multi-line action block.
3. Blank line → finalize node.

Depth assignment: count distinct heading levels in file on first pass (or read from cached metadata). Map shallowest `#` to highest depth, action blocks to depth 0. Store mapping for the session.

## Commands

Two commands. That's the API.

### `tsugiki next [--dir <dir>]`

Read cursor from `phase-cursor.csv`. If understand phase: walk `source.fountain` looking for the next sentence-level node whose UUID is not in `source-structure.csv`. Print:

```
[c7f3f522] Уважаемая Алла Эдуардовна!
  parent: Обращение [aaec663b]
```

If regrow phase: same but walk `structure.fountain`, check `structure-target.csv`.

Update cursor to point to this node.

### `tsugiki annotate <short-id> "<text>" [--note "<note>"] [--dir <dir>]`

1. Grep `source.fountain` for `[[{short-id}]]` → get source node, find its parent heading.
2. Grep `source-structure.csv` for the parent's full UUID → find parent's structure node.
3. Generate new UUID for the structure sentence node.
4. If the parent block has only this one sentence child (check `source-child.csv`): also auto-generate the block-level structure node with the same annotation text, insert into `structure.fountain` as a heading, append to `structure-child.csv`.
5. Insert into `structure.fountain`: action line with `[[new-uuid]]` under the parent structure heading. If `--note`, add `[[note text]]` line after.
6. Append to `structure-child.csv`: `{parent-structure-uuid},{new-uuid}`.
7. Append to `source-structure.csv`: `{source-uuid},{new-uuid}`.
8. Advance cursor.

The Fountain insertion: read file, find the heading line containing `[[parent-structure-uuid]]`, find the end of that heading's children (next heading of same or shallower depth, or EOF), insert new lines before that boundary.

## What this layer does NOT do

- No `Depth` newtype, no phantom types, no smart constructors — depths are `u8`, tree kinds are strings.
- No `MemTree` — all operations work directly on files.
- No proptest — correctness comes from running on real intents and eyeballing.
- No streaming abstraction — each command reads what it needs via grep/regex.
- No parser contract — `decompose` is deferred (troper is already decomposed; cong is already complete).
- No `TreeWalk` trait — each command knows which files to read.
- No `init`, `status`, `show`, `diff`, `render`, `regrow` — these belong in later layers once the core loop is validated.

## What this layer validates

- Is the `[[note]]` Fountain convention readable and writable?
- Does the auto-copy for single-sentence blocks work in practice?
- Does `tsugiki next` → `tsugiki annotate` → `tsugiki next` feel like a natural loop?
- Can a translator make progress on the understand phase of troper using only this CLI?

## Done when

- `tsugiki next` + `tsugiki annotate` loop works on troper (which already has a decomposed source tree).
- The translator (you) has used it for at least one real session on troper and reported what felt wrong.

## Deferred to later layers

These commands are real but not needed to validate the core loop:

- `tsugiki init` — scaffold a new intent directory (Layer 4)
- `tsugiki status` — progress counts, mapped vs unmapped (Layer 4)
- `tsugiki show <id>` — inspect a node and its relationships (Layer 4)
- `tsugiki diff` — tree-level changelog against last git commit (Layer 4)
- `tsugiki render <tree>` — Fountain → clean markdown (Layer 4)
- `tsugiki regrow <id> <text>` — write target sentences (Layer 4)
- `tsugiki decompose <source>` — parse source text into source tree (Layer 5)

## Feeds into Layer 0

Findings from this layer become constraints for the typed implementation:
- Which invariants actually got violated during use → these become proptest properties.
- Which commands were awkward → these get redesigned before typing.
- Whether `[[note]]` parsing has edge cases → these become test fixtures.
- Whether the cursor model works linearly → confirms or revises Layer 4's assumptions.
