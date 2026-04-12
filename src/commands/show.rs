use std::path::{Path, PathBuf};

use csvs::{Dataset, Entry};

use crate::resolve;
use crate::scan;
use crate::types::{Addr, ScannedNode, TreeKind};

/// Display a node with context: text, parent, children, bridges, sequence neighbors.
///
/// tree_filter: if Some, only show that tree. If None, show all matching trees.
/// child_limit: max children to display (0 = unlimited).
/// depth: how many levels of children to show (0 = node only, 1 = direct children, etc.)
pub async fn run(
    intent_dir: &Path,
    addr_str: &str,
    tree_filter: Option<&TreeKind>,
    child_limit: usize,
    depth: usize,
) -> Result<(), String> {
    let addr = Addr::parse(addr_str);

    let trees = match tree_filter {
        Some(kind) => vec![kind.clone()],
        None => vec![TreeKind::Source, TreeKind::Structure, TreeKind::Target],
    };

    let show_all = tree_filter.is_none() && matches!(addr, Addr::Line(_));
    let csvs_dir = intent_dir.join("csvs");
    let mut found = false;

    for kind in &trees {
        let path = intent_dir.join("prose").join(kind.fountain_filename());
        if !path.exists() {
            continue;
        }

        let nodes = scan::scan_all(&path);
        if let Some(node) = resolve::resolve(&nodes, &addr) {
            if found {
                println!();
            }
            found = true;

            print_node(&nodes, kind, node, child_limit, depth, 0);

            if csvs_dir.exists() {
                print_bridges(&csvs_dir, kind, &node.id.short).await;
            }

            // Prev/next
            print_neighbors(&nodes, kind, node);

            if !show_all {
                return Ok(());
            }
        }
    }

    if found {
        Ok(())
    } else {
        Err(format!("Node not found: {addr_str}"))
    }
}

/// Print a single node's details.
fn print_node(
    nodes: &[ScannedNode],
    kind: &TreeKind,
    node: &ScannedNode,
    child_limit: usize,
    max_depth: usize,
    current_depth: usize,
) {
    let indent = "  ".repeat(current_depth);

    if current_depth == 0 {
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

        if let Some(parent) = scan::find_parent(nodes, node) {
            println!("  parent: [{}]", parent.id.short);
        }

        if !node.notes.is_empty() {
            println!("  notes:");
            for note in &node.notes {
                println!("    [[{note}]]");
            }
        }
    } else {
        // Nested child display
        let prefix = if node.depth.is_some() { "#" } else { " " };
        let text = truncate(&node.text, 72);
        println!("{indent}{prefix} [{} L{}] {text}", node.id.short, node.line_number);
    }

    // Show children if within depth limit
    if current_depth < max_depth {
        let children = find_children(nodes, kind, node);
        if !children.is_empty() {
            let total = children.len();
            let show_count = if child_limit == 0 { total } else { child_limit.min(total) };

            if current_depth == 0 {
                println!("  children ({total}):");
            }

            let child_indent = if current_depth == 0 { "    " } else { &format!("{}  ", indent) };
            for child in &children[..show_count] {
                if current_depth + 1 < max_depth {
                    // Recurse deeper
                    print_node(nodes, kind, child, child_limit, max_depth, current_depth + 1);
                } else {
                    let prefix = if child.depth.is_some() { "#" } else { " " };
                    let text = truncate(&child.text, 72);
                    println!("{child_indent}{prefix} [{} L{}] {text}", child.id.short, child.line_number);
                }
            }
            if show_count < total {
                println!("{child_indent}... and {} more", total - show_count);
            }
        }
    }
}

/// Print prev/next neighbors.
/// For structure tree: siblings at the same depth under the same parent.
/// For source/target: file-order neighbors.
fn print_neighbors(nodes: &[ScannedNode], kind: &TreeKind, node: &ScannedNode) {
    let idx = match nodes.iter().position(|n| n.line_number == node.line_number) {
        Some(i) => i,
        None => return,
    };

    match kind {
        TreeKind::Structure => {
            // Find siblings: nodes at same depth under same parent
            let siblings = find_siblings(nodes, node);
            let sib_idx = siblings.iter().position(|n| n.line_number == node.line_number);
            if let Some(si) = sib_idx {
                if si > 0 {
                    println!("  prev: [{}]", siblings[si - 1].id.short);
                }
                if si + 1 < siblings.len() {
                    println!("  next: [{}]", siblings[si + 1].id.short);
                }
            }
        }
        _ => {
            // File-order neighbors
            if idx > 0 {
                println!("  prev: [{}]", nodes[idx - 1].id.short);
            }
            if idx + 1 < nodes.len() {
                println!("  next: [{}]", nodes[idx + 1].id.short);
            }
        }
    }
}

/// Find direct children of a node.
/// For structure tree: only heading children (depth+1), not action blocks.
/// For source/target: all direct children including action blocks.
fn find_children<'a>(
    nodes: &'a [ScannedNode],
    kind: &TreeKind,
    parent: &ScannedNode,
) -> Vec<&'a ScannedNode> {
    let idx = match nodes.iter().position(|n| n.line_number == parent.line_number) {
        Some(i) => i,
        None => return vec![],
    };
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
            None => {
                // Action blocks are children in source/target, but not in structure
                if *kind != TreeKind::Structure {
                    children.push(node);
                }
            }
            _ => {}
        }
    }
    children
}

/// Find siblings of a node: other children of the same parent at the same depth.
fn find_siblings<'a>(nodes: &'a [ScannedNode], node: &ScannedNode) -> Vec<&'a ScannedNode> {
    // Find the parent
    let parent = match scan::find_parent(nodes, node) {
        Some(p) => p,
        None => {
            // No parent — siblings are all nodes at the same depth (top-level)
            let node_depth = node.depth;
            return nodes.iter()
                .filter(|n| n.depth == node_depth)
                .collect();
        }
    };

    let parent_idx = match nodes.iter().position(|n| n.line_number == parent.line_number) {
        Some(i) => i,
        None => return vec![],
    };
    let parent_depth = match parent.depth {
        Some(d) => d,
        None => return vec![],
    };

    let node_depth = node.depth;
    let mut siblings = vec![];
    for n in &nodes[parent_idx + 1..] {
        match n.depth {
            Some(d) if d <= parent_depth => break,
            d if d == node_depth => siblings.push(n),
            _ => {}
        }
    }
    siblings
}

/// Truncate a string to max_len characters, appending "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let end = s.char_indices()
            .nth(max_len)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}...", &s[..end])
    }
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
