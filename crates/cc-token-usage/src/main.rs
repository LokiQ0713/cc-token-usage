use anyhow::{bail, Context, Result};
use chrono::Datelike;
use clap::Parser;

use cc_token_usage::analysis::overview::analyze_overview;
use cc_token_usage::analysis::project::analyze_projects;
use cc_token_usage::analysis::session::analyze_session;
use cc_token_usage::analysis::trend::analyze_trend;
use cc_token_usage::analysis::validate;
use cc_token_usage::analysis::wrapped::analyze_wrapped;
use cc_token_usage::cli::{Cli, Command, GroupBy, OutputFormat};
use cc_token_usage::config::Config;
use cc_token_usage::data::loader;
use cc_token_usage::data::models::SessionData;
use cc_token_usage::output::html::render_session_html;
use cc_token_usage::output::html_new::render_vue_dashboard;
use cc_token_usage::output::json::{render_html_payload, render_overview_json, render_projects_json, render_session_json, render_trend_json, render_wrapped_json};
use cc_token_usage::output::text::{render_overview, render_projects, render_session, render_trend, render_validation, render_wrapped};
use cc_token_usage::pricing::calculator::PricingCalculator;

fn main() -> Result<()> {
    // 1. Parse CLI arguments
    let cli = Cli::parse();

    // 1.5. Handle `update` early — no data loading needed
    let command = cli.command.unwrap_or(Command::Overview);
    if let Command::Update { check } = &command {
        if *check {
            let status = cc_token_usage::update::check_for_update()?;
            eprintln!("Current version: v{}", status.current_version);
            eprintln!("Latest version:  v{}", status.latest_version);
            if status.update_available {
                eprintln!("\nUpdate available! Run `cc-token-usage update` to upgrade.");
            } else {
                eprintln!("\nAlready up to date.");
            }
        } else {
            cc_token_usage::update::perform_update()?;
        }
        return Ok(());
    }

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

    // 7. Determine output modes: None → both text + html; Json is exclusive
    let want_json = matches!(cli.format, Some(OutputFormat::Json));
    let want_text = !want_json && (cli.format.is_none() || matches!(cli.format, Some(OutputFormat::Text)));
    let want_html = !want_json && (cli.format.is_none() || matches!(cli.format, Some(OutputFormat::Html)));

    // 8. Execute analysis + render output
    match command {
        // ── Overview (default when no subcommand) ────────────────────────
        Command::Overview => {
            let overview = analyze_overview(&sessions, quality.clone(), &calc, subscription_price);

            if want_json {
                println!("{}", render_overview_json(&overview));
            }
            if want_text {
                println!("{}", render_overview(&overview, &calc));
            }
            if want_html {
                let projects = analyze_projects(&sessions, &calc, 20);
                let trend = analyze_trend(&sessions, &calc, 0, false);
                let year = chrono::Utc::now().year();
                let wrapped = analyze_wrapped(&sessions, &calc, year);
                let json_payload = render_html_payload(&overview, &projects, &trend, &sessions, &calc, Some(&wrapped));
                let html = render_vue_dashboard(&json_payload);
                write_html(&html, cli.output.as_deref(), "cc-token-report.html")?;
            }
        }

        // ── Project ─────────────────────────────────────────────────────────
        Command::Project { name, top } => {
            let filtered: Vec<SessionData>;
            let target_sessions = if let Some(ref filter) = name {
                filtered = sessions.iter().filter(|s| s.project.as_ref().is_some_and(|p| p.contains(filter.as_str()))).cloned().collect();
                &filtered
            } else {
                &sessions
            };
            let projects = analyze_projects(target_sessions, &calc, top);

            if want_json {
                println!("{}", render_projects_json(&projects));
            }
            if want_text {
                println!("{}", render_projects(&projects));
            }
            if want_html {
                let overview =
                    analyze_overview(target_sessions, quality.clone(), &calc, subscription_price);
                let trend = analyze_trend(target_sessions, &calc, 0, false);
                let year = chrono::Utc::now().year();
                let wrapped = analyze_wrapped(target_sessions, &calc, year);
                let json_payload = render_html_payload(&overview, &projects, &trend, target_sessions, &calc, Some(&wrapped));
                let html = render_vue_dashboard(&json_payload);
                write_html(&html, cli.output.as_deref(), "cc-token-report.html")?;
            }
        }

        // ── Session ─────────────────────────────────────────────────────────
        Command::Session { id, latest } => {
            let session = if latest {
                sessions
                    .iter()
                    .filter(|s| s.last_timestamp.is_some())
                    .max_by_key(|s| s.last_timestamp)
                    .context("no sessions found with timestamps")?
            } else if let Some(ref prefix) = id {
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

            let raw_meta = cc_token_usage::data::scanner::load_agent_meta(&session.session_id, &claude_home);
            let agent_meta: std::collections::HashMap<String, cc_token_usage::analysis::session::AgentMeta> = raw_meta.into_iter()
                .map(|(k, (t, d))| (k, cc_token_usage::analysis::session::AgentMeta { agent_type: t, description: d }))
                .collect();

            let result = analyze_session(session, &calc, &agent_meta);

            if want_json {
                println!("{}", render_session_json(&result));
            }
            if want_text {
                println!("{}", render_session(&result));
            }
            if want_html {
                let html = render_session_html(&result);
                write_html(&html, cli.output.as_deref(), "cc-session-report.html")?;
            }
        }

        // ── Validate (text only, no HTML view) ──────────────────────────────
        Command::Validate { id, failures_only } => {
            let target_sessions: Vec<&SessionData> = if let Some(ref prefix) = id {
                let matches: Vec<_> = sessions
                    .iter()
                    .filter(|s| s.session_id.starts_with(prefix))
                    .collect();
                if matches.is_empty() {
                    bail!("no session found matching prefix '{}'", prefix);
                }
                matches
            } else {
                sessions.iter().collect()
            };

            let report = validate::validate_all(&target_sessions, &quality, &claude_home, &calc)
                .context("validation failed")?;
            if want_json {
                eprintln!("Note: JSON output is not yet supported for validate. Showing text instead.");
            }
            println!("{}", render_validation(&report, failures_only));
        }

        // ── Wrapped ──────────────────────────────────────────────────────────
        Command::Wrapped { year } => {
            let year = year.unwrap_or_else(|| chrono::Utc::now().year());
            let result = analyze_wrapped(&sessions, &calc, year);

            if want_json {
                println!("{}", render_wrapped_json(&result));
            }
            if want_text {
                println!("{}", render_wrapped(&result));
            }
            if want_html && !want_text {
                eprintln!("Note: HTML output is not yet supported for wrapped. Showing text instead.");
                println!("{}", render_wrapped(&result));
            }
        }

        // ── Update (already handled above, unreachable) ─────────────────────
        Command::Update { .. } => unreachable!(),

        // ── Trend ───────────────────────────────────────────────────────────
        Command::Trend { days, group_by } => {
            let group_by_month = matches!(group_by, GroupBy::Month);
            let trend = analyze_trend(&sessions, &calc, days, group_by_month);

            if want_json {
                println!("{}", render_trend_json(&trend));
            }
            if want_text {
                println!("{}", render_trend(&trend));
            }
            if want_html {
                let overview =
                    analyze_overview(&sessions, quality.clone(), &calc, subscription_price);
                let projects = analyze_projects(&sessions, &calc, 20);
                let year = chrono::Utc::now().year();
                let wrapped = analyze_wrapped(&sessions, &calc, year);
                let json_payload = render_html_payload(&overview, &projects, &trend, &sessions, &calc, Some(&wrapped));
                let html = render_vue_dashboard(&json_payload);
                write_html(&html, cli.output.as_deref(), "cc-token-report.html")?;
            }
        }
    }

    Ok(())
}

/// Write HTML to file and print the path for the user to click.
fn write_html(html: &str, output: Option<&std::path::Path>, default_name: &str) -> Result<()> {
    let path = match output {
        Some(p) => p.to_path_buf(),
        None => std::env::temp_dir().join(default_name),
    };
    std::fs::write(&path, html)
        .with_context(|| format!("failed to write {}", path.display()))?;
    println!("\nHTML report: {}", path.display());
    Ok(())
}
