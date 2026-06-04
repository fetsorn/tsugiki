use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Which tree to render.
#[derive(Debug, Clone, Copy)]
pub enum RenderTree {
    Source,
    Structure,
    Target,
}

impl RenderTree {
    pub fn child_tablet(&self) -> &'static str {
        match self {
            RenderTree::Source => "source-child.csv",
            RenderTree::Structure => "structure-child.csv",
            RenderTree::Target => "target-child.csv",
        }
    }

    pub fn fountain_filename(&self) -> &'static str {
        match self {
            RenderTree::Source => "source.fountain",
            RenderTree::Structure => "structure.fountain",
            RenderTree::Target => "target.fountain",
        }
    }

    pub fn md_filename(&self) -> &'static str {
        match self {
            RenderTree::Source => "source.md",
            RenderTree::Structure => "structure.md",
            RenderTree::Target => "target.md",
        }
    }
}

/// A loaded tree ready for rendering.
struct Tree {
    /// parent → [children] in order
    children: HashMap<String, Vec<String>>,
    /// All UUIDs that appear as children
    child_set: std::collections::HashSet<String>,
    /// UUID → prose text
    prose: HashMap<String, String>,
}

impl Tree {
    fn root(&self) -> Option<&str> {
        // Root is the parent that never appears as a child
        for parent in self.children.keys() {
            if !self.child_set.contains(parent) {
                return Some(parent);
            }
        }
        None
    }

    fn is_leaf(&self, uuid: &str) -> bool {
        match self.children.get(uuid) {
            Some(kids) => kids.is_empty(),
            None => true,
        }
    }
}

/// Load a tree from a child tablet and prose store.
fn load_tree(csvs_dir: &Path, tablet_name: &str) -> Result<Tree, String> {
    let tablet_path = csvs_dir.join(tablet_name);
    if !tablet_path.exists() {
        return Err(format!("{tablet_name} not found"));
    }

    let content = fs::read_to_string(&tablet_path)
        .map_err(|e| format!("Failed to read {tablet_name}: {e}"))?;

    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut child_set = std::collections::HashSet::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, ',');
        let parent = parts.next().unwrap_or("").to_string();
        let child = parts.next().unwrap_or("").to_string();
        if parent.is_empty() || child.is_empty() {
            continue;
        }
        children.entry(parent.clone()).or_default().push(child.clone());
        child_set.insert(child);
        // Ensure parent exists in map even if it has no entry yet
        children.entry(parent).or_default();
    }

    // Load prose blobs
    let prose_dir = csvs_dir.join("prose");
    let mut prose = HashMap::new();
    if prose_dir.exists() {
        for uuid in children.keys().chain(child_set.iter()) {
            let blob_path = prose_dir.join(uuid);
            if blob_path.exists() {
                if let Ok(text) = fs::read_to_string(&blob_path) {
                    if !text.is_empty() {
                        prose.insert(uuid.clone(), text);
                    }
                }
            }
        }
    }

    Ok(Tree { children, child_set, prose })
}

/// Render a tree to fountain format.
pub fn run(
    intent_dir: &Path,
    tree: RenderTree,
    markdown_only: bool,
) -> Result<(), String> {
    let csvs_dir = intent_dir.join("csvs");
    let prose_dir = intent_dir.join("prose");

    let loaded = load_tree(&csvs_dir, tree.child_tablet())?;

    let root = loaded.root()
        .ok_or("No root found — every node appears as a child")?
        .to_string();

    let mut fountain = String::new();
    let mut md = String::new();

    render_node(&loaded, &root, 0, &tree, &mut fountain, &mut md);

    fs::create_dir_all(&prose_dir)
        .map_err(|e| format!("Failed to create prose/: {e}"))?;

    if !markdown_only {
        let fountain_path = prose_dir.join(tree.fountain_filename());
        fs::write(&fountain_path, &fountain)
            .map_err(|e| format!("Failed to write {}: {e}", fountain_path.display()))?;
        println!("wrote {}", fountain_path.display());
    }

    let md_path = prose_dir.join(tree.md_filename());
    fs::write(&md_path, &md)
        .map_err(|e| format!("Failed to write {}: {e}", md_path.display()))?;
    println!("wrote {}", md_path.display());

    Ok(())
}

/// Recursively render a node and its children.
fn render_node(
    tree: &Tree,
    uuid: &str,
    depth: usize,
    render_tree: &RenderTree,
    fountain: &mut String,
    md: &mut String,
) {
    match render_tree {
        RenderTree::Structure => {
            render_structure_node(tree, uuid, depth, fountain, md);
        }
        _ => {
            render_content_node(tree, uuid, depth, fountain, md);
        }
    }
}

/// Render a source or target node.
fn render_content_node(
    tree: &Tree,
    uuid: &str,
    depth: usize,
    fountain: &mut String,
    md: &mut String,
) {
    let short = short_id(uuid);
    let prose = tree.prose.get(uuid).map(|s| s.as_str()).unwrap_or("");
    let is_leaf = tree.is_leaf(uuid);

    if is_leaf {
        // Action block — leaf prose with UUID
        if !prose.is_empty() {
            fountain.push_str(&format!("{prose} [[{short}]]\n\n"));
        }
    } else {
        // Heading for this inner node
        let hashes = "#".repeat(depth + 1);
        if !prose.is_empty() {
            fountain.push_str(&format!("{hashes} {prose} [[{short}]]\n\n"));
            if depth > 0 {
                md.push_str(&format!("{hashes} {prose}\n\n"));
            }
        } else {
            fountain.push_str(&format!("{hashes} [[{short}]]\n\n"));
        }

        // Render children
        if let Some(children) = tree.children.get(uuid) {
            // Leaves under a container (multi-sentence paragraph) group into
            // one md paragraph. Leaves directly under root are each their own
            // md paragraph — they were separate paragraphs in the source.
            let group_leaves = depth > 0;
            let mut leaf_run = String::new();

            for child in children {
                if tree.is_leaf(child) {
                    let child_prose = tree.prose.get(child.as_str()).map(|s| s.as_str()).unwrap_or("");
                    let child_short = short_id(child);

                    // Fountain: each leaf is its own action block
                    if !child_prose.is_empty() {
                        fountain.push_str(&format!("{child_prose} [[{child_short}]]\n\n"));

                        if group_leaves {
                            leaf_run.push_str(child_prose);
                            leaf_run.push(' ');
                        } else {
                            md.push_str(child_prose);
                            md.push_str("\n\n");
                        }
                    }
                } else {
                    // Flush accumulated leaf text as a md paragraph
                    if !leaf_run.is_empty() {
                        md.push_str(leaf_run.trim_end());
                        md.push_str("\n\n");
                        leaf_run.clear();
                    }
                    // Recurse into inner child
                    render_content_node(tree, child, depth + 1, fountain, md);
                }
            }

            // Flush remaining leaf run
            if !leaf_run.is_empty() {
                md.push_str(leaf_run.trim_end());
                md.push_str("\n\n");
            }
        }
    }
}

/// Render a structure node.
fn render_structure_node(
    tree: &Tree,
    uuid: &str,
    depth: usize,
    fountain: &mut String,
    md: &mut String,
) {
    let short = short_id(uuid);
    let prose = tree.prose.get(uuid).map(|s| s.as_str()).unwrap_or("");
    let is_leaf = tree.is_leaf(uuid);

    // Parse prose blob: annotation text + [[note]] lines
    let (annotation, notes) = parse_structure_prose(prose);

    if is_leaf {
        // Action block with annotation
        if !annotation.is_empty() {
            fountain.push_str(&format!("{annotation} [[{short}]]\n"));
        } else {
            fountain.push_str(&format!("[[{short}]]\n"));
        }
        for note in &notes {
            fountain.push_str(&format!("{note}\n"));
        }
        fountain.push('\n');
    } else {
        // Heading
        let hashes = "#".repeat(depth + 1);
        if !annotation.is_empty() {
            fountain.push_str(&format!("{hashes} {annotation} [[{short}]]\n"));
        } else {
            fountain.push_str(&format!("{hashes} [[{short}]]\n"));
        }
        for note in &notes {
            fountain.push_str(&format!("{note}\n"));
        }
        fountain.push('\n');

        if !annotation.is_empty() && depth > 0 {
            md.push_str(&format!("{hashes} {annotation}\n\n"));
        }

        // Render children
        if let Some(children) = tree.children.get(uuid) {
            let mut leaf_run = String::new();

            for child in children {
                if tree.is_leaf(child) {
                    let child_prose = tree.prose.get(child.as_str()).map(|s| s.as_str()).unwrap_or("");
                    let (ann, _) = parse_structure_prose(child_prose);

                    // Fountain: render child directly
                    render_structure_node(tree, child, depth + 1, fountain, md);

                    // Md: accumulate annotations
                    if !ann.is_empty() {
                        leaf_run.push_str(&ann);
                        leaf_run.push(' ');
                    }
                } else {
                    if !leaf_run.is_empty() {
                        md.push_str(leaf_run.trim_end());
                        md.push_str("\n\n");
                        leaf_run.clear();
                    }
                    render_structure_node(tree, child, depth + 1, fountain, md);
                }
            }

            if !leaf_run.is_empty() {
                md.push_str(leaf_run.trim_end());
                md.push_str("\n\n");
            }
        }
    }
}

/// Parse structure prose blob into annotation text and note lines.
fn parse_structure_prose(prose: &str) -> (String, Vec<String>) {
    if prose.is_empty() {
        return (String::new(), Vec::new());
    }

    let mut annotation_lines = Vec::new();
    let mut notes = Vec::new();

    for line in prose.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
            notes.push(trimmed.to_string());
        } else {
            annotation_lines.push(line.to_string());
        }
    }

    let annotation = annotation_lines.join("\n").trim().to_string();
    (annotation, notes)
}

/// First 8 hex chars of a UUID.
fn short_id(uuid: &str) -> String {
    uuid.split('-').next().unwrap_or(uuid).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_tree(edges: &[(& str, &str)], prose_map: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        let csvs_dir = dir.path().join("csvs");
        let prose_dir = csvs_dir.join("prose");
        fs::create_dir_all(&prose_dir).unwrap();

        let edge_str: String = edges.iter()
            .map(|(p, c)| format!("{p},{c}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(csvs_dir.join("source-child.csv"), format!("{edge_str}\n")).unwrap();

        for (uuid, text) in prose_map {
            fs::write(prose_dir.join(uuid), text).unwrap();
        }

        fs::create_dir_all(dir.path().join("prose")).unwrap();
        dir
    }

    #[test]
    fn render_flat_tree() {
        let dir = setup_tree(
            &[("root-uuid", "leaf-1"), ("root-uuid", "leaf-2")],
            &[("leaf-1", "First sentence."), ("leaf-2", "Second sentence.")],
        );

        run(dir.path(), RenderTree::Source, false).unwrap();

        let fountain = fs::read_to_string(dir.path().join("prose/source.fountain")).unwrap();
        assert!(fountain.contains("# [[root]]"));
        assert!(fountain.contains("First sentence. [[leaf]]"));
        assert!(fountain.contains("Second sentence. [[leaf]]"));
    }
}
