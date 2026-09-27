mod agent;
mod block;
mod cli;
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
            headless,
            no_tui,
        } => {
            let h = resolve_harness(harness)?;
            if !no_verification {
                doctor::ensure(h, recheck)?;
            }
            let model = resolve_model(h, model);
            agent::cmd_run(h, &query, &locale, model.as_deref(), headless, no_tui).await
        }
        // Pure file transform, no browser session needed — reads actions.json
        // (importing the latest `autoqa run` MCP session into it first, unless
        // it was already hand-edited more recently via `autoqa review`),
        // writes a Playwright .spec.ts. Kept separate from `autoqa review`
        // for scripting/CI use.
        Commands::Codegen { out } => {
            state::sync_actions_from_latest_mcp_session()?;
            let title = state::latest_query()
                .unwrap_or_else(|| "generated from autoqa session".to_string());
            let ts = playwright_codegen::generate(&state::read_actions(), &title)?;
            if let Some(parent) = std::path::Path::new(&out).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out, ts)?;
            println!("wrote {out}");
            Ok(())
        }
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
