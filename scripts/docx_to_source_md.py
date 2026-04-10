#!/usr/bin/env python3
"""
Convert a .docx file to a clean source.md for a tsugiki intent.

Usage:
    python3 docx_to_source_md.py <input.docx> <output_dir>

The script:
1. Runs pandoc to convert docx to markdown
2. Strips {.mark}, {=html} and other pandoc artifacts
3. Removes bold markers from section headings
4. Strips frontmatter (everything before the title line)

The title line is identified as the first line matching a configurable pattern.
Edit TITLE_PATTERN below or pass --title-pattern as an argument.
"""

import re
import subprocess
import sys
import os
import argparse


def convert_docx_to_markdown(docx_path):
    """Run pandoc to get raw markdown from docx."""
    result = subprocess.run(
        ["pandoc", docx_path, "-t", "markdown", "--wrap=none"],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        print(f"pandoc error: {result.stderr}", file=sys.stderr)
        sys.exit(1)
    return result.stdout


def clean_markdown(text, title_pattern=None):
    """Strip pandoc artifacts and frontmatter."""
    # Remove {.mark} spans: [text]{.mark} -> text
    text = re.sub(r'\[([^\]]*)\]\{\.mark\}', r'\1', text)

    # Remove {=html} code blocks
    text = re.sub(r'```\{=html\}\n.*?\n```', '', text, flags=re.DOTALL)

    # Remove bold markers from section headings
    # Strip "**1. " (numbered bold headings)
    text = re.sub(r'\*\*(\d+\.\s+)', '', text)
    # Strip specific known bold labels
    text = re.sub(r'\*\*Аннотация:\*\*', 'Аннотация:', text)
    text = re.sub(r'\*\*Выводы\*\*', 'Выводы', text)
    # Close remaining ** at end of lines
    text = re.sub(r'\*\*(?=\n)', '', text)
    # Strip remaining opening ** at start of lines
    text = re.sub(r'^\*\*', '', text, flags=re.MULTILINE)

    # Clean up stray formatting
    text = text.replace("\\'", "'")
    text = text.replace("\\-", "-")
    text = text.replace("\\...", "...")

    # Remove frontmatter (everything before the title)
    if title_pattern:
        lines = text.split('\n')
        start = 0
        for i, line in enumerate(lines):
            if re.search(title_pattern, line):
                start = i
                break
        text = '\n'.join(lines[start:])

    # Collapse triple+ newlines
    text = re.sub(r'\n{3,}', '\n\n', text)

    return text


def main():
    parser = argparse.ArgumentParser(description='Convert docx to clean source.md')
    parser.add_argument('docx', help='Path to input .docx file')
    parser.add_argument('output_dir', help='Intent directory (writes prose/source.md)')
    parser.add_argument('--title-pattern', default=None,
                        help='Regex pattern to identify the title line (strips everything before it)')
    args = parser.parse_args()

    raw_md = convert_docx_to_markdown(args.docx)
    clean_md = clean_markdown(raw_md, title_pattern=args.title_pattern)

    prose_dir = os.path.join(args.output_dir, 'prose')
    os.makedirs(prose_dir, exist_ok=True)

    output_path = os.path.join(prose_dir, 'source.md')
    with open(output_path, 'w') as f:
        f.write(clean_md)

    print(f"Written to {output_path}")


if __name__ == '__main__':
    main()
