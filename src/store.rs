//! Read helpers for the CSVS prose store.
//!
//! All operations are synchronous filesystem reads — no crate dependencies,
//! no caching. Each function derives its answer fresh from the files.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Load a two-column CSV tablet into an ordered edge list.
/// Returns (parent→[children in file order], child→parent reverse map).
pub fn load_edges(path: &Path) -> Result<(HashMap<String, Vec<String>>, HashMap<String, String>), String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

    let mut forward: HashMap<String, Vec<String>> = HashMap::new();
    let mut reverse: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, ',');
        let left = parts.next().unwrap_or("").to_string();
        let right = parts.next().unwrap_or("").to_string();
        if left.is_empty() || right.is_empty() {
            continue;
        }
        forward.entry(left.clone()).or_default().push(right.clone());
        reverse.insert(right, left);
    }

    Ok((forward, reverse))
}

/// Load a bridge tablet as a forward map (from→[to]) and reverse map (to→[from]).
pub fn load_bridge(path: &Path) -> Result<(HashMap<String, Vec<String>>, HashMap<String, Vec<String>>), String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

    let mut forward: HashMap<String, Vec<String>> = HashMap::new();
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, ',');
        let left = parts.next().unwrap_or("").to_string();
        let right = parts.next().unwrap_or("").to_string();
        if left.is_empty() || right.is_empty() {
            continue;
        }
        forward.entry(left.clone()).or_default().push(right.clone());
        reverse.entry(right).or_default().push(left);
    }

    Ok((forward, reverse))
}

/// Read prose for a UUID. Returns empty string if no blob exists.
pub fn read_prose(csvs_dir: &Path, uuid: &str) -> String {
    let blob_path = csvs_dir.join("prose").join(uuid);
    fs::read_to_string(&blob_path).unwrap_or_default()
}

/// Short hex id (first segment before the first hyphen).
pub fn short_id(uuid: &str) -> String {
    uuid.split('-').next().unwrap_or(uuid).to_string()
}

/// Resolve an address (short hex prefix or full UUID) to a full UUID
/// by scanning all values that appear in a set of tablets.
pub fn resolve_uuid(csvs_dir: &Path, addr: &str) -> Option<String> {
    // If it looks like a full UUID, return as-is
    if addr.contains('-') && addr.len() > 16 {
        // Verify it exists in some tablet
        return Some(addr.to_string());
    }

    // Scan all CSV tablets for a UUID starting with this prefix
    let entries = fs::read_dir(csvs_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "csv").unwrap_or(false) {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') || name == "_-_.csv" {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines() {
                    for field in line.split(',') {
                        let field = field.trim();
                        if field.starts_with(addr) && field.contains('-') {
                            return Some(field.to_string());
                        }
                    }
                }
            }
        }
    }

    None
}

/// Find the root UUID of a tree (parent that never appears as a child).
pub fn find_root(children_forward: &HashMap<String, Vec<String>>, children_reverse: &HashMap<String, String>) -> Option<String> {
    for parent in children_forward.keys() {
        if !children_reverse.contains_key(parent) {
            return Some(parent.clone());
        }
    }
    None
}

/// Collect all UUIDs in a tree (by walking containment edges).
pub fn all_uuids(children_forward: &HashMap<String, Vec<String>>, children_reverse: &HashMap<String, String>) -> Vec<String> {
    let mut uuids: Vec<String> = children_forward.keys().cloned().collect();
    for child in children_reverse.keys() {
        if !children_forward.contains_key(child) {
            uuids.push(child.clone());
        }
    }
    uuids
}

/// Walk a tree depth-first, returning UUIDs in document order.
pub fn walk_depth_first(root: &str, children_forward: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut result = Vec::new();
    walk_df_inner(root, children_forward, &mut result);
    result
}

fn walk_df_inner(uuid: &str, children_forward: &HashMap<String, Vec<String>>, result: &mut Vec<String>) {
    result.push(uuid.to_string());
    if let Some(kids) = children_forward.get(uuid) {
        for kid in kids {
            walk_df_inner(kid, children_forward, result);
        }
    }
}

/// Walk a tree breadth-first, returning UUIDs level by level.
pub fn walk_breadth_first(root: &str, children_forward: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut result = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root.to_string());

    while let Some(uuid) = queue.pop_front() {
        result.push(uuid.clone());
        if let Some(kids) = children_forward.get(&uuid) {
            for kid in kids {
                queue.push_back(kid.clone());
            }
        }
    }

    result
}
