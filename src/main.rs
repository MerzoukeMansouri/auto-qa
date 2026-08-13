mod action_entry;
mod agent;
mod block;
mod cli;
mod commands;
mod doctor;
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

/// Resolution order: explicit --model flag, then the model saved for this
/// harness in ~/.autoqa/config.json. `None` if neither is set — callers
/// pass that straight through to `Harness::build_run_command`/
/// `build_chat_command`, which fall back to the harness's own
/// `default_model()`. Unlike `resolve_harness`, this never prompts: a model
/// picker only ever runs via `autoqa config`.
fn resolve_model(harness: Harness, flag: Option<String>) -> Option<String> {
    flag.or_else(|| state::read_model_config(harness))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            query,
            harness,
            model,
            locale,
            recheck,
            no_verification,
        } => {
            let h = resolve_harness(harness)?;
            if !no_verification {
                doctor::ensure(h, recheck)?;
            }
            let model = resolve_model(h, model);
            agent::cmd_run(h, &query, &locale, model.as_deref()).await
        }
        Commands::Codegen { out } => commands::cmd_codegen(&out).await,
        Commands::Review {
            port,
            harness,
            model,
            recheck,
            no_verification,
        } => {
            let h = resolve_harness(harness)?;
            if !no_verification {
                doctor::ensure(h, recheck)?;
            }
            let model = resolve_model(h, model);
            review_server::serve(port, h, model).await
        }
        Commands::Config { harness, model } => {
            let harness_explicit = harness.is_some();
            let h = match harness {
                Some(h) => h,
                None => tui::pick_harness(state::read_harness_config())?,
            };
            state::write_harness_config(h)?;

            match model {
                Some(m) => {
                    state::write_model_config(h, &m)?;
                    println!("harness set to {h}, model set to {m}");
                }
                // Fully-interactive invocation (`autoqa config`, no flags at
                // all): chain straight into the model picker too. An
                // explicit --harness with no --model leaves the saved model
                // untouched — only the harness changed.
                None if !harness_explicit => {
                    let m = tui::pick_model(h, state::read_model_config(h))?;
                    state::write_model_config(h, &m)?;
                    println!("harness set to {h}, model set to {m}");
                }
                None => {
                    println!("harness set to {h}");
                }
            }
            Ok(())
        }
        Commands::Doctor { harness } => {
            let h = resolve_harness(harness)?;
            // Always shows the checklist screen, cache hit or not — unlike
            // `run`/`review`'s fast path, the whole point of running this
            // command explicitly is to see it.
            doctor::ensure(h, true)?;
            println!("all checks passed for harness '{h}'");
            Ok(())
        }
    }
}
