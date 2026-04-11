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
    /// Show the next node that needs attention
    Next,

    /// Display a node with full context
    Show {
        /// Address: line number, short hex id, or full UUID
        addr: String,
    },

    /// Write annotation text to a structure node
    Annotate {
        /// Address of the structure node to annotate
        addr: String,

        /// The annotation text
        text: String,

        /// Optional translator note
        #[arg(long)]
        note: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Intent directory is the current working directory
    let intent_dir = PathBuf::from(".");

    let result = match cli.command {
        Command::Next => commands::next::run(&intent_dir).await,
        Command::Show { ref addr } => commands::show::run(&intent_dir, addr).await,
        Command::Annotate {
            ref addr,
            ref text,
            ref note,
        } => {
            commands::annotate::run(&intent_dir, addr, text, note.as_deref()).await
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
