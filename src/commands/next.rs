use std::collections::HashSet;
use std::path::{Path, PathBuf};

use csvs::{Dataset, Entry};

use crate::scan;

/// Find the next source node that needs a structure annotation.
///
/// Algorithm:
/// 1. Walk source nodes in document order (source.fountain).
/// 2. For each, look up its structure UUID via source-structure.csv.
/// 3. Collect all UUIDs present in structure.fountain.
/// 4. Return the first source node whose structure UUID is absent from fountain.
pub async fn run(intent_dir: &Path) -> Result<(), String> {
    let source_path = intent_dir.join("prose/source.fountain");
    let structure_path = intent_dir.join("prose/structure.fountain");
    let csvs_dir = intent_dir.join("csvs");

    if !source_path.exists() {
        return Err("No source.fountain found. Run init first.".into());
    }

    // Collect all UUIDs present in structure.fountain (just the short hex ids)
    let structure_ids: HashSet<String> = if structure_path.exists() {
        scan::scan_all(&structure_path)
            .into_iter()
            .map(|n| n.id.short)
            .collect()
    } else {
        HashSet::new()
    };

    // Load source-structure mappings from CSVS
    let bridge = load_source_structure(&csvs_dir).await?;

    // Walk source nodes in document order
    let source_nodes = scan::scan_all(&source_path);

    for source_node in &source_nodes {
        // Skip section headers with no text — they're grouping markers,
        // annotated via their children's structure headings
        if source_node.text.is_empty() {
            continue;
        }

        // Look up the structure UUID for this source node
        let struct_id = match bridge.get(&source_node.id.short) {
            Some(id) => id,
            None => continue, // no mapping — skip
        };

        // Check if this structure UUID is already in structure.fountain
        if !structure_ids.contains(struct_id.as_str()) {
            println!("annotate phase");
            println!(
                "  source L{} [{}]",
                source_node.line_number, source_node.id.short,
            );
            println!("  text: {}", source_node.text);
            println!("  structure: [{struct_id}] (not yet in fountain)");

            // Show parent context
            if let Some(parent) = scan::find_parent(&source_nodes, source_node) {
                println!("  parent: {} [{}]", parent.text, parent.id.short);
            }

            return Ok(());
        }
    }

    println!("annotate phase complete — all source nodes have structure annotations.");
    Ok(())
}

/// Load source→structure short-id mappings from source-structure.csv via CSVS.
/// Returns a map from source short id to structure short id.
async fn load_source_structure(
    csvs_dir: &Path,
) -> Result<std::collections::HashMap<String, String>, String> {
    let dir = PathBuf::from(csvs_dir);
    let dataset = Dataset::open(&dir)
        .await
        .map_err(|e| format!("Failed to open csvs: {e}"))?;

    let query = Entry {
        base: "source".to_string(),
        base_value: None,
        leader_value: None,
        leaves: std::collections::HashMap::from([(
            "structure".to_string(),
            vec![Entry::new("structure")],
        )]),
    };

    let results: Vec<Entry> = dataset
        .select_record(vec![query])
        .await
        .map_err(|e| format!("Failed to query source-structure: {e}"))?;

    let mut map = std::collections::HashMap::new();
    for entry in &results {
        if let Some(src_full) = &entry.base_value {
            let src_short = src_full.split('-').next().unwrap_or(src_full);
            if let Some(leaves) = entry.leaves.get("structure") {
                for l in leaves {
                    if let Some(struct_full) = &l.base_value {
                        let struct_short = struct_full.split('-').next().unwrap_or(struct_full);
                        map.insert(src_short.to_string(), struct_short.to_string());
                    }
                }
            }
        }
    }

    Ok(map)
}
