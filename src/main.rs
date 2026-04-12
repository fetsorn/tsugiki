mod commands;
mod resolve;
mod scan;
mod types;

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
    /// Display a node with full context
    Show {
        /// Address: line number, short hex id, or full UUID
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Intent directory is the current working directory
    let intent_dir = PathBuf::from(".");

    let result = match cli.command {
        Command::Show {
            ref addr,
            source,
            structure,
            target,
            limit,
            depth,
        } => {
            let tree_filter = if source {
                Some(types::TreeKind::Source)
            } else if structure {
                Some(types::TreeKind::Structure)
            } else if target {
                Some(types::TreeKind::Target)
            } else {
                None
            };
            commands::show::run(&intent_dir, addr, tree_filter.as_ref(), limit, depth).await
        }
        Command::Annotate {
            ref addr,
            ref text,
            ref note,
            overwrite,
        } => match (text, addr) {
            (Some(t), Some(a)) => {
                let res = commands::annotate::run(&intent_dir, a, t, note.as_deref(), overwrite).await;
                if res.is_ok() {
                    // Show next unannotated node
                    let _ = commands::next::run(&intent_dir).await;
                }
                res
            }
            (Some(t), None) => {
                // No addr — find next unannotated node and annotate it
                match commands::next::find_next(&intent_dir).await {
                    Ok(Some(next_addr)) => {
                        let res = commands::annotate::run(&intent_dir, &next_addr, t, note.as_deref(), overwrite).await;
                        if res.is_ok() {
                            // Show next unannotated node after this one
                            let _ = commands::next::run(&intent_dir).await;
                        }
                        res
                    }
                    Ok(None) => Err("No unannotated nodes remaining.".into()),
                    Err(e) => Err(e),
                }
            }
            (None, _) => commands::next::run(&intent_dir).await,
        },
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
