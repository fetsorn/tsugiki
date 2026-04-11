use std::fs;
use std::io::Write;
use std::path::Path;

use crate::scan;
use crate::types::Addr;

/// Write annotation text to a structure node.
///
/// Streaming transformation: read line by line, write to temp file, replace original.
/// When the target node is found, insert the annotation as an action block after it.
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

    // First, scan to find the target node
    let nodes = scan::scan_all(&structure_path);
    let target = crate::resolve::resolve(&nodes, &addr)
        .ok_or_else(|| format!("Node not found: {addr_str}"))?;

    if !target.text.is_empty() {
        return Err(format!(
            "Node [{}] already has text: \"{}\"",
            target.id.short, target.text
        ));
    }

    let target_line = target.line_number;
    let target_id = target.id.short.clone();

    // Streaming write: read original, write to temp, replace
    let content = fs::read_to_string(&structure_path)
        .map_err(|e| format!("Failed to read structure.fountain: {e}"))?;

    let lines: Vec<&str> = content.lines().collect();
    let temp_path = structure_path.with_extension("fountain.tmp");
    let mut out = fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create temp file: {e}"))?;

    let mut new_line_number = 0usize;

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1; // 1-indexed

        if line_num == target_line {
            // This is the heading line — keep it as is
            writeln!(out, "{line}").map_err(|e| e.to_string())?;
            // Insert the annotation as an action block on the next line
            writeln!(out).map_err(|e| e.to_string())?;
            write!(out, "{text} [[{target_id}]]").map_err(|e| e.to_string())?;

            if let Some(n) = note {
                writeln!(out).map_err(|e| e.to_string())?;
                write!(out, "[[{n}]]").map_err(|e| e.to_string())?;
            }

            writeln!(out).map_err(|e| e.to_string())?;
            new_line_number = line_num + 2;
        } else {
            writeln!(out, "{line}").map_err(|e| e.to_string())?;
        }
    }

    // Replace original with temp
    fs::rename(&temp_path, &structure_path)
        .map_err(|e| format!("Failed to replace structure.fountain: {e}"))?;

    println!(
        "Annotated [{}] at L{}: \"{}\"",
        target_id, new_line_number, text
    );

    Ok(())
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
        // Original heading preserved
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
