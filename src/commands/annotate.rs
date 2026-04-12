use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use csvs::{Dataset, Entry};

use crate::resolve;
use crate::scan;
use crate::types::Addr;

/// Write annotation text to a structure node.
///
/// Two modes:
/// 1. Address resolves in structure.fountain → insert action block after the heading (existing).
/// 2. Address resolves in source.fountain → look up structure UUID via CSVS,
///    find parent structure heading via structure-child.csv, insert new action block
///    under that parent in structure.fountain.
pub async fn run(
    intent_dir: &Path,
    addr_str: &str,
    text: &str,
    note: Option<&str>,
) -> Result<(), String> {
    let structure_path = intent_dir.join("prose/structure.fountain");

    if !structure_path.exists() {
        return Err("No structure.fountain found.".into());
    }

    let addr = Addr::parse(addr_str);

    // Try structure.fountain first
    let struct_nodes = scan::scan_all(&structure_path);
    if let Some(target) = resolve::resolve(&struct_nodes, &addr) {
        if !target.text.is_empty() {
            return Err(format!(
                "Node [{}] already has text: \"{}\"",
                target.id.short, target.text
            ));
        }
        return annotate_existing(&structure_path, target.line_number, &target.id.short, text, note);
    }

    // Not in structure — try source.fountain
    let source_path = intent_dir.join("prose/source.fountain");
    if !source_path.exists() {
        return Err(format!("Node not found: {addr_str}"));
    }

    let source_nodes = scan::scan_all(&source_path);
    let source_node = resolve::resolve(&source_nodes, &addr)
        .ok_or_else(|| format!("Node not found: {addr_str}"))?;

    let source_short = &source_node.id.short;

    // Look up structure UUID via source-structure.csv
    let csvs_dir = intent_dir.join("csvs");
    let struct_short = lookup_structure_for_source(&csvs_dir, source_short)
        .await
        .ok_or_else(|| {
            format!("No structure mapping found for source [{source_short}] in source-structure.csv")
        })?;

    // Check if structure UUID already exists in fountain
    if let Some(existing) = scan::find_by_hex(&struct_nodes, &struct_short) {
        if !existing.text.is_empty() {
            return Err(format!(
                "Structure node [{struct_short}] already has text: \"{}\"",
                existing.text
            ));
        }
        return annotate_existing(&structure_path, existing.line_number, &struct_short, text, note);
    }

    // Structure UUID not in fountain yet — find parent via structure-child.csv
    let parent_short = lookup_parent_structure(&csvs_dir, &struct_short)
        .ok_or_else(|| {
            format!("No parent found for structure [{struct_short}] in structure-child.csv")
        })?;

    // Find the parent heading in structure.fountain
    let parent_node = scan::find_by_hex(&struct_nodes, &parent_short).ok_or_else(|| {
        format!("Parent structure [{parent_short}] not found in structure.fountain")
    })?;

    // Insert after the parent heading's last child (or right after the heading if no children)
    annotate_new(&structure_path, &struct_nodes, parent_node.line_number, &struct_short, text, note)
}

/// Insert annotation as action block after an existing empty heading in structure.fountain.
fn annotate_existing(
    structure_path: &Path,
    target_line: usize,
    target_id: &str,
    text: &str,
    note: Option<&str>,
) -> Result<(), String> {
    let content = fs::read_to_string(structure_path)
        .map_err(|e| format!("Failed to read structure.fountain: {e}"))?;

    let lines: Vec<&str> = content.lines().collect();
    let temp_path = structure_path.with_extension("fountain.tmp");
    let mut out = fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create temp file: {e}"))?;

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;

        if line_num == target_line {
            writeln!(out, "{line}").map_err(|e| e.to_string())?;
            writeln!(out).map_err(|e| e.to_string())?;
            write!(out, "{text} [[{target_id}]]").map_err(|e| e.to_string())?;

            if let Some(n) = note {
                writeln!(out).map_err(|e| e.to_string())?;
                write!(out, "[[{n}]]").map_err(|e| e.to_string())?;
            }

            writeln!(out).map_err(|e| e.to_string())?;
        } else {
            writeln!(out, "{line}").map_err(|e| e.to_string())?;
        }
    }

    fs::rename(&temp_path, structure_path)
        .map_err(|e| format!("Failed to replace structure.fountain: {e}"))?;

    println!("Annotated [{target_id}] (existing heading): \"{text}\"");
    Ok(())
}

/// Insert a brand-new action block under a parent heading in structure.fountain.
/// The new block goes after the parent's last existing child.
fn annotate_new(
    structure_path: &Path,
    nodes: &[crate::types::ScannedNode],
    parent_line: usize,
    struct_id: &str,
    text: &str,
    note: Option<&str>,
) -> Result<(), String> {
    let content = fs::read_to_string(structure_path)
        .map_err(|e| format!("Failed to read structure.fountain: {e}"))?;

    let lines: Vec<&str> = content.lines().collect();

    // Find where the parent's section ends — the next node at same or lower depth
    let parent_idx = nodes.iter().position(|n| n.line_number == parent_line)
        .ok_or("Parent node not found in scanned nodes")?;
    let parent_depth = nodes[parent_idx].depth
        .ok_or("Parent must be a heading")?;

    // Find the line after the last child of this parent (or after parent itself)
    let mut insert_after_line = parent_line;
    for node in &nodes[parent_idx + 1..] {
        match node.depth {
            Some(d) if d <= parent_depth => break,
            _ => {
                // This node is a child/grandchild — track the last line in this section
                insert_after_line = node.line_number;
                // Also account for note lines that follow this node
                // (they're on subsequent lines but not separate nodes)
            }
        }
    }

    // Skip past any note lines or blank lines after the last child node
    let mut actual_insert = insert_after_line;
    for i in insert_after_line..lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || line.starts_with("[[") {
            actual_insert = i + 1; // 0-indexed line index
        } else {
            break;
        }
    }

    let temp_path = structure_path.with_extension("fountain.tmp");
    let mut out = fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create temp file: {e}"))?;

    for (i, line) in lines.iter().enumerate() {
        writeln!(out, "{line}").map_err(|e| e.to_string())?;

        if i + 1 == actual_insert {
            // Insert blank line + action block
            writeln!(out).map_err(|e| e.to_string())?;
            write!(out, "{text} [[{struct_id}]]").map_err(|e| e.to_string())?;

            if let Some(n) = note {
                writeln!(out).map_err(|e| e.to_string())?;
                write!(out, "[[{n}]]").map_err(|e| e.to_string())?;
            }

            writeln!(out).map_err(|e| e.to_string())?;
        }
    }

    // If insert point is at the very end
    if actual_insert >= lines.len() {
        writeln!(out).map_err(|e| e.to_string())?;
        write!(out, "{text} [[{struct_id}]]").map_err(|e| e.to_string())?;

        if let Some(n) = note {
            writeln!(out).map_err(|e| e.to_string())?;
            write!(out, "[[{n}]]").map_err(|e| e.to_string())?;
        }

        writeln!(out).map_err(|e| e.to_string())?;
    }

    fs::rename(&temp_path, structure_path)
        .map_err(|e| format!("Failed to replace structure.fountain: {e}"))?;

    println!("Annotated [{struct_id}] (new block under parent): \"{text}\"");
    Ok(())
}

/// Look up the structure short ID for a source short ID via source-structure.csv.
async fn lookup_structure_for_source(csvs_dir: &Path, source_short: &str) -> Option<String> {
    let dir = PathBuf::from(csvs_dir);
    let dataset = Dataset::open(&dir).await.ok()?;

    let query = Entry {
        base: "source".to_string(),
        base_value: None,
        leader_value: None,
        leaves: std::collections::HashMap::from([(
            "structure".to_string(),
            vec![Entry::new("structure")],
        )]),
    };

    let results: Vec<Entry> = dataset.select_record(vec![query]).await.ok()?;

    for entry in &results {
        if let Some(src_full) = &entry.base_value {
            let short = src_full.split('-').next().unwrap_or(src_full);
            if short == source_short {
                if let Some(leaves) = entry.leaves.get("structure") {
                    for l in leaves {
                        if let Some(struct_full) = &l.base_value {
                            let struct_short = struct_full.split('-').next().unwrap_or(struct_full);
                            return Some(struct_short.to_string());
                        }
                    }
                }
            }
        }
    }

    None
}

/// Look up the parent structure short ID for a child structure short ID via structure-child.csv.
/// Reads the CSV directly — each line is "parent_uuid,child_uuid".
fn lookup_parent_structure(csvs_dir: &Path, child_short: &str) -> Option<String> {
    let csv_path = csvs_dir.join("structure-child.csv");
    let content = fs::read_to_string(&csv_path).ok()?;

    for line in content.lines() {
        let mut parts = line.splitn(2, ',');
        let parent_full = parts.next()?;
        let child_full = parts.next()?;
        let child_s = child_full.split('-').next().unwrap_or(child_full);
        if child_s == child_short {
            let parent_s = parent_full.split('-').next().unwrap_or(parent_full);
            return Some(parent_s.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_intent(structure_content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let prose_dir = dir.path().join("prose");
        fs::create_dir_all(&prose_dir).unwrap();
        fs::write(prose_dir.join("structure.fountain"), structure_content).unwrap();
        dir
    }

    #[tokio::test]
    async fn annotate_empty_heading() {
        let dir = setup_intent("### [[abcd1234]]\n\n### already done [[efef5678]]\n");
        run(dir.path(), "abcd1234", "this is what it does", None)
            .await
            .unwrap();

        let result = fs::read_to_string(dir.path().join("prose/structure.fountain")).unwrap();
        assert!(result.contains("this is what it does [[abcd1234]]"));
        assert!(result.contains("### [[abcd1234]]"));
    }

    #[tokio::test]
    async fn annotate_with_note() {
        let dir = setup_intent("### [[abcd1234]]\n");
        run(
            dir.path(),
            "abcd1234",
            "annotation text",
            Some("translator note here"),
        )
        .await
        .unwrap();

        let result = fs::read_to_string(dir.path().join("prose/structure.fountain")).unwrap();
        assert!(result.contains("annotation text [[abcd1234]]"));
        assert!(result.contains("[[translator note here]]"));
    }

    #[tokio::test]
    async fn refuses_nonempty() {
        let dir = setup_intent("### already has text [[abcd1234]]\n");
        let result = run(dir.path(), "abcd1234", "new text", None).await;
        assert!(result.is_err());
    }
}
