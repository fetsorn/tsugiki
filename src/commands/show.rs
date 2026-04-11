use std::path::{Path, PathBuf};

use csvs::{Dataset, Entry};

use crate::resolve;
use crate::scan;
use crate::types::{Addr, ScannedNode, TreeKind};

/// Display a node with full context: text, parent, children, bridge counterparts, notes.
pub async fn run(intent_dir: &Path, addr_str: &str) -> Result<(), String> {
    let addr = Addr::parse(addr_str);

    // Try to find the node in each tree
    let trees = [TreeKind::Source, TreeKind::Structure, TreeKind::Target];

    for kind in &trees {
        let path = intent_dir.join("prose").join(kind.fountain_filename());
        if !path.exists() {
            continue;
        }

        let nodes = scan::scan_all(&path);
        if let Some(node) = resolve::resolve(&nodes, &addr) {
            // Found it
            println!("[{:?}] L{} [{}]", kind, node.line_number, node.id.short);

            if let Some(d) = node.depth {
                println!("  depth: {d}");
            } else {
                println!("  (action block)");
            }

            if node.text.is_empty() {
                println!("  text: (empty)");
            } else {
                println!("  text: {}", node.text);
            }

            // Parent
            if let Some(parent) = scan::find_parent(&nodes, node) {
                println!("  parent: {} [{}]", parent.text, parent.id.short);
            }

            // Children: nodes that follow until next node at same or lower depth
            let children = find_children(&nodes, node);
            if !children.is_empty() {
                println!("  children:");
                for child in &children {
                    let prefix = if child.depth.is_some() { "#" } else { " " };
                    println!("    {prefix} {} [{}]", child.text, child.id.short);
                }
            }

            // Notes
            if !node.notes.is_empty() {
                println!("  notes:");
                for note in &node.notes {
                    println!("    [[{note}]]");
                }
            }

            // Bridge counterparts via CSVS
            let csvs_dir = intent_dir.join("csvs");
            if csvs_dir.exists() {
                print_bridges(&csvs_dir, kind, &node.id.short).await;
            }

            return Ok(());
        }
    }

    Err(format!("Node not found: {addr_str}"))
}

/// Find direct children of a node in the scanned list.
fn find_children<'a>(nodes: &'a [ScannedNode], parent: &ScannedNode) -> Vec<&'a ScannedNode> {
    let idx = match nodes.iter().position(|n| n.line_number == parent.line_number) {
        Some(i) => i,
        None => return vec![],
    };
    // Action blocks (depth=None) are leaves — no children
    let parent_depth = match parent.depth {
        Some(d) => d,
        None => return vec![],
    };
    let child_depth = parent_depth + 1;

    let mut children = vec![];
    for node in &nodes[idx + 1..] {
        match node.depth {
            Some(d) if d <= parent_depth => break,
            Some(d) if d == child_depth => children.push(node),
            None => children.push(node),
            _ => {}
        }
    }
    children
}

/// Print bridge counterparts by querying CSVS tablets.
async fn print_bridges(csvs_dir: &Path, kind: &TreeKind, short_id: &str) {
    match kind {
        TreeKind::Source => {
            if let Some(ids) = lookup_forward(csvs_dir, "source", "structure", short_id).await {
                println!("  structure:");
                for id in ids {
                    println!("    [{id}]");
                }
            }
        }
        TreeKind::Structure => {
            if let Some(ids) = lookup_reverse(csvs_dir, "source", "structure", short_id).await {
                println!("  source:");
                for id in ids {
                    println!("    [{id}]");
                }
            }
            if let Some(ids) = lookup_forward(csvs_dir, "structure", "target", short_id).await {
                println!("  target:");
                for id in ids {
                    println!("    [{id}]");
                }
            }
        }
        TreeKind::Target => {
            if let Some(ids) = lookup_reverse(csvs_dir, "structure", "target", short_id).await {
                println!("  structure:");
                for id in ids {
                    println!("    [{id}]");
                }
            }
        }
    }
}

/// Given tablet "base-leaf.csv", find all leaf values where base starts with prefix.
/// Reopens the dataset each time because select_record consumes self.
async fn lookup_forward(
    csvs_dir: &Path,
    base: &str,
    leaf: &str,
    id_prefix: &str,
) -> Option<Vec<String>> {
    let dir = PathBuf::from(csvs_dir);
    let dataset = Dataset::open(&dir).await.ok()?;

    let query = Entry {
        base: base.to_string(),
        base_value: None,
        leader_value: None,
        leaves: std::collections::HashMap::from([(
            leaf.to_string(),
            vec![Entry::new(leaf)],
        )]),
    };

    let results: Vec<Entry> = dataset.select_record(vec![query]).await.ok()?;
    let mut matches = vec![];
    for entry in &results {
        if let Some(bv) = &entry.base_value {
            if bv.starts_with(id_prefix) {
                if let Some(leaves) = entry.leaves.get(leaf) {
                    for l in leaves {
                        if let Some(lv) = &l.base_value {
                            let short = lv.split('-').next().unwrap_or(lv);
                            matches.push(short.to_string());
                        }
                    }
                }
            }
        }
    }

    if matches.is_empty() { None } else { Some(matches) }
}

/// Reverse lookup: given tablet "base-leaf.csv", find all base values where leaf starts with prefix.
async fn lookup_reverse(
    csvs_dir: &Path,
    base: &str,
    leaf: &str,
    id_prefix: &str,
) -> Option<Vec<String>> {
    let dir = PathBuf::from(csvs_dir);
    let dataset = Dataset::open(&dir).await.ok()?;

    let query = Entry {
        base: base.to_string(),
        base_value: None,
        leader_value: None,
        leaves: std::collections::HashMap::from([(
            leaf.to_string(),
            vec![Entry::new(leaf)],
        )]),
    };

    let results: Vec<Entry> = dataset.select_record(vec![query]).await.ok()?;
    let mut matches = vec![];
    for entry in &results {
        if let Some(leaves) = entry.leaves.get(leaf) {
            for l in leaves {
                if let Some(lv) = &l.base_value {
                    if lv.starts_with(id_prefix) {
                        if let Some(bv) = &entry.base_value {
                            let short = bv.split('-').next().unwrap_or(bv);
                            matches.push(short.to_string());
                        }
                    }
                }
            }
        }
    }

    if matches.is_empty() { None } else { Some(matches) }
}
