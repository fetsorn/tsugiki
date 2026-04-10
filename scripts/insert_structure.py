#!/usr/bin/env python3
"""
Insert sentence-level structure annotations into an intent's structure files.

Usage:
    python3 insert_structure.py <intent_dir> <annotations_file>

The annotations file is a simple text format:

    # Comment lines start with #
    # Blank lines are ignored

    # Each block maps a source sentence to a structure annotation:
    .cfb0b898
    realist legal theory is commonly described as follows...

    # Footnotes are indented under their parent sentence:
    .cfb0b898
    realist legal theory is commonly described as follows...
      .cbb00554
      to APA

    # Or use >> prefix for footnotes:
    .cfb0b898
    realist legal theory is commonly described as follows...
    >> .cbb00554
    >> to APA

The script:
1. Reads the annotations file
2. Finds each source sentence's parent paragraph in source-child.csv
3. Finds the paragraph's structure node in source-structure.csv
4. Generates new structure UUIDs for sentence annotations
5. Inserts into structure.fountain after the paragraph heading
6. Appends to structure-child.csv and source-structure.csv
"""

import sys
import os
import uuid
import re
import argparse


def new_uuid():
    return str(uuid.uuid4())


def short(u):
    return u[:8]


def load_csv(path):
    if not os.path.exists(path):
        return []
    with open(path) as f:
        return [line.strip().split(',') for line in f if line.strip()]


def parse_annotations(path):
    """Parse annotations file into list of (source_short, annotation, footnotes).

    footnotes is list of (fn_source_short, fn_annotation).
    """
    with open(path) as f:
        lines = f.readlines()

    entries = []
    current_src = None
    current_text = None
    current_fns = []
    in_fn = False
    fn_src = None
    fn_text = None

    def flush_fn():
        nonlocal fn_src, fn_text, in_fn
        if fn_src and fn_text:
            current_fns.append((fn_src, fn_text.strip()))
        fn_src = None
        fn_text = None
        in_fn = False

    def flush_entry():
        nonlocal current_src, current_text, current_fns
        flush_fn()
        if current_src and current_text:
            entries.append((current_src, current_text.strip(), list(current_fns)))
        current_src = None
        current_text = None
        current_fns = []

    for line in lines:
        stripped = line.rstrip()

        # Skip comments
        if stripped.startswith('#'):
            continue

        # Footnote with >> prefix
        if stripped.startswith('>> .') and len(stripped.split()[0]) == 3 + 8:
            flush_fn()
            fn_src = stripped[3:11]  # >> .XXXXXXXX
            fn_text = None
            in_fn = True
            continue
        if stripped.startswith('>> ') and in_fn:
            if fn_text is None:
                fn_text = stripped[3:]
            else:
                fn_text += ' ' + stripped[3:]
            continue

        # Indented footnote
        if (stripped.startswith('  .') or stripped.startswith('\t.')) and len(stripped.strip()) == 9:
            flush_fn()
            fn_src = stripped.strip()[1:]
            fn_text = None
            in_fn = True
            continue
        if (line.startswith('  ') or line.startswith('\t')) and in_fn and stripped:
            if fn_text is None:
                fn_text = stripped.strip()
            else:
                fn_text += ' ' + stripped.strip()
            continue

        # Source UUID line
        if stripped.startswith('.') and len(stripped) == 9 and not in_fn:
            flush_entry()
            current_src = stripped[1:]
            continue

        # Blank line
        if not stripped:
            if in_fn:
                flush_fn()
            continue

        # Annotation text
        if current_src and not in_fn:
            if current_text is None:
                current_text = stripped
            else:
                current_text += ' ' + stripped
            continue

        # Footnote text
        if in_fn:
            if fn_text is None:
                fn_text = stripped
            else:
                fn_text += ' ' + stripped

    flush_entry()
    return entries


def main():
    parser = argparse.ArgumentParser(description='Insert structure annotations')
    parser.add_argument('intent_dir', help='Path to intent directory')
    parser.add_argument('annotations', help='Path to annotations file')
    parser.add_argument('--dry-run', action='store_true', help='Show what would be done')
    args = parser.parse_args()

    intent = args.intent_dir

    # Load current state
    src_child = load_csv(os.path.join(intent, 'csvs', 'source-child.csv'))
    src_struct = load_csv(os.path.join(intent, 'csvs', 'source-structure.csv'))
    struct_child = load_csv(os.path.join(intent, 'csvs', 'structure-child.csv'))

    with open(os.path.join(intent, 'prose', 'structure.fountain')) as f:
        fountain = f.read()

    # Build lookups (by short prefix)
    src_parent_of = {}  # child_short -> parent_short
    src_children_of = {}
    src_full_of = {}
    for p, c in src_child:
        src_parent_of[c[:8]] = p[:8]
        src_children_of.setdefault(p[:8], []).append(c[:8])
        src_full_of[p[:8]] = p
        src_full_of[c[:8]] = c

    src_to_struct = {}  # source_short -> struct_full
    for s, t in src_struct:
        src_to_struct[s[:8]] = t

    struct_full_of = {}
    for p, c in struct_child:
        struct_full_of[p[:8]] = p
        struct_full_of[c[:8]] = c
    for s, t in src_struct:
        struct_full_of[t[:8]] = t

    # Parse annotations
    entries = parse_annotations(args.annotations)
    print(f"Parsed {len(entries)} sentence annotations")

    # Group by parent paragraph's structure UUID
    new_struct_child_edges = []
    new_src_struct_edges = []
    fountain_insertions = {}  # struct_para_short -> list of lines

    for src_sent_short, annotation, footnotes in entries:
        # Find parent paragraph in source tree
        para_short = src_parent_of.get(src_sent_short)
        if not para_short:
            print(f"  WARNING: .{src_sent_short} has no parent in source-child.csv, skipping")
            continue

        # Find structure paragraph UUID
        struct_para_full = src_to_struct.get(para_short)
        if not struct_para_full:
            print(f"  WARNING: source para .{para_short} has no structure mapping, skipping")
            continue

        struct_para_short = struct_para_full[:8]

        # Generate structure sentence UUID
        sent_struct = new_uuid()

        new_struct_child_edges.append((struct_para_full, sent_struct))
        new_src_struct_edges.append((src_full_of[src_sent_short], sent_struct))

        lines = fountain_insertions.setdefault(struct_para_short, [])
        lines.append(f'.{short(sent_struct)}')
        lines.append('')
        lines.append(annotation)
        lines.append('')

        # Handle footnotes
        for fn_src_short, fn_annotation in footnotes:
            fn_struct = new_uuid()
            new_struct_child_edges.append((sent_struct, fn_struct))
            new_src_struct_edges.append((src_full_of.get(fn_src_short, fn_src_short), fn_struct))

            lines.append(f'.{short(fn_struct)}')
            lines.append('')
            lines.append(fn_annotation)
            lines.append('')

    if args.dry_run:
        print(f"\nWould insert {len(new_struct_child_edges)} structure-child edges")
        print(f"Would insert {len(new_src_struct_edges)} source-structure edges")
        for para_short, lines in fountain_insertions.items():
            print(f"\nAfter .{para_short}:")
            for line in lines[:6]:
                print(f"  {line}")
            if len(lines) > 6:
                print(f"  ... ({len(lines)} lines total)")
        return

    # Insert into fountain
    ftn_lines = fountain.split('\n')
    result = []
    i = 0
    while i < len(ftn_lines):
        line = ftn_lines[i]
        result.append(line)

        if line.startswith('.'):
            uid = line[1:].strip()
            if uid in fountain_insertions:
                # Copy heading + blank line
                i += 1
                while i < len(ftn_lines):
                    result.append(ftn_lines[i])
                    if ftn_lines[i].strip() == '':
                        i += 1
                        break
                    i += 1
                # Insert annotations
                for ins_line in fountain_insertions[uid]:
                    result.append(ins_line)
                continue

        i += 1

    # Write fountain
    with open(os.path.join(intent, 'prose', 'structure.fountain'), 'w') as f:
        f.write('\n'.join(result))

    # Append to CSVs
    with open(os.path.join(intent, 'csvs', 'structure-child.csv'), 'a') as f:
        for p, c in new_struct_child_edges:
            f.write(f'{p},{c}\n')

    with open(os.path.join(intent, 'csvs', 'source-structure.csv'), 'a') as f:
        for s, t in new_src_struct_edges:
            f.write(f'{s},{t}\n')

    print(f"Inserted {len(new_struct_child_edges)} structure nodes")
    print(f"Updated structure.fountain, structure-child.csv, source-structure.csv")


if __name__ == '__main__':
    main()
