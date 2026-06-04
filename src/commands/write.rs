use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::store;

/// State file tracking the "current parent" for implicit parenting.
const STATE_FILE: &str = ".tsugiki-parent";

/// Create a new target node with text, parented under an explicit or implicit parent.
///
/// If --root, creates the target root (must be first node).
/// If --parent <addr>, parents under that node.
/// Otherwise, parents under the last-used parent (or root if none).
pub fn run(
    intent_dir: &Path,
    text: &str,
    parent_addr: Option<&str>,
    is_root: bool,
) -> Result<(), String> {
    let csvs_dir = intent_dir.join("csvs");
    let prose_dir = csvs_dir.join("prose");
    let tc_path = csvs_dir.join("target-child.csv");

    fs::create_dir_all(&prose_dir)
        .map_err(|e| format!("Failed to create prose dir: {e}"))?;

    let new_uuid = Uuid::new_v4();

    if is_root {
        // Must be the first target node
        if tc_path.exists() {
            let content = fs::read_to_string(&tc_path).unwrap_or_default();
            if !content.trim().is_empty() {
                return Err("Target root already exists. Use --parent to add children.".into());
            }
        }

        // Write prose
        fs::write(prose_dir.join(new_uuid.to_string()), text)
            .map_err(|e| format!("Failed to write prose: {e}"))?;

        // Create empty target-child.csv with a placeholder so root is discoverable
        // We need at least one edge for root to appear in the tree.
        // For now, root with no children is just a prose blob. First child will create the edge.
        // Save state
        save_current_parent(intent_dir, &new_uuid.to_string())?;

        let short = store::short_id(&new_uuid.to_string());
        println!("root [{short}]: \"{text}\"");
        print_progress(&csvs_dir)?;
        return Ok(());
    }

    // Determine parent
    let parent_uuid = if let Some(addr) = parent_addr {
        store::resolve_uuid(&csvs_dir, addr)
            .ok_or_else(|| format!("Parent not found: {addr}"))?
    } else {
        // Use saved current parent, or find root
        load_current_parent(intent_dir)?
            .or_else(|| find_target_root(&csvs_dir).ok().flatten())
            .ok_or("No target nodes exist yet. Use --root to create the first one.")?
    };

    // Write prose
    fs::write(prose_dir.join(new_uuid.to_string()), text)
        .map_err(|e| format!("Failed to write prose: {e}"))?;

    // Append edge to target-child.csv
    let edge_line = format!("{parent_uuid},{new_uuid}\n");
    let mut content = if tc_path.exists() {
        fs::read_to_string(&tc_path).unwrap_or_default()
    } else {
        String::new()
    };
    content.push_str(&edge_line);
    fs::write(&tc_path, &content)
        .map_err(|e| format!("Failed to write target-child.csv: {e}"))?;

    // Update current parent to this node's parent (siblings by default)
    save_current_parent(intent_dir, &parent_uuid)?;

    let short = store::short_id(&new_uuid.to_string());
    let parent_short = store::short_id(&parent_uuid);
    println!("[{short}] under [{parent_short}]: \"{text}\"");
    print_progress(&csvs_dir)?;

    Ok(())
}

fn save_current_parent(intent_dir: &Path, uuid: &str) -> Result<(), String> {
    fs::write(intent_dir.join(STATE_FILE), uuid)
        .map_err(|e| format!("Failed to save parent state: {e}"))
}

fn load_current_parent(intent_dir: &Path) -> Result<Option<String>, String> {
    let path = intent_dir.join(STATE_FILE);
    if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read parent state: {e}"))?;
        let trimmed = content.trim().to_string();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed))
        }
    } else {
        Ok(None)
    }
}

fn find_target_root(csvs_dir: &Path) -> Result<Option<String>, String> {
    let tc_path = csvs_dir.join("target-child.csv");
    if !tc_path.exists() {
        return Ok(None);
    }
    let (forward, reverse) = store::load_edges(&tc_path)?;
    Ok(store::find_root(&forward, &reverse))
}

fn print_progress(csvs_dir: &Path) -> Result<(), String> {
    // Count target nodes
    let tc_path = csvs_dir.join("target-child.csv");
    let target_count = if tc_path.exists() {
        let (forward, reverse) = store::load_edges(&tc_path)?;
        let all: std::collections::HashSet<&String> = forward.keys()
            .chain(reverse.keys())
            .collect();
        all.len()
    } else {
        0
    };
    // +1 for root if it has no edges yet but prose exists
    // Count prose blobs that are target nodes (approximation)

    // Count structure nodes
    let sc_path = csvs_dir.join("structure-child.csv");
    let structure_count = if sc_path.exists() {
        let (forward, reverse) = store::load_edges(&sc_path)?;
        let root = store::find_root(&forward, &reverse);
        let all: std::collections::HashSet<&String> = forward.keys()
            .chain(reverse.keys())
            .collect();
        // Subtract root
        if root.is_some() { all.len() - 1 } else { all.len() }
    } else {
        0
    };

    // Count mapped (structure-target edges)
    let st_path = csvs_dir.join("structure-target.csv");
    let mapped = if st_path.exists() {
        let content = fs::read_to_string(&st_path).unwrap_or_default();
        content.lines().filter(|l| !l.trim().is_empty()).count()
    } else {
        0
    };

    println!("  target: {target_count} nodes, {mapped}/{structure_count} mapped to structure");
    Ok(())
}
