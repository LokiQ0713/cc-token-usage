use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::models::{
    DataQuality, GlobalDataQuality, HookUsage, PluginUsage, SessionData, SessionMetadata,
    SkillUsage, Subagent,
};
use super::parser::parse_session_file;
use crate::pricing::calculator::PricingCalculator;
use cc_session_jsonl::Session;

/// Extract the Claude Code version string from the first line of a JSONL file.
///
/// Both `user` and `assistant` entries carry a `version` field at the top level.
fn extract_version(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let first_line = reader.lines().next()?.ok()?;
    let val: serde_json::Value = serde_json::from_str(&first_line).ok()?;
    val.get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Compute the min and max timestamps from a slice of turns that have timestamps.
fn time_range<'a, I>(timestamps: I) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>)
where
    I: Iterator<Item = &'a DateTime<Utc>>,
{
    let mut min: Option<DateTime<Utc>> = None;
    let mut max: Option<DateTime<Utc>> = None;
    for ts in timestamps {
        min = Some(min.map_or(*ts, |m: DateTime<Utc>| m.min(*ts)));
        max = Some(max.map_or(*ts, |m: DateTime<Utc>| m.max(*ts)));
    }
    (min, max)
}

/// Build a set of requestIds from the main session turns for cross-file dedup.
fn request_id_set(turns: &[super::models::ValidatedTurn]) -> HashSet<String> {
    turns
        .iter()
        .filter_map(|t| t.request_id.as_ref())
        .cloned()
        .collect()
}

/// Load all session data from a Claude home directory.
///
/// 1. Calls `cc_session_jsonl::load_all_sessions` (which scans + groups files
///    by session id and surfaces a `Vec<Session>`).
/// 2. Parses every session's main JSONL and every agent JSONL through the
///    analysis pipeline (in parallel via rayon) — the cc-session-jsonl
///    aggregation only enumerates files, it does not run the validation /
///    dedup / metadata pipeline this crate owns.
/// 3. Groups parsed agent turns into `Subagent` entries; aggregates plugins,
///    skills, hooks; computes time ranges and the global quality summary.
pub fn load_all(
    claude_home: &Path,
    calc: &PricingCalculator,
) -> Result<(Vec<SessionData>, GlobalDataQuality)> {
    let raw_sessions = cc_session_jsonl::load_all_sessions(claude_home)
        .with_context(|| format!("failed to load sessions from {}", claude_home.display()))?;
    load_from_sessions(raw_sessions, calc)
}

/// Parsed result from a single main session file, ready for serial assembly.
struct ParsedMain {
    session_id: String,
    project: Option<String>,
    turns: Vec<super::models::ValidatedTurn>,
    version: Option<String>,
    first_ts: Option<DateTime<Utc>>,
    last_ts: Option<DateTime<Utc>>,
    quality: DataQuality,
    metadata: SessionMetadata,
    hooks: Vec<HookUsage>,
}

/// Parsed result from a single agent file. One `ParsedAgent` becomes one
/// `Subagent` under its parent session.
struct ParsedAgent {
    /// The parent session this subagent belongs to.
    target_id: String,
    /// Project context (used only when the parent main session is missing).
    project: Option<String>,
    /// The subagent ID, taken verbatim from the agent JSONL file stem.
    agent_id: String,
    turns: Vec<super::models::ValidatedTurn>,
    quality: DataQuality,
    /// The workflow run id, if this agent was discovered under a workflow run
    /// directory. `None` for ordinary subagents.
    workflow_run_id: Option<String>,
    /// Metadata sidecar resolved by the loader (agent-id stripped key).
    meta: Option<cc_session_jsonl::AgentMetadata>,
}

/// Shared loading logic.
fn load_from_sessions(
    raw_sessions: Vec<Session>,
    calc: &PricingCalculator,
) -> Result<(Vec<SessionData>, GlobalDataQuality)> {
    // Flatten the (Session, [Agent]) hierarchy into two parallelizable work
    // lists. This keeps the analysis-side pipeline unchanged — we still parse
    // each JSONL file independently and then group results.
    // (target_id, agent_id, project, path, workflow_run_id, meta)
    type AgentJob = (
        String,
        String,
        Option<String>,
        std::path::PathBuf,
        Option<String>,
        Option<cc_session_jsonl::AgentMetadata>,
    );
    let mut main_jobs: Vec<(String, Option<String>, std::path::PathBuf)> = Vec::new();
    let mut agent_jobs: Vec<AgentJob> = Vec::new();
    for session in raw_sessions {
        let main_path = derive_main_path(&session);
        if let Some(path) = main_path {
            main_jobs.push((session.id.clone(), session.project.clone(), path));
        }
        for agent in session.agents {
            agent_jobs.push((
                agent.parent_session_id,
                agent.agent_id,
                agent.project,
                agent.path,
                agent.workflow_run_id,
                agent.meta,
            ));
        }
    }

    let mut global_quality = GlobalDataQuality {
        total_session_files: main_jobs.len(),
        total_agent_files: agent_jobs.len(),
        ..Default::default()
    };

    // ── Phase 1: Parse all main sessions in parallel ──────────────────────
    let parsed_mains: Vec<Result<ParsedMain>> = main_jobs
        .par_iter()
        .map(|(session_id, project, path)| {
            let (turns, quality, metadata, hooks) = parse_session_file(path, false)
                .with_context(|| format!("failed to parse session: {}", path.display()))?;
            let version = extract_version(path);
            let (first_ts, last_ts) = time_range(turns.iter().map(|t| &t.timestamp));
            Ok(ParsedMain {
                session_id: session_id.clone(),
                project: project.clone(),
                turns,
                version,
                first_ts,
                last_ts,
                quality,
                metadata,
                hooks,
            })
        })
        .collect();

    let mut sessions: HashMap<String, SessionData> = HashMap::with_capacity(parsed_mains.len());
    for result in parsed_mains {
        let pm = result?;
        global_quality.total_valid_turns += pm.quality.valid_turns;
        global_quality.total_skipped += pm.quality.skipped_synthetic
            + pm.quality.skipped_sidechain
            + pm.quality.skipped_invalid
            + pm.quality.skipped_parse_error;

        sessions.insert(
            pm.session_id.clone(),
            SessionData {
                session_id: pm.session_id,
                project: pm.project,
                turns: pm.turns,
                subagents: Vec::new(),
                plugins: Vec::new(),
                skills: Vec::new(),
                hooks: pm.hooks,
                first_timestamp: pm.first_ts,
                last_timestamp: pm.last_ts,
                version: pm.version,
                quality: pm.quality,
                metadata: pm.metadata,
                is_orphan: false,
            },
        );
    }

    // ── Phase 2: Parse all agent files in parallel ────────────────────────
    let parsed_agents: Vec<Result<ParsedAgent>> = agent_jobs
        .par_iter()
        .map(|(target_id, agent_id, project, path, workflow_run_id, meta)| {
            let (turns, quality, _meta, _hooks) = parse_session_file(path, true)
                .with_context(|| format!("failed to parse agent file: {}", path.display()))?;
            Ok(ParsedAgent {
                target_id: target_id.clone(),
                project: project.clone(),
                agent_id: agent_id.clone(),
                turns,
                quality,
                workflow_run_id: workflow_run_id.clone(),
                meta: meta.clone(),
            })
        })
        .collect();

    let mut agents_by_parent: HashMap<String, Vec<ParsedAgent>> = HashMap::new();
    for result in parsed_agents {
        let pa = result?;
        global_quality.total_valid_turns += pa.quality.valid_turns;
        global_quality.total_skipped += pa.quality.skipped_synthetic
            + pa.quality.skipped_sidechain
            + pa.quality.skipped_invalid
            + pa.quality.skipped_parse_error;
        agents_by_parent
            .entry(pa.target_id.clone())
            .or_default()
            .push(pa);
    }

    // Merge each parent's agents into Subagent records.
    for (target_id, agents) in agents_by_parent {
        // Ensure parent session exists (create orphan placeholder if missing).
        if !sessions.contains_key(&target_id) {
            let project = agents
                .iter()
                .find_map(|a| a.project.clone())
                .or_else(|| Some("(orphan)".to_string()));
            sessions.insert(
                target_id.clone(),
                SessionData {
                    session_id: target_id.clone(),
                    project,
                    turns: Vec::new(),
                    subagents: Vec::new(),
                    plugins: Vec::new(),
                    skills: Vec::new(),
                    hooks: Vec::new(),
                    first_timestamp: None,
                    last_timestamp: None,
                    version: None,
                    quality: DataQuality::default(),
                    metadata: SessionMetadata::default(),
                    is_orphan: true,
                },
            );
            global_quality.orphan_agents += 1;
        }

        let parent = sessions.get_mut(&target_id).unwrap();
        let existing_rids = request_id_set(&parent.turns);

        // Build subagents in deterministic order: sorted by agent_id.
        let mut agents = agents;
        agents.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));

        for pa in agents {
            // Cross-file dedup: drop turns whose requestId already appears in
            // the main session.
            let mut kept_count = 0usize;
            let mut dropped_count = 0usize;
            let mut kept_turns: Vec<super::models::ValidatedTurn> =
                Vec::with_capacity(pa.turns.len());
            for turn in pa.turns {
                let dominated = turn
                    .request_id
                    .as_ref()
                    .is_some_and(|rid| existing_rids.contains(rid));
                if dominated {
                    dropped_count += 1;
                } else {
                    kept_count += 1;
                    kept_turns.push(turn);
                }
            }

            parent.quality.total_lines += pa.quality.total_lines;
            parent.quality.valid_turns += kept_count;
            parent.quality.skipped_synthetic += pa.quality.skipped_synthetic;
            parent.quality.skipped_sidechain += pa.quality.skipped_sidechain;
            parent.quality.skipped_invalid += pa.quality.skipped_invalid;
            parent.quality.skipped_parse_error += pa.quality.skipped_parse_error;
            parent.quality.duplicate_turns += pa.quality.duplicate_turns + dropped_count;

            kept_turns.sort_by_key(|t| t.timestamp);
            let (first_ts, last_ts) = time_range(kept_turns.iter().map(|t| &t.timestamp));

            // Loader already resolved the `.meta.json` sidecar; pull
            // agent_type/description straight off the supplied metadata.
            let (agent_type, description) = match pa.meta {
                Some(m) => (m.agent_type, m.description),
                None => (None, None),
            };

            parent.subagents.push(Subagent {
                agent_id: pa.agent_id,
                agent_type,
                description,
                turns: kept_turns,
                first_timestamp: first_ts,
                last_timestamp: last_ts,
                workflow_run_id: pa.workflow_run_id,
            });
        }
    }

    // ── Phase 3: Aggregate plugins / skills (main session turns only) ─────
    for session in sessions.values_mut() {
        session.plugins = aggregate_plugins(&session.turns, calc);
        session.skills = aggregate_skills(&session.turns, calc);
    }

    // ── Phase 4: Recompute time ranges (serial, cheap) ────────────────────
    let mut result: Vec<SessionData> = sessions.into_values().collect();
    result.sort_by_key(|b| std::cmp::Reverse(b.first_timestamp));
    let mut global_min: Option<DateTime<Utc>> = None;
    let mut global_max: Option<DateTime<Utc>> = None;

    for session in &mut result {
        let all_timestamps = session.all_responses();
        let (first_ts, last_ts) = time_range(all_timestamps.iter().map(|t| &t.timestamp));
        session.first_timestamp = first_ts;
        session.last_timestamp = last_ts;

        if let Some(ts) = first_ts {
            global_min = Some(global_min.map_or(ts, |m: DateTime<Utc>| m.min(ts)));
        }
        if let Some(ts) = last_ts {
            global_max = Some(global_max.map_or(ts, |m: DateTime<Utc>| m.max(ts)));
        }
    }

    global_quality.time_range = match (global_min, global_max) {
        (Some(min), Some(max)) => Some((min, max)),
        _ => None,
    };

    Ok((result, global_quality))
}

/// `Session.main_entries` is a parsed `Vec<Entry>` (no path attached). To re-run
/// the analysis pipeline (which is JSONL-stream based and computes file-line
/// quality counters) we need the original file path. cc-session-jsonl stores
/// each `Agent.path` but not the main-session path on `Session`; reconstruct it
/// from the workflow records (`workflows[*].session_id`/`project`) when they
/// exist, otherwise from an agent's parent layout. The fallback is unreachable
/// for any session loaded through `cc_session_jsonl::load_all_sessions` because
/// the scanner only emits sessions for which a main `<uuid>.jsonl` exists in
/// `projects/<project>/<uuid>.jsonl`.
fn derive_main_path(session: &Session) -> Option<std::path::PathBuf> {
    // The loader populates `Session.main_path` for every real session;
    // orphan-only sessions (agents without a main file) get a synthetic path
    // pointing to a non-existent file. Treat non-existent paths as "no main".
    if session.main_path.is_file() {
        Some(session.main_path.clone())
    } else {
        None
    }
}

/// Aggregate per-plugin usage from a main session's turns.
fn aggregate_plugins(
    turns: &[super::models::ValidatedTurn],
    calc: &PricingCalculator,
) -> Vec<PluginUsage> {
    let mut acc: HashMap<String, PluginUsage> = HashMap::new();
    for turn in turns {
        let plugin = match turn.attribution_plugin.as_deref() {
            Some(p) if !p.is_empty() => p,
            _ => continue,
        };
        let cost = calc.calculate_turn_cost(&turn.model, &turn.usage).total;
        let input = turn.usage.input_tokens.unwrap_or(0);
        let output = turn.usage.output_tokens.unwrap_or(0);
        let entry = acc
            .entry(plugin.to_string())
            .or_insert_with(|| PluginUsage {
                plugin: plugin.to_string(),
                turns: 0,
                cost: 0.0,
                input_tokens: 0,
                output_tokens: 0,
            });
        entry.turns += 1;
        entry.cost += cost;
        entry.input_tokens += input;
        entry.output_tokens += output;
    }
    let mut out: Vec<PluginUsage> = acc.into_values().collect();
    out.sort_by(|a, b| a.plugin.cmp(&b.plugin));
    out
}

/// Aggregate per-skill usage from a main session's turns.
fn aggregate_skills(
    turns: &[super::models::ValidatedTurn],
    calc: &PricingCalculator,
) -> Vec<SkillUsage> {
    let mut acc: HashMap<String, SkillUsage> = HashMap::new();
    for turn in turns {
        let skill = match turn.attribution_skill.as_deref() {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        let cost = calc.calculate_turn_cost(&turn.model, &turn.usage).total;
        let input = turn.usage.input_tokens.unwrap_or(0);
        let output = turn.usage.output_tokens.unwrap_or(0);
        let entry = acc.entry(skill.to_string()).or_insert_with(|| SkillUsage {
            skill: skill.to_string(),
            turns: 0,
            cost: 0.0,
            input_tokens: 0,
            output_tokens: 0,
        });
        entry.turns += 1;
        entry.cost += cost;
        entry.input_tokens += input;
        entry.output_tokens += output;
    }
    let mut out: Vec<SkillUsage> = acc.into_values().collect();
    out.sort_by(|a, b| a.skill.cmp(&b.skill));
    out
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::{TokenUsage, ValidatedTurn};
    use std::fs;
    use tempfile::TempDir;

    /// Helper to build a minimal `ValidatedTurn` with optional attribution fields.
    fn turn(
        ts: &str,
        cost_input: u64,
        cost_output: u64,
        attribution: Option<(&str, &str)>,
    ) -> ValidatedTurn {
        ValidatedTurn {
            uuid: format!("u-{ts}"),
            request_id: Some(format!("r-{ts}")),
            timestamp: ts.parse().unwrap(),
            model: "claude-opus-4-6".into(),
            usage: TokenUsage {
                input_tokens: Some(cost_input),
                output_tokens: Some(cost_output),
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                cache_creation: None,
                server_tool_use: None,
                service_tier: None,
                speed: None,
                inference_geo: None,
            },
            stop_reason: None,
            content_types: vec![],
            is_agent: false,
            agent_id: None,
            user_text: None,
            assistant_text: None,
            tool_names: vec![],
            service_tier: None,
            speed: None,
            inference_geo: None,
            tool_error_count: 0,
            git_branch: None,
            attribution_plugin: attribution.map(|(p, _)| p.to_string()),
            attribution_skill: attribution.map(|(_, s)| s.to_string()),
        }
    }

    #[test]
    fn pipeline_plugins_skills_aggregation() {
        let turns = vec![
            turn(
                "2026-05-01T00:00:00Z",
                10,
                20,
                Some(("superpowers", "superpowers:brainstorming")),
            ),
            turn(
                "2026-05-01T00:00:01Z",
                30,
                40,
                Some(("superpowers", "superpowers:brainstorming")),
            ),
            turn("2026-05-01T00:00:02Z", 1, 2, None),
        ];
        let calc = PricingCalculator::new();
        let plugins = aggregate_plugins(&turns, &calc);
        let skills = aggregate_skills(&turns, &calc);

        assert_eq!(plugins.len(), 1, "two plugin turns should fold to one row");
        assert_eq!(plugins[0].plugin, "superpowers");
        assert_eq!(plugins[0].turns, 2);
        assert_eq!(plugins[0].input_tokens, 40);
        assert_eq!(plugins[0].output_tokens, 60);

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].skill, "superpowers:brainstorming");
        assert_eq!(skills[0].turns, 2);

        assert!((plugins[0].cost - skills[0].cost).abs() < 1e-9);
    }

    #[test]
    fn pipeline_plugins_empty_when_no_attribution() {
        let turns = vec![
            turn("2026-05-01T00:00:00Z", 10, 20, None),
            turn("2026-05-01T00:00:01Z", 30, 40, None),
        ];
        let calc = PricingCalculator::new();
        assert!(aggregate_plugins(&turns, &calc).is_empty());
        assert!(aggregate_skills(&turns, &calc).is_empty());
    }

    /// Lay down a fake `~/.claude/projects/<project>/<uuid>.jsonl` plus two
    /// agent files under `subagents/`. Verify the pipeline groups them as
    /// `Subagent` records with correct turn counts, metadata, and aggregation
    /// invariants.
    fn write_fixture_session() -> (TempDir, String) {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project).unwrap();

        let session_uuid = "11111111-2222-3333-4444-555555555555";
        let main_path = project.join(format!("{}.jsonl", session_uuid));

        // Two valid main turns. requestIds r-main-1, r-main-2.
        let main_turn_1 = r#"{"type":"assistant","uuid":"m1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"11111111-2222-3333-4444-555555555555","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":"p1","requestId":"r-main-1"}"#;
        let main_turn_2 = r#"{"type":"assistant","uuid":"m2","timestamp":"2026-05-01T10:01:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":30,"output_tokens":40,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"bye"}]},"sessionId":"11111111-2222-3333-4444-555555555555","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":"p1","requestId":"r-main-2","attributionPlugin":"superpowers","attributionSkill":"superpowers:brainstorming"}"#;
        let main_hook = r#"{"type":"system","subtype":"stop_hook_summary","hookCount":1,"hookInfos":[{"command":"bash hook.sh","durationMs":50}],"hookErrors":[],"preventedContinuation":false,"sessionId":"11111111-2222-3333-4444-555555555555"}"#;
        fs::write(
            &main_path,
            format!("{}\n{}\n{}\n", main_turn_1, main_turn_2, main_hook),
        )
        .unwrap();

        let subagents_dir = project.join(session_uuid).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        // Agent A: 2 unique turns.
        let agent_a_turn_1 = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-05-01T10:02:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":5,"output_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"agent-a-1"}]},"sessionId":"agent-aaa1","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r-agentA-1"}"#;
        let agent_a_turn_2 = r#"{"type":"assistant","uuid":"a2","timestamp":"2026-05-01T10:03:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":11,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"agent-a-2"}]},"sessionId":"agent-aaa1","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r-agentA-2"}"#;
        fs::write(
            subagents_dir.join("agent-aaa1.jsonl"),
            format!("{}\n{}\n", agent_a_turn_1, agent_a_turn_2),
        )
        .unwrap();
        fs::write(
            subagents_dir.join("agent-aaa1.meta.json"),
            r#"{"agentType":"builder","description":"Implement Phase 2"}"#,
        )
        .unwrap();

        // Agent B: 1 turn that *also* appears in the main session by requestId
        // (cross-file dup) and 1 unique turn.
        let agent_b_dup = r#"{"type":"assistant","uuid":"b1","timestamp":"2026-05-01T10:04:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":200,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"dup"}]},"sessionId":"agent-bbb2","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r-main-2"}"#;
        let agent_b_unique = r#"{"type":"assistant","uuid":"b2","timestamp":"2026-05-01T10:05:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":4,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"unique"}]},"sessionId":"agent-bbb2","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r-agentB-2"}"#;
        fs::write(
            subagents_dir.join("agent-bbb2.jsonl"),
            format!("{}\n{}\n", agent_b_dup, agent_b_unique),
        )
        .unwrap();

        (tmp, session_uuid.to_string())
    }

    #[test]
    fn pipeline_subagents_grouping_and_meta_injection() {
        let (tmp, session_uuid) = write_fixture_session();
        let calc = PricingCalculator::new();
        let (sessions, _quality) = load_all(tmp.path(), &calc).unwrap();

        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.session_id, session_uuid);

        // Two subagents, sorted by agent_id (aaa1 < bbb2).
        assert_eq!(s.subagents.len(), 2, "two agent files -> two subagents");
        assert_eq!(s.subagents[0].agent_id, "agent-aaa1");
        assert_eq!(s.subagents[1].agent_id, "agent-bbb2");

        // Agent A: 2 unique turns, .meta.json hydrated.
        assert_eq!(s.subagents[0].turns.len(), 2);
        assert_eq!(s.subagents[0].agent_type.as_deref(), Some("builder"));
        assert_eq!(
            s.subagents[0].description.as_deref(),
            Some("Implement Phase 2")
        );
        assert!(s.subagents[0].first_timestamp.is_some());
        assert!(s.subagents[0].last_timestamp.is_some());

        // Agent B: cross-file dedup drops the duplicate (r-main-2) -> 1 unique turn.
        assert_eq!(
            s.subagents[1].turns.len(),
            1,
            "cross-file dedup should drop the duplicate"
        );
        assert!(s.subagents[1].agent_type.is_none());
        assert!(s.subagents[1].description.is_none());

        assert_eq!(s.turns.len(), 2);

        assert_eq!(s.plugins.len(), 1);
        assert_eq!(s.plugins[0].plugin, "superpowers");
        assert_eq!(s.plugins[0].turns, 1);
        assert_eq!(s.skills.len(), 1);
        assert_eq!(s.skills[0].skill, "superpowers:brainstorming");

        assert_eq!(s.hooks.len(), 1);
        assert_eq!(s.hooks[0].command, "bash hook.sh");
        assert_eq!(s.hooks[0].invocations, 1);
        assert_eq!(s.hooks[0].total_duration_ms, 50);

        assert_eq!(s.total_turn_count(), 2 + 2 + 1);
        assert_eq!(s.agent_turn_count(), 3);
    }

    #[test]
    fn pipeline_aggregation_invariants() {
        let (tmp, _session_uuid) = write_fixture_session();
        let calc = PricingCalculator::new();
        let (sessions, _quality) = load_all(tmp.path(), &calc).unwrap();
        let s = &sessions[0];

        let total_sub_turns: usize = s.subagents.iter().map(|sa| sa.turns.len()).sum();
        assert_eq!(total_sub_turns, s.agent_turn_count());
        assert_eq!(total_sub_turns, 3);

        let attributed_turns = s
            .turns
            .iter()
            .filter(|t| t.attribution_plugin.is_some())
            .count() as u64;
        let plugin_turn_sum: u64 = s.plugins.iter().map(|p| p.turns).sum();
        assert_eq!(plugin_turn_sum, attributed_turns);

        let session_turn_cost: f64 = s
            .turns
            .iter()
            .map(|t| calc.calculate_turn_cost(&t.model, &t.usage).total)
            .sum();
        let plugin_cost: f64 = s.plugins.iter().map(|p| p.cost).sum();
        assert!(
            plugin_cost <= session_turn_cost + 1e-9,
            "plugin cost {plugin_cost} must be <= session turn cost {session_turn_cost}"
        );

        let hook_invocations: u64 = s.hooks.iter().map(|h| h.invocations).sum();
        assert!(
            hook_invocations >= 1,
            "expected at least one hook invocation in fixture"
        );

        for sa in &s.subagents {
            for t in &sa.turns {
                assert!(t.attribution_plugin.is_none());
                assert!(t.attribution_skill.is_none());
            }
        }
    }

    #[test]
    fn pipeline_hooks_aggregation_multi_invocation() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project).unwrap();
        let uuid = "22222222-3333-4444-5555-666666666666";

        let asst = r#"{"type":"assistant","uuid":"m1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"22222222-3333-4444-5555-666666666666","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":"p1","requestId":"r-main-1"}"#;
        let h1 = r#"{"type":"system","subtype":"stop_hook_summary","hookCount":1,"hookInfos":[{"command":"bash run.sh","durationMs":100}],"hookErrors":[],"preventedContinuation":false,"sessionId":"22222222-3333-4444-5555-666666666666"}"#;
        let h2 = r#"{"type":"system","subtype":"stop_hook_summary","hookCount":1,"hookInfos":[{"command":"bash run.sh","durationMs":200}],"hookErrors":[{"msg":"oops"}],"preventedContinuation":false,"sessionId":"22222222-3333-4444-5555-666666666666"}"#;
        let h3 = r#"{"type":"system","subtype":"stop_hook_summary","hookCount":1,"hookInfos":[{"command":"bash run.sh","durationMs":300}],"hookErrors":[],"preventedContinuation":true,"sessionId":"22222222-3333-4444-5555-666666666666"}"#;
        fs::write(
            project.join(format!("{}.jsonl", uuid)),
            format!("{asst}\n{h1}\n{h2}\n{h3}\n"),
        )
        .unwrap();

        let calc = PricingCalculator::new();
        let (sessions, _q) = load_all(tmp.path(), &calc).unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.hooks.len(), 1, "all three invocations share one command");
        let h = &s.hooks[0];
        assert_eq!(h.command, "bash run.sh");
        assert_eq!(h.invocations, 3);
        assert_eq!(h.total_duration_ms, 600);
        assert_eq!(h.error_count, 1);
        assert_eq!(h.prevented_continuation_count, 1);
    }

    #[test]
    fn pipeline_old_session_has_empty_capability_arrays() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project).unwrap();
        let uuid = "33333333-4444-5555-6666-777777777777";
        let asst = r#"{"type":"assistant","uuid":"m1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"33333333-4444-5555-6666-777777777777","version":"2.1.90","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":"p1","requestId":"r-main-1"}"#;
        fs::write(project.join(format!("{}.jsonl", uuid)), format!("{asst}\n")).unwrap();

        let calc = PricingCalculator::new();
        let (sessions, _q) = load_all(tmp.path(), &calc).unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert!(s.plugins.is_empty());
        assert!(s.skills.is_empty());
        assert!(s.hooks.is_empty());
        assert!(s.subagents.is_empty());
    }

    #[test]
    fn loader_marks_orphan_subagent_as_orphan() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        let parent_uuid = "99999999-aaaa-bbbb-cccc-dddddddddddd";
        let subagents_dir = project.join(parent_uuid).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        let agent_turn = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":5,"output_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"orphan-agent"}]},"sessionId":"agent-orphan-1","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r-orphan-1"}"#;
        fs::write(
            subagents_dir.join("agent-orphan-1.jsonl"),
            format!("{}\n", agent_turn),
        )
        .unwrap();

        let calc = PricingCalculator::new();
        let (sessions, quality) = load_all(tmp.path(), &calc).unwrap();

        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.session_id, parent_uuid);
        assert!(s.is_orphan);
        assert_eq!(s.subagents.len(), 1);
        assert_eq!(s.subagents[0].turns.len(), 1);
        assert_eq!(quality.orphan_agents, 1);
    }

    #[test]
    fn loader_marks_normal_session_as_not_orphan() {
        let (tmp, session_uuid) = write_fixture_session();
        let calc = PricingCalculator::new();
        let (sessions, quality) = load_all(tmp.path(), &calc).unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.session_id, session_uuid);
        assert!(!s.is_orphan);
        assert_eq!(quality.orphan_agents, 0);
    }

    /// Group three subagents (2x builder, 1x code-reviewer) plus one with
    /// no agent_type (None) → expect three entries: builder x2, code-reviewer
    /// x1, and "unknown" x1 (data not dropped).
    #[test]
    fn subagent_type_aggregation_groups_by_agent_type() {
        use crate::data::models::{Subagent, ValidatedTurn};

        let calc = PricingCalculator::new();

        let make_agent = |agent_id: &str,
                          agent_type: Option<&str>,
                          description: Option<&str>,
                          turns: usize|
         -> Subagent {
            let mut tlist = Vec::with_capacity(turns);
            for i in 0..turns {
                tlist.push(ValidatedTurn {
                    uuid: format!("{}-{}", agent_id, i),
                    request_id: Some(format!("{}-r-{}", agent_id, i)),
                    timestamp: "2026-05-01T10:00:00Z".parse().unwrap(),
                    model: "claude-opus-4-6".into(),
                    usage: crate::data::models::TokenUsage {
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
                    stop_reason: None,
                    content_types: vec![],
                    is_agent: true,
                    agent_id: Some(agent_id.to_string()),
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
                });
            }
            Subagent {
                agent_id: agent_id.to_string(),
                agent_type: agent_type.map(|s| s.to_string()),
                description: description.map(|s| s.to_string()),
                turns: tlist,
                first_timestamp: None,
                last_timestamp: None,
                workflow_run_id: None,
            }
        };

        let session = SessionData {
            session_id: "s1".into(),
            project: Some("p".into()),
            turns: Vec::new(),
            subagents: vec![
                make_agent("agent-aaa", Some("builder"), Some("task A"), 2),
                make_agent("agent-bbb", Some("builder"), Some("task B"), 3),
                make_agent("agent-ccc", Some("code-reviewer"), Some("review X"), 1),
            ],
            plugins: Vec::new(),
            skills: Vec::new(),
            hooks: Vec::new(),
            first_timestamp: None,
            last_timestamp: None,
            version: None,
            quality: DataQuality::default(),
            metadata: super::SessionMetadata::default(),
            is_orphan: false,
        };

        let aggs = session.subagent_type_aggregates(&calc);
        assert_eq!(aggs.len(), 2);
        assert_eq!(aggs[0].agent_type, "builder");
        assert_eq!(aggs[0].count, 2);
        assert_eq!(aggs[0].total_turns, 5);
        assert_eq!(aggs[0].total_input_tokens, 500);
        assert_eq!(aggs[0].total_output_tokens, 1000);
        assert!(aggs[0].total_cost > 0.0);
        assert_eq!(
            aggs[0].descriptions,
            vec!["task A".to_string(), "task B".to_string()]
        );

        assert_eq!(aggs[1].agent_type, "code-reviewer");
        assert_eq!(aggs[1].count, 1);
    }

    #[test]
    fn subagent_type_aggregation_handles_missing_type() {
        use crate::data::models::{Subagent, ValidatedTurn};

        let calc = PricingCalculator::new();
        let make_turn = |id: &str| ValidatedTurn {
            uuid: id.to_string(),
            request_id: Some(format!("r-{}", id)),
            timestamp: "2026-05-01T10:00:00Z".parse().unwrap(),
            model: "claude-opus-4-6".into(),
            usage: crate::data::models::TokenUsage {
                input_tokens: Some(50),
                output_tokens: Some(50),
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                cache_creation: None,
                server_tool_use: None,
                service_tier: None,
                speed: None,
                inference_geo: None,
            },
            stop_reason: None,
            content_types: vec![],
            is_agent: true,
            agent_id: Some("agent-no-meta".into()),
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
        };

        let session = SessionData {
            session_id: "s1".into(),
            project: Some("p".into()),
            turns: Vec::new(),
            subagents: vec![Subagent {
                agent_id: "agent-no-meta".into(),
                agent_type: None,
                description: None,
                turns: vec![make_turn("t1")],
                first_timestamp: None,
                last_timestamp: None,
                workflow_run_id: None,
            }],
            plugins: Vec::new(),
            skills: Vec::new(),
            hooks: Vec::new(),
            first_timestamp: None,
            last_timestamp: None,
            version: None,
            quality: DataQuality::default(),
            metadata: super::SessionMetadata::default(),
            is_orphan: false,
        };

        let aggs = session.subagent_type_aggregates(&calc);
        assert_eq!(aggs.len(), 1);
        assert_eq!(aggs[0].agent_type, "unknown");
    }

    #[test]
    fn global_totals_include_orphan_sessions() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        let parent_uuid = "88888888-aaaa-bbbb-cccc-dddddddddddd";
        let subagents_dir = project.join(parent_uuid).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();
        let t1 = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":1000,"output_tokens":2000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"x"}]},"sessionId":"agent-orphan-z","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r-orph-1"}"#;
        let t2 = r#"{"type":"assistant","uuid":"a2","timestamp":"2026-05-01T10:01:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3000,"output_tokens":4000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"y"}]},"sessionId":"agent-orphan-z","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r-orph-2"}"#;
        fs::write(
            subagents_dir.join("agent-orphan-z.jsonl"),
            format!("{}\n{}\n", t1, t2),
        )
        .unwrap();

        let calc = PricingCalculator::new();
        let (sessions, quality) = load_all(tmp.path(), &calc).unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].is_orphan);

        let overview =
            crate::analysis::overview::analyze_overview(&sessions, quality, &calc, None);
        assert_eq!(overview.total_sessions, 1);
        assert_eq!(overview.total_turns, 2);
        assert_eq!(overview.total_agent_turns, 2);
        assert!(overview.total_cost > 0.0);
        assert_eq!(overview.total_output_tokens, 6000);
    }

    #[test]
    fn workflow_agent_tokens_enter_parent_total_cost() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project).unwrap();

        let session_uuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let main_path = project.join(format!("{}.jsonl", session_uuid));

        let main_turn = r#"{"type":"assistant","uuid":"m1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee","version":"2.1.159","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":"p1","requestId":"r-main-1"}"#;
        fs::write(&main_path, format!("{}\n", main_turn)).unwrap();

        let wf_dir = project
            .join(session_uuid)
            .join("subagents")
            .join("workflows")
            .join("wf_run123");
        fs::create_dir_all(&wf_dir).unwrap();

        let wf_agent_a = r#"{"type":"assistant","uuid":"wa1","timestamp":"2026-05-01T10:05:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":1000,"output_tokens":2000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"wf-a"}]},"sessionId":"agent-wfa","version":"2.1.159","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r-wf-a-1"}"#;
        let wf_agent_b = r#"{"type":"assistant","uuid":"wb1","timestamp":"2026-05-01T10:06:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3000,"output_tokens":4000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"wf-b"}]},"sessionId":"agent-wfb","version":"2.1.159","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r-wf-b-1"}"#;
        fs::write(wf_dir.join("agent-wfa.jsonl"), format!("{}\n", wf_agent_a)).unwrap();
        fs::write(wf_dir.join("agent-wfb.jsonl"), format!("{}\n", wf_agent_b)).unwrap();
        fs::write(
            wf_dir.join("agent-wfa.meta.json"),
            r#"{"agentType":"researcher","description":"gather facts"}"#,
        )
        .unwrap();

        let calc = PricingCalculator::new();
        let (sessions, _quality) = load_all(tmp.path(), &calc).unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.session_id, session_uuid);

        assert_eq!(s.subagents.len(), 2);
        for sa in &s.subagents {
            assert_eq!(sa.workflow_run_id.as_deref(), Some("wf_run123"));
        }
        let agent_a = s
            .subagents
            .iter()
            .find(|sa| sa.agent_id == "agent-wfa")
            .expect("agent-wfa present");
        assert_eq!(agent_a.agent_type.as_deref(), Some("researcher"));

        assert_eq!(s.agent_turn_count(), 2);
        assert_eq!(s.total_turn_count(), 3);

        let all = s.all_responses();
        let total_output: u64 = all.iter().map(|t| t.usage.output_tokens.unwrap_or(0)).sum();
        assert_eq!(total_output, 20 + 2000 + 4000);
    }
}
