use std::collections::HashMap;
use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::store;

/// State file tracking the "current parent" for implicit parenting.
const STATE_FILE: &str = ".tsugiki-parent";

/// Create a new target node with text.
///
/// If --root, creates the target root (must be first node).
/// If --parent <addr>, parents under that node — explicit mode, no auto-linking.
/// Otherwise, auto mode: link to the current structure leaf (first unmapped
/// in depth-first order), parented under the target counterpart of its
/// structure parent. With --same, repeat the previous write's structure
/// links and parent instead (1:N split).
pub fn run(
    intent_dir: &Path,
    text: &str,
    parent_addr: Option<&str>,
    is_root: bool,
    same: bool,
) -> Result<(), String> {
    if !is_root && parent_addr.is_none() {
        return run_auto(intent_dir, text, same);
    }

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

        // Map the target root to the structure root so auto mode can find it
        // before the root has any containment edges.
        let sc_path = csvs_dir.join("structure-child.csv");
        if sc_path.exists() {
            let (s_fwd, s_rev) = store::load_edges(&sc_path)?;
            if let Some(s_root) = store::find_root(&s_fwd, &s_rev) {
                append_line(&csvs_dir.join("structure-target.csv"), &format!("{s_root},{new_uuid}"))?;
            }
        }

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

/// Auto mode: derive everything from state, no cursor file.
///
/// The current structure leaf is the first leaf in depth-first order with no
/// target mapping — the same derivation `next` uses. Completion is implicit:
/// once this write links the leaf, the next write lands on the following one.
fn run_auto(intent_dir: &Path, text: &str, same: bool) -> Result<(), String> {
    let csvs_dir = intent_dir.join("csvs");
    let prose_dir = csvs_dir.join("prose");
    fs::create_dir_all(&prose_dir)
        .map_err(|e| format!("Failed to create prose dir: {e}"))?;

    let sc_path = csvs_dir.join("structure-child.csv");
    if !sc_path.exists() {
        return Err("No structure-child.csv found. Run init first.".into());
    }
    let (s_fwd, s_rev) = store::load_edges(&sc_path)?;
    let s_root = store::find_root(&s_fwd, &s_rev)
        .ok_or("No root found in structure tree")?;

    let st_path = csvs_dir.join("structure-target.csv");
    let (mut st_fwd, st_rev) = if st_path.exists() {
        store::load_bridge(&st_path)?
    } else {
        (HashMap::new(), HashMap::new())
    };

    let tc_path = csvs_dir.join("target-child.csv");
    let (t_fwd, t_rev) = if tc_path.exists() {
        store::load_edges(&tc_path)?
    } else {
        (HashMap::new(), HashMap::new())
    };

    // Target root: from containment edges, or via the structure-root mapping
    // while the root has no children yet.
    let t_root = store::find_root(&t_fwd, &t_rev)
        .or_else(|| st_fwd.get(&s_root).and_then(|ts| ts.first().cloned()))
        .ok_or("No target root. Create it with write --root first.")?;

    let (parent_uuid, structures) = if same {
        // Repeat the previous target leaf's parent and structure links.
        let df = store::walk_depth_first(&t_root, &t_fwd);
        let prev = df
            .iter()
            .rev()
            .find(|u| t_fwd.get(*u).map(|k| k.is_empty()).unwrap_or(true))
            .ok_or("No previous target leaf — nothing for --same to repeat")?;
        let parent = t_rev
            .get(prev)
            .cloned()
            .ok_or("Previous target leaf has no parent")?;
        let structs = st_rev.get(prev).cloned().unwrap_or_default();
        if structs.is_empty() {
            return Err("Previous target leaf has no structure links".into());
        }
        // A 1:N run forms one prose paragraph. If the previous leaf sits in a
        // shared container (root, section), give the run its own paragraph
        // node linked to the same structure node(s), and adopt the previous
        // leaf into it.
        let prev_set: std::collections::HashSet<&String> = structs.iter().collect();
        let parent_set: std::collections::HashSet<&String> = st_rev
            .get(&parent)
            .map(|v| v.iter().collect())
            .unwrap_or_default();
        let parent = if parent_set == prev_set {
            parent
        } else {
            let para = Uuid::new_v4().to_string();
            replace_line(&tc_path, &format!("{parent},{prev}"), &format!("{parent},{para}"))?;
            append_line(&tc_path, &format!("{para},{prev}"))?;
            for s in &structs {
                append_line(&st_path, &format!("{s},{para}"))?;
            }
            println!(
                "  paragraph [{}] adopts [{}]",
                store::short_id(&para),
                store::short_id(prev)
            );
            para
        };
        (parent, structs)
    } else {
        let df = store::walk_depth_first(&s_root, &s_fwd);
        let current = df
            .iter()
            .filter(|u| *u != &s_root)
            .find(|u| {
                let is_leaf = s_fwd.get(*u).map(|k| k.is_empty()).unwrap_or(true);
                is_leaf && !st_fwd.contains_key(*u)
            })
            .cloned()
            .ok_or("All structure leaves are mapped — regrow complete.")?;
        let s_parent = s_rev
            .get(&current)
            .cloned()
            .ok_or("Current structure leaf has no parent")?;
        let parent =
            ensure_target_parent(&csvs_dir, &s_parent, &s_root, &s_rev, &mut st_fwd, &t_root)?;
        (parent, vec![current])
    };

    let new_uuid = Uuid::new_v4().to_string();
    fs::write(prose_dir.join(&new_uuid), text)
        .map_err(|e| format!("Failed to write prose: {e}"))?;
    append_line(&tc_path, &format!("{parent_uuid},{new_uuid}"))?;
    for s in &structures {
        append_line(&st_path, &format!("{s},{new_uuid}"))?;
    }

    let short = store::short_id(&new_uuid);
    let parent_short = store::short_id(&parent_uuid);
    let linked: Vec<String> = structures.iter().map(|s| store::short_id(s)).collect();
    println!(
        "[{short}] under [{parent_short}] → [{}]: \"{text}\"",
        linked.join(", ")
    );
    print_progress(&csvs_dir)?;

    Ok(())
}

/// Resolve (or create) the target counterpart of a structure inner node.
/// Missing counterparts become empty inner nodes — paragraph breaks, not prose.
fn ensure_target_parent(
    csvs_dir: &Path,
    s_node: &str,
    s_root: &str,
    s_rev: &HashMap<String, String>,
    st_fwd: &mut HashMap<String, Vec<String>>,
    t_root: &str,
) -> Result<String, String> {
    if s_node == s_root {
        return Ok(t_root.to_string());
    }
    if let Some(first) = st_fwd.get(s_node).and_then(|ts| ts.first()) {
        return Ok(first.clone());
    }
    let s_grand = s_rev
        .get(s_node)
        .cloned()
        .ok_or("Structure parent chain broken")?;
    let t_grand = ensure_target_parent(csvs_dir, &s_grand, s_root, s_rev, st_fwd, t_root)?;
    let new_uuid = Uuid::new_v4().to_string();
    append_line(&csvs_dir.join("target-child.csv"), &format!("{t_grand},{new_uuid}"))?;
    append_line(&csvs_dir.join("structure-target.csv"), &format!("{s_node},{new_uuid}"))?;
    st_fwd
        .entry(s_node.to_string())
        .or_default()
        .push(new_uuid.clone());
    println!(
        "  paragraph [{}] under [{}]",
        store::short_id(&new_uuid),
        store::short_id(&t_grand)
    );
    Ok(new_uuid)
}

/// Replace the first exact-match line in a tablet, preserving file order.
fn replace_line(path: &Path, old: &str, new: &str) -> Result<(), String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut replaced = false;
    let lines: Vec<&str> = content
        .lines()
        .map(|l| {
            if !replaced && l == old {
                replaced = true;
                new
            } else {
                l
            }
        })
        .collect();
    if !replaced {
        return Err(format!("Line not found in {}: {old}", path.display()));
    }
    fs::write(path, lines.join("\n") + "\n")
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

fn append_line(path: &Path, line: &str) -> Result<(), String> {
    use std::io::Write as IoWrite;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
    writeln!(f, "{line}")
        .map_err(|e| format!("Failed to append to {}: {e}", path.display()))
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
