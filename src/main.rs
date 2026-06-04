mod commands;
mod store;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tsugiki", about = "Translation witness CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a markdown source file into source and structure trees
    Init {
        /// Path to the source markdown file
        source: PathBuf,

        /// Keep paragraphs as leaves (don't split into sentences)
        #[arg(long)]
        no_split: bool,
    },

    /// Display a node with full context
    Show {
        /// Address: short hex id or full UUID
        addr: String,

        /// Show only source tree
        #[arg(long)]
        source: bool,

        /// Show only structure tree
        #[arg(long)]
        structure: bool,

        /// Show only target tree
        #[arg(long)]
        target: bool,

        /// Max children to display (default 5, 0 for all)
        #[arg(long, default_value = "5")]
        limit: usize,

        /// Depth of children to show (default 1 = direct children, 0 = node only, 2+ = deeper)
        #[arg(long, default_value = "1")]
        depth: usize,
    },

    /// Render a tree to fountain and/or markdown
    Render {
        /// Which tree: source, structure, or target
        tree: String,

        /// Only produce markdown, skip fountain
        #[arg(long)]
        md_only: bool,
    },

    /// Show the next node that needs attention
    Next,

    /// Write a target sentence
    Write {
        /// The sentence text
        text: String,

        /// Parent node address (default: last used parent or root)
        #[arg(long)]
        parent: Option<String>,

        /// Create the target root node
        #[arg(long)]
        root: bool,
    },

    /// Link a target node to structure (provenance) or reparent it
    Link {
        /// Target node address
        addr: String,

        /// Structure node to map to
        #[arg(long)]
        structure: Option<String>,

        /// New parent node address
        #[arg(long)]
        parent: Option<String>,
    },

    /// Write annotation text to a structure node, or show next unannotated node
    Annotate {
        /// The annotation text (omit to show next unannotated node)
        text: Option<String>,

        /// Address of the structure node to annotate (omit to use next unannotated)
        #[arg(long)]
        addr: Option<String>,

        /// Optional translator note
        #[arg(long)]
        note: Option<String>,

        /// Overwrite existing annotation
        #[arg(long)]
        overwrite: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    // Intent directory is the current working directory
    let intent_dir = PathBuf::from(".");

    let result = match cli.command {
        Command::Init { ref source, no_split } => {
            commands::init::run(&intent_dir, source, no_split)
        }
        Command::Render { ref tree, md_only } => {
            let render_tree = match tree.as_str() {
                "source" => commands::render::RenderTree::Source,
                "structure" => commands::render::RenderTree::Structure,
                "target" => commands::render::RenderTree::Target,
                _ => return eprintln!("error: tree must be source, structure, or target"),
            };
            commands::render::run(&intent_dir, render_tree, md_only)
        }
        Command::Show {
            ref addr,
            source,
            structure,
            target,
            limit,
            depth,
        } => {
            let tree_filter = if source {
                Some("source")
            } else if structure {
                Some("structure")
            } else if target {
                Some("target")
            } else {
                None
            };
            commands::show::run(&intent_dir, addr, tree_filter, limit, depth)
        }
        Command::Write {
            ref text,
            ref parent,
            root,
        } => {
            commands::write::run(&intent_dir, text, parent.as_deref(), root)
        }
        Command::Link {
            ref addr,
            ref structure,
            ref parent,
        } => {
            commands::link::run(&intent_dir, addr, structure.as_deref(), parent.as_deref())
        }
        Command::Next => {
            commands::next::run(&intent_dir)
        }
        Command::Annotate {
            ref addr,
            ref text,
            ref note,
            overwrite,
        } => match (text, addr) {
            (Some(t), Some(a)) => {
                let res = commands::annotate::run(&intent_dir, a, t, note.as_deref(), overwrite);
                if res.is_ok() {
                    let _ = commands::next::run(&intent_dir);
                }
                res
            }
            (Some(t), None) => {
                match commands::next::find_next_unannotated(&intent_dir) {
                    Ok(Some(next_addr)) => {
                        let res = commands::annotate::run(&intent_dir, &next_addr, t, note.as_deref(), overwrite);
                        if res.is_ok() {
                            let _ = commands::next::run(&intent_dir);
                        }
                        res
                    }
                    Ok(None) => Err("No unannotated nodes remaining.".into()),
                    Err(e) => Err(e),
                }
            }
            (None, _) => commands::next::run(&intent_dir),
        },
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
