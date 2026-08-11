---
status: accepted
date: 2026-06-11
---

# Cursorless regrow: write derives position from state

## Context and Problem Statement

ADR-0002 gave annotate a smooth loop: position is derived from file state (first node with empty prose), no cursor file, restart-safe. Regrow had no such loop. Writing one target sentence took three decisions made by hand: `write "text" --parent <addr>` with an explicitly chosen parent, then `link <new-id> --structure <id>` for provenance. Working through a text leaf by leaf, the translator re-derives the parent and the structure address for every sentence — mechanical work the state already knows.

The first instinct was a flag for advancing (`write "text" --complete` to mark a structure node done and move on). That inverts the actual derivability: completion is already implicit in the data — a structure leaf with at least one target link is done — but "I am not done with this node" is not derivable. The flag belongs on staying, not leaving.

## Decision

### Plain write is the regrow loop

`tsugiki write "text"` with no flags:

1. **Current structure leaf** = first leaf in depth-first order with no entry in `structure-target.csv` — the same derivation `next` uses. No cursor file (the `.tsugiki-parent` state file is bypassed in this mode).
2. A new target leaf is created with the text, linked to the current structure leaf.
3. **Parent inference**: the new leaf is parented under the target node already linked to the current leaf's structure parent. If none exists, an empty inner node is created there (recursively up to the roots) and linked to that structure parent. Empty inner nodes carry no prose — they render as paragraph breaks, not headings.
4. After the write, the CLI prints the next node with full regrow context (intent chain, source, anaphora, cataphora) — write's output is the prompt for the next write.

Advancing is free: once the leaf is linked, the derivation lands on the following leaf. 1:1, the common case, is the unmarked case.

### --same stays on the node

`tsugiki write "text" --same` creates a target leaf with the same structure links and the same parent as the previous target leaf (last leaf of the target tree in document order). This is the 1:N split: several target sentences expressing one structure node. The non-derivable intent gets the flag.

### Explicit mode remains

`write --parent <addr>` keeps the old behavior — explicit parent, no auto-linking, no advancement. Together with `link` (N:1 merges, reparenting, repairs) it covers everything the auto loop cannot express.

`write --root` now also maps the new target root to the structure root in `structure-target.csv`, so auto mode can resolve the root before it has containment edges.

## Consequences

- The regrow loop matches annotate's shape: `write "sentence"`, read the context that prints, write the next sentence. One command per sentence, zero addresses.
- Restart-safe and idempotent in the same way annotate is: position is derived fresh from the tablets every time.
- N:1 merges still require explicit `link` calls after a write. Acceptable: merges are the rare case and a deliberate act.
- Auto-created paragraph nodes mean the target tree's default shape mirrors the structure tree's grouping. The translator reshapes with `link --parent` where the target wants a different shape — the scaffold-not-law principle from ADR-0001 applied to the target tree.
- The mapped/total count in progress output now includes inner-node mappings created for paragraph inference, not only regrown leaves.
