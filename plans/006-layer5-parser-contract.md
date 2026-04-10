---
title: "Layer 5: Parser contract and source decomposition"
status: active
layer: 5
adr: decisions/0003-typed-tree-model-in-rust.md
depends: [001-layer0-types, 002-layer1-memtree-proptest, 003-layer2-serialization]
---

# Layer 5: Parser contract and source decomposition

## Goal

Formalize the source text parser as a typed contract with explicit warnings, approval flow, and language-specific strategies. This replaces `decompose.py` and makes parse errors catchable before they propagate into the tree.

## New files

```
tsugiki-core/src/
  parser/
    mod.rs
    contract.rs     — ParseResult, ParseWarning, approval flow
    markdown.rs     — markdown → blocks
    sentence.rs     — block → sentences (language-specific)
    russian.rs      — Russian sentence splitter
    english.rs      — English sentence splitter (stub)
```

## Parser contract (`contract.rs`)

```rust
pub struct ParseResult<T> {
    pub tree: MemTree<T>,
    pub warnings: Vec<ParseWarning>,
}

pub enum ParseWarning {
    /// Sentence boundary is ambiguous (e.g., abbreviation vs. period)
    AmbiguousSplit {
        line: usize,
        context: String,
        candidates: Vec<usize>,  // byte offsets of candidate split points
    },
    /// Heading detection is uncertain
    AmbiguousHeading {
        line: usize,
        text: String,
    },
    /// Footnote reference couldn't be matched to a footnote definition
    UnmatchedFootnote {
        marker: String,
        line: usize,
    },
    /// Block couldn't be classified (paragraph vs. list vs. quote)
    UnclassifiedBlock {
        line: usize,
        text: String,
    },
}

pub enum Approval {
    Accept,
    Reject,
    Override(String),  // human provides corrected version
}

impl<T> ParseResult<T> {
    pub fn is_clean(&self) -> bool { self.warnings.is_empty() }

    /// Validate that the parse result satisfies tree invariants.
    pub fn validate(&self) -> Result<(), Vec<TreeError>> {
        // Check depth monotonicity, single parent, etc.
        // Uses the same checks as proptest properties 1-2.
    }
}
```

### Approval flow

The CLI (Layer 4) presents warnings to the human one by one:

```
WARNING: Ambiguous split at line 42
  "...Дж. Тарелло изучал..."
  Split here? [y/n/override]
```

The parser never silently resolves ambiguity. Every uncertain decision is surfaced.

## Markdown parser (`markdown.rs`)

Replaces `decompose.py`'s markdown parsing.

```rust
pub fn parse_markdown(input: &str, config: &ParseConfig) -> ParseResult<Source>
```

`ParseConfig`:
```rust
pub struct ParseConfig {
    pub headings: Vec<String>,          // known section headings
    pub conclusion_heading: Option<String>,
    pub language: Language,
    pub footnote_style: FootnoteStyle,  // [^N] or inline
}

pub enum Language {
    Russian,
    English,
    // extensible
}

pub enum FootnoteStyle {
    Markdown,  // [^1], [^2]
    Inline,
}
```

Steps (depth assignment is intent-specific, not fixed — see `ParseConfig`):
1. Split on headings → depth 1 nodes (sections). If no headings, the whole document is one depth-0 root with direct children.
2. Split sections on blank lines → depth 2 nodes (paragraphs).
3. Split paragraphs on sentence boundaries (language-specific) → depth 3 nodes (sentences).
4. Optionally match footnote markers to footnote definitions → depth 4 nodes (footnote children of sentences). Not all intents have footnotes.
5. Assign UUIDs. Build `MemTree<Source>` with containment edges. Actual max depth depends on the source text.
6. Collect warnings for every ambiguous decision.

## Sentence splitter trait (`sentence.rs`)

```rust
pub trait SentenceSplitter {
    fn split(&self, text: &str) -> Vec<SplitResult>;
}

pub struct SplitResult {
    pub text: String,
    pub confidence: Confidence,
    pub split_offset: usize,
}

pub enum Confidence {
    Certain,       // unambiguous sentence boundary
    Likely,        // high confidence (e.g., period + capital letter)
    Ambiguous,     // needs human approval
}
```

### Russian splitter (`russian.rs`)

Port of `decompose.py`'s Russian logic to Rust:

Protection patterns:
- Single-letter Cyrillic initials: `М.`, `В.`, `Г.`
- Multi-letter initials: `Дж.`
- Century patterns: `ХХ в.`, `XIX в.`
- Decade patterns: `80-х гг.`
- Abbreviations: `т.е.`, `т.к.`, `и т.д.`, `т.н.`, `т.п.`
- Page references: `С. 136`, `P. 105`
- Footnote markers at sentence end: `[^N]`

Strategy: replace protected patterns with tokens, split on `.!?` followed by space+capital or newline, restore tokens. Return `Confidence::Ambiguous` for any split point within 3 chars of a protected pattern.

### English splitter (`english.rs`)

Stub initially. Common patterns:
- `Mr.`, `Mrs.`, `Dr.`, `Prof.`, `vs.`, `etc.`, `i.e.`, `e.g.`
- Single-letter initials.

## Integration with existing workflow

The parser produces a `ParseResult<Source>` which can be:
1. Serialized to `source.fountain` + `source-child.csv` via Layer 2.
2. Validated via `validate()` before serialization.
3. Presented to the human with warnings via CLI (Layer 4).

The human approves or overrides each warning, then the clean tree is written.

## Tests

- Unit: Russian splitter on known sentences from the troper source.
- Unit: markdown parser on a small markdown document with headings and footnotes.
- Unit: approval flow — parse with warnings, approve all, validate passes.
- Unit: ambiguous split detected at `Дж. Тарелло`.
- Proptest: arbitrary markdown-like strings → parse never panics, always returns ParseResult.
- Proptest: for any ParseResult where `is_clean()`, `validate()` succeeds.
- Integration: parse the actual troper `source.md`, compare node count to existing `source.fountain`.

## Potential csvs-rs interaction

When writing the parsed tree to CSVS, we generate full v4 UUIDs for each node. The serialization layer (Layer 2) handles the write. The parser itself only produces `NodeId` values, never touches files directly.

## Done when

- Russian sentence splitter matches or exceeds `decompose.py` accuracy on troper source.
- Parse warnings surface all cases that `decompose.py` got wrong (e.g., `Дж.` split).
- `ParseResult::validate()` catches invariant violations before they reach files.
- English splitter stub exists and is marked as incomplete.
- Parser never silently resolves ambiguity.
