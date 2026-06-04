use std::path::Path;

use crate::store;

/// Find the next node that needs attention and return its address.
///
/// Annotate phase (breadth-first): first structure node with no prose blob.
/// Regrow phase (depth-first): first structure leaf not in structure-target.csv.
pub fn run(intent_dir: &Path) -> Result<(), String> {
    let csvs_dir = intent_dir.join("csvs");

    let sc_path = csvs_dir.join("structure-child.csv");
    if !sc_path.exists() {
        return Err("No structure-child.csv found. Run init first.".into());
    }

    let (struct_forward, struct_reverse) = store::load_edges(&sc_path)?;
    let root = store::find_root(&struct_forward, &struct_reverse)
        .ok_or("No root found in structure tree")?;

    // Check annotate phase: any structure node without prose?
    let bf_order = store::walk_breadth_first(&root, &struct_forward);
    let mut unannotated = None;

    for uuid in &bf_order {
        // Skip root — it's a container, not annotated
        if uuid == &root {
            continue;
        }
        let prose = store::read_prose(&csvs_dir, uuid);
        if prose.is_empty() {
            unannotated = Some(uuid.clone());
            break;
        }
    }

    if let Some(struct_uuid) = unannotated {
        // Load source tree for recursive text gathering
        let src_path = csvs_dir.join("source-child.csv");
        let source_forward = if src_path.exists() {
            let (fwd, _) = store::load_edges(&src_path)?;
            fwd
        } else {
            std::collections::HashMap::new()
        };

        // Find corresponding source node via bridge
        let ss_path = csvs_dir.join("source-structure.csv");
        let source_text = if ss_path.exists() {
            let (_, ss_reverse) = store::load_bridge(&ss_path)?;
            if let Some(sources) = ss_reverse.get(&struct_uuid) {
                sources.iter()
                    .map(|s| gather_source_text(&csvs_dir, s, &source_forward))
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>()
                    .join(" | ")
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let short = store::short_id(&struct_uuid);
        let has_children = struct_forward.get(&struct_uuid).map(|k| !k.is_empty()).unwrap_or(false);
        let kind = if !has_children { "leaf" } else { "paragraph" };

        // Parent
        let parent_info = if let Some(parent_uuid) = struct_reverse.get(&struct_uuid) {
            let parent_short = store::short_id(parent_uuid);
            let parent_prose = store::read_prose(&csvs_dir, parent_uuid);
            if parent_prose.is_empty() {
                format!("[{parent_short}]")
            } else {
                format!("[{parent_short}] {}", first_line(&parent_prose, 60))
            }
        } else {
            String::new()
        };

        println!("annotate phase");
        println!("  structure [{short}] {kind}");
        if !source_text.is_empty() {
            println!("  source: {source_text}");
        }
        if !parent_info.is_empty() {
            println!("  parent: {parent_info}");
        }

        return Ok(());
    }

    // All annotated — check regrow phase
    let st_path = csvs_dir.join("structure-target.csv");
    let mapped_structures: std::collections::HashSet<String> = if st_path.exists() {
        let (forward, _) = store::load_bridge(&st_path)?;
        forward.keys().cloned().collect()
    } else {
        std::collections::HashSet::new()
    };

    let df_order = store::walk_depth_first(&root, &struct_forward);

    for uuid in &df_order {
        if uuid == &root {
            continue;
        }
        // Only leaf structure nodes need regrow
        let is_leaf = !struct_forward.contains_key(uuid)
            || struct_forward.get(uuid).map(|k| k.is_empty()).unwrap_or(true);
        if !is_leaf {
            continue;
        }
        if mapped_structures.contains(uuid) {
            continue;
        }

        let short = store::short_id(uuid);
        let prose = store::read_prose(&csvs_dir, uuid);
        let annotation = if prose.is_empty() { "(empty)" } else { &prose };

        // Find source text
        let src_path = csvs_dir.join("source-child.csv");
        let source_forward = if src_path.exists() {
            let (fwd, _) = store::load_edges(&src_path)?;
            fwd
        } else {
            std::collections::HashMap::new()
        };
        let ss_path = csvs_dir.join("source-structure.csv");
        let source_text = if ss_path.exists() {
            let (_, ss_reverse) = store::load_bridge(&ss_path)?;
            if let Some(sources) = ss_reverse.get(uuid) {
                sources.iter()
                    .map(|s| gather_source_text(&csvs_dir, s, &source_forward))
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>()
                    .join(" | ")
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        println!("regrow phase");
        println!("  structure [{short}] {}", first_line(annotation, 80));
        if !source_text.is_empty() {
            println!("  source: {}", first_line(&source_text, 120));
        }

        return Ok(());
    }

    println!("all phases complete — all structure nodes annotated and mapped to target.");
    Ok(())
}

/// Find the next unannotated structure UUID (for use by annotate command).
pub fn find_next_unannotated(intent_dir: &Path) -> Result<Option<String>, String> {
    let csvs_dir = intent_dir.join("csvs");

    let sc_path = csvs_dir.join("structure-child.csv");
    if !sc_path.exists() {
        return Err("No structure-child.csv found. Run init first.".into());
    }

    let (struct_forward, struct_reverse) = store::load_edges(&sc_path)?;
    let root = store::find_root(&struct_forward, &struct_reverse)
        .ok_or("No root found in structure tree")?;

    let bf_order = store::walk_breadth_first(&root, &struct_forward);

    for uuid in &bf_order {
        if uuid == &root {
            continue;
        }
        let prose = store::read_prose(&csvs_dir, uuid);
        if prose.is_empty() {
            return Ok(Some(uuid.clone()));
        }
    }

    Ok(None)
}

/// Gather text from a source node, recursively collecting from children if the node itself is empty.
fn gather_source_text(
    csvs_dir: &Path,
    uuid: &str,
    source_forward: &std::collections::HashMap<String, Vec<String>>,
) -> String {
    let prose = store::read_prose(csvs_dir, uuid);
    if !prose.is_empty() {
        return prose;
    }
    // Container node — collect from children in order
    if let Some(kids) = source_forward.get(uuid) {
        let parts: Vec<String> = kids.iter()
            .map(|k| gather_source_text(csvs_dir, k, source_forward))
            .filter(|p| !p.is_empty())
            .collect();
        return parts.join(" ");
    }
    String::new()
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
