//! Snapshot tests for the terminal text renderer (`src/output/text.rs`).
//!
//! Why snapshots: `text.rs` is 1430+ lines of formatting code with only two
//! trivial unit tests for `format_number` / `format_cost`. Column ordering,
//! alignment widths, header bars, totals lines — none of that has any
//! regression guard. A snapshot per subcommand locks the entire rendered
//! output, so any drift is surfaced as a reviewable diff.
//!
//! Fixture design: three sessions across two projects, two models, two
//! months in 2025. Timestamps are far in the past so they fall within any
//! `analyze_trend(days=0)` window without dependence on "today".
//!
//! When a snapshot drifts intentionally:
//!   cargo insta review
//! When unsure:
//!   INSTA_UPDATE=no cargo test --test text_snapshots
//!   (then read the .snap.new files manually)

use cc_token_usage::analysis::{
    overview::analyze_overview, project::analyze_projects, trend::analyze_trend,
};
use cc_token_usage::data::models::{
    DataQuality, GlobalDataQuality, SessionData, SessionMetadata, TokenUsage, ValidatedTurn,
};
use cc_token_usage::output::text::{render_overview, render_projects, render_trend};
use cc_token_usage::pricing::calculator::PricingCalculator;
use chrono::{DateTime, Utc};

// ─── Fixture helpers ────────────────────────────────────────────────────────

fn turn(uuid: &str, ts: &str, model: &str, input: u64, output: u64) -> ValidatedTurn {
    ValidatedTurn {
        uuid: uuid.into(),
        parent_uuid: None,
        request_id: Some(format!("req-{uuid}")),
        timestamp: ts.parse::<DateTime<Utc>>().unwrap(),
        model: model.into(),
        usage: TokenUsage {
            input_tokens: Some(input),
            output_tokens: Some(output),
            cache_creation_input_tokens: Some(0),
            cache_read_input_tokens: Some(0),
            cache_creation: None,
            server_tool_use: None,
            service_tier: None,
            speed: None,
            inference_geo: None,
        },
        stop_reason: Some("end_turn".into()),
        content_types: vec!["text".into()],
        is_agent: false,
        agent_id: None,
        user_text: None,
        assistant_text: None,
        tool_names: vec![],
        service_tier: None,
        speed: None,
        inference_geo: None,
        tool_error_count: 0,
        git_branch: Some("main".into()),
        attribution_plugin: None,
        attribution_skill: None,
    }
}

fn session(id: &str, project: &str, turns: Vec<ValidatedTurn>) -> SessionData {
    let first = turns.iter().map(|t| t.timestamp).min();
    let last = turns.iter().map(|t| t.timestamp).max();
    SessionData {
        source_path: std::path::PathBuf::from("/tmp/test.jsonl"),
        session_id: id.into(),
        project: Some(project.into()),
        turns,
        user_entries: vec![],
        subagents: vec![],
        plugins: vec![],
        skills: vec![],
        hooks: vec![],
        first_timestamp: first,
        last_timestamp: last,
        version: Some("2.1.140".into()),
        quality: DataQuality::default(),
        metadata: SessionMetadata::default(),
        is_orphan: false,
    }
}

/// Build a 3-session fixture spanning 2 projects, 2 models, 2 months.
/// Timestamps are pinned to 2025 so snapshots are stable across "today".
fn fixture_sessions() -> Vec<SessionData> {
    let alpha = session(
        "11111111-1111-1111-1111-111111111111",
        "-Users-dev-alpha",
        vec![
            turn("a1", "2025-03-10T10:00:00Z", "claude-opus-4-6", 1_000, 500),
            turn("a2", "2025-03-10T10:05:00Z", "claude-opus-4-6", 2_000, 800),
            turn("a3", "2025-03-12T14:00:00Z", "claude-opus-4-6", 500, 200),
        ],
    );
    let beta = session(
        "22222222-2222-2222-2222-222222222222",
        "-Users-dev-beta",
        vec![
            turn("b1", "2025-03-15T09:00:00Z", "claude-sonnet-4-5", 3_000, 1_500),
            turn("b2", "2025-03-15T09:30:00Z", "claude-sonnet-4-5", 1_000, 400),
        ],
    );
    let gamma = session(
        "33333333-3333-3333-3333-333333333333",
        "-Users-dev-alpha",
        vec![turn(
            "g1",
            "2025-04-02T11:00:00Z",
            "claude-opus-4-6",
            800,
            300,
        )],
    );
    vec![alpha, beta, gamma]
}

// ─── Snapshots ──────────────────────────────────────────────────────────────

/// Overview output: aggregates, model breakdown, top sessions, hourly
/// histogram, usage insights. The full string is the contract — any field
/// rename or column shuffle shows up as a diff.
#[test]
fn snapshot_overview_three_session_fixture() {
    let sessions = fixture_sessions();
    let calc = PricingCalculator::new();
    let overview = analyze_overview(&sessions, GlobalDataQuality::default(), &calc, None);
    let rendered = render_overview(&overview, &calc);
    insta::assert_snapshot!(rendered);
}

/// Projects table: rank ordering, truncation, total line. Locks the full
/// terminal table including comfy-table borders.
#[test]
fn snapshot_projects_two_project_fixture() {
    let sessions = fixture_sessions();
    let calc = PricingCalculator::new();
    let projects = analyze_projects(&sessions, &calc, 10);
    let rendered = render_projects(&projects);
    insta::assert_snapshot!(rendered);
}

/// Monthly trend (group_by_month=true, days=0=all history). Months are
/// 2025-03 and 2025-04; ordering and per-month cost must match.
#[test]
fn snapshot_trend_monthly_fixture() {
    let sessions = fixture_sessions();
    let calc = PricingCalculator::new();
    let trend = analyze_trend(&sessions, &calc, 0, true);
    let rendered = render_trend(&trend);
    insta::assert_snapshot!(rendered);
}

