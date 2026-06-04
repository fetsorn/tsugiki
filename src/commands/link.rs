use std::fs;
use std::path::Path;

use crate::store;

/// Link a target node to a structure node (provenance), or reparent it.
///
/// `tsugiki link <target-addr>` — shows numbered unmapped structure annotations, pick one.
/// `tsugiki link <target-addr> --structure <addr>` — directly map to structure node.
/// `tsugiki link <target-addr> --parent <addr>` — reparent target node.
pub fn run(
    intent_dir: &Path,
    target_addr: &str,
    structure_addr: Option<&str>,
    parent_addr: Option<&str>,
) -> Result<(), String> {
    let csvs_dir = intent_dir.join("csvs");

    let target_uuid = store::resolve_uuid(&csvs_dir, target_addr)
        .ok_or_else(|| format!("Target node not found: {target_addr}"))?;

    if let Some(parent) = parent_addr {
        return reparent(&csvs_dir, &target_uuid, parent);
    }

    if let Some(struct_addr) = structure_addr {
        let struct_uuid = store::resolve_uuid(&csvs_dir, struct_addr)
            .ok_or_else(|| format!("Structure node not found: {struct_addr}"))?;
        return map_to_structure(&csvs_dir, &target_uuid, &struct_uuid);
    }

    // Interactive: show candidates
    show_candidates(&csvs_dir, &target_uuid)
}

/// Map a target node to a structure node via structure-target.csv
fn map_to_structure(csvs_dir: &Path, target_uuid: &str, struct_uuid: &str) -> Result<(), String> {
    let st_path = csvs_dir.join("structure-target.csv");

    let edge_line = format!("{struct_uuid},{target_uuid}\n");
    let mut content = if st_path.exists() {
        fs::read_to_string(&st_path).unwrap_or_default()
    } else {
        String::new()
    };
    content.push_str(&edge_line);
    fs::write(&st_path, &content)
        .map_err(|e| format!("Failed to write structure-target.csv: {e}"))?;

    let t_short = store::short_id(target_uuid);
    let s_short = store::short_id(struct_uuid);
    let struct_prose = store::read_prose(csvs_dir, struct_uuid);
    let annotation = if struct_prose.is_empty() { "(empty)".to_string() } else { first_line(&struct_prose, 60) };

    println!("[{t_short}] → [{s_short}] {annotation}");
    Ok(())
}

/// Show numbered candidate structure nodes for mapping.
fn show_candidates(csvs_dir: &Path, target_uuid: &str) -> Result<(), String> {
    let target_prose = store::read_prose(csvs_dir, target_uuid);
    let t_short = store::short_id(target_uuid);

    println!("target [{t_short}]: {}", first_line(&target_prose, 80));
    println!();

    // Get all structure nodes
    let sc_path = csvs_dir.join("structure-child.csv");
    if !sc_path.exists() {
        return Err("No structure tree found.".into());
    }
    let (struct_forward, struct_reverse) = store::load_edges(&sc_path)?;
    let root = store::find_root(&struct_forward, &struct_reverse)
        .ok_or("No root in structure tree")?;

    // Get already-mapped structure nodes
    let st_path = csvs_dir.join("structure-target.csv");
    let mapped: std::collections::HashSet<String> = if st_path.exists() {
        let (forward, _) = store::load_bridge(&st_path)?;
        forward.keys().cloned().collect()
    } else {
        std::collections::HashSet::new()
    };

    // Walk structure tree, show unmapped nodes with annotations
    let df_order = store::walk_depth_first(&root, &struct_forward);

    // Simple keyword matching: extract words from target prose
    let target_words: std::collections::HashSet<String> = target_prose
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .collect();

    // Score all unmapped nodes
    let mut candidates: Vec<(usize, String, String, usize)> = Vec::new(); // (rank, uuid, prose, score)

    for uuid in &df_order {
        if uuid == &root || mapped.contains(uuid) {
            continue;
        }
        let prose = store::read_prose(csvs_dir, uuid);
        if prose.is_empty() {
            continue;
        }

        let node_words: std::collections::HashSet<String> = prose
            .to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|w| !w.is_empty())
            .collect();

        let overlap = target_words.intersection(&node_words).count();
        candidates.push((0, uuid.clone(), prose, overlap));
    }

    // Sort by overlap descending
    candidates.sort_by(|a, b| b.3.cmp(&a.3));

    // Show top 10
    let show = candidates.len().min(10);
    if show == 0 {
        println!("no unmapped structure nodes with annotations found.");
        return Ok(());
    }

    println!("candidates (by keyword overlap):");
    for (i, (_rank, uuid, prose, score)) in candidates[..show].iter().enumerate() {
        let short = store::short_id(uuid);
        let display = first_line(prose, 70);
        println!("  {}: [{short}] ({score}) {display}", i + 1);
    }

    let unmapped_total = candidates.len();
    if unmapped_total > show {
        println!("  ... and {} more unmapped", unmapped_total - show);
    }

    println!();
    println!("use: tsugiki link {t_short} --structure <addr>");

    Ok(())
}

/// Reparent a target node under a new parent.
fn reparent(csvs_dir: &Path, target_uuid: &str, parent_addr: &str) -> Result<(), String> {
    let parent_uuid = store::resolve_uuid(csvs_dir, parent_addr)
        .ok_or_else(|| format!("Parent not found: {parent_addr}"))?;

    let tc_path = csvs_dir.join("target-child.csv");
    if !tc_path.exists() {
        return Err("No target tree found.".into());
    }

    let content = fs::read_to_string(&tc_path)
        .map_err(|e| format!("Failed to read target-child.csv: {e}"))?;

    // Remove existing edge where target is the child
    let mut new_lines: Vec<String> = Vec::new();
    let mut found = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(2, ',');
        let _left = parts.next().unwrap_or("");
        let right = parts.next().unwrap_or("");
        if right == target_uuid {
            found = true;
            continue; // remove old edge
        }
        new_lines.push(trimmed.to_string());
    }

    // Add new edge
    new_lines.push(format!("{parent_uuid},{target_uuid}"));

    let new_content = if new_lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", new_lines.join("\n"))
    };

    fs::write(&tc_path, &new_content)
        .map_err(|e| format!("Failed to write target-child.csv: {e}"))?;

    let t_short = store::short_id(target_uuid);
    let p_short = store::short_id(&parent_uuid);
    if found {
        println!("reparented [{t_short}] under [{p_short}]");
    } else {
        println!("parented [{t_short}] under [{p_short}]");
    }

    Ok(())
}

fn first_line(s: &str, max_len: usize) -> String {
    let line = s.lines().next().unwrap_or(s);
    if line.len() <= max_len {
        line.to_string()
    } else {
        let end = line.char_indices().nth(max_len).map(|(i, _)| i).unwrap_or(line.len());
        format!("{}...", &line[..end])
    }
}
