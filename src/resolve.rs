use crate::types::{Addr, ScannedNode};
use crate::scan;

/// Resolve an address to a node within a list of scanned nodes.
pub fn resolve<'a>(nodes: &'a [ScannedNode], addr: &Addr) -> Option<&'a ScannedNode> {
    match addr {
        Addr::Line(n) => scan::find_by_line(nodes, *n),
        Addr::Hex(h) => scan::find_by_hex(nodes, h),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NodeId;

    fn make_node(line: usize, hex: &str, text: &str) -> ScannedNode {
        ScannedNode {
            line_number: line,
            id: NodeId::from_short(hex),
            depth: Some(1),
            text: text.to_string(),
            notes: vec![],
        }
    }

    #[test]
    fn resolve_by_line() {
        let nodes = vec![make_node(5, "aabb1122", "hello")];
        let found = resolve(&nodes, &Addr::Line(5));
        assert!(found.is_some());
        assert_eq!(found.unwrap().text, "hello");
    }

    #[test]
    fn resolve_by_hex() {
        let nodes = vec![make_node(5, "aabb1122", "hello")];
        let found = resolve(&nodes, &Addr::Hex("aabb1122".into()));
        assert!(found.is_some());
    }

    #[test]
    fn resolve_miss() {
        let nodes = vec![make_node(5, "aabb1122", "hello")];
        assert!(resolve(&nodes, &Addr::Line(99)).is_none());
        assert!(resolve(&nodes, &Addr::Hex("deadbeef".into())).is_none());
    }
}
