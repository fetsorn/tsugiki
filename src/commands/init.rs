use std::collections::HashMap;
use std::fs;
use std::path::Path;

use uuid::Uuid;

/// A node produced by parsing.
struct ParsedNode {
    uuid: Uuid,
    depth: u8,
    text: String,
    children: Vec<usize>,      // indices into the nodes vec
    footnote_refs: Vec<String>, // footnote labels referenced by this node's text
}

/// Namespace UUID for tsugiki deterministic IDs.
const TSUGIKI_NS: Uuid = Uuid::from_bytes([
    0x74, 0x73, 0x75, 0x67, 0x69, 0x6b, 0x69, 0x2d,
    0x6e, 0x73, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
]); // "tsugiki-ns\0\0\0\0\0\0"

/// Accumulated parsing state.
struct ParseState {
    nodes: Vec<ParsedNode>,
    /// footnote label → node index (for footnote definitions)
    footnote_defs: HashMap<String, usize>,
    /// text → occurrence count (for disambiguating duplicate content)
    occurrence: HashMap<String, usize>,
}

/// Generate a deterministic source UUID from text content + occurrence index.
fn source_uuid(text: &str, occurrence: usize) -> Uuid {
    let name = format!("source:{text}:{occurrence}");
    Uuid::new_v5(&TSUGIKI_NS, name.as_bytes())
}

/// Generate a deterministic structure UUID from its corresponding source UUID.
fn structure_uuid(source: &Uuid) -> Uuid {
    let name = format!("structure:{source}");
    Uuid::new_v5(&TSUGIKI_NS, name.as_bytes())
}

/// Track occurrence and return the current count for this text.
fn next_occurrence(state: &mut ParseState, text: &str) -> usize {
    let count = state.occurrence.entry(text.to_string()).or_insert(0);
    let n = *count;
    *count += 1;
    n
}

/// Parse a markdown file into source and structure trees, write CSVS tablets and prose blobs.
pub fn run(intent_dir: &Path, source_md: &Path, no_split: bool) -> Result<(), String> {
    let csvs_dir = intent_dir.join("csvs");
    let prose_dir = csvs_dir.join("prose");

    if csvs_dir.exists() {
        // Check if tablets already exist
        if csvs_dir.join("source-child.csv").exists() {
            return Err("Intent already initialized. Remove csvs/ to reinitialize.".into());
        }
    }

    let md_text = fs::read_to_string(source_md)
        .map_err(|e| format!("Failed to read {}: {e}", source_md.display()))?;

    let mdast = markdown::to_mdast(&md_text, &markdown::ParseOptions::gfm())
        .map_err(|e| format!("Markdown parse error: {e}"))?;

    let mut state = ParseState {
        nodes: Vec::new(),
        footnote_defs: HashMap::new(),
        occurrence: HashMap::new(),
    };

    // Root node at depth 0
    let root_occ = next_occurrence(&mut state, "");
    let root_uuid = source_uuid("", root_occ);
    state.nodes.push(ParsedNode {
        uuid: root_uuid,
        depth: 0,
        text: String::new(),
        children: Vec::new(),
        footnote_refs: Vec::new(),
    });

    // Walk the mdast and build source nodes
    if let markdown::mdast::Node::Root(root) = &mdast {
        parse_children(&root.children, 0, 1, &mut state, no_split);
    }

    let nodes = &state.nodes;

    // Build containment edges
    let mut source_child: Vec<(Uuid, Uuid)> = Vec::new();
    collect_edges(nodes, 0, &mut source_child);

    // Build source-footnote edges: source node → footnote definition node
    let mut source_footnote: Vec<(Uuid, Uuid)> = Vec::new();
    for node in nodes {
        for label in &node.footnote_refs {
            if let Some(&fn_idx) = state.footnote_defs.get(label) {
                source_footnote.push((node.uuid, nodes[fn_idx].uuid));
            }
        }
    }

    // Build 1:1 structure scaffold (exclude footnote definitions — they don't get structure nodes)
    let mut source_to_structure: HashMap<Uuid, Uuid> = HashMap::new();
    let mut structure_nodes: Vec<ParsedNode> = Vec::new();
    let footnote_uuids: std::collections::HashSet<Uuid> = state.footnote_defs.values()
        .map(|&idx| nodes[idx].uuid)
        .collect();

    build_structure_scaffold(nodes, 0, &mut structure_nodes, &mut source_to_structure, &footnote_uuids);

    let mut structure_child: Vec<(Uuid, Uuid)> = Vec::new();
    collect_edges(&structure_nodes, 0, &mut structure_child);

    // Write everything
    fs::create_dir_all(&prose_dir)
        .map_err(|e| format!("Failed to create {}: {e}", prose_dir.display()))?;

    // .csvs.csv
    let dataset_uuid = Uuid::new_v4();
    fs::write(
        csvs_dir.join(".csvs.csv"),
        format!("csvs,0.0.4\nuuid,{dataset_uuid}\n"),
    )
    .map_err(|e| format!("Failed to write .csvs.csv: {e}"))?;

    // _-_.csv
    let mut schema = "source,child\nstructure,child\ntarget,child\nsource,structure\nstructure,target\n".to_string();
    if !source_footnote.is_empty() {
        schema.push_str("source,footnote\n");
    }
    fs::write(csvs_dir.join("_-_.csv"), &schema)
        .map_err(|e| format!("Failed to write _-_.csv: {e}"))?;

    // source-child.csv
    write_tablet(&csvs_dir.join("source-child.csv"), &source_child)?;

    // structure-child.csv
    write_tablet(&csvs_dir.join("structure-child.csv"), &structure_child)?;

    // source-structure.csv
    let bridge: Vec<(Uuid, Uuid)> = source_to_structure.iter().map(|(s, t)| (*s, *t)).collect();
    write_tablet(&csvs_dir.join("source-structure.csv"), &bridge)?;

    // source-footnote.csv (if any)
    if !source_footnote.is_empty() {
        write_tablet(&csvs_dir.join("source-footnote.csv"), &source_footnote)?;
    }

    // Prose blobs for source nodes
    let mut source_count = 0;
    for node in nodes {
        if !node.text.is_empty() {
            fs::write(prose_dir.join(node.uuid.to_string()), &node.text)
                .map_err(|e| format!("Failed to write prose blob: {e}"))?;
        }
        source_count += 1;
    }

    // No prose blobs for structure nodes — they start empty (awaiting annotation)

    // Summary
    let leaf_count = nodes.iter().filter(|n| n.children.is_empty()).count();
    println!("initialized: {source_count} source nodes ({leaf_count} leaves), {} structure nodes",
        structure_nodes.len());

    // Depth breakdown
    let mut depth_counts: HashMap<u8, usize> = HashMap::new();
    for node in nodes {
        *depth_counts.entry(node.depth).or_insert(0) += 1;
    }
    let mut depths: Vec<u8> = depth_counts.keys().copied().collect();
    depths.sort();
    for d in depths {
        println!("  depth {d}: {}", depth_counts[&d]);
    }

    Ok(())
}

/// Recursively parse mdast children into source nodes.
fn parse_children(
    children: &[markdown::mdast::Node],
    parent_idx: usize,
    depth: u8,
    state: &mut ParseState,
    no_split: bool,
) {
    use markdown::mdast::Node;
    use unicode_segmentation::UnicodeSegmentation;

    for child in children {
        match child {
            Node::Heading(heading) => {
                let text = extract_text(&heading.children);
                let heading_depth = heading.depth as u8;
                let actual_depth = heading_depth.max(depth);

                let occ = next_occurrence(state, &text);
                let idx = state.nodes.len();
                state.nodes.push(ParsedNode {
                    uuid: source_uuid(&text, occ),
                    depth: actual_depth,
                    text,
                    children: Vec::new(),
                    footnote_refs: Vec::new(),
                });
                state.nodes[parent_idx].children.push(idx);
            }
            Node::FootnoteDefinition(fndef) => {
                // Footnote body — always a single leaf, never sentence-split
                let text = fndef.children.iter()
                    .map(|c| extract_text_from_node(c))
                    .collect::<Vec<_>>()
                    .join("\n");
                let text = normalize_whitespace(&text);

                let occ = next_occurrence(state, &text);
                let idx = state.nodes.len();
                state.nodes.push(ParsedNode {
                    uuid: source_uuid(&text, occ),
                    depth,
                    text,
                    children: Vec::new(),
                    footnote_refs: Vec::new(),
                });
                state.nodes[parent_idx].children.push(idx);
                state.footnote_defs.insert(fndef.identifier.clone(), idx);
            }
            Node::Paragraph(paragraph) => {
                let raw_text = extract_text(&paragraph.children);
                let text = normalize_whitespace(&raw_text);
                if text.is_empty() {
                    continue;
                }

                // Collect footnote references from this paragraph
                let fn_refs = collect_footnote_refs(&paragraph.children);

                if no_split {
                    let occ = next_occurrence(state, &text);
                    let idx = state.nodes.len();
                    state.nodes.push(ParsedNode {
                        uuid: source_uuid(&text, occ),
                        depth,
                        text,
                        children: Vec::new(),
                        footnote_refs: fn_refs,
                    });
                    state.nodes[parent_idx].children.push(idx);
                } else {
                    let sentences: Vec<&str> = text.unicode_sentences().collect();

                    if sentences.len() <= 1 {
                        let occ = next_occurrence(state, &text);
                        let idx = state.nodes.len();
                        state.nodes.push(ParsedNode {
                            uuid: source_uuid(&text, occ),
                            depth,
                            text,
                            children: Vec::new(),
                            footnote_refs: fn_refs,
                        });
                        state.nodes[parent_idx].children.push(idx);
                    } else {
                        // Multi-sentence paragraph → container gets UUID after children
                        // so we can derive it from their UUIDs
                        let para_idx = state.nodes.len();
                        // placeholder — UUID filled in after children are created
                        state.nodes.push(ParsedNode {
                            uuid: Uuid::nil(),
                            depth,
                            text: String::new(),
                            children: Vec::new(),
                            footnote_refs: Vec::new(),
                        });
                        state.nodes[parent_idx].children.push(para_idx);

                        let mut child_uuids = Vec::new();
                        for sentence in &sentences {
                            let s = sentence.trim().to_string();
                            if s.is_empty() {
                                continue;
                            }
                            let occ = next_occurrence(state, &s);
                            let uuid = source_uuid(&s, occ);
                            child_uuids.push(uuid);
                            let idx = state.nodes.len();
                            let sentence_refs: Vec<String> = fn_refs.iter()
                                .filter(|r| s.contains(&format!("[^{}]", r)))
                                .cloned()
                                .collect();
                            state.nodes.push(ParsedNode {
                                uuid,
                                depth: depth + 1,
                                text: s,
                                children: Vec::new(),
                                footnote_refs: sentence_refs,
                            });
                            state.nodes[para_idx].children.push(idx);
                        }
                        // Container UUID derived from children
                        let children_key: String = child_uuids.iter()
                            .map(|u| u.to_string())
                            .collect::<Vec<_>>()
                            .join("+");
                        let container_name = format!("container:{children_key}");
                        state.nodes[para_idx].uuid = Uuid::new_v5(&TSUGIKI_NS, container_name.as_bytes());
                    }
                }
            }
            Node::List(list) => {
                // Each list item becomes a leaf node (never sentence-split)
                for item_node in &list.children {
                    if let Node::ListItem(item) = item_node {
                        let text = item.children.iter()
                            .map(|c| extract_text_from_node(c))
                            .collect::<Vec<_>>()
                            .join(" ");
                        let text = normalize_whitespace(&text);
                        if text.is_empty() {
                            continue;
                        }
                        let fn_refs = item.children.iter()
                            .flat_map(|c| collect_footnote_refs_from_node(c))
                            .collect();
                        let occ = next_occurrence(state, &text);
                        let idx = state.nodes.len();
                        state.nodes.push(ParsedNode {
                            uuid: source_uuid(&text, occ),
                            depth,
                            text,
                            children: Vec::new(),
                            footnote_refs: fn_refs,
                        });
                        state.nodes[parent_idx].children.push(idx);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extract plain text from mdast inline nodes.
fn extract_text(children: &[markdown::mdast::Node]) -> String {
    use markdown::mdast::Node;

    let mut text = String::new();
    for child in children {
        match child {
            Node::Text(t) => text.push_str(&t.value),
            Node::Strong(s) => text.push_str(&extract_text(&s.children)),
            Node::Emphasis(e) => text.push_str(&extract_text(&e.children)),
            Node::InlineCode(c) => text.push_str(&c.value),
            Node::Link(l) => text.push_str(&extract_text(&l.children)),
            Node::FootnoteReference(r) => {
                text.push_str(&format!("[^{}]", r.identifier));
            }
            Node::Break(_) => text.push('\n'),
            _ => {}
        }
    }
    text
}

/// Collapse runs of whitespace (newlines, tabs, spaces) into single spaces.
fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Recursively collect parent→child edges.
fn collect_edges(nodes: &[ParsedNode], idx: usize, edges: &mut Vec<(Uuid, Uuid)>) {
    for &child_idx in &nodes[idx].children {
        edges.push((nodes[idx].uuid, nodes[child_idx].uuid));
        collect_edges(nodes, child_idx, edges);
    }
}

/// Extract plain text from any mdast node (for footnote definitions).
fn extract_text_from_node(node: &markdown::mdast::Node) -> String {
    use markdown::mdast::Node;
    match node {
        Node::Text(t) => t.value.clone(),
        Node::Paragraph(p) => extract_text(&p.children),
        Node::Strong(s) => extract_text(&s.children),
        Node::Emphasis(e) => extract_text(&e.children),
        Node::InlineCode(c) => c.value.clone(),
        Node::Link(l) => extract_text(&l.children),
        Node::Break(_) => "\n".to_string(),
        _ => String::new(),
    }
}

/// Collect footnote reference labels from inline nodes.
fn collect_footnote_refs(children: &[markdown::mdast::Node]) -> Vec<String> {
    use markdown::mdast::Node;
    let mut refs = Vec::new();
    for child in children {
        match child {
            Node::FootnoteReference(r) => {
                refs.push(r.identifier.clone());
            }
            Node::Strong(s) => refs.extend(collect_footnote_refs(&s.children)),
            Node::Emphasis(e) => refs.extend(collect_footnote_refs(&e.children)),
            Node::Link(l) => refs.extend(collect_footnote_refs(&l.children)),
            _ => {}
        }
    }
    refs
}

/// Collect footnote reference labels from any mdast node (for list items).
fn collect_footnote_refs_from_node(node: &markdown::mdast::Node) -> Vec<String> {
    use markdown::mdast::Node;
    match node {
        Node::Paragraph(p) => collect_footnote_refs(&p.children),
        Node::Strong(s) => collect_footnote_refs(&s.children),
        Node::Emphasis(e) => collect_footnote_refs(&e.children),
        Node::Link(l) => collect_footnote_refs(&l.children),
        Node::FootnoteReference(r) => vec![r.identifier.clone()],
        _ => Vec::new(),
    }
}

/// Build a 1:1 structure scaffold mirroring the source tree.
/// Footnote definition nodes (in `skip_uuids`) are excluded.
fn build_structure_scaffold(
    source_nodes: &[ParsedNode],
    source_idx: usize,
    structure_nodes: &mut Vec<ParsedNode>,
    source_to_structure: &mut HashMap<Uuid, Uuid>,
    skip_uuids: &std::collections::HashSet<Uuid>,
) {
    let source = &source_nodes[source_idx];

    // Skip footnote definitions — they don't get structure nodes
    if skip_uuids.contains(&source.uuid) {
        return;
    }

    let struct_uuid = structure_uuid(&source.uuid);

    let struct_idx = structure_nodes.len();
    structure_nodes.push(ParsedNode {
        uuid: struct_uuid,
        depth: source.depth,
        text: String::new(),
        children: Vec::new(),
        footnote_refs: Vec::new(),
    });

    source_to_structure.insert(source.uuid, struct_uuid);

    for &child_source_idx in &source.children {
        if skip_uuids.contains(&source_nodes[child_source_idx].uuid) {
            continue;
        }
        let child_struct_idx = structure_nodes.len();
        build_structure_scaffold(source_nodes, child_source_idx, structure_nodes, source_to_structure, skip_uuids);
        structure_nodes[struct_idx].children.push(child_struct_idx);
    }
}

/// Write a list of UUID pairs as a CSV tablet.
fn write_tablet(path: &Path, edges: &[(Uuid, Uuid)]) -> Result<(), String> {
    let content: String = edges
        .iter()
        .map(|(a, b)| format!("{a},{b}"))
        .collect::<Vec<_>>()
        .join("\n");
    let content = if content.is_empty() {
        String::new()
    } else {
        format!("{content}\n")
    };
    fs::write(path, content).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn setup_md(content: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let md_path = dir.path().join("source.md");
        fs::write(&md_path, content).unwrap();
        (dir, md_path)
    }

    #[test]
    fn init_simple_paragraphs() {
        let (dir, md_path) = setup_md("Hello world.\n\nSecond paragraph.\n");
        run(dir.path(), &md_path, true).unwrap();

        let csvs_dir = dir.path().join("csvs");
        assert!(csvs_dir.join(".csvs.csv").exists());
        assert!(csvs_dir.join("_-_.csv").exists());
        assert!(csvs_dir.join("source-child.csv").exists());
        assert!(csvs_dir.join("structure-child.csv").exists());
        assert!(csvs_dir.join("source-structure.csv").exists());

        // Two leaves under root = 2 source-child edges
        let sc = fs::read_to_string(csvs_dir.join("source-child.csv")).unwrap();
        assert_eq!(sc.lines().count(), 2);

        // Prose blobs exist for leaves
        let prose_dir = csvs_dir.join("prose");
        let blobs: Vec<_> = fs::read_dir(&prose_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        // root (no text) + 2 paragraphs with text = 2 blobs
        assert_eq!(blobs.len(), 2);
    }

    #[test]
    fn init_sentence_split() {
        let (dir, md_path) = setup_md("First sentence. Second sentence.\n");
        run(dir.path(), &md_path, false).unwrap();

        let csvs_dir = dir.path().join("csvs");
        let sc = fs::read_to_string(csvs_dir.join("source-child.csv")).unwrap();
        // root → paragraph → 2 sentences = 3 edges
        assert_eq!(sc.lines().count(), 3);
    }

    #[test]
    fn init_footnotes_not_split() {
        let md = "Some text with a reference[^1]. Another sentence here.\n\n[^1]: This is footnote one. It has multiple sentences. They should not be split.\n";
        let (dir, md_path) = setup_md(md);
        run(dir.path(), &md_path, false).unwrap();

        let csvs_dir = dir.path().join("csvs");

        // source-footnote.csv should exist
        assert!(csvs_dir.join("source-footnote.csv").exists());
        let sf = fs::read_to_string(csvs_dir.join("source-footnote.csv")).unwrap();
        assert_eq!(sf.lines().count(), 1, "one footnote edge");

        // The footnote prose blob should contain all sentences unsplit
        let prose_dir = csvs_dir.join("prose");
        let blobs: Vec<_> = fs::read_dir(&prose_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        // Check that one blob contains the footnote text (unsplit)
        let has_footnote_blob = blobs.iter().any(|b| {
            let content = fs::read_to_string(b.path()).unwrap_or_default();
            content.contains("This is footnote one. It has multiple sentences. They should not be split.")
        });
        assert!(has_footnote_blob, "footnote body should be a single unsplit blob");

        // structure-child.csv should NOT have edges for the footnote node
        // (footnotes are excluded from structure scaffold)
        let sc = fs::read_to_string(csvs_dir.join("source-child.csv")).unwrap();
        let struct_c = fs::read_to_string(csvs_dir.join("structure-child.csv")).unwrap();
        // source tree has more edges than structure tree (footnote adds source edges but not structure)
        assert!(sc.lines().count() > struct_c.lines().count(),
            "source tree should have more edges than structure tree due to footnote");
    }

    #[test]
    fn init_refuses_reinit() {
        let (dir, md_path) = setup_md("Hello.\n");
        run(dir.path(), &md_path, true).unwrap();
        let result = run(dir.path(), &md_path, true);
        assert!(result.is_err());
    }
}
