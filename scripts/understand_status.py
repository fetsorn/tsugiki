#!/usr/bin/env python3
"""
Show progress of Phase 2 (Understand) for a tsugiki intent.

Usage:
    python3 understand_status.py <intent_dir>

Reports which source nodes have structure annotations and which don't.
"""

import sys
import os


def load_csv(path):
    """Load a two-column CSV, return list of (col1, col2) tuples."""
    if not os.path.exists(path):
        return []
    with open(path) as f:
        return [line.strip().split(',') for line in f if line.strip()]


def load_fountain_uuids(path):
    """Extract short UUIDs and their depth markers from a fountain file."""
    if not os.path.exists(path):
        return {}
    nodes = {}  # short_uuid -> depth_marker or None
    with open(path) as f:
        lines = f.readlines()
    for i, line in enumerate(lines):
        line = line.rstrip()
        if line.startswith('.') and len(line) == 9:
            uid = line[1:]
            depth = None
            for j in range(i + 1, min(i + 3, len(lines))):
                stripped = lines[j].strip()
                if stripped.startswith('#'):
                    depth = stripped.split()[0]  # '#', '##', '###'
                    break
            nodes[uid] = depth
    return nodes


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <intent_dir>", file=sys.stderr)
        sys.exit(1)

    intent_dir = sys.argv[1]

    src_child = load_csv(os.path.join(intent_dir, 'csvs', 'source-child.csv'))
    src_struct = load_csv(os.path.join(intent_dir, 'csvs', 'source-structure.csv'))
    struct_child = load_csv(os.path.join(intent_dir, 'csvs', 'structure-child.csv'))

    src_nodes = load_fountain_uuids(os.path.join(intent_dir, 'prose', 'source.fountain'))
    struct_nodes = load_fountain_uuids(os.path.join(intent_dir, 'prose', 'structure.fountain'))

    # Build source tree
    src_children = {}
    for p, c in src_child:
        src_children.setdefault(p[:8], []).append(c[:8])

    # Which source nodes have structure mappings?
    mapped_src = set()
    for s, t in src_struct:
        mapped_src.add(s[:8])

    # Find root
    parents = set(e[0][:8] for e in src_child)
    children = set(e[1][:8] for e in src_child)
    roots = parents - children
    if not roots:
        print("No root found in source-child.csv")
        sys.exit(1)
    root = roots.pop()

    # Count by depth
    total = {'#': 0, '##': 0, '###': 0, 'leaf': 0, 'fn': 0}
    mapped = {'#': 0, '##': 0, '###': 0, 'leaf': 0, 'fn': 0}

    def classify(uid):
        depth = src_nodes.get(uid)
        if depth:
            return depth
        # Leaf: no children in source tree
        kids = src_children.get(uid, [])
        if not kids:
            # Check if parent is a leaf (then this is a footnote)
            return 'fn'  # simplified; could check parent
        return 'leaf'

    def walk(uid, parent_is_leaf=False):
        kids = src_children.get(uid, [])
        depth = src_nodes.get(uid)

        if depth:
            cat = depth
        elif not kids:
            if parent_is_leaf:
                cat = 'fn'
            else:
                cat = 'leaf'
        else:
            cat = 'leaf'

        total[cat] = total.get(cat, 0) + 1
        if uid in mapped_src:
            mapped[cat] = mapped.get(cat, 0) + 1

        is_leaf = (cat == 'leaf') or (depth is None and kids)
        for kid in kids:
            walk(kid, parent_is_leaf=(not depth and not kids))

    walk(root)

    print(f"Intent: {intent_dir}")
    print(f"Source nodes: {len(src_nodes)} in fountain")
    print(f"Structure nodes: {len(struct_nodes)} in fountain")
    print()
    print(f"{'Level':<12} {'Total':>6} {'Mapped':>7} {'Remaining':>10}")
    print(f"{'─' * 12} {'─' * 6} {'─' * 7} {'─' * 10}")
    for cat in ['#', '##', '###', 'leaf', 'fn']:
        t = total.get(cat, 0)
        m = mapped.get(cat, 0)
        remaining = t - m
        label = {'#': 'document', '##': 'section', '###': 'paragraph',
                 'leaf': 'sentence', 'fn': 'footnote'}.get(cat, cat)
        print(f"{label:<12} {t:>6} {m:>7} {remaining:>10}")

    grand_total = sum(total.values())
    grand_mapped = sum(mapped.values())
    print(f"{'─' * 12} {'─' * 6} {'─' * 7} {'─' * 10}")
    print(f"{'TOTAL':<12} {grand_total:>6} {grand_mapped:>7} {grand_total - grand_mapped:>10}")

    # Show unmapped paragraphs with their first sentence
    if '--detail' in sys.argv:
        print("\nUnmapped paragraphs:")
        for uid in src_nodes:
            if src_nodes[uid] == '###' and uid not in mapped_src:
                kids = src_children.get(uid, [])
                first_kid = kids[0] if kids else '?'
                print(f"  .{uid} ({len(kids)} sentences, first: .{first_kid})")


if __name__ == '__main__':
    main()
