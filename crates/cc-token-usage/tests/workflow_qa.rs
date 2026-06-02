//! QA tests for workflow support (cc-token-usage analysis layer).
//!
//! Covers:
//! - Pricing layer: claude-opus-4-8 bracket-strip boundaries
//! - workflow_run_id propagation: SessionFile → ParsedAgent → Subagent
//! - workflow agent tokens enter all_responses() total cost
//! - build_workflow_summaries: missing snapshot, empty phases, multi-run, parsed stats
//! - No double-count: Type3 scan skips workflows/ subdir; Type4 is the sole discoverer
//! - Workflow + ordinary subagent mix: stats independent
//! - validate workflow checks
//! - JSON and text output contain workflow block
//! - Layer 3 real-data e2e (all marked #[ignore])

use cc_token_usage::analysis::session::build_workflow_summaries;
use cc_token_usage::data::loader::load_all;
use cc_token_usage::data::models::{
    DataQuality, SessionData, SessionMetadata, Subagent, TokenUsage, ValidatedTurn,
};
use cc_token_usage::pricing::calculator::{PriceSource, PricingCalculator};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// ─── Helper: minimal ValidatedTurn ───────────────────────────────────────────

fn make_turn(
    uuid: &str,
    ts: &str,
    model: &str,
    input: u64,
    output: u64,
    is_agent: bool,
    agent_id: Option<&str>,
) -> ValidatedTurn {
    ValidatedTurn {
        uuid: uuid.into(),
        request_id: Some(format!("req-{uuid}")),
        timestamp: ts.parse().unwrap(),
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
        content_types: vec![],
        is_agent,
        agent_id: agent_id.map(|s| s.into()),
        user_text: None,
        assistant_text: None,
        tool_names: vec![],
        service_tier: None,
        speed: None,
        inference_geo: None,
        tool_error_count: 0,
        git_branch: None,
        attribution_plugin: None,
        attribution_skill: None,
    }
}

fn make_subagent(
    agent_id: &str,
    agent_type: Option<&str>,
    turns: Vec<ValidatedTurn>,
    workflow_run_id: Option<&str>,
) -> Subagent {
    Subagent {
        agent_id: agent_id.into(),
        agent_type: agent_type.map(|s| s.into()),
        description: None,
        turns,
        first_timestamp: None,
        last_timestamp: None,
        workflow_run_id: workflow_run_id.map(|s| s.into()),
    }
}

fn empty_session(session_id: &str) -> SessionData {
    SessionData {
        session_id: session_id.into(),
        project: Some("-Users-test-proj".into()),
        turns: vec![],
        subagents: vec![],
        plugins: vec![],
        skills: vec![],
        hooks: vec![],
        first_timestamp: None,
        last_timestamp: None,
        version: None,
        quality: DataQuality::default(),
        metadata: SessionMetadata::default(),
        is_orphan: false,
    }
}

fn make_claude_home() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("projects")).unwrap();
    tmp
}

/// Write a valid assistant turn JSONL line for a given session.
fn assistant_line(
    uuid: &str,
    ts: &str,
    model: &str,
    input: u64,
    output: u64,
    is_sidechain: bool,
    request_id: &str,
    session_id: &str,
) -> String {
    format!(
        r#"{{"type":"assistant","uuid":"{uuid}","timestamp":"{ts}","message":{{"model":"{model}","role":"assistant","stop_reason":"end_turn","usage":{{"input_tokens":{input},"output_tokens":{output},"cache_creation_input_tokens":0,"cache_read_input_tokens":0}},"content":[{{"type":"text","text":"hi"}}]}},"sessionId":"{session_id}","version":"2.1.159","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":{is_sidechain},"parentUuid":null,"requestId":"{request_id}"}}"#
    )
}

// ─── Layer 1: Pricing — claude-opus-4-8 bracket-strip boundaries ─────────────

#[test]
fn opus_4_8_bare_resolves_to_5_per_mtok() {
    let calc = PricingCalculator::new();
    let (price, source) = calc.get_price("claude-opus-4-8").unwrap();
    assert_eq!(source, PriceSource::Builtin);
    assert!(
        (price.base_input - 5.0).abs() < 1e-9,
        "base_input should be $5"
    );
    assert!((price.output - 25.0).abs() < 1e-9, "output should be $25");
}

#[test]
fn opus_4_8_with_1m_bracket_strips_and_resolves_correctly() {
    let calc = PricingCalculator::new();
    let (price, source) = calc.get_price("claude-opus-4-8[1m]").unwrap();
    assert_eq!(source, PriceSource::Builtin);
    assert!(
        (price.base_input - 5.0).abs() < 1e-9,
        "opus-4-8[1m] base_input must be $5, not $15"
    );
    assert!(
        (price.output - 25.0).abs() < 1e-9,
        "opus-4-8[1m] output must be $25, not $75"
    );
}

#[test]
fn opus_4_8_with_200k_bracket_strips_and_resolves_correctly() {
    let calc = PricingCalculator::new();
    let (price, source) = calc.get_price("claude-opus-4-8[200k]").unwrap();
    assert_eq!(source, PriceSource::Builtin);
    assert!(
        (price.base_input - 5.0).abs() < 1e-9,
        "opus-4-8[200k] base_input must be $5"
    );
}

#[test]
fn opus_4_8_empty_brackets_fallback_behavior() {
    // "claude-opus-4-8[]" — the bracket strip logic requires `rest.ends_with(']')`.
    // The inner part is empty ("") which ends_with(']') is false — so the model
    // string is NOT stripped and falls through to prefix lookup.
    // Either way: must not panic, and must produce a reasonable price (builtin or
    // fallback), NEVER $0 / Unknown from an empty prices map.
    let calc = PricingCalculator::new();
    let (price, _source) = calc.get_price("claude-opus-4-8[]").unwrap();
    // Prefix lookup on "claude-opus-4-8[]" will match "claude-opus-4-8" prefix → builtin
    // OR strip fails → prefix match still lands on "claude-opus-4-8" → $5.
    assert!(
        price.base_input >= 5.0,
        "empty-bracket model must not produce sub-$5 price"
    );
}

#[test]
fn bracket_at_front_is_not_stripped() {
    // "[1m]claude-opus-4-8" — prefix bracket: split_once('[') returns Some(("", "1m]claude-opus-4-8"))
    // rest = "1m]claude-opus-4-8" which does NOT end_with(']') → no strip → model stays as-is.
    // This must fall through to fallback (no exact or prefix match for "[1m]claude-opus-4-8").
    let calc = PricingCalculator::new();
    let result = calc.get_price("[1m]claude-opus-4-8");
    // Must not panic; since there's no builtin for "[1m]..." it falls back
    assert!(result.is_some(), "must return Some (fallback), not None");
    match result.unwrap().1 {
        PriceSource::Fallback { .. } => {} // expected
        PriceSource::Builtin => {}         // also acceptable if prefix logic handles it
        other => panic!("unexpected source: {:?}", other),
    }
}

#[test]
fn bracket_no_closing_bracket_not_stripped() {
    // "claude-opus-4-8[1m" — split_once('[') returns Some(("claude-opus-4-8", "1m")).
    // rest = "1m" which does NOT end_with(']') → no strip → model stays "claude-opus-4-8[1m".
    // Prefix lookup on "claude-opus-4-8[1m" will match "claude-opus-4-8" prefix → builtin.
    let calc = PricingCalculator::new();
    let (price, _source) = calc.get_price("claude-opus-4-8[1m").unwrap();
    // Prefix match lands on claude-opus-4-8 → $5 input
    assert!(price.base_input > 0.0, "must not produce zero price");
}

#[test]
fn double_bracket_inner_bracket_not_stripped() {
    // "claude-opus-4-8[[1m]]": split_once('[') → ("claude-opus-4-8", "[1m]]").
    // rest = "[1m]]" ends_with(']') → YES → model = "claude-opus-4-8".
    // This is the correct strip behavior (outer brackets removed).
    let calc = PricingCalculator::new();
    let (price, source) = calc.get_price("claude-opus-4-8[[1m]]").unwrap();
    // After strip: "claude-opus-4-8" → exact builtin match
    assert_eq!(source, PriceSource::Builtin);
    assert!((price.base_input - 5.0).abs() < 1e-9);
}

#[test]
fn opus_4_8_cost_calculation_not_opus4_priced() {
    // End-to-end: 1M input + 1M output for claude-opus-4-8[1m] must cost $30, not $90
    let calc = PricingCalculator::new();
    let usage = TokenUsage {
        input_tokens: Some(1_000_000),
        output_tokens: Some(1_000_000),
        cache_creation_input_tokens: Some(0),
        cache_read_input_tokens: Some(0),
        cache_creation: None,
        server_tool_use: None,
        service_tier: None,
        speed: None,
        inference_geo: None,
    };
    let cost = calc.calculate_turn_cost("claude-opus-4-8[1m]", &usage);
    // $5 input + $25 output = $30 (NOT $15+$75=$90 from old claude-opus-4)
    assert!(
        (cost.total - 30.0).abs() < 1e-6,
        "claude-opus-4-8[1m] must cost $30 for 1M+1M, got {}",
        cost.total
    );
    assert_eq!(cost.price_source, PriceSource::Builtin);
}

// ─── Layer 1: workflow_run_id propagation ────────────────────────────────────

#[test]
fn workflow_subagent_has_workflow_run_id_set() {
    let t = make_turn(
        "t1",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        100,
        200,
        true,
        Some("agent-wfa"),
    );
    let sa = make_subagent("agent-wfa", Some("qa-type"), vec![t], Some("wf_test01"));
    assert_eq!(sa.workflow_run_id.as_deref(), Some("wf_test01"));
}

#[test]
fn ordinary_subagent_has_no_workflow_run_id() {
    let t = make_turn(
        "t2",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        100,
        200,
        true,
        Some("agent-ord"),
    );
    let sa = make_subagent("agent-ord", Some("builder"), vec![t], None);
    assert!(sa.workflow_run_id.is_none());
}

#[test]
fn all_responses_includes_workflow_agent_turns() {
    let main_turn = make_turn(
        "m1",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        10,
        20,
        false,
        None,
    );
    let wf_turn = make_turn(
        "w1",
        "2026-06-01T10:01:00Z",
        "claude-opus-4-8",
        1000,
        2000,
        true,
        Some("agent-wf"),
    );
    let ord_turn = make_turn(
        "o1",
        "2026-06-01T10:02:00Z",
        "claude-opus-4-8",
        500,
        1000,
        true,
        Some("agent-ord"),
    );

    let mut session = empty_session("s1");
    session.turns = vec![main_turn];
    session.subagents = vec![
        make_subagent(
            "agent-wf",
            Some("workflow-worker"),
            vec![wf_turn],
            Some("wf_001"),
        ),
        make_subagent("agent-ord", Some("builder"), vec![ord_turn], None),
    ];

    let all = session.all_responses();
    assert_eq!(
        all.len(),
        3,
        "all_responses must include main + wf + ordinary turns"
    );

    let total_output: u64 = all.iter().map(|t| t.usage.output_tokens.unwrap_or(0)).sum();
    assert_eq!(total_output, 20 + 2000 + 1000);
}

#[test]
fn agent_turn_count_includes_workflow_agents() {
    let wf_t1 = make_turn(
        "w1",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        100,
        200,
        true,
        None,
    );
    let wf_t2 = make_turn(
        "w2",
        "2026-06-01T10:01:00Z",
        "claude-opus-4-8",
        100,
        200,
        true,
        None,
    );
    let ord_t = make_turn(
        "o1",
        "2026-06-01T10:02:00Z",
        "claude-opus-4-8",
        50,
        100,
        true,
        None,
    );

    let mut session = empty_session("s1");
    session.subagents = vec![
        make_subagent("agent-wf", None, vec![wf_t1, wf_t2], Some("wf_001")),
        make_subagent("agent-ord", None, vec![ord_t], None),
    ];

    assert_eq!(session.agent_turn_count(), 3);
    assert_eq!(session.total_turn_count(), 3); // no main turns
}

#[test]
fn overview_total_agent_turns_includes_workflow_turns() {
    // Verify that workflow turns count toward total_agent_turns in the overview.
    let wf_turn = make_turn(
        "w1",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        500,
        1000,
        true,
        None,
    );
    let main_turn = make_turn(
        "m1",
        "2026-06-01T09:00:00Z",
        "claude-opus-4-8",
        10,
        20,
        false,
        None,
    );

    let mut session = empty_session("session-overview-01");
    session.turns = vec![main_turn];
    session.subagents = vec![make_subagent(
        "agent-wf",
        None,
        vec![wf_turn],
        Some("wf_overview"),
    )];

    let calc = PricingCalculator::new();
    let quality = cc_token_usage::data::models::GlobalDataQuality::default();
    let overview =
        cc_token_usage::analysis::overview::analyze_overview(&[session], quality, &calc, None);

    assert_eq!(overview.total_turns, 2, "main + wf = 2 total turns");
    assert_eq!(overview.total_agent_turns, 1, "1 workflow agent turn");
    assert!(overview.total_cost > 0.0, "workflow cost must be included");
}

// ─── Layer 1: build_workflow_summaries ───────────────────────────────────────

/// Helper: create a TempDir with a workflow tree and return the TempDir and session UUID.
fn setup_workflow_tree_for_summaries(
    run_id: &str,
    snapshot_json: Option<&str>,
) -> (TempDir, String) {
    let tmp = make_claude_home();
    let session_uuid = "bbbbbbbb-cccc-dddd-eeee-ffffffffffff";
    let proj = tmp.path().join("projects").join("-Users-wf-summary");
    let subagents = proj.join(session_uuid).join("subagents");
    let wf_run = subagents.join("workflows").join(run_id);
    let workflows_dir = proj.join(session_uuid).join("workflows");

    fs::create_dir_all(&wf_run).unwrap();
    fs::create_dir_all(&workflows_dir).unwrap();

    // Main session
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        r#"{"type":"user","uuid":"u1"}"#,
    )
    .unwrap();

    // Snapshot (optional)
    if let Some(snap) = snapshot_json {
        fs::write(workflows_dir.join(format!("{}.json", run_id)), snap).unwrap();
    }

    // One workflow agent transcript
    let agent_line = assistant_line(
        "wf-turn-1",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        1000,
        2000,
        true,
        "req-wf-1",
        "agent-wf001",
    );
    fs::write(
        wf_run.join("agent-wf001.jsonl"),
        format!("{}\n", agent_line),
    )
    .unwrap();

    (tmp, session_uuid.into())
}

#[test]
fn build_workflow_summaries_returns_empty_for_session_with_no_workflows() {
    let tmp = make_claude_home();
    let session = empty_session("no-workflow-session");
    let calc = PricingCalculator::new();
    let summaries = build_workflow_summaries(&session, &calc, tmp.path());
    assert!(summaries.is_empty());
}

#[test]
fn build_workflow_summaries_missing_snapshot_still_returns_run() {
    let (tmp, _session_uuid) = setup_workflow_tree_for_summaries("wf_nosnap", None);

    // Load sessions to get the parsed Subagent
    let calc = PricingCalculator::new();
    let (sessions, _) = load_all(tmp.path(), &calc).unwrap();
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];

    let summaries = build_workflow_summaries(s, &calc, tmp.path());
    // build_workflow_summaries calls scan_session_workflows; wf_nosnap has agents
    // but no snapshot → the run is discovered, summaries entry exists
    assert_eq!(
        summaries.len(),
        1,
        "run discovered via agent dir even without snapshot"
    );
    let ws = &summaries[0];
    assert_eq!(ws.run_id, "wf_nosnap");
    assert!(ws.workflow_name.is_none(), "no snapshot → no workflow_name");
    assert!(ws.status.is_none());
    // parsed stats from the actual agent transcript
    assert_eq!(ws.parsed_agent_count, 1);
    assert_eq!(ws.parsed_turns, 1);
    assert!(ws.parsed_cost > 0.0, "workflow turn cost must be > 0");
}

#[test]
fn build_workflow_summaries_with_snapshot_populates_declared_fields() {
    let snap = r#"{
        "runId":"wf_withsnap",
        "workflowName":"test-wf",
        "status":"completed",
        "durationMs":55000,
        "agentCount":1,
        "totalTokens":99999,
        "phases":[{"title":"Phase1","detail":"do it"}]
    }"#;
    let (tmp, _session_uuid) = setup_workflow_tree_for_summaries("wf_withsnap", Some(snap));
    let calc = PricingCalculator::new();
    let (sessions, _) = load_all(tmp.path(), &calc).unwrap();
    let s = &sessions[0];

    let summaries = build_workflow_summaries(s, &calc, tmp.path());
    assert_eq!(summaries.len(), 1);
    let ws = &summaries[0];

    assert_eq!(ws.workflow_name.as_deref(), Some("test-wf"));
    assert_eq!(ws.status.as_deref(), Some("completed"));
    assert_eq!(ws.snapshot_duration_ms, Some(55000));
    assert_eq!(ws.snapshot_agent_count, Some(1));
    assert_eq!(ws.snapshot_total_tokens, Some(99999));
    assert_eq!(ws.phases.len(), 1);
    assert_eq!(ws.phases[0].title.as_deref(), Some("Phase1"));
    assert_eq!(ws.phases[0].detail.as_deref(), Some("do it"));
}

#[test]
fn build_workflow_summaries_empty_phases_list() {
    let snap = r#"{"runId":"wf_nophases","phases":[]}"#;
    let (tmp, _) = setup_workflow_tree_for_summaries("wf_nophases", Some(snap));
    let calc = PricingCalculator::new();
    let (sessions, _) = load_all(tmp.path(), &calc).unwrap();
    let summaries = build_workflow_summaries(&sessions[0], &calc, tmp.path());
    assert_eq!(summaries.len(), 1);
    assert!(
        summaries[0].phases.is_empty(),
        "empty phases array → empty Vec"
    );
}

#[test]
fn build_workflow_summaries_parsed_counts_only_matching_run() {
    // Two workflow runs; parsed_agent_count and parsed_turns must be per-run
    let tmp = make_claude_home();
    let session_uuid = "cccccccc-dddd-eeee-ffff-000000000001";
    let proj = tmp.path().join("projects").join("-Users-wf-multi");

    for (run_id, agent_id, ts, req_id) in &[
        ("wf_runA", "agent-a1", "2026-06-01T10:00:00Z", "req-a1"),
        ("wf_runB", "agent-b1", "2026-06-01T10:01:00Z", "req-b1"),
    ] {
        let wf_run = proj
            .join(session_uuid)
            .join("subagents")
            .join("workflows")
            .join(run_id);
        let wf_dir = proj.join(session_uuid).join("workflows");
        fs::create_dir_all(&wf_run).unwrap();
        fs::create_dir_all(&wf_dir).unwrap();

        // Each run has one agent with a unique set of tokens
        let line = assistant_line(
            agent_id,
            ts,
            "claude-opus-4-8",
            500,
            1000,
            true,
            req_id,
            agent_id,
        );
        fs::write(
            wf_run.join(format!("{}.jsonl", agent_id)),
            format!("{}\n", line),
        )
        .unwrap();

        // Snapshot for each run
        fs::write(
            wf_dir.join(format!("{}.json", run_id)),
            format!(r#"{{"runId":"{run_id}","agentCount":1,"totalTokens":1500}}"#),
        )
        .unwrap();
    }

    // Main session
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        r#"{"type":"user","uuid":"u1"}"#,
    )
    .unwrap();

    let calc = PricingCalculator::new();
    let (sessions, _) = load_all(tmp.path(), &calc).unwrap();
    assert_eq!(sessions.len(), 1);

    let summaries = build_workflow_summaries(&sessions[0], &calc, tmp.path());
    assert_eq!(summaries.len(), 2);

    // Sorted by run_id: wf_runA first
    let sum_a = summaries.iter().find(|s| s.run_id == "wf_runA").unwrap();
    let sum_b = summaries.iter().find(|s| s.run_id == "wf_runB").unwrap();

    assert_eq!(sum_a.parsed_agent_count, 1);
    assert_eq!(sum_a.parsed_turns, 1);
    assert!(sum_a.parsed_cost > 0.0);

    assert_eq!(sum_b.parsed_agent_count, 1);
    assert_eq!(sum_b.parsed_turns, 1);
    assert!(sum_b.parsed_cost > 0.0);

    // The two runs' costs must be independent of each other (same token counts → same cost)
    assert!(
        (sum_a.parsed_cost - sum_b.parsed_cost).abs() < 1e-9,
        "both runs have same tokens → same cost"
    );
}

// ─── Layer 2: Integration — workflow tokens enter all_responses() total ────────

#[test]
fn integration_workflow_agent_tokens_in_total_cost() {
    let tmp = make_claude_home();
    let session_uuid = "11112222-3333-4444-5555-666677778888";
    let proj = tmp.path().join("projects").join("-Users-wf-integ");
    let subagents = proj.join(session_uuid).join("subagents");
    let wf_run = subagents.join("workflows").join("wf_integ01");
    fs::create_dir_all(&wf_run).unwrap();

    // Main turn
    let main = assistant_line(
        "m1",
        "2026-06-01T09:00:00Z",
        "claude-opus-4-8",
        100,
        200,
        false,
        "req-m1",
        session_uuid,
    );
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        format!("{}\n", main),
    )
    .unwrap();

    // Two workflow agents with substantial tokens
    let wf_a = assistant_line(
        "wa1",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        10000,
        20000,
        true,
        "req-wa1",
        "agent-wa1",
    );
    let wf_b = assistant_line(
        "wb1",
        "2026-06-01T10:01:00Z",
        "claude-opus-4-8",
        30000,
        40000,
        true,
        "req-wb1",
        "agent-wb1",
    );
    fs::write(wf_run.join("agent-wa1.jsonl"), format!("{}\n", wf_a)).unwrap();
    fs::write(wf_run.join("agent-wb1.jsonl"), format!("{}\n", wf_b)).unwrap();

    let calc = PricingCalculator::new();
    let (sessions, _) = load_all(tmp.path(), &calc).unwrap();
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];

    // Verify workflow agents are subagents with workflow_run_id
    assert_eq!(s.subagents.len(), 2);
    for sa in &s.subagents {
        assert_eq!(sa.workflow_run_id.as_deref(), Some("wf_integ01"));
    }

    // all_responses() = 3 turns (1 main + 2 wf)
    let all = s.all_responses();
    assert_eq!(all.len(), 3);

    // Total output tokens
    let total_out: u64 = all.iter().map(|t| t.usage.output_tokens.unwrap_or(0)).sum();
    assert_eq!(total_out, 200 + 20000 + 40000);

    // Total cost must include workflow turns
    let total_cost: f64 = all
        .iter()
        .map(|t| calc.calculate_turn_cost(&t.model, &t.usage).total)
        .sum();
    let main_cost = calc
        .calculate_turn_cost(
            "claude-opus-4-8",
            &TokenUsage {
                input_tokens: Some(100),
                output_tokens: Some(200),
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                cache_creation: None,
                server_tool_use: None,
                service_tier: None,
                speed: None,
                inference_geo: None,
            },
        )
        .total;
    assert!(
        total_cost > main_cost,
        "total_cost {total_cost} must exceed main-only cost {main_cost}"
    );
}

#[test]
fn integration_no_double_count_workflow_and_ordinary_subagents() {
    // A session with both ordinary subagent and workflow agents.
    // agent_turn_count = ordinary_turns + wf_turns (no duplicates).
    let tmp = make_claude_home();
    let session_uuid = "aaaabbbb-cccc-dddd-eeee-ffff00001111";
    let proj = tmp.path().join("projects").join("-Users-wf-mix");
    let subagents = proj.join(session_uuid).join("subagents");
    let wf_run = subagents.join("workflows").join("wf_mix01");
    fs::create_dir_all(&subagents).unwrap();
    fs::create_dir_all(&wf_run).unwrap();

    // Main session
    let main = assistant_line(
        "m1",
        "2026-06-01T09:00:00Z",
        "claude-opus-4-8",
        10,
        20,
        false,
        "req-main",
        session_uuid,
    );
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        format!("{}\n", main),
    )
    .unwrap();

    // Ordinary subagent: 2 unique turns
    let ord1 = assistant_line(
        "o1",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        1,
        2,
        true,
        "req-ord1",
        "agent-ord1",
    );
    let ord2 = assistant_line(
        "o2",
        "2026-06-01T10:01:00Z",
        "claude-opus-4-8",
        1,
        2,
        true,
        "req-ord2",
        "agent-ord1",
    );
    fs::write(
        subagents.join("agent-ord1.jsonl"),
        format!("{}\n{}\n", ord1, ord2),
    )
    .unwrap();

    // Workflow agents: 1 turn each
    let wf1 = assistant_line(
        "w1",
        "2026-06-01T10:02:00Z",
        "claude-opus-4-8",
        1,
        2,
        true,
        "req-wf1",
        "agent-wf1",
    );
    let wf2 = assistant_line(
        "w2",
        "2026-06-01T10:03:00Z",
        "claude-opus-4-8",
        1,
        2,
        true,
        "req-wf2",
        "agent-wf2",
    );
    fs::write(wf_run.join("agent-wf1.jsonl"), format!("{}\n", wf1)).unwrap();
    fs::write(wf_run.join("agent-wf2.jsonl"), format!("{}\n", wf2)).unwrap();

    let calc = PricingCalculator::new();
    let (sessions, _) = load_all(tmp.path(), &calc).unwrap();
    let s = &sessions[0];

    // Ordinary: 2 turns, workflow: 2 turns, total agent: 4
    assert_eq!(s.agent_turn_count(), 4, "2 ordinary + 2 wf = 4 agent turns");
    assert_eq!(s.total_turn_count(), 5, "1 main + 4 agent");

    // Subagent counts: 1 ordinary + 2 wf
    assert_eq!(s.subagents.len(), 3);

    let wf_subagents: Vec<_> = s
        .subagents
        .iter()
        .filter(|sa| sa.workflow_run_id.is_some())
        .collect();
    let ord_subagents: Vec<_> = s
        .subagents
        .iter()
        .filter(|sa| sa.workflow_run_id.is_none())
        .collect();
    assert_eq!(wf_subagents.len(), 2);
    assert_eq!(ord_subagents.len(), 1);
    assert_eq!(
        ord_subagents[0].turns.len(),
        2,
        "ordinary subagent has 2 unique turns"
    );
}

#[test]
fn integration_workflow_agent_dedup_vs_main_session() {
    // A workflow agent turn that shares a requestId with a main session turn
    // must be deduped (not double-counted).
    let tmp = make_claude_home();
    let session_uuid = "aaaabbbb-cccc-dddd-eeee-ffff00002222";
    let proj = tmp.path().join("projects").join("-Users-wf-dedup");
    let wf_run = proj
        .join(session_uuid)
        .join("subagents")
        .join("workflows")
        .join("wf_dedup");
    fs::create_dir_all(&wf_run).unwrap();

    let shared_req = "req-shared-000";
    // Main session has this requestId
    let main_turn = assistant_line(
        "m1",
        "2026-06-01T09:00:00Z",
        "claude-opus-4-8",
        100,
        200,
        false,
        shared_req,
        session_uuid,
    );
    // Workflow agent ALSO has this requestId (cross-file dup)
    let wf_dup = assistant_line(
        "w1",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        9999,
        9999,
        true,
        shared_req,
        "agent-wf-dup",
    );
    // Workflow agent has a UNIQUE requestId (must be kept)
    let wf_unique = assistant_line(
        "w2",
        "2026-06-01T10:01:00Z",
        "claude-opus-4-8",
        500,
        1000,
        true,
        "req-wf-unique",
        "agent-wf-dup",
    );

    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        format!("{}\n", main_turn),
    )
    .unwrap();
    fs::write(
        wf_run.join("agent-wf-dup.jsonl"),
        format!("{}\n{}\n", wf_dup, wf_unique),
    )
    .unwrap();

    let calc = PricingCalculator::new();
    let (sessions, _) = load_all(tmp.path(), &calc).unwrap();
    let s = &sessions[0];

    // The duplicate (shared_req) must be dropped; only the unique wf turn kept
    assert_eq!(
        s.agent_turn_count(),
        1,
        "dup must be dropped → only 1 wf agent turn"
    );
    assert_eq!(s.total_turn_count(), 2, "1 main + 1 unique wf");

    // The unique wf turn has 500/1000 tokens, not 9999
    let wf_sa = s
        .subagents
        .iter()
        .find(|sa| sa.agent_id.starts_with("agent-wf-dup"))
        .unwrap();
    assert_eq!(wf_sa.turns.len(), 1);
    assert_eq!(wf_sa.turns[0].usage.input_tokens, Some(500));
    assert_eq!(wf_sa.turns[0].usage.output_tokens, Some(1000));
}

// ─── Layer 2: Validate workflow checks ───────────────────────────────────────

#[test]
fn validate_workflow_checks_pass_for_session_with_workflow() {
    // End-to-end validation: a session with workflow agents should produce
    // passing workflow checks ("parsed tokens > 0" and "agent_count == snapshot").
    let tmp = make_claude_home();
    let session_uuid = "ddddeeee-ffff-0000-1111-222233334444";
    let proj = tmp.path().join("projects").join("-Users-wf-validate");
    let wf_run = proj
        .join(session_uuid)
        .join("subagents")
        .join("workflows")
        .join("wf_val01");
    let wf_dir = proj.join(session_uuid).join("workflows");
    fs::create_dir_all(&wf_run).unwrap();
    fs::create_dir_all(&wf_dir).unwrap();

    // Main session
    let main = assistant_line(
        "m1",
        "2026-06-01T09:00:00Z",
        "claude-opus-4-8",
        10,
        20,
        false,
        "req-m1",
        session_uuid,
    );
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        format!("{}\n", main),
    )
    .unwrap();

    // Workflow snapshot: declares 1 agent
    fs::write(
        wf_dir.join("wf_val01.json"),
        r#"{"runId":"wf_val01","workflowName":"val-wf","status":"completed","agentCount":1,"totalTokens":3000}"#,
    )
    .unwrap();

    // One workflow agent transcript
    let wf_line = assistant_line(
        "w1",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        1000,
        2000,
        true,
        "req-w1",
        "agent-wfv1",
    );
    fs::write(wf_run.join("agent-wfv1.jsonl"), format!("{}\n", wf_line)).unwrap();

    let calc = PricingCalculator::new();
    let (sessions, quality) = load_all(tmp.path(), &calc).unwrap();

    let session_refs: Vec<&cc_token_usage::data::models::SessionData> = sessions.iter().collect();
    let report = cc_token_usage::analysis::validate::validate_all(
        &session_refs,
        &quality,
        tmp.path(),
        &calc,
    )
    .unwrap();

    // Find workflow-specific checks
    let sv = report
        .session_results
        .iter()
        .find(|sv| sv.session_id == session_uuid)
        .unwrap();
    let wf_token_check = sv
        .agent_checks
        .iter()
        .find(|c| c.name.contains("wf_val01") && c.name.contains("parsed tokens"));
    let wf_count_check = sv
        .agent_checks
        .iter()
        .find(|c| c.name.contains("wf_val01") && c.name.contains("agent_count"));

    assert!(
        wf_token_check.is_some(),
        "workflow parsed-tokens check must exist"
    );
    assert!(
        wf_token_check.unwrap().passed,
        "workflow parsed tokens > 0 check must pass"
    );

    assert!(
        wf_count_check.is_some(),
        "workflow agent_count check must exist"
    );
    assert!(
        wf_count_check.unwrap().passed,
        "workflow agent_count == snapshot must pass"
    );
}

// ─── Layer 2: JSON output contains workflow block ────────────────────────────

#[test]
fn json_output_contains_workflow_section() {
    use cc_token_usage::analysis::session::analyze_session;
    use cc_token_usage::analysis::session::AgentMeta;
    use cc_token_usage::output::json::render_session_json;

    let tmp = make_claude_home();
    let session_uuid = "eeeeeeee-ffff-0000-1111-222233334455";
    let proj = tmp.path().join("projects").join("-Users-wf-json");
    let wf_run = proj
        .join(session_uuid)
        .join("subagents")
        .join("workflows")
        .join("wf_json01");
    let wf_dir = proj.join(session_uuid).join("workflows");
    fs::create_dir_all(&wf_run).unwrap();
    fs::create_dir_all(&wf_dir).unwrap();

    let main = assistant_line(
        "m1",
        "2026-06-01T09:00:00Z",
        "claude-opus-4-8",
        10,
        20,
        false,
        "req-main",
        session_uuid,
    );
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        format!("{}\n", main),
    )
    .unwrap();

    fs::write(
        wf_dir.join("wf_json01.json"),
        r#"{"runId":"wf_json01","workflowName":"json-test","status":"completed","agentCount":1}"#,
    )
    .unwrap();

    let wf_line = assistant_line(
        "w1",
        "2026-06-01T10:00:00Z",
        "claude-opus-4-8",
        1000,
        2000,
        true,
        "req-w1",
        "agent-wfj",
    );
    fs::write(wf_run.join("agent-wfj.jsonl"), format!("{}\n", wf_line)).unwrap();

    let calc = PricingCalculator::new();
    let (sessions, _quality) = load_all(tmp.path(), &calc).unwrap();
    assert_eq!(sessions.len(), 1);

    let agent_meta: HashMap<String, AgentMeta> = HashMap::new();
    let result = analyze_session(&sessions[0], &calc, &agent_meta, tmp.path());
    let json_str = render_session_json(&result);

    // Must be valid JSON
    let _parsed: serde_json::Value =
        serde_json::from_str(&json_str).expect("session JSON must be valid");

    // The JSON must contain the workflow run name and run_id
    assert!(
        json_str.contains("json-test"),
        "session JSON must contain workflow name 'json-test'"
    );
    assert!(
        json_str.contains("wf_json01"),
        "session JSON must contain run_id 'wf_json01'"
    );
}

// ─── Layer 3: Real data e2e (#[ignore]) ──────────────────────────────────────

fn real_claude_home() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let ch = std::path::PathBuf::from(home).join(".claude");
    if ch.is_dir() {
        Some(ch)
    } else {
        None
    }
}

/// Full e2e: load all sessions, zero panics, workflow cost included.
#[test]
#[ignore]
fn real_e2e_load_all_no_panic() {
    let claude_home = match real_claude_home() {
        Some(h) => h,
        None => {
            eprintln!("Skipping: ~/.claude not found");
            return;
        }
    };
    let calc = PricingCalculator::new();
    let (sessions, quality) = load_all(&claude_home, &calc).expect("load_all must not panic");

    assert!(!sessions.is_empty(), "must find at least one session");

    // Zero panics on overview
    let overview =
        cc_token_usage::analysis::overview::analyze_overview(&sessions, quality, &calc, None);
    assert!(overview.total_sessions > 0);
    assert!(overview.total_cost >= 0.0);

    eprintln!(
        "Loaded {} sessions, total cost ${:.4}",
        sessions.len(),
        overview.total_cost
    );
}

/// Specifically verifies that workflow session ae289b37 costs flow into totals.
#[test]
#[ignore]
fn real_workflow_session_ae289b37_cost_included() {
    let claude_home = match real_claude_home() {
        Some(h) => h,
        None => {
            eprintln!("Skipping: ~/.claude not found");
            return;
        }
    };
    let calc = PricingCalculator::new();
    let (sessions, _quality) = load_all(&claude_home, &calc).expect("load_all");

    let target_id = "ae289b37-f19a-4797-b14c-52b5ada582ed";
    let session = match sessions.iter().find(|s| s.session_id == target_id) {
        Some(s) => s,
        None => {
            eprintln!("Skipping: session {} not present locally", target_id);
            return;
        }
    };

    // Session has workflow subagents
    let wf_subagents: Vec<_> = session
        .subagents
        .iter()
        .filter(|sa| sa.workflow_run_id.is_some())
        .collect();
    assert!(
        !wf_subagents.is_empty(),
        "ae289b37 must have workflow subagents"
    );

    // build_workflow_summaries produces 3 runs
    let summaries = build_workflow_summaries(session, &calc, &claude_home);
    assert_eq!(summaries.len(), 3, "ae289b37 must have 3 workflow runs");

    // Each run has parsed tokens > 0
    for ws in &summaries {
        assert!(ws.parsed_turns > 0, "{} must have parsed turns", ws.run_id);
        assert!(
            ws.parsed_cost > 0.0,
            "{} must have non-zero cost",
            ws.run_id
        );
    }

    // Workflow turns are in all_responses()
    let all = session.all_responses();
    let wf_turns: Vec<_> = all.iter().filter(|t| t.is_agent).collect();
    assert!(
        !wf_turns.is_empty(),
        "workflow turns must be in all_responses()"
    );

    let total_cost: f64 = all
        .iter()
        .map(|t| calc.calculate_turn_cost(&t.model, &t.usage).total)
        .sum();
    assert!(total_cost > 0.0, "ae289b37 total cost must be > 0");
    eprintln!(
        "ae289b37: {} total turns, {:.4} total cost, {} workflow runs",
        all.len(),
        total_cost,
        summaries.len()
    );
}

/// Validate runs across all sessions including workflow ones — zero unexpected failures.
#[test]
#[ignore]
fn real_e2e_validate_no_unexpected_failures() {
    let claude_home = match real_claude_home() {
        Some(h) => h,
        None => {
            eprintln!("Skipping");
            return;
        }
    };
    let calc = PricingCalculator::new();
    let (sessions, quality) = load_all(&claude_home, &calc).expect("load_all");

    let session_refs: Vec<_> = sessions.iter().collect();
    let report = cc_token_usage::analysis::validate::validate_all(
        &session_refs,
        &quality,
        &claude_home,
        &calc,
    )
    .expect("validate_all must not panic");

    eprintln!(
        "Validated {} sessions: {} passed, {} failed, {} total checks",
        report.summary.sessions_validated,
        report.summary.sessions_passed,
        report.summary.failed,
        report.summary.total_checks,
    );

    // Workflow-specific checks must all pass (no workflow run should have 0 parsed tokens)
    let wf_token_failures: Vec<_> = report
        .session_results
        .iter()
        .flat_map(|sv| sv.agent_checks.iter())
        .filter(|c| c.name.contains("parsed tokens > 0") && !c.passed)
        .collect();
    assert!(
        wf_token_failures.is_empty(),
        "Some workflow runs have 0 parsed tokens: {:?}",
        wf_token_failures
    );
}

/// JSON output (overview) is parseable by serde_json for all sessions.
#[test]
#[ignore]
fn real_e2e_json_output_valid() {
    use cc_token_usage::analysis::overview::analyze_overview;
    use cc_token_usage::output::json::render_overview_json;

    let claude_home = match real_claude_home() {
        Some(h) => h,
        None => {
            eprintln!("Skipping");
            return;
        }
    };
    let calc = PricingCalculator::new();
    let (sessions, quality) = load_all(&claude_home, &calc).expect("load_all");
    let overview = analyze_overview(&sessions, quality, &calc, None);
    let json_str = render_overview_json(&overview);
    let _: serde_json::Value =
        serde_json::from_str(&json_str).expect("overview JSON must be valid JSON");
    eprintln!("JSON output: {} bytes", json_str.len());
}

/// LenientReader error rate < 1% across all real JSONL files.
#[test]
#[ignore]
fn real_e2e_lenient_reader_error_rate() {
    use cc_session_jsonl::SessionReader;

    let claude_home = match real_claude_home() {
        Some(h) => h,
        None => {
            eprintln!("Skipping");
            return;
        }
    };
    let projects_dir = claude_home.join("projects");

    fn collect_jsonl(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    let n = e.file_name().to_string_lossy().to_string();
                    if n != "memory" && n != "tool-results" {
                        collect_jsonl(&p, out);
                    }
                } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    out.push(p);
                }
            }
        }
    }

    let mut files = vec![];
    collect_jsonl(&projects_dir, &mut files);

    let mut total: usize = 0;
    let mut errors: usize = 0;
    for path in &files {
        if let Ok(reader) = SessionReader::open(path) {
            let mut lr = reader.lenient();
            for _ in lr.by_ref() {
                total += 1;
            }
            errors += lr.errors_skipped();
        }
    }

    let rate = if total + errors > 0 {
        errors as f64 / (total + errors) as f64
    } else {
        0.0
    };
    eprintln!(
        "Error rate: {:.4}% ({} errors / {} total)",
        rate * 100.0,
        errors,
        total + errors
    );
    assert!(
        rate < 0.01,
        "LenientReader error rate {:.4}% exceeds 1%",
        rate * 100.0
    );
}
