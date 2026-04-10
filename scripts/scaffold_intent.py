#!/usr/bin/env python3
"""
Scaffold the directory structure and CSVS identity files for a new intent.

Usage:
    python3 scaffold_intent.py <intent_dir>

Creates:
    <intent_dir>/
        prose/
        csvs/
            .csvs.csv       — dataset identity (generates a new UUID)
            _-_.csv          — schema (source/structure/target edges)
            source-child.csv     (empty)
            structure-child.csv  (empty)
            target-child.csv     (empty)
            source-structure.csv (empty)
            structure-target.csv (empty)
"""

import os
import sys
import uuid


SCHEMA = """\
source,child
structure,child
target,child
source,structure
structure,target
"""

DATA_TABLETS = [
    'source-child.csv',
    'structure-child.csv',
    'target-child.csv',
    'source-structure.csv',
    'structure-target.csv',
]


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <intent_dir>", file=sys.stderr)
        sys.exit(1)

    intent_dir = sys.argv[1]
    prose_dir = os.path.join(intent_dir, 'prose')
    csvs_dir = os.path.join(intent_dir, 'csvs')

    os.makedirs(prose_dir, exist_ok=True)
    os.makedirs(csvs_dir, exist_ok=True)

    # Identity tablet
    dataset_id = str(uuid.uuid4())
    csvs_path = os.path.join(csvs_dir, '.csvs.csv')
    if not os.path.exists(csvs_path):
        with open(csvs_path, 'w') as f:
            f.write(f'version,0.0.3\n')
            f.write(f'id,{dataset_id}\n')
        print(f"Created {csvs_path} (id: {dataset_id})")
    else:
        print(f"Skipped {csvs_path} (already exists)")

    # Schema tablet
    schema_path = os.path.join(csvs_dir, '_-_.csv')
    if not os.path.exists(schema_path):
        with open(schema_path, 'w') as f:
            f.write(SCHEMA)
        print(f"Created {schema_path}")
    else:
        print(f"Skipped {schema_path} (already exists)")

    # Empty data tablets
    for tablet in DATA_TABLETS:
        tablet_path = os.path.join(csvs_dir, tablet)
        if not os.path.exists(tablet_path):
            with open(tablet_path, 'w') as f:
                pass  # empty file
            print(f"Created {tablet_path}")
        else:
            print(f"Skipped {tablet_path} (already exists)")

    print(f"\nScaffold complete: {intent_dir}")


if __name__ == '__main__':
    main()
