mod action_entry;
mod agent;
mod block;
mod cli;
mod commands;
mod harness;
mod playwright_codegen;
mod review_server;
mod state;
mod tui;

use clap::Parser;
use cli::{Cli, Commands};
use harness::Harness;

/// Resolution order: explicit --harness flag, then the harness saved from a
/// prior first-run prompt, then prompt-and-persist one now.
fn resolve_harness(flag: Option<Harness>) -> anyhow::Result<Harness> {
    if let Some(h) = flag {
        return Ok(h);
    }
    if let Some(h) = state::read_harness_config() {
        return Ok(h);
    }
    let h = tui::pick_harness(None)?;
    state::write_harness_config(h)?;
    Ok(h)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            query,
            harness,
            locale,
        } => agent::cmd_run(resolve_harness(harness)?, &query, &locale).await,
        Commands::Codegen { out } => commands::cmd_codegen(&out).await,
        Commands::Review { port, harness } => {
            review_server::serve(port, resolve_harness(harness)?).await
        }
        Commands::Config { harness } => {
            let h = match harness {
                Some(h) => h,
                None => tui::pick_harness(state::read_harness_config())?,
            };
            state::write_harness_config(h)?;
            println!("harness set to {h}");
            Ok(())
        }
    }
}
