use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use super::models::{
    DataQuality, GlobalDataQuality, HookUsage, PluginUsage, SessionData, SessionFile,
    SessionMetadata, SkillUsage, Subagent,
};
use super::parser::parse_session_file;
use super::scanner::{resolve_agent_parents, scan_claude_home};
use crate::pricing::calculator::PricingCalculator;

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
/// 1. Scans for JSONL files (main sessions + agents)
/// 2. Resolves legacy agent parent relationships
/// 3. Parses main sessions in parallel; groups agent files by `agent_id` into
///    `Subagent` entries on their parent session
/// 4. Aggregates plugins / skills from main turns and hooks from main session
///    `stop_hook_summary` entries (Claude Code 2.1.104+)
/// 5. Computes global time range and quality metrics
///
/// The `PricingCalculator` is used to populate per-plugin / per-skill `cost`
/// fields on the aggregated metadata. Cost / token totals on the underlying
/// turns are untouched.
pub fn load_all(
    claude_home: &Path,
    calc: &PricingCalculator,
) -> Result<(Vec<SessionData>, GlobalDataQuality)> {
    let mut files =
        scan_claude_home(claude_home).context("failed to scan claude home for session files")?;
    resolve_agent_parents(&mut files).context("failed to resolve agent parent sessions")?;
    load_from_files(files, claude_home, calc)
}

/// Parsed result from a single main session file, ready for serial assembly.
struct ParsedMain {
    session_id: String,
    /// Path to the main session JSONL file.
    source_path: PathBuf,
    project: Option<String>,
    turns: Vec<super::models::ValidatedTurn>,
    user_entries: Vec<super::models::ValidatedUserEntry>,
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
    /// The subagent ID, taken verbatim from the agent JSONL file stem
    /// (e.g. `"agent-abc123"`).
    agent_id: String,
    /// Path to the agent file (used to derive `.meta.json` lookups if needed).
    #[allow(dead_code)]
    path: PathBuf,
    turns: Vec<super::models::ValidatedTurn>,
    quality: DataQuality,
    /// The workflow run id (`wf_<runId>`) this agent file belongs to, if it was
    /// discovered under `subagents/workflows/wf_<runId>/`. `None` for ordinary
    /// (legacy / Task-tool) subagent files.
    workflow_run_id: Option<String>,
}

/// Shared loading logic: parse files in parallel, group agent turns into
/// `Subagent` entries, aggregate plugins/skills/hooks, compute time ranges.
fn load_from_files(
    files: Vec<SessionFile>,
    claude_home: &Path,
    calc: &PricingCalculator,
) -> Result<(Vec<SessionData>, GlobalDataQuality)> {
    let (main_files, agent_files): (Vec<_>, Vec<_>) = files.into_iter().partition(|f| !f.is_agent);

    let mut global_quality = GlobalDataQuality {
        total_session_files: main_files.len(),
        total_agent_files: agent_files.len(),
        ..Default::default()
    };

    // ── Phase 1: Parse all main sessions in parallel ──────────────────────
    let parsed_mains: Vec<Result<ParsedMain>> = main_files
        .par_iter()
        .map(|sf| {
            let (turns, user_entries, quality, metadata, hooks) = parse_session_file(&sf.path, false)
                .with_context(|| format!("failed to parse session: {}", sf.path.display()))?;
            let version = extract_version(&sf.path);
            let (first_ts, last_ts) = time_range(turns.iter().map(|t| &t.timestamp));
            Ok(ParsedMain {
                session_id: sf.session_id.clone(),
                source_path: sf.path.clone(),
                project: sf.project.clone(),
                turns,
            user_entries,
                version,
                first_ts,
                last_ts,
                quality,
                metadata,
                hooks,
            })
        })
        .collect();

    // Assemble the sessions map serially (cheap — just moving Vecs)
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
                source_path: pm.source_path,
                project: pm.project,
                turns: pm.turns,
                user_entries: pm.user_entries,
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
    let parsed_agents: Vec<Result<ParsedAgent>> = agent_files
        .par_iter()
        .map(|sf| {
            let (turns, _user_entries, quality, _meta, _hooks) = parse_session_file(&sf.path, true)
                .with_context(|| format!("failed to parse agent file: {}", sf.path.display()))?;
            let target_id = sf
                .parent_session_id
                .clone()
                .unwrap_or_else(|| sf.session_id.clone());
            Ok(ParsedAgent {
                target_id,
                project: sf.project.clone(),
                agent_id: sf.session_id.clone(),
                path: sf.path.clone(),
                turns,
                quality,
                workflow_run_id: sf.workflow_run_id.clone(),
            })
        })
        .collect();

    // Group agent results into a per-parent map: target_id -> Vec<ParsedAgent>
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
                    source_path: PathBuf::new(), // orphan — no main session file
                    project,
                    turns: Vec::new(),
                    user_entries: vec![],
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

        // Load .meta.json sidecars once per parent. Keys are stripped of the
        // "agent-" prefix (matching cc-session-jsonl::load_agent_meta).
        // The first-level loader only scans `subagents/agent-*.meta.json`; merge
        // in the workflow-agent sidecars under `subagents/workflows/wf_*/` so
        // workflow agents also surface their agentType. First-level entries win
        // on key collisions (none expected — agent ids are unique).
        let mut agent_meta_map = crate::data::scanner::load_agent_meta(&target_id, claude_home);
        for (k, v) in crate::data::scanner::load_workflow_agent_meta(&target_id, claude_home) {
            agent_meta_map.entry(k).or_insert(v);
        }

        let parent = sessions.get_mut(&target_id).unwrap();
        let existing_rids = request_id_set(&parent.turns);

        // Build subagents in deterministic order: sorted by agent_id.
        let mut agents = agents;
        agents.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));

        for pa in agents {
            // Cross-file dedup: drop turns whose requestId already appears in
            // the main session (Claude Code writes agent responses to both
            // the agent file and the main file).
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

            // Accumulate quality into parent's quality (same accounting the
            // legacy merge_agent_turns helper used).
            parent.quality.total_lines += pa.quality.total_lines;
            parent.quality.valid_turns += kept_count;
            parent.quality.skipped_synthetic += pa.quality.skipped_synthetic;
            parent.quality.skipped_sidechain += pa.quality.skipped_sidechain;
            parent.quality.skipped_invalid += pa.quality.skipped_invalid;
            parent.quality.skipped_parse_error += pa.quality.skipped_parse_error;
            parent.quality.duplicate_turns += pa.quality.duplicate_turns + dropped_count;

            // Compute per-subagent time range and sort turns by timestamp.
            kept_turns.sort_by_key(|t| t.timestamp);
            let (first_ts, last_ts) = time_range(kept_turns.iter().map(|t| &t.timestamp));

            // .meta.json key is the agent_id WITHOUT the "agent-" prefix.
            let meta_key = pa
                .agent_id
                .strip_prefix("agent-")
                .unwrap_or(&pa.agent_id)
                .to_string();
            let (agent_type, description) = agent_meta_map
                .get(&meta_key)
                .map(|(t, d)| (Some(t.clone()), Some(d.clone())))
                .unwrap_or((None, None));

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
    // Sort by time descending (most recent first) for deterministic output
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

/// Aggregate per-plugin usage from a main session's turns.
///
/// Groups turns by `attribution_plugin` (skipping `None`). Output Vec is
/// sorted by plugin name for deterministic JSON output.
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
///
/// Mirror of `aggregate_plugins` but keyed on `attribution_skill`.
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
            parent_uuid: None,
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
        // Three turns, two share a plugin, one is unattributed.
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

        // Costs equal across plugin/skill rollups because both fields are set on
        // the same two turns.
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
        // The second carries attributionPlugin/Skill.
        let main_turn_1 = r#"{"type":"assistant","uuid":"m1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"11111111-2222-3333-4444-555555555555","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r-main-1"}"#;
        let main_turn_2 = r#"{"type":"assistant","uuid":"m2","timestamp":"2026-05-01T10:01:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":30,"output_tokens":40,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"bye"}]},"sessionId":"11111111-2222-3333-4444-555555555555","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r-main-2","attributionPlugin":"superpowers","attributionSkill":"superpowers:brainstorming"}"#;
        // One stop_hook_summary system entry.
        let main_hook = r#"{"type":"system","subtype":"stop_hook_summary","hookCount":1,"hookInfos":[{"command":"bash hook.sh","durationMs":50}],"hookErrors":[],"preventedContinuation":false,"sessionId":"11111111-2222-3333-4444-555555555555"}"#;
        fs::write(
            &main_path,
            format!("{}\n{}\n{}\n", main_turn_1, main_turn_2, main_hook),
        )
        .unwrap();

        // Subagents directory with two agent files and one .meta.json sidecar.
        let subagents_dir = project.join(session_uuid).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        // Agent A: 2 unique turns. r-agentA-1, r-agentA-2.
        let agent_a_turn_1 = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-05-01T10:02:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":5,"output_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"agent-a-1"}]},"sessionId":"agent-aaa1","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-agentA-1"}"#;
        let agent_a_turn_2 = r#"{"type":"assistant","uuid":"a2","timestamp":"2026-05-01T10:03:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":11,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"agent-a-2"}]},"sessionId":"agent-aaa1","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-agentA-2"}"#;
        fs::write(
            subagents_dir.join("agent-aaa1.jsonl"),
            format!("{}\n{}\n", agent_a_turn_1, agent_a_turn_2),
        )
        .unwrap();
        // Sidecar — note that the .meta.json key strips the "agent-" prefix.
        fs::write(
            subagents_dir.join("agent-aaa1.meta.json"),
            r#"{"agentType":"builder","description":"Implement Phase 2"}"#,
        )
        .unwrap();

        // Agent B: 1 turn that *also* appears in the main session by requestId
        // (cross-file dup) and 1 unique turn.
        let agent_b_dup = r#"{"type":"assistant","uuid":"b1","timestamp":"2026-05-01T10:04:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":200,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"dup"}]},"sessionId":"agent-bbb2","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-main-2"}"#; // same rid as main_turn_2
        let agent_b_unique = r#"{"type":"assistant","uuid":"b2","timestamp":"2026-05-01T10:05:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":4,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"unique"}]},"sessionId":"agent-bbb2","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-agentB-2"}"#;
        fs::write(
            subagents_dir.join("agent-bbb2.jsonl"),
            format!("{}\n{}\n", agent_b_dup, agent_b_unique),
        )
        .unwrap();
        // No meta.json for agent B (verify None fallback).

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
        // No .meta.json -> agent_type/description are None.
        assert_eq!(
            s.subagents[1].turns.len(),
            1,
            "cross-file dedup should drop the duplicate"
        );
        assert!(s.subagents[1].agent_type.is_none());
        assert!(s.subagents[1].description.is_none());

        // Main session has 2 turns.
        assert_eq!(s.turns.len(), 2);

        // Plugins / skills aggregated from main turns only (1 turn carries attribution).
        assert_eq!(s.plugins.len(), 1);
        assert_eq!(s.plugins[0].plugin, "superpowers");
        assert_eq!(s.plugins[0].turns, 1);
        assert_eq!(s.skills.len(), 1);
        assert_eq!(s.skills[0].skill, "superpowers:brainstorming");

        // Hooks aggregated from main session.
        assert_eq!(s.hooks.len(), 1);
        assert_eq!(s.hooks[0].command, "bash hook.sh");
        assert_eq!(s.hooks[0].invocations, 1);
        assert_eq!(s.hooks[0].total_duration_ms, 50);
        assert_eq!(s.hooks[0].error_count, 0);
        assert_eq!(s.hooks[0].prevented_continuation_count, 0);

        // total_turn_count / agent_turn_count derive from nested subagents.
        assert_eq!(s.total_turn_count(), 2 + 2 + 1); // main + agent-A + agent-B(deduped)
        assert_eq!(s.agent_turn_count(), 3);
    }

    #[test]
    fn pipeline_aggregation_invariants() {
        // The 5 spec invariants (section 2.6) bundled into one comprehensive test.
        let (tmp, _session_uuid) = write_fixture_session();
        let calc = PricingCalculator::new();
        let (sessions, _quality) = load_all(tmp.path(), &calc).unwrap();
        let s = &sessions[0];

        // (1) Reorganization lossless: sum(subagent.turns) equals the number we
        // accept after cross-file dedup (2 from agent-A + 1 unique from agent-B).
        let total_sub_turns: usize = s.subagents.iter().map(|sa| sa.turns.len()).sum();
        assert_eq!(total_sub_turns, s.agent_turn_count());
        assert_eq!(total_sub_turns, 3);

        // (2) Plugin aggregation no-miss/no-double: sum(plugins.turns) equals
        // number of main turns with attribution_plugin set.
        let attributed_turns = s
            .turns
            .iter()
            .filter(|t| t.attribution_plugin.is_some())
            .count() as u64;
        let plugin_turn_sum: u64 = s.plugins.iter().map(|p| p.turns).sum();
        assert_eq!(plugin_turn_sum, attributed_turns);

        // (3) Upper bound: plugin cost <= session main turn cost.
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

        // (4) Hook total: every hookInfos[] element in every stop_hook_summary
        // SystemEntry is counted. Because hooks are grouped by command, the
        // total invocations sum equals sum(hookInfos[].len()) across all
        // SystemEntries — which on observed 2.1.104+ data also equals
        // sum(SystemEntry.hookCount). Asserting a literal count here would
        // bind the test to a single SystemEntry's fixture; the parser-side
        // `debug_assert_eq!` (parser.rs) already guards the hookCount ==
        // hookInfos.len() invariant. Here we only assert the lower bound.
        let hook_invocations: u64 = s.hooks.iter().map(|h| h.invocations).sum();
        assert!(
            hook_invocations >= 1,
            "expected at least one hook invocation in fixture"
        );

        // (5) Hypothesis regression: no subagent turn carries attribution.
        for sa in &s.subagents {
            for t in &sa.turns {
                assert!(
                    t.attribution_plugin.is_none(),
                    "subagent turn unexpectedly has attributionPlugin"
                );
                assert!(
                    t.attribution_skill.is_none(),
                    "subagent turn unexpectedly has attributionSkill"
                );
            }
        }
    }

    #[test]
    fn pipeline_hooks_aggregation_multi_invocation() {
        // Build a fixture with the SAME command running 3 times, where one
        // invocation has errors and another prevents continuation.
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project).unwrap();
        let uuid = "22222222-3333-4444-5555-666666666666";

        // One assistant turn so the session has some content (otherwise the
        // session has no first_timestamp).
        let asst = r#"{"type":"assistant","uuid":"m1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"22222222-3333-4444-5555-666666666666","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r-main-1"}"#;
        // Three stop_hook_summary entries: same command, varying flags.
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
        // A session JSONL with NO attribution fields and NO stop_hook_summary
        // entries should produce empty plugins/skills/hooks Vecs (not None).
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project).unwrap();
        let uuid = "33333333-4444-5555-6666-777777777777";
        let asst = r#"{"type":"assistant","uuid":"m1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"33333333-4444-5555-6666-777777777777","version":"2.1.90","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r-main-1"}"#;
        fs::write(project.join(format!("{}.jsonl", uuid)), format!("{asst}\n")).unwrap();

        let calc = PricingCalculator::new();
        let (sessions, _q) = load_all(tmp.path(), &calc).unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert!(
            s.plugins.is_empty(),
            "old session must produce empty plugins Vec"
        );
        assert!(
            s.skills.is_empty(),
            "old session must produce empty skills Vec"
        );
        assert!(
            s.hooks.is_empty(),
            "old session must produce empty hooks Vec"
        );
        assert!(
            s.subagents.is_empty(),
            "session without agent files must produce empty subagents Vec"
        );
    }

    /// A subagent jsonl exists at `<proj>/<uuid>/subagents/agent-X.jsonl`
    /// but the parent main session jsonl `<proj>/<uuid>.jsonl` was deleted.
    /// The loader still picks up the subagent (data is preserved), but flags
    /// the synthesized parent SessionData as orphan.
    #[test]
    fn loader_marks_orphan_subagent_as_orphan() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        let parent_uuid = "99999999-aaaa-bbbb-cccc-dddddddddddd";
        let subagents_dir = project.join(parent_uuid).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();
        // Note: NO `<project>/<parent_uuid>.jsonl` — the parent main session
        // was deleted by the user.

        let agent_turn = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":5,"output_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"orphan-agent"}]},"sessionId":"agent-orphan-1","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-orphan-1"}"#;
        fs::write(
            subagents_dir.join("agent-orphan-1.jsonl"),
            format!("{}\n", agent_turn),
        )
        .unwrap();

        let calc = PricingCalculator::new();
        let (sessions, quality) = load_all(tmp.path(), &calc).unwrap();

        assert_eq!(
            sessions.len(),
            1,
            "loader should reconstruct an orphan parent session"
        );
        let s = &sessions[0];
        assert_eq!(s.session_id, parent_uuid);
        assert!(s.is_orphan, "synthesized parent must be flagged as orphan");
        // The subagent's turn is preserved.
        assert_eq!(s.subagents.len(), 1);
        assert_eq!(s.subagents[0].turns.len(), 1);
        // Quality counter also records the orphan.
        assert_eq!(quality.orphan_agents, 1);
    }

    /// A normal session with its main `<uuid>.jsonl` present *and* subagent
    /// files under `<uuid>/subagents/` must NOT be flagged as orphan.
    #[test]
    fn loader_marks_normal_session_as_not_orphan() {
        let (tmp, session_uuid) = write_fixture_session();
        let calc = PricingCalculator::new();
        let (sessions, quality) = load_all(tmp.path(), &calc).unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.session_id, session_uuid);
        assert!(
            !s.is_orphan,
            "session with parent main jsonl present must not be orphan"
        );
        // No orphans counted at the global level either.
        assert_eq!(quality.orphan_agents, 0);
    }

    /// Group three subagents (2x builder, 1x code-reviewer) plus one with
    /// no agent_type (None) → expect three entries: builder x2, code-reviewer
    /// x1, and "unknown" x1 (data not dropped).
    #[test]
    fn subagent_type_aggregation_groups_by_agent_type() {
        use crate::data::models::{Subagent, ValidatedTurn};

        let calc = PricingCalculator::new();

        // Helper to build a synthetic Subagent with N turns of given token counts.
        let make_agent = |agent_id: &str,
                          agent_type: Option<&str>,
                          description: Option<&str>,
                          turns: usize|
         -> Subagent {
            let mut tlist = Vec::with_capacity(turns);
            for i in 0..turns {
                tlist.push(ValidatedTurn {
                    uuid: format!("{}-{}", agent_id, i),
            parent_uuid: None,
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
            source_path: std::path::PathBuf::from("/tmp/test.jsonl"),
            project: Some("p".into()),
            turns: Vec::new(),
            user_entries: vec![],
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
        // Sorted alphabetically: builder, code-reviewer.
        assert_eq!(aggs.len(), 2);
        assert_eq!(aggs[0].agent_type, "builder");
        assert_eq!(aggs[0].count, 2);
        assert_eq!(aggs[0].total_turns, 5); // 2 + 3
        assert_eq!(aggs[0].total_input_tokens, 500); // (2+3) * 100
        assert_eq!(aggs[0].total_output_tokens, 1000); // (2+3) * 200
        assert!(aggs[0].total_cost > 0.0);
        assert_eq!(
            aggs[0].descriptions,
            vec!["task A".to_string(), "task B".to_string()]
        );

        assert_eq!(aggs[1].agent_type, "code-reviewer");
        assert_eq!(aggs[1].count, 1);
        assert_eq!(aggs[1].total_turns, 1);
        assert_eq!(aggs[1].descriptions, vec!["review X".to_string()]);
    }

    /// A subagent with `agent_type = None` must be grouped under the literal
    /// "unknown" key, never silently dropped.
    #[test]
    fn subagent_type_aggregation_handles_missing_type() {
        use crate::data::models::{Subagent, ValidatedTurn};

        let calc = PricingCalculator::new();
        let make_turn = |id: &str| ValidatedTurn {
            uuid: id.to_string(),
            parent_uuid: None,
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
            source_path: std::path::PathBuf::from("/tmp/test.jsonl"),
            project: Some("p".into()),
            turns: Vec::new(),
            user_entries: vec![],
            subagents: vec![Subagent {
                agent_id: "agent-no-meta".into(),
                agent_type: None, // .meta.json missing
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
        assert_eq!(
            aggs.len(),
            1,
            "agent_type=None should still produce one aggregate, not drop the data"
        );
        assert_eq!(aggs[0].agent_type, "unknown");
        assert_eq!(aggs[0].count, 1);
        assert_eq!(aggs[0].total_turns, 1);
    }

    /// Orphan sessions must contribute to the *global* overview totals
    /// (cost / turns / tokens). The orphan flag is for display only.
    #[test]
    fn global_totals_include_orphan_sessions() {
        // Same fixture as the orphan-flag test, but verify overview math.
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        let parent_uuid = "88888888-aaaa-bbbb-cccc-dddddddddddd";
        let subagents_dir = project.join(parent_uuid).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();
        // Two turns under the orphan parent.
        let t1 = r#"{"type":"assistant","uuid":"a1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":1000,"output_tokens":2000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"x"}]},"sessionId":"agent-orphan-z","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-orph-1"}"#;
        let t2 = r#"{"type":"assistant","uuid":"a2","timestamp":"2026-05-01T10:01:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3000,"output_tokens":4000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"y"}]},"sessionId":"agent-orphan-z","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-orph-2"}"#;
        fs::write(
            subagents_dir.join("agent-orphan-z.jsonl"),
            format!("{}\n{}\n", t1, t2),
        )
        .unwrap();

        let calc = PricingCalculator::new();
        let (sessions, quality) = load_all(tmp.path(), &calc).unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].is_orphan);

        // Now drive the overview analysis and ensure totals reflect the
        // orphan session's data (cost > 0, agent turns counted).
        let overview = crate::analysis::overview::analyze_overview(&sessions, quality, &calc, None);
        assert_eq!(overview.total_sessions, 1);
        assert_eq!(overview.total_turns, 2);
        assert_eq!(overview.total_agent_turns, 2);
        assert!(
            overview.total_cost > 0.0,
            "orphan session's cost must flow into total_cost"
        );
        // Output tokens accumulated from the two orphan turns.
        assert_eq!(overview.total_output_tokens, 6000);
    }

    /// Task 0: a workflow agent transcript under
    /// `<proj>/<uuid>/subagents/workflows/wf_<runId>/agent-*.jsonl` must be
    /// discovered by the scanner (Type 4), parsed as an agent, grouped under the
    /// correct parent session, tagged with its `workflow_run_id`, and have its
    /// tokens/cost flow into the parent's `all_responses()` total. The workflow
    /// turns carry `isSidechain=true` (like all agent files) and must survive
    /// the sidechain filter (is_agent=true) and cross-file dedup (their
    /// requestIds do not appear in the main jsonl).
    #[test]
    fn workflow_agent_tokens_enter_parent_total_cost() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project).unwrap();

        let session_uuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let main_path = project.join(format!("{}.jsonl", session_uuid));

        // One ordinary main turn (requestId r-main-1).
        let main_turn = r#"{"type":"assistant","uuid":"m1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee","version":"2.1.159","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r-main-1"}"#;
        fs::write(&main_path, format!("{}\n", main_turn)).unwrap();

        // Workflow run directory: <uuid>/subagents/workflows/wf_run123/
        let wf_dir = project
            .join(session_uuid)
            .join("subagents")
            .join("workflows")
            .join("wf_run123");
        fs::create_dir_all(&wf_dir).unwrap();

        // Two workflow agent transcripts, each with one sidechain assistant turn
        // carrying real usage. Unique requestIds (not present in the main file).
        let wf_agent_a = r#"{"type":"assistant","uuid":"wa1","timestamp":"2026-05-01T10:05:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":1000,"output_tokens":2000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"wf-a"}]},"sessionId":"agent-wfa","version":"2.1.159","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-wf-a-1"}"#;
        let wf_agent_b = r#"{"type":"assistant","uuid":"wb1","timestamp":"2026-05-01T10:06:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3000,"output_tokens":4000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"wf-b"}]},"sessionId":"agent-wfb","version":"2.1.159","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-wf-b-1"}"#;
        fs::write(wf_dir.join("agent-wfa.jsonl"), format!("{}\n", wf_agent_a)).unwrap();
        fs::write(wf_dir.join("agent-wfb.jsonl"), format!("{}\n", wf_agent_b)).unwrap();
        // Meta sidecar for agent A only (verify workflow meta hydration).
        fs::write(
            wf_dir.join("agent-wfa.meta.json"),
            r#"{"agentType":"researcher","description":"gather facts"}"#,
        )
        .unwrap();

        let calc = PricingCalculator::new();

        // Baseline cost WITHOUT the workflow agents (main turn only): compute
        // directly so we can assert the delta the workflow turns contribute.
        let main_only_cost = {
            let usage = TokenUsage {
                input_tokens: Some(10),
                output_tokens: Some(20),
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                cache_creation: None,
                server_tool_use: None,
                service_tier: None,
                speed: None,
                inference_geo: None,
            };
            calc.calculate_turn_cost("claude-opus-4-6", &usage).total
        };

        let (sessions, _quality) = load_all(tmp.path(), &calc).unwrap();
        assert_eq!(sessions.len(), 1, "one parent session");
        let s = &sessions[0];
        assert_eq!(s.session_id, session_uuid);

        // The two workflow agents were grouped under the parent session.
        assert_eq!(
            s.subagents.len(),
            2,
            "two workflow agent files -> two subagents"
        );
        for sa in &s.subagents {
            assert_eq!(
                sa.workflow_run_id.as_deref(),
                Some("wf_run123"),
                "workflow subagent must carry its run id"
            );
        }
        // Workflow meta sidecar hydrated agent A's type.
        let agent_a = s
            .subagents
            .iter()
            .find(|sa| sa.agent_id == "agent-wfa")
            .expect("agent-wfa present");
        assert_eq!(agent_a.agent_type.as_deref(), Some("researcher"));

        // The workflow turns are present (not dropped by sidechain/dedup).
        assert_eq!(s.agent_turn_count(), 2, "both workflow turns kept");
        assert_eq!(s.total_turn_count(), 3, "1 main + 2 workflow");

        // all_responses() includes main + workflow turns.
        let all = s.all_responses();
        assert_eq!(all.len(), 3);

        // Total cost over all_responses() includes the workflow turns: it must
        // exceed the main-only cost by exactly the two workflow turns' cost.
        let total_cost: f64 = all
            .iter()
            .map(|t| calc.calculate_turn_cost(&t.model, &t.usage).total)
            .sum();
        let wf_a_cost = {
            let usage = TokenUsage {
                input_tokens: Some(1000),
                output_tokens: Some(2000),
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                cache_creation: None,
                server_tool_use: None,
                service_tier: None,
                speed: None,
                inference_geo: None,
            };
            calc.calculate_turn_cost("claude-opus-4-6", &usage).total
        };
        let wf_b_cost = {
            let usage = TokenUsage {
                input_tokens: Some(3000),
                output_tokens: Some(4000),
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                cache_creation: None,
                server_tool_use: None,
                service_tier: None,
                speed: None,
                inference_geo: None,
            };
            calc.calculate_turn_cost("claude-opus-4-6", &usage).total
        };
        assert!(
            (total_cost - (main_only_cost + wf_a_cost + wf_b_cost)).abs() < 1e-9,
            "total {total_cost} must equal main {main_only_cost} + wf_a {wf_a_cost} + wf_b {wf_b_cost}"
        );
        assert!(
            total_cost > main_only_cost,
            "workflow tokens must increase total cost above main-only baseline"
        );

        // Workflow output tokens (2000 + 4000) are in the total.
        let total_output: u64 = all.iter().map(|t| t.usage.output_tokens.unwrap_or(0)).sum();
        assert_eq!(total_output, 20 + 2000 + 4000);
    }

    #[test]
    fn pipeline_subagents_many() {
        // Construct a fixture with N=10 distinct subagent files to verify the
        // grouping scales correctly (spec mentions 69-subagent sessions).
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project).unwrap();
        let uuid = "44444444-5555-6666-7777-888888888888";

        // Main session with one turn.
        let main_turn = r#"{"type":"assistant","uuid":"m1","timestamp":"2026-05-01T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"44444444-5555-6666-7777-888888888888","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r-main-1"}"#;
        fs::write(
            project.join(format!("{}.jsonl", uuid)),
            format!("{}\n", main_turn),
        )
        .unwrap();

        let subagents_dir = project.join(uuid).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        for i in 0..10 {
            // Each agent file has 2 turns, unique request_ids.
            let line1 = format!(
                r#"{{"type":"assistant","uuid":"a{i}-1","timestamp":"2026-05-01T10:0{i}:00Z","message":{{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{{"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}},"content":[{{"type":"text","text":"a"}}]}},"sessionId":"agent-id{i:03}","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-{i}-1"}}"#
            );
            let line2 = format!(
                r#"{{"type":"assistant","uuid":"a{i}-2","timestamp":"2026-05-01T10:0{i}:01Z","message":{{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{{"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}},"content":[{{"type":"text","text":"a"}}]}},"sessionId":"agent-id{i:03}","version":"2.1.140","cwd":"/tmp","gitBranch":"main","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r-{i}-2"}}"#
            );
            fs::write(
                subagents_dir.join(format!("agent-id{i:03}.jsonl")),
                format!("{line1}\n{line2}\n"),
            )
            .unwrap();
        }

        let calc = PricingCalculator::new();
        let (sessions, _q) = load_all(tmp.path(), &calc).unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.subagents.len(), 10, "all 10 agent files become subagents");
        for sa in &s.subagents {
            assert_eq!(sa.turns.len(), 2);
        }
        // Subagent ordering: ascending by agent_id (deterministic).
        let ids: Vec<&str> = s.subagents.iter().map(|sa| sa.agent_id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);

        // Total turn count: 1 main + 20 subagent.
        assert_eq!(s.total_turn_count(), 21);
        assert_eq!(s.agent_turn_count(), 20);
    }
}
