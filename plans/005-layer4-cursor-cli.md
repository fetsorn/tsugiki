---
title: "Layer 4: Session cursor and CLI"
status: active
layer: 4
adr: decisions/0003-typed-tree-model-in-rust.md
depends: [001-layer0-types, 003-layer2-serialization, 004-layer3-fountain-walk]
---

# Layer 4: Session cursor and CLI

## Goal

Implement the session cursor (single UUID tracking progress) and CLI commands from ADR-0003. The CLI is the human-facing tool that replaces the Python scripts.

## New files

```
tsugiki-cli/
  src/
    main.rs
    cursor.rs
    commands/
      mod.rs
      status.rs       — tsugiki status
      next.rs         — tsugiki next
      annotate.rs     — tsugiki annotate
      show.rs         — tsugiki show-node / show-context
      init.rs         — tsugiki init
  Cargo.toml
```

`tsugiki-cli` depends on `tsugiki-core`.

## Session cursor (`cursor.rs`)

```rust
pub struct Cursor {
    pub current: NodeId,
    pub phase: Phase,
}

pub enum Phase {
    Decompose,
    Understand,
    Regrow,
}
```

Persisted as `.tsugiki-cursor` file in the intent directory:
```
phase=understand
current=cfb0b898-full-uuid-here
```

- `Cursor::load(intent_dir) -> Result<Option<Cursor>>`
- `Cursor::save(&self, intent_dir) -> Result<()>`
- `Cursor::advance(&mut self, tree: &impl TreeWalk<T>) -> Option<NodeId>` — move to next unmapped node.

### Advance logic for Understand phase

1. Current node is in source tree.
2. Check if current has a bridge to structure tree (grep `source-structure.csv`).
3. If mapped, find next sibling or next parent's sibling (DFS post-order).
4. If unmapped, stay (this is the node to annotate).

### Advance logic for Regrow phase

Same pattern but walks structure tree and checks `structure-target.csv`.

## Index tablet

`source-line.csv` (and `structure-line.csv`, `target-line.csv`) map short IDs to line numbers in their Fountain files. Schema addition to `_-_.csv`: `source,line` / `structure,line` / `target,line`.

Built on first access by scanning the Fountain file for `.{short_id}` lines. Stored in CSVS. Queried by `FountainWalk` methods instead of grepping the Fountain file on every call. Rebuilt when a Fountain file is modified (detected via mtime or explicit invalidation after `annotate`/`regrow` writes).

This gives O(1) UUID→line-number lookup after the first scan, while keeping the data in CSVS where it belongs.

## CLI commands

### `tsugiki init <dir>`

Replaces `scaffold_intent.py`. Creates directory layout, empty CSVs, `.csvs.csv`, `_-_.csv`, `.tsugiki-config`.

### `tsugiki status [<dir>]`

Replaces `understand_status.py`. Shows:
- Current phase and cursor position.
- Count of nodes by depth, mapped vs unmapped.
- Progress bar or fraction.

Uses `FountainWalk` — no full tree load.

### `tsugiki next [<dir>]`

Advance cursor to next unmapped node. Print:
- Node ID (short).
- Source text.
- Parent context (paragraph heading).
- Sibling context (previous/next sentence if available).

### `tsugiki annotate <short_id> <text> [<dir>]`

Core understand operation. Does exactly:
1. Look up source node by short id.
2. Find parent paragraph's structure node (via source-structure + structure-child).
3. Generate new structure UUID.
4. Insert into `structure.fountain` after paragraph heading (streaming insert).
5. Append to `structure-child.csv`.
6. Append to `source-structure.csv`.
7. Advance cursor.

Replaces `insert_structure.py` for single annotations.

### `tsugiki show-node <short_id> [<dir>]`

Print node's text, depth, parent, children, bridge target (if any).

### `tsugiki show-context <short_id> [<dir>]`

Print the node's surrounding context: parent, siblings, bridge targets. Useful during annotation to see what's around the current sentence.

### `tsugiki batch-annotate <annotations_file> [<dir>]`

Replaces `insert_structure.py` for batch operations. Parses the annotations file format (same as current script), runs `annotate` for each entry.

## Argument parsing

Use `clap` with derive API. Intent directory defaults to current directory.

```rust
#[derive(Parser)]
#[command(name = "tsugiki")]
enum Cli {
    Init { dir: PathBuf },
    Status { dir: Option<PathBuf> },
    Next { dir: Option<PathBuf> },
    Annotate {
        id: String,
        text: String,
        #[arg(long)]
        note: Option<String>,
        dir: Option<PathBuf>,
    },
    ShowNode { id: String, dir: Option<PathBuf> },
    ShowContext { id: String, dir: Option<PathBuf> },
    BatchAnnotate {
        annotations: PathBuf,
        dir: Option<PathBuf>,
    },
    Decompose {
        source: PathBuf,
        #[arg(long, default_value = "8")]
        short_len: usize,
        dir: Option<PathBuf>,
    },
    Regrow {
        id: String,
        text: String,
        #[arg(long)]
        note: Option<String>,
        dir: Option<PathBuf>,
    },
    RegrowNext {
        dir: Option<PathBuf>,
    },
    Render {
        tree: String,  // "source", "target", "structure"
        dir: Option<PathBuf>,
    },
}
```

## Tests

- Unit: cursor advance on a small tree with partial mapping.
- Unit: cursor persistence (save + load roundtrip).
- Integration: `init` creates correct directory layout.
- Integration: `annotate` modifies fountain and CSVs correctly.
- Integration: `status` output matches manual count.
- Integration: `next` skips already-mapped nodes.

### `tsugiki decompose <source.md> [<dir>]`

Phase 1 entry point. Runs the parser (Layer 5) on source markdown, presents warnings interactively, writes `source.fountain` + `source-child.csv` on approval. Sets cursor to phase=understand, current=first leaf.

Steps:
1. Parse markdown → `ParseResult<Source>` (Layer 5).
2. Present each `ParseWarning` to the human. Accept/reject/override.
3. Validate the approved tree.
4. Render to `source.fountain` (Layer 2).
5. Write `source-child.csv` (Layer 2).
6. Build and write `source-line.csv` index.
7. Set cursor to `phase=understand, current=<first leaf>`.

### `tsugiki regrow <short_id> <text> [<dir>]`

Phase 3 write operation. Writes a target sentence for a structure node.

Steps:
1. Look up structure node by short id. Verify it exists and is a leaf (Depth 3).
2. Find the structure node's parent → determine target block context.
3. If the target block doesn't exist yet, prompt the human to name it (or auto-create from structure block annotation).
4. Generate new target UUID.
5. Insert into `target.fountain` (streaming insert under the target block heading).
6. Append to `target-child.csv`.
7. Append to `structure-target.csv`.
8. Optionally add a regrow-phase note: prompt for parenthetical, write to `structure.fountain` with `|` prefix.
9. Advance cursor.

### `tsugiki regrow-next [<dir>]`

Advance cursor to next unmapped structure leaf (checks `structure-target.csv`). Print structure annotation, parentheticals, source text (via bridge), and target block context.

### `tsugiki render <source|target|structure> [<dir>]`

Generate clean markdown from Fountain. Strips UUIDs, scene headings, section markers, parentheticals. Reassembles prose with paragraph breaks.

- `tsugiki render source` → `source.md`
- `tsugiki render target` → `target.md`
- `tsugiki render structure` → `structure.md` (optional, for review)

Respects multiline nodes. Preserves the translator's exact text — no reformatting, no normalization.

This closes the circle: markdown → fountain → csvs → fountain → markdown.

## Replaces these Python scripts

| Python script | CLI command |
|---|---|
| `scaffold_intent.py` | `tsugiki init` |
| `decompose.py` | `tsugiki decompose` |
| `understand_status.py` | `tsugiki status` |
| `insert_structure.py` | `tsugiki batch-annotate` |
| (manual process) | `tsugiki annotate`, `tsugiki next` |
| (manual process) | `tsugiki regrow`, `tsugiki regrow-next` |
| (manual process) | `tsugiki render source\|target` |

## Done when

- All CLI commands work on a real intent directory (troper).
- Cursor correctly tracks progress across sessions.
- `decompose` parses source markdown and produces valid source tree.
- `regrow` writes target sentences with proper bridge edges.
- `render source` and `render target` produce clean markdown matching hand-written originals.
- Index tablets (`source-line.csv` etc.) built and queried correctly.
- Full circle demonstrated: `source.md` → `decompose` → understand → regrow → `render target` → `target.md`.
- Python scripts are no longer needed for any phase.
