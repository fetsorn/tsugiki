use std::path::Path;

use crate::store;

/// Display a node with context: text, parent, children, bridges.
///
/// tree_filter: if Some, only show in that tree. If None, show in all trees.
/// child_limit: max children to display (0 = unlimited).
/// max_depth: how many levels of children to show.
pub fn run(
    intent_dir: &Path,
    addr_str: &str,
    tree_filter: Option<&str>,
    child_limit: usize,
    max_depth: usize,
) -> Result<(), String> {
    let csvs_dir = intent_dir.join("csvs");

    // Resolve address to full UUID
    let uuid = store::resolve_uuid(&csvs_dir, addr_str)
        .ok_or_else(|| format!("Node not found: {addr_str}"))?;

    let trees: Vec<(&str, &str)> = match tree_filter {
        Some("source") => vec![("source", "source-child.csv")],
        Some("structure") => vec![("structure", "structure-child.csv")],
        Some("target") => vec![("target", "target-child.csv")],
        Some(t) => return Err(format!("Unknown tree: {t}")),
        None => vec![
            ("source", "source-child.csv"),
            ("structure", "structure-child.csv"),
            ("target", "target-child.csv"),
        ],
    };

    let mut found = false;

    for (tree_name, tablet_name) in &trees {
        let tablet_path = csvs_dir.join(tablet_name);
        if !tablet_path.exists() {
            continue;
        }

        let (forward, reverse) = store::load_edges(&tablet_path)?;

        // Check if this UUID appears in this tree
        let in_tree = forward.contains_key(&uuid) || reverse.contains_key(&uuid);
        if !in_tree {
            continue;
        }

        if found {
            println!();
        }
        found = true;

        let short = store::short_id(&uuid);
        let prose = store::read_prose(&csvs_dir, &uuid);
        let has_children = forward.get(&uuid).map(|k| !k.is_empty()).unwrap_or(false);
        let has_parent = reverse.contains_key(&uuid);
        let kind = node_kind(has_parent, has_children, &prose);

        println!("[{tree_name}] [{short}] {kind}");

        if prose.is_empty() {
            println!("  text: (empty)");
        } else {
            let display = truncate(&prose, 120);
            println!("  text: {display}");
        }

        // Parent
        if let Some(parent_uuid) = reverse.get(&uuid) {
            let parent_short = store::short_id(parent_uuid);
            let parent_prose = store::read_prose(&csvs_dir, parent_uuid);
            let parent_text = if parent_prose.is_empty() {
                "(empty)".to_string()
            } else {
                truncate(&parent_prose, 60)
            };
            println!("  parent: [{parent_short}] {parent_text}");
        }

        // Children
        if let Some(kids) = forward.get(&uuid) {
            if !kids.is_empty() {
                let total = kids.len();
                let show_count = if child_limit == 0 { total } else { child_limit.min(total) };
                println!("  children ({total}):");
                print_children(&csvs_dir, &forward, &kids[..show_count], max_depth, 1);
                if show_count < total {
                    println!("    ... and {} more", total - show_count);
                }
            }
        }

        // Bridges
        print_bridges(&csvs_dir, tree_name, &uuid)?;

        // Siblings (prev/next in parent's child list)
        if let Some(parent_uuid) = reverse.get(&uuid) {
            if let Some(siblings) = forward.get(parent_uuid) {
                if let Some(pos) = siblings.iter().position(|s| s == &uuid) {
                    if pos > 0 {
                        println!("  prev: [{}]", store::short_id(&siblings[pos - 1]));
                    }
                    if pos + 1 < siblings.len() {
                        println!("  next: [{}]", store::short_id(&siblings[pos + 1]));
                    }
                }
            }
        }
    }

    if found {
        Ok(())
    } else {
        Err(format!("Node not found in any tree: {addr_str}"))
    }
}

/// Print children recursively up to max_depth.
fn print_children(
    csvs_dir: &Path,
    forward: &std::collections::HashMap<String, Vec<String>>,
    kids: &[String],
    max_depth: usize,
    current_depth: usize,
) {
    let indent = "  ".repeat(current_depth + 1);
    for kid in kids {
        let short = store::short_id(kid);
        let prose = store::read_prose(csvs_dir, kid);
        let text = if prose.is_empty() {
            "(empty)".to_string()
        } else {
            truncate(&prose, 72)
        };
        let has_kids = forward.get(kid).map(|k| !k.is_empty()).unwrap_or(false);
        let kind = node_kind(true, has_kids, &prose);
        println!("{indent}[{short}] {kind} {text}");

        if current_depth < max_depth {
            if let Some(grandkids) = forward.get(kid) {
                print_children(csvs_dir, forward, grandkids, max_depth, current_depth + 1);
            }
        }
    }
}

/// Print bridge counterparts.
fn print_bridges(csvs_dir: &Path, tree_name: &str, uuid: &str) -> Result<(), String> {
    match tree_name {
        "source" => {
            let bridge_path = csvs_dir.join("source-structure.csv");
            if bridge_path.exists() {
                let (forward, _) = store::load_bridge(&bridge_path)?;
                if let Some(targets) = forward.get(uuid) {
                    println!("  structure:");
                    for t in targets {
                        let short = store::short_id(t);
                        let prose = store::read_prose(csvs_dir, t);
                        let text = if prose.is_empty() { "(empty)".to_string() } else { truncate(&prose, 60) };
                        println!("    [{short}] {text}");
                    }
                }
            }
        }
        "structure" => {
            let ss_path = csvs_dir.join("source-structure.csv");
            if ss_path.exists() {
                let (_, reverse) = store::load_bridge(&ss_path)?;
                if let Some(sources) = reverse.get(uuid) {
                    println!("  source:");
                    for s in sources {
                        let short = store::short_id(s);
                        let prose = store::read_prose(csvs_dir, s);
                        let text = if prose.is_empty() { "(empty)".to_string() } else { truncate(&prose, 60) };
                        println!("    [{short}] {text}");
                    }
                }
            }
            let st_path = csvs_dir.join("structure-target.csv");
            if st_path.exists() {
                let (forward, _) = store::load_bridge(&st_path)?;
                if let Some(targets) = forward.get(uuid) {
                    println!("  target:");
                    for t in targets {
                        let short = store::short_id(t);
                        let prose = store::read_prose(csvs_dir, t);
                        let text = if prose.is_empty() { "(empty)".to_string() } else { truncate(&prose, 60) };
                        println!("    [{short}] {text}");
                    }
                }
            }
        }
        "target" => {
            let bridge_path = csvs_dir.join("structure-target.csv");
            if bridge_path.exists() {
                let (_, reverse) = store::load_bridge(&bridge_path)?;
                if let Some(sources) = reverse.get(uuid) {
                    println!("  structure:");
                    for s in sources {
                        let short = store::short_id(s);
                        let prose = store::read_prose(csvs_dir, s);
                        let text = if prose.is_empty() { "(empty)".to_string() } else { truncate(&prose, 60) };
                        println!("    [{short}] {text}");
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Infer a human-readable kind label from tree position and prose.
fn node_kind(has_parent: bool, has_children: bool, prose: &str) -> &'static str {
    match (has_parent, has_children, prose.is_empty()) {
        (false, _, _) => "root",
        (true, true, true) => "paragraph",   // container, no text
        (true, true, false) => "heading",     // container with text
        (true, false, true) => "empty",       // leaf, no text yet
        (true, false, false) => "leaf",       // leaf with text
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    // Take first line only, then truncate
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.len() <= max_len {
        first_line.to_string()
    } else {
        let end = first_line
            .char_indices()
            .nth(max_len)
            .map(|(i, _)| i)
            .unwrap_or(first_line.len());
        format!("{}...", &first_line[..end])
    }
}
