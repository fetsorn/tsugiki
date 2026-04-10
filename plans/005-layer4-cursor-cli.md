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
    pub current: ShortId,
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
current=cfb0b898
```

- `Cursor::load(intent_dir) -> Result<Option<Cursor>>`
- `Cursor::save(&self, intent_dir) -> Result<()>`
- `Cursor::advance(&mut self, tree: &impl TreeWalk<T>) -> Option<ShortId>` — move to next unmapped node.

### Advance logic for Understand phase

1. Current node is in source tree.
2. Check if current has a bridge to structure tree (grep `source-structure.csv`).
3. If mapped, find next sibling or next parent's sibling (DFS post-order).
4. If unmapped, stay (this is the node to annotate).

### Advance logic for Regrow phase

Same pattern but walks structure tree and checks `structure-target.csv`.

## CLI commands

### `tsugiki init <dir>`

Replaces `scaffold_intent.py`. Creates directory layout, empty CSVs, `.csvs.csv`, `_-_.csv`.

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
}
```

## Tests

- Unit: cursor advance on a small tree with partial mapping.
- Unit: cursor persistence (save + load roundtrip).
- Integration: `init` creates correct directory layout.
- Integration: `annotate` modifies fountain and CSVs correctly.
- Integration: `status` output matches manual count.
- Integration: `next` skips already-mapped nodes.

## Replaces these Python scripts

| Python script | CLI command |
|---|---|
| `scaffold_intent.py` | `tsugiki init` |
| `understand_status.py` | `tsugiki status` |
| `insert_structure.py` | `tsugiki batch-annotate` |
| (manual process) | `tsugiki annotate`, `tsugiki next` |

## Done when

- All CLI commands work on a real intent directory (troper).
- Cursor correctly tracks progress across sessions.
- Python scripts are no longer needed for Phase 2 workflow.
