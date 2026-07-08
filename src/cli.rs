use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cua", about = "Drive a real Chrome browser via CDP")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Launch Chrome, optionally navigating to a URL.
    Open {
        url: Option<String>,
    },
    /// Kill the Chrome session and clean up state.
    Close,
    /// Screenshot the current page without acting.
    Screenshot,
    Click {
        x: f64,
        y: f64,
    },
    DoubleClick {
        x: f64,
        y: f64,
    },
    TripleClick {
        x: f64,
        y: f64,
    },
    RightClick {
        x: f64,
        y: f64,
    },
    MiddleClick {
        x: f64,
        y: f64,
    },
    MouseDown {
        x: f64,
        y: f64,
    },
    MouseUp {
        x: f64,
        y: f64,
    },
    Hover {
        x: f64,
        y: f64,
    },
    Drag {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    /// Type text into the focused element.
    Type {
        text: String,
        #[arg(long)]
        enter: bool,
    },
    /// Press a key or `+`-joined combo, e.g. "Enter", "control+a".
    Key {
        combo: String,
    },
    KeyDown {
        key: String,
    },
    KeyUp {
        key: String,
    },
    Scroll {
        x: f64,
        y: f64,
        direction: String,
        #[arg(default_value_t = 800.0)]
        magnitude: f64,
    },
    Navigate {
        url: String,
    },
    Back,
    Forward,
    Wait {
        seconds: u64,
    },
    /// Ensure a browser session, run `claude -p` on it, then tear down.
    Run {
        #[arg(long)]
        query: String,
    },
}
