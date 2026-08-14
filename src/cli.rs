use crate::harness::Harness;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "autoqa",
    about = "Drive a real Chrome browser via Playwright MCP"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run the selected harness wired to Playwright MCP, driving a real browser.
    Run {
        #[arg(long)]
        query: String,
        /// Which harness to drive. Without this flag: uses the harness saved
        /// from a prior first-run prompt (~/.autoqa/config.json), or prompts
        /// and persists one if none is saved yet.
        #[arg(long, value_enum)]
        harness: Option<Harness>,
        /// Model to use for this run, overriding whatever's saved for this
        /// harness in ~/.autoqa/config.json (set via `autoqa config`).
        /// Without this flag: saved model, or the harness's own default.
        #[arg(long)]
        model: Option<String>,
        /// BCP 47 locale for the MCP browser context. Without pinning this,
        /// MCP and any later `playwright test` run can default to different
        /// locales, producing mismatched form input / date formats between
        /// the recorded session and the generated test.
        #[arg(long, default_value = "en-US")]
        locale: String,
        /// Force a fresh environment check instead of trusting the cached
        /// result from a prior run (~/.autoqa/doctor.json).
        #[arg(long)]
        recheck: bool,
        /// Skip the environment check entirely (Node/Chrome/harness) — for
        /// when you already know it's set up and just want to go. Overrides
        /// --recheck if both are somehow given.
        #[arg(long)]
        no_verification: bool,
        /// Run Chrome headless (`--headless=new`), with `--no-sandbox` added
        /// automatically — for CI runners with no display.
        #[arg(long)]
        headless: bool,
        /// Stream plain log lines to stdout instead of the ratatui live
        /// pane, and skip the pre-run block picker — for CI runners with no
        /// controlling terminal (the ratatui pane needs one and fails with
        /// ENXIO otherwise).
        #[arg(long)]
        no_tui: bool,
    },
    /// Generate a Playwright .spec.ts from the latest `autoqa run` session.
    Codegen {
        #[arg(long, default_value = "playwright-tests/autoqa-generated.spec.ts")]
        out: String,
    },
    /// Open the local review UI for the latest `autoqa run` session.
    Review {
        #[arg(long, default_value_t = 4321)]
        port: u16,
        /// See `run`'s --harness: same resolution (flag > saved config > prompt).
        #[arg(long, value_enum)]
        harness: Option<Harness>,
        /// See `run`'s --model.
        #[arg(long)]
        model: Option<String>,
        /// See `run`'s --recheck.
        #[arg(long)]
        recheck: bool,
        /// See `run`'s --no-verification.
        #[arg(long)]
        no_verification: bool,
    },
    /// View or change the default harness/model saved in
    /// ~/.autoqa/config.json.
    Config {
        /// Set directly, no picker. Omit to open the ratatui picker instead.
        #[arg(long, value_enum)]
        harness: Option<Harness>,
        /// Set the model for the resolved harness directly, no picker. Omit
        /// with --harness also omitted to chain into the model picker right
        /// after the harness picker; omit with --harness given to leave the
        /// saved model untouched.
        #[arg(long)]
        model: Option<String>,
    },
    /// Run the environment check on its own, without starting a run/review.
    /// Always shows the checklist screen, ignoring any cached result.
    Doctor {
        /// See `run`'s --harness: same resolution (flag > saved config > prompt).
        #[arg(long, value_enum)]
        harness: Option<Harness>,
    },
}
