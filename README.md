<div align="center">

part of the [ontonomy](https://norcivilianlabs.org) software suite

AGPL-3.0. Anton Davydov.

</div>

# tsugiki

A structured translation assistant.

tsugiki breaks a document into three trees: source (what was
written), structure (what it means), and target (what you write).
The structure tree sits between the two languages. You annotate
it with meaning, then write fresh target speech from it. This
keeps source phrasing from leaking into the translation.

```
source (Russian)          structure              target (English)
├── § Introduction        ├── § Introduction     ├── § Introduction
│   ├── sentence 1   ──── │   ├── idea 1    ──── │   ├── sentence A
│   ├── sentence 2   ──── │   ├── idea 2    ──── │   ├── sentence B
│   └── sentence 3   ──┐  │   └── idea 3    ──── │   └── sentence C
│                       └──│       (merged)  ──┘
```

Works one sentence at a time, one paragraph, one section, one
kind of thinking. `next` shows what needs attention. `write`
adds a target sentence. The data lives in csvs tablets and
prose files, so you can inspect and edit everything with a
text editor.

## Install

```sh
cargo install --git https://codeberg.org/fetsorn/tsugiki
```

## Use

```sh
# parse a source document into source and structure trees
tsugiki init paper.md

# see what needs translation next
tsugiki next

# write a target sentence (auto-links to the current structure node)
tsugiki write "The article compares two processes."

# show a node with context
tsugiki show ae28 --depth 2

# annotate a structure node with meaning
tsugiki annotate "compares visualization and symbolization"

# render a tree back to markdown
tsugiki render target
```

## Data model

Each intent (translation project) is a directory with csvs
tablets tracking the three trees and their links:

| Tablet                   | What it holds                    |
|--------------------------|----------------------------------|
| `source-child.csv`       | source tree parent-child edges   |
| `structure-child.csv`    | structure tree parent-child edges|
| `target-child.csv`       | target tree parent-child edges   |
| `source-structure.csv`   | source to structure mapping      |
| `structure-target.csv`   | structure to target mapping      |

Prose for each node lives in `prose/{tree}/{uuid}`.

See the [design decisions](decisions/) for the full rationale.

## Source

- Codeberg: [fetsorn/tsugiki](https://codeberg.org/fetsorn/tsugiki)
- GitHub: [fetsorn/tsugiki](https://github.com/fetsorn/tsugiki)
