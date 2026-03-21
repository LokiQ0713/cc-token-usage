use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;

use cc_token_usage::analysis::overview::analyze_overview;
use cc_token_usage::analysis::project::analyze_projects;
use cc_token_usage::analysis::session::analyze_session;
use cc_token_usage::analysis::trend::analyze_trend;
use cc_token_usage::cli::{Cli, Command, GroupBy, OutputFormat};
use cc_token_usage::config::Config;
use cc_token_usage::data::loader;
use cc_token_usage::output::html::{render_full_report_html, render_session_html};
use cc_token_usage::output::text::{render_overview, render_projects, render_session, render_trend};
use cc_token_usage::pricing::calculator::PricingCalculator;

fn main() -> Result<()> {
    // 1. Parse CLI arguments
    let cli = Cli::parse();

    // 2. Determine claude_home
    let claude_home = match cli.claude_home {
        Some(ref path) => path.clone(),
        None => dirs::home_dir()
            .context("could not determine home directory")?
            .join(".claude"),
    };

    // 3. Load configuration
    let config = if let Some(ref config_path) = cli.config {
        Config::load(config_path)
            .with_context(|| format!("failed to load config from {}", config_path.display()))?
    } else {
        let default_config_path = dirs::home_dir()
            .context("could not determine home directory")?
            .join(".config/cc-token-analyzer/config.toml");
        if default_config_path.exists() {
            Config::load(&default_config_path).unwrap_or_default()
        } else {
            Config::default()
        }
    };

    // 4. Initialize PricingCalculator
    let calc = PricingCalculator::new().with_overrides(config.to_model_prices());

    // 5. Determine subscription price: CLI arg takes priority, otherwise last config period
    let subscription_price = cli.subscription_price.or_else(|| {
        config
            .subscription
            .last()
            .map(|p| p.monthly_price_usd)
    });

    // 6. Load data
    let (sessions, quality) = loader::load_all(&claude_home)
        .with_context(|| format!("failed to load data from {}", claude_home.display()))?;

    // 7. Execute analysis + render output
    let command = cli.command.unwrap_or(Command::Overview);
    match command {
        // ── Overview (default when no subcommand) ────────────────────────
        Command::Overview => {
            let overview = analyze_overview(&sessions, quality.clone(), &calc, subscription_price);

            match cli.format {
                OutputFormat::Text => {
                    println!("{}", render_overview(&overview, &calc));
                }
                OutputFormat::Html => {
                    let projects = analyze_projects(&sessions, &calc, 20);
                    let trend = analyze_trend(&sessions, &calc, 0, false);
                    let html = render_full_report_html(&overview, &projects, &trend, &calc);

                    let output_path = cli
                        .output
                        .unwrap_or_else(|| PathBuf::from("/tmp/cc-token-report.html"));
                    std::fs::write(&output_path, &html)
                        .with_context(|| format!("failed to write {}", output_path.display()))?;
                    println!("Report written to {}", output_path.display());
                }
            }
        }

        // ── Project ─────────────────────────────────────────────────────────
        Command::Project { name: _, top } => {
            let projects = analyze_projects(&sessions, &calc, top);

            match cli.format {
                OutputFormat::Text => {
                    println!("{}", render_projects(&projects));
                }
                OutputFormat::Html => {
                    let overview =
                        analyze_overview(&sessions, quality.clone(), &calc, subscription_price);
                    let trend = analyze_trend(&sessions, &calc, 0, false);
                    let html = render_full_report_html(&overview, &projects, &trend, &calc);
                    let output_path = cli
                        .output
                        .unwrap_or_else(|| PathBuf::from("/tmp/cc-token-report.html"));
                    std::fs::write(&output_path, &html)
                        .with_context(|| format!("failed to write {}", output_path.display()))?;
                    println!("Report written to {}", output_path.display());
                }
            }
        }

        // ── Session ─────────────────────────────────────────────────────────
        Command::Session { id, latest } => {
            let session = if latest {
                // Find the session with the most recent last_timestamp
                sessions
                    .iter()
                    .filter(|s| s.last_timestamp.is_some())
                    .max_by_key(|s| s.last_timestamp)
                    .context("no sessions found with timestamps")?
            } else if let Some(ref prefix) = id {
                // Prefix match on session ID
                let matches: Vec<_> = sessions
                    .iter()
                    .filter(|s| s.session_id.starts_with(prefix))
                    .collect();
                match matches.len() {
                    0 => bail!("no session found matching prefix '{}'", prefix),
                    1 => matches[0],
                    n => bail!(
                        "ambiguous prefix '{}': {} sessions match. Provide a longer prefix.",
                        prefix,
                        n
                    ),
                }
            } else {
                bail!("specify a session ID or use --latest");
            };

            let result = analyze_session(session, &calc);

            match cli.format {
                OutputFormat::Text => {
                    println!("{}", render_session(&result));
                }
                OutputFormat::Html => {
                    let html = render_session_html(&result);
                    let output_path = cli
                        .output
                        .unwrap_or_else(|| PathBuf::from("/tmp/cc-session-report.html"));
                    std::fs::write(&output_path, &html)
                        .with_context(|| format!("failed to write {}", output_path.display()))?;
                    println!("Report written to {}", output_path.display());
                }
            }
        }

        // ── Trend ───────────────────────────────────────────────────────────
        Command::Trend { days, group_by } => {
            let group_by_month = matches!(group_by, GroupBy::Month);
            let trend = analyze_trend(&sessions, &calc, days, group_by_month);

            match cli.format {
                OutputFormat::Text => {
                    println!("{}", render_trend(&trend));
                }
                OutputFormat::Html => {
                    let overview =
                        analyze_overview(&sessions, quality.clone(), &calc, subscription_price);
                    let projects = analyze_projects(&sessions, &calc, 20);
                    let html = render_full_report_html(&overview, &projects, &trend, &calc);
                    let output_path = cli
                        .output
                        .unwrap_or_else(|| PathBuf::from("/tmp/cc-token-report.html"));
                    std::fs::write(&output_path, &html)
                        .with_context(|| format!("failed to write {}", output_path.display()))?;
                    println!("Report written to {}", output_path.display());
                }
            }
        }
    }

    Ok(())
}
