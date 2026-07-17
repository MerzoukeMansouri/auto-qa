use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cua", about = "Drive a real Chrome browser via Playwright MCP")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run `claude -p` wired to Playwright MCP, driving a real browser.
    Run {
        #[arg(long)]
        query: String,
    },
    /// Generate a Playwright .spec.ts from the latest `cua run` session.
    Codegen {
        #[arg(long, default_value = "playwright-tests/cua-generated.spec.ts")]
        out: String,
    },
    /// Open the local review UI for the latest `cua run` session.
    Review {
        #[arg(long, default_value_t = 4321)]
        port: u16,
    },
}
