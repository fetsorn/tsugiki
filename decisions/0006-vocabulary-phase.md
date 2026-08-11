---
status: proposed
date: 2026-06-10
---

# Vocabulary phase: a palette of target options per source term

## Context and Problem Statement

During regrow, the same source terms recur across many leaves. The translator decides a term's rendering once, then re-derives or misremembers that decision at leaf 47. Key terminology is also the highest-leverage part of a translation: even when a client rewrites every sentence for their own flow (e.g. turning an article into a conference speech), the terms survive into their final text.

The first instinct — a glossary that fixes one translation per term — is wrong for real prose. The right rendering of a recurring word depends on the flow of the sentence it lands in. What the translator actually needs is the set of *defensible* options per term, settled once, chosen from per occurrence.

Discovered while translating the symbol intent: terminology conflicts surfaced between the author's own English abstract and the translator's structure annotations (образное мышление: figurative vs. visual thinking), and pedagogical terms had several established English names (принцип наглядности: visual instruction / visualization / concretization). These option sets deserved a home.

## Decision

### Palette, not glossary

A vocabulary entry maps one source-language term to one or more target-language options. The final choice happens per occurrence during regrow, argued in flow context. The vocabulary constrains nothing; it ensures the option space is derived once.

Some terms are **pinned** — the load-bearing terms a reader or listener tracks the argument by (in symbol: symbolization, visualization, legal design). A pinned term is simply an entry with exactly one option.

### CSVS mapping

The palette model maps directly onto CSVS one-to-many semantics:

- Schema line: `word,vocab`
- Data tablet: `word-vocab.csv` — key is the source term, value is a target option
- A key with multiple values is a palette; a key with one value is a pin. Cardinality *is* the pinning — no extra notation.
- Register and connotation notes (e.g. "prohibited for the page, banned for the mouth") live in the prose store as descriptions on the value, never in the tablet.

Keys are not limited to single words — multiword terms and names to be transliterated (for the bibliography, for pronunciation) are valid keys.

### Position in the workflow

Vocabulary sits between annotate and regrow in recommendation, but like split it is available anytime: conflicts keep surfacing during regrow and the tablet grows as they do. The phase is cross-cutting — entries are not node-scoped, so they live in the tablet, not the prose store of any node.

### No-AI rule

ADR-0001's constraint stands: no target-language word comes from AI. Options enter the palette only by the translator's decision. The AI may surface conflicts between the translator's own materials and ask which options belong; the translator ratifies every entry.

### Integration with regrow context

ADR-0001 describes the regrow decision context: preceding target nodes, the structure ancestor chain up to the root intent, the source node and its sequence neighbors. The CLI does not show this yet. When it does, a regrow-mode `next` should also match the source node's text against `word` keys and surface the relevant palettes inline. The vocabulary phase feeds the regrow display.

### Minimal version

The tablet is hand-edited. This works today with no CLI changes — the symbol intent already carries one. CLI support (`tsugiki vocab <word>`, palette matching in `next`) comes after the main loop is proven, same policy as footnotes.

## Consequences

- Terminology decisions are derived once and consulted everywhere, without freezing prose into glossary-speak.
- Pinning requires no new concept: it is cardinality 1 in an existing format.
- The tablet gives the AI interlocutor something concrete to reference during regrow discussion — palette options by key — without ever generating target prose itself.
- A future regrow `next` gains a third context layer (vocabulary) alongside anaphora and the intent chain, at the cost of substring matching against tablet keys.
- One more tablet per intent; schema grows by one line.
