/// A node identifier — always the first 8 hex chars of a v4 UUID in Fountain files,
/// full UUID in CSVS tablets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId {
    pub full: String,  // full UUID e.g. "cd18caf7-c9c0-4d14-9896-4b31cb7fe2ae"
    pub short: String, // first 8 hex chars e.g. "cd18caf7"
}

impl NodeId {
    pub fn from_full(uuid: &str) -> Self {
        let short = uuid.split('-').next().unwrap_or(uuid).to_string();
        Self {
            full: uuid.to_string(),
            short,
        }
    }

    pub fn from_short(hex: &str) -> Self {
        Self {
            full: hex.to_string(),
            short: hex.to_string(),
        }
    }

    pub fn matches(&self, query: &str) -> bool {
        self.short == query || self.full == query
    }
}

/// Address: how the user refers to a node on the command line.
#[derive(Debug, Clone)]
pub enum Addr {
    Line(usize),
    Hex(String),
}

impl Addr {
    pub fn parse(s: &str) -> Self {
        match s.parse::<usize>() {
            Ok(n) => Addr::Line(n),
            Err(_) => Addr::Hex(s.to_string()),
        }
    }
}

/// Which tree a Fountain file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    Source,
    Structure,
    Target,
}

impl TreeKind {
    pub fn fountain_filename(&self) -> &'static str {
        match self {
            TreeKind::Source => "source.fountain",
            TreeKind::Structure => "structure.fountain",
            TreeKind::Target => "target.fountain",
        }
    }
}

/// A node as scanned from a Fountain file — not a full AST node,
/// just what we found at one position.
#[derive(Debug, Clone)]
pub struct ScannedNode {
    pub line_number: usize,
    pub id: NodeId,
    pub depth: Option<u8>,  // None for action blocks, Some(n) for headings
    pub text: String,       // heading title or action block text (without the [[id]])
    pub notes: Vec<String>, // any [[note]] lines that follow
}
