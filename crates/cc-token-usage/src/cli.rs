use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    Text,
    Html,
    Json,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum GroupBy {
    Day,
    Month,
}

#[derive(Parser, Debug)]
#[command(
    name = "cc-token-usage",
    version,
    about = "Analyze Claude Code session token usage, costs, and efficiency"
)]
pub struct Cli {
    /// Output format (default: both text + html)
    #[arg(long, value_enum, global = true)]
    pub format: Option<OutputFormat>,

    /// Write output to a file instead of stdout
    #[arg(long, global = true)]
    pub output: Option<PathBuf>,

    /// Path to Claude home directory (default: ~/.claude)
    #[arg(long, global = true)]
    pub claude_home: Option<PathBuf>,

    /// Monthly subscription price in USD for cost calculations
    #[arg(long, global = true)]
    pub subscription_price: Option<f64>,

    /// Path to config file
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Show overall usage overview across all projects
    Overview,

    /// Analyze token usage for a specific project
    Project {
        /// Project name to analyze
        #[arg(long)]
        name: Option<String>,

        /// Show top N sessions by token usage
        #[arg(long, default_value_t = 10)]
        top: usize,
    },

    /// Analyze a specific session
    Session {
        /// Session ID to analyze
        id: Option<String>,

        /// Analyze the latest session instead of specifying an ID
        #[arg(long)]
        latest: bool,
    },

    /// Validate token counting accuracy against raw JSONL data
    Validate {
        /// Validate a specific session ID (optional, validates all if omitted)
        id: Option<String>,

        /// Show only failed checks
        #[arg(long)]
        failures_only: bool,
    },

    /// Check for updates and self-update the binary
    Update {
        /// Only check for a new version, don't download
        #[arg(long)]
        check: bool,
    },

    /// Show usage trends over time
    Trend {
        /// Number of days to include (0 = all history)
        #[arg(long, default_value_t = 0)]
        days: u32,

        /// Group by: day or month
        #[arg(long, value_enum, default_value_t = GroupBy::Day)]
        group_by: GroupBy,
    },

    /// Generate a "Wrapped" annual summary of your Claude Code usage
    Wrapped {
        /// Year to summarize (default: current year)
        #[arg(long)]
        year: Option<i32>,
    },
}
