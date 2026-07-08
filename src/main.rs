mod agent;
mod browser;
mod cdp_actions;
mod cli;
mod commands;
mod output;
mod state;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Open { url } => commands::cmd_open(url.as_deref()).await,
        Commands::Close => commands::cmd_close().await,
        Commands::Run { query } => agent::cmd_run(&query).await,
        other => commands::cmd_action(other).await,
    }
}
