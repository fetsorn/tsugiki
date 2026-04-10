#!/usr/bin/env python3
"""
Decompose a source.md into source.fountain and source-child.csv.

Usage:
    python3 decompose.py <intent_dir> [--section-headings-file <file>]

Reads:  <intent_dir>/prose/source.md
Writes: <intent_dir>/prose/source.fountain
        <intent_dir>/csvs/source-child.csv

The script:
1. Parses source.md into sections, paragraphs, and sentences
2. Generates UUIDs for each node
3. Associates footnotes ([^N]) as children of their sentences
4. Writes Fountain markup and CSVS containment edges

Section headings must be provided in a file (one per line) or will be
auto-detected as standalone short lines between paragraphs.
"""

import re
import uuid
import sys
import os
import argparse
import json


# ============================================================
# Sentence splitter for Russian academic text
# ============================================================

SPLIT_MARKER = "<<SPLIT>>"


def split_sentences(text):
    """Split Russian academic text into sentences.

    Handles:
    - Single and multi-letter initials (М., Г., Дж., Ж.-П.)
    - Century patterns (ХХ в., XIX в.)
    - Decade patterns (80-х гг.)
    - Common abbreviations (т.е., т.к., т.ч., и др., и т.д.)
    - Page references (С. 136, P. 105)
    - Footnote markers ([^N]) at sentence boundaries
    """
    protections = {}
    counter = [0]

    def protect(match):
        key = f"PROT{counter[0]}PROT"
        protections[key] = match.group(0)
        counter[0] += 1
        return key

    t = text

    # Multi-letter initials
    t = re.sub(r'Дж\.\s', protect, t)
    t = re.sub(r'Ж\.-П\.\s', protect, t)

    # Single-letter Cyrillic/Latin initials: X. followed by word char
    t = re.sub(r'([А-ЯA-Z])\.\s(?=[А-ЯA-Zа-яa-z])', protect, t)

    # Century patterns: ХХ в., XIX в.
    t = re.sub(r'([XIVХХ]+)\s+в\.', protect, t)

    # Decade patterns: 80-х гг. ХХ
    t = re.sub(r'(\d+-х)\s+гг?\.\s', protect, t)

    # Multi-word abbreviations
    for pat in [r'в\s+т\.\s*ч\.', r'и\s+т\.\s*д\.', r'и\s+т\.\s*п\.',
                r'т\.\s*е\.', r'т\.\s*к\.', r'и\s+др\.', r'и\s+пр\.']:
        t = re.sub(pat, protect, t)

    # Page refs
    t = re.sub(r'С\.\s+\d+', protect, t)
    t = re.sub(r'P\.\s+\d+', protect, t)
    t = re.sub(r'Pp\.\s+\d+', protect, t)

    # Split on: ./?/! + optional [^N] + optional closing )/»/" + space + uppercase/opening quote
    t = re.sub(
        r'([.!?](?:\[\^\d+\])?(?:[)\u00bb\u201d"])?)\s+(?=[А-ЯA-Z\u00c0-\u024f\u00ab\u201e\u201c"(])',
        lambda m: m.group(1) + SPLIT_MARKER,
        t
    )

    sentences = t.split(SPLIT_MARKER)

    result = []
    for s in sentences:
        for key, val in protections.items():
            s = s.replace(key, val)
        s = s.strip()
        if s:
            result.append(s)

    return result


# ============================================================
# Parser: source.md -> sections -> paragraphs
# ============================================================

def parse_source(source_text, section_headings):
    """Parse source.md into (title, sections, footnotes).

    Returns:
        title: str
        sections: list of (heading, [paragraph_texts])
        footnotes: dict of {int: str}
    """
    # Split footnotes
    fn_start = re.search(r'\n\[\^1\]:', source_text)
    if fn_start:
        body = source_text[:fn_start.start()].strip()
        fn_section = source_text[fn_start.start():].strip()
    else:
        body = source_text.strip()
        fn_section = ""

    # Parse footnotes
    footnotes = {}
    if fn_section:
        fn_entries = re.split(r'\n(?=\[\^\d+\]:)', fn_section)
        for entry in fn_entries:
            entry = entry.strip()
            if not entry:
                continue
            m = re.match(r'\[\^(\d+)\]:\s*(.*)', entry, re.DOTALL)
            if m:
                num = int(m.group(1))
                fn_text = re.sub(r'\n\s+', ' ', m.group(2).strip())
                fn_text = re.sub(r'\n', ' ', fn_text)
                footnotes[num] = fn_text

    # Parse body
    lines = body.split('\n')
    title = lines[0].strip()

    # Group lines into raw blocks (separated by blank lines)
    blocks = []
    current = []
    for line in lines[1:]:
        if line.strip() == '':
            if current:
                blocks.append(current)
                current = []
        else:
            current.append(line)
    if current:
        blocks.append(current)

    # Merge list-item blocks with preceding non-heading block
    merged = []
    for block in blocks:
        text_joined = '\n'.join(block)
        is_list_item = block[0].strip().startswith('-')
        if is_list_item and merged and merged[-1] not in section_headings:
            merged[-1] = merged[-1] + '\n' + text_joined
        else:
            merged.append(text_joined)

    # Split into sections
    sections = []
    current_section_name = section_headings[0] if section_headings else 'Аннотация'
    current_paras = []

    # The first section collects paragraphs before any heading
    first_heading_seen = False
    for block_text in merged:
        block_text = block_text.strip()
        if block_text in section_headings:
            if not first_heading_seen and not current_paras:
                # This is the implicit first section name
                current_section_name = block_text
                first_heading_seen = True
                continue
            if current_paras:
                sections.append((current_section_name, current_paras))
            current_section_name = block_text
            current_paras = []
            first_heading_seen = True
        else:
            current_paras.append(block_text)

    if current_paras:
        sections.append((current_section_name, current_paras))

    return title, sections, footnotes


# ============================================================
# Tree builder
# ============================================================

def build_tree(title, sections, footnotes, conclusion_heading='Выводы'):
    """Build the source tree: nodes and edges.

    Returns:
        nodes: list of (full_uuid, text, depth_marker)
        edges: list of (parent_uuid, child_uuid)
    """
    nodes = []
    edges = []

    def new_uuid():
        return str(uuid.uuid4())

    doc_uuid = new_uuid()
    nodes.append((doc_uuid, title, '#'))

    for sec_name, paras in sections:
        sec_uuid = new_uuid()
        nodes.append((sec_uuid, sec_name, '##'))
        edges.append((doc_uuid, sec_uuid))

        is_conclusion = (sec_name == conclusion_heading)

        for para_text in paras:
            if is_conclusion:
                # No paragraph sub-level for conclusion
                sentences = split_sentences(para_text)
                for sent in sentences:
                    sent_uuid = new_uuid()
                    nodes.append((sent_uuid, sent, None))
                    edges.append((sec_uuid, sent_uuid))
                    for fn_num in [int(x) for x in re.findall(r'\[\^(\d+)\]', sent)]:
                        if fn_num in footnotes:
                            fn_uuid = new_uuid()
                            nodes.append((fn_uuid, footnotes[fn_num], None))
                            edges.append((sent_uuid, fn_uuid))
            else:
                para_uuid = new_uuid()
                nodes.append((para_uuid, None, '###'))
                edges.append((sec_uuid, para_uuid))

                sentences = split_sentences(para_text)
                for sent in sentences:
                    sent_uuid = new_uuid()
                    nodes.append((sent_uuid, sent, None))
                    edges.append((para_uuid, sent_uuid))
                    for fn_num in [int(x) for x in re.findall(r'\[\^(\d+)\]', sent)]:
                        if fn_num in footnotes:
                            fn_uuid = new_uuid()
                            nodes.append((fn_uuid, footnotes[fn_num], None))
                            edges.append((sent_uuid, fn_uuid))

    return nodes, edges


# ============================================================
# Writers
# ============================================================

def write_fountain(nodes, edges, output_path):
    """Write source.fountain from the tree."""
    children_of = {}
    for parent, child in edges:
        children_of.setdefault(parent, []).append(child)
    node_map = {n[0]: n for n in nodes}

    lines = []

    def write_node(node_uuid):
        full_uuid, text_content, depth_marker = node_map[node_uuid]

        lines.append(f'.{full_uuid[:8]}')
        if depth_marker:
            if text_content:
                lines.append(f'{depth_marker} {text_content}')
            else:
                lines.append(f'{depth_marker}')
        lines.append('')

        if depth_marker is None and text_content:
            lines.append(text_content)
            lines.append('')

        for kid_uuid in children_of.get(node_uuid, []):
            write_node(kid_uuid)

    root_uuid = nodes[0][0]
    write_node(root_uuid)

    with open(output_path, 'w') as f:
        f.write('\n'.join(lines))


def write_csv(edges, output_path):
    """Write source-child.csv from containment edges."""
    csv_lines = [f'{p},{c}' for p, c in edges]
    with open(output_path, 'w') as f:
        f.write('\n'.join(csv_lines) + '\n')


# ============================================================
# Main
# ============================================================

def main():
    parser = argparse.ArgumentParser(description='Decompose source.md into Fountain + CSVS')
    parser.add_argument('intent_dir', help='Path to the intent directory')
    parser.add_argument('--headings', help='File with section headings, one per line')
    parser.add_argument('--headings-json', help='JSON array of section headings')
    parser.add_argument('--conclusion', default='Выводы',
                        help='Name of the conclusion section (no paragraph sub-level)')
    args = parser.parse_args()

    # Read section headings
    if args.headings:
        with open(args.headings) as f:
            section_headings = [line.strip() for line in f if line.strip()]
    elif args.headings_json:
        section_headings = json.loads(args.headings_json)
    else:
        print("Warning: no section headings provided. Will treat entire text as one section.",
              file=sys.stderr)
        section_headings = []

    # Read source
    source_path = os.path.join(args.intent_dir, 'prose', 'source.md')
    with open(source_path) as f:
        source_text = f.read()

    # Parse
    title, sections, footnotes = parse_source(source_text, section_headings)

    # Build tree
    nodes, edges = build_tree(title, sections, footnotes, conclusion_heading=args.conclusion)

    # Write outputs
    fountain_path = os.path.join(args.intent_dir, 'prose', 'source.fountain')
    csv_path = os.path.join(args.intent_dir, 'csvs', 'source-child.csv')

    write_fountain(nodes, edges, fountain_path)
    write_csv(edges, csv_path)

    # Stats
    section_count = sum(1 for n in nodes if n[2] == '##')
    para_count = sum(1 for n in nodes if n[2] == '###')
    leaf_count = sum(1 for n in nodes if n[2] is None)
    fn_count = len(footnotes)

    print(f"Title: {title}")
    print(f"Nodes: {len(nodes)} ({section_count} sections, {para_count} paragraphs, {leaf_count} leaves)")
    print(f"Edges: {len(edges)}")
    print(f"Footnotes: {fn_count}")
    print(f"Written: {fountain_path}")
    print(f"Written: {csv_path}")

    for sec_name, paras in sections:
        total_sents = sum(len(split_sentences(p)) for p in paras)
        print(f"  {sec_name[:60]}: {len(paras)} paras, {total_sents} sentences")


if __name__ == '__main__':
    main()
