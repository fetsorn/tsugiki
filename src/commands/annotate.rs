use std::fs;
use std::path::Path;

use crate::store;

/// Write annotation text to a structure node's prose blob.
///
/// If note is provided, it's appended as a [[note]] line.
/// If overwrite is false, refuses to write over existing prose.
pub fn run(
    intent_dir: &Path,
    addr_str: &str,
    text: &str,
    note: Option<&str>,
    overwrite: bool,
) -> Result<(), String> {
    let csvs_dir = intent_dir.join("csvs");
    let prose_dir = csvs_dir.join("prose");

    // Resolve address to full UUID
    let uuid = store::resolve_uuid(&csvs_dir, addr_str)
        .ok_or_else(|| format!("Node not found: {addr_str}"))?;

    // Verify this UUID is in the structure tree
    let sc_path = csvs_dir.join("structure-child.csv");
    if sc_path.exists() {
        let (forward, reverse) = store::load_edges(&sc_path)?;
        let in_structure = forward.contains_key(&uuid) || reverse.contains_key(&uuid);
        if !in_structure {
            return Err(format!("[{}] is not in the structure tree", store::short_id(&uuid)));
        }
    }

    let existing = store::read_prose(&csvs_dir, &uuid);

    if !existing.is_empty() && !overwrite {
        return Err(format!(
            "Node [{}] already has text: \"{}\". Use --overwrite to replace.",
            store::short_id(&uuid),
            first_line(&existing, 60)
        ));
    }

    // Build prose content
    let mut content = text.to_string();
    if let Some(n) = note {
        content.push('\n');
        content.push_str(&format!("[[{n}]]"));
    }

    fs::create_dir_all(&prose_dir)
        .map_err(|e| format!("Failed to create prose dir: {e}"))?;

    fs::write(prose_dir.join(&uuid), &content)
        .map_err(|e| format!("Failed to write prose blob: {e}"))?;

    let short = store::short_id(&uuid);
    if existing.is_empty() {
        println!("annotated [{short}]: \"{text}\"");
    } else {
        println!("replaced [{short}]: \"{text}\"");
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_intent() -> TempDir {
        let dir = TempDir::new().unwrap();
        let csvs_dir = dir.path().join("csvs");
        let prose_dir = csvs_dir.join("prose");
        fs::create_dir_all(&prose_dir).unwrap();

        // Minimal structure tree: root → one child
        fs::write(
            csvs_dir.join("structure-child.csv"),
            "aaaa1111-0000-0000-0000-000000000000,bbbb2222-0000-0000-0000-000000000000\n",
        ).unwrap();

        fs::write(csvs_dir.join(".csvs.csv"), "csvs,0.0.4\nuuid,test\n").unwrap();
        fs::write(csvs_dir.join("_-_.csv"), "structure,child\n").unwrap();

        dir
    }

    #[test]
    fn annotate_empty_node() {
        let dir = setup_intent();
        run(dir.path(), "bbbb2222", "this is what it does", None, false).unwrap();

        let prose = fs::read_to_string(
            dir.path().join("csvs/prose/bbbb2222-0000-0000-0000-000000000000")
        ).unwrap();
        assert_eq!(prose, "this is what it does");
    }

    #[test]
    fn annotate_with_note() {
        let dir = setup_intent();
        run(dir.path(), "bbbb2222", "annotation", Some("a translator note"), false).unwrap();

        let prose = fs::read_to_string(
            dir.path().join("csvs/prose/bbbb2222-0000-0000-0000-000000000000")
        ).unwrap();
        assert!(prose.contains("annotation"));
        assert!(prose.contains("[[a translator note]]"));
    }

    #[test]
    fn refuses_nonempty() {
        let dir = setup_intent();
        run(dir.path(), "bbbb2222", "first", None, false).unwrap();
        let result = run(dir.path(), "bbbb2222", "second", None, false);
        assert!(result.is_err());
    }

    #[test]
    fn overwrite_existing() {
        let dir = setup_intent();
        run(dir.path(), "bbbb2222", "first", None, false).unwrap();
        run(dir.path(), "bbbb2222", "second", None, true).unwrap();

        let prose = fs::read_to_string(
            dir.path().join("csvs/prose/bbbb2222-0000-0000-0000-000000000000")
        ).unwrap();
        assert_eq!(prose, "second");
    }
}
