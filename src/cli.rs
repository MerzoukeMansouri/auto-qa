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
        /// BCP 47 locale for the MCP browser context. Without pinning this,
        /// MCP and any later `playwright test` run can default to different
        /// locales, producing mismatched form input / date formats between
        /// the recorded session and the generated test.
        #[arg(long, default_value = "en-US")]
        locale: String,
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
    },
}
