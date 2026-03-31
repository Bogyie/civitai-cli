use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the interactive terminal UI (default)
    Ui,
    /// Download a model by ID or Hash
    Download {
        /// The model ID to download (downloads latest version)
        #[arg(short, long)]
        id: Option<u64>,
        /// The model version hash to download
        #[arg(long)]
        hash: Option<String>,
    },
    /// Configure settings
    Config {
        #[arg(long)]
        api_key: Option<String>,
        #[arg(long)]
        comfyui_path: Option<String>,
    },
}
