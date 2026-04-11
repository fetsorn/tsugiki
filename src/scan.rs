use regex::Regex;
use std::fs;
use std::path::Path;

use crate::types::{NodeId, ScannedNode};

/// Regex for a line containing a hex UUID note: `[[hex-chars]]`
/// Captures the hex id. Matches both short (8-char) and full UUIDs.
fn id_regex() -> Regex {
    Regex::new(r"\[\[([0-9a-f]{8}(?:-[0-9a-f]{4}){0,3}(?:-[0-9a-f]{12})?)\]\]").unwrap()
}

/// Regex for a standalone note line: `[[text]]` where text is NOT a hex UUID.
fn note_regex() -> Regex {
    Regex::new(r"^\[\[([^\]]+)\]\]$").unwrap()
}

/// Extract the hex id from a line, if present.
fn extract_id(line: &str) -> Option<String> {
    id_regex().captures(line).map(|c| c[1].to_string())
}

/// Strip the `[[id]]` suffix from a line, returning the text content.
fn strip_id(line: &str) -> String {
    id_regex().replace(line, "").trim().to_string()
}

/// Count leading `#` characters to determine heading depth.
fn heading_depth(line: &str) -> Option<u8> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        let count = trimmed.chars().take_while(|&c| c == '#').count();
        Some(count as u8)
    } else {
        None
    }
}

/// Strip leading `#` and whitespace from a heading line.
fn strip_heading_markers(line: &str) -> String {
    let trimmed = line.trim_start();
    trimmed.trim_start_matches('#').trim().to_string()
}

/// Scan a Fountain file and yield all nodes found.
pub fn scan_all(path: &Path) -> Vec<ScannedNode> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let lines: Vec<&str> = content.lines().collect();
    let mut nodes = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Skip blank lines
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Check if this line has a node ID
        if let Some(id_str) = extract_id(line) {
            // Check if this is a standalone note (no non-note content)
            if note_regex().is_match(line.trim()) {
                // This is a translator note, not a node — skip
                i += 1;
                continue;
            }

            let depth = heading_depth(line);
            let text = if depth.is_some() {
                strip_id(&strip_heading_markers(line))
            } else {
                strip_id(line)
            };

            // Collect any following note lines
            let mut notes = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let next = lines[j].trim();
                if next.is_empty() {
                    break;
                }
                if note_regex().is_match(next) {
                    let cap = note_regex().captures(next).unwrap();
                    notes.push(cap[1].to_string());
                }
                j += 1;
            }

            nodes.push(ScannedNode {
                line_number: i + 1, // 1-indexed
                id: NodeId::from_short(&id_str),
                depth,
                text,
                notes,
            });

            i = j;
        } else {
            i += 1;
        }
    }

    nodes
}

/// Find a node by short hex id.
pub fn find_by_hex<'a>(nodes: &'a [ScannedNode], hex: &str) -> Option<&'a ScannedNode> {
    nodes.iter().find(|n| n.id.matches(hex))
}

/// Find a node by line number.
pub fn find_by_line(nodes: &[ScannedNode], line: usize) -> Option<&ScannedNode> {
    nodes.iter().find(|n| n.line_number == line)
}

/// Find the first structure node with empty text (next to annotate).
pub fn first_empty(nodes: &[ScannedNode]) -> Option<&ScannedNode> {
    nodes.iter().find(|n| n.text.is_empty())
}

/// Find the parent of a node: the nearest preceding heading with smaller depth.
pub fn find_parent<'a>(nodes: &'a [ScannedNode], node: &ScannedNode) -> Option<&'a ScannedNode> {
    let idx = nodes.iter().position(|n| n.line_number == node.line_number)?;
    let node_depth = node.depth.unwrap_or(u8::MAX);

    nodes[..idx].iter().rev().find(|n| {
        if let Some(d) = n.depth {
            d < node_depth
        } else {
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn scan_str(content: &str) -> Vec<ScannedNode> {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        scan_all(f.path())
    }

    #[test]
    fn heading_with_id() {
        let nodes = scan_str("# The title [[abcd1234]]\n");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].depth, Some(1));
        assert_eq!(nodes[0].text, "The title");
        assert_eq!(nodes[0].id.short, "abcd1234");
    }

    #[test]
    fn action_block_with_id() {
        let nodes = scan_str("Some prose text here. [[abcd1234]]\n");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].depth, None);
        assert_eq!(nodes[0].text, "Some prose text here.");
    }

    #[test]
    fn empty_heading() {
        let nodes = scan_str("### [[abcd1234]]\n");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].depth, Some(3));
        assert_eq!(nodes[0].text, "");
    }

    #[test]
    fn standalone_note_skipped() {
        let nodes = scan_str("[[this is a translator note]]\n");
        assert_eq!(nodes.len(), 0);
    }

    #[test]
    fn node_with_trailing_notes() {
        let content = "# Heading [[abcd1234]]\n[[a note about this]]\n\n";
        let nodes = scan_str(content);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].notes, vec!["a note about this"]);
    }

    #[test]
    fn finds_first_empty() {
        let content = "### annotated heading [[aaaa1111]]\n\nsome text [[bbbb2222]]\n\n### [[cccc3333]]\n";
        let nodes = scan_str(content);
        let empty = first_empty(&nodes).unwrap();
        assert_eq!(empty.id.short, "cccc3333");
    }

    #[test]
    fn finds_parent() {
        let content = "# Top [[aaaa1111]]\n\n## Mid [[bbbb2222]]\n\n### Leaf [[cccc3333]]\n";
        let nodes = scan_str(content);
        let leaf = &nodes[2];
        let parent = find_parent(&nodes, leaf).unwrap();
        assert_eq!(parent.id.short, "bbbb2222");
    }
}
