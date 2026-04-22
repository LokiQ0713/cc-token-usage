use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::models::{DataQuality, GlobalDataQuality, SessionData, SessionFile, SessionMetadata};
use super::parser::parse_session_file;
use super::scanner::{resolve_agent_parents, scan_claude_home};

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

/// Merge agent turns into a parent session, deduplicating by requestId.
///
/// Claude Code writes agent responses to both the main session file and the
/// agent file. We keep the main session's copy and skip duplicates from agents.
fn merge_agent_turns(
    parent: &mut SessionData,
    agent_turns: Vec<super::models::ValidatedTurn>,
    quality: &DataQuality,
) {
    let existing_rids = request_id_set(&parent.turns);
    let before = parent.agent_turns.len();

    for turn in agent_turns {
        let dominated = turn
            .request_id
            .as_ref()
            .is_some_and(|rid| existing_rids.contains(rid));
        if !dominated {
            parent.agent_turns.push(turn);
        }
    }

    let added = parent.agent_turns.len() - before;
    let deduped = quality.valid_turns.saturating_sub(added);

    // Accumulate agent quality into parent's quality
    parent.quality.total_lines += quality.total_lines;
    parent.quality.valid_turns += added;
    parent.quality.skipped_synthetic += quality.skipped_synthetic;
    parent.quality.skipped_sidechain += quality.skipped_sidechain;
    parent.quality.skipped_invalid += quality.skipped_invalid;
    parent.quality.skipped_parse_error += quality.skipped_parse_error;
    parent.quality.duplicate_turns += quality.duplicate_turns + deduped;
}

/// Load all session data from a Claude home directory.
///
/// 1. Scans for JSONL files (main sessions + agents)
/// 2. Resolves legacy agent parent relationships
/// 3. Parses main sessions first, then merges agent turns into their parents
/// 4. Computes global time range and quality metrics
pub fn load_all(claude_home: &Path) -> Result<(Vec<SessionData>, GlobalDataQuality)> {
    let mut files =
        scan_claude_home(claude_home).context("failed to scan claude home for session files")?;
    resolve_agent_parents(&mut files).context("failed to resolve agent parent sessions")?;
    load_from_files(files)
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
}

/// Parsed result from a single agent file, ready for serial merge.
struct ParsedAgent {
    target_id: String,
    project: Option<String>,
    turns: Vec<super::models::ValidatedTurn>,
    quality: DataQuality,
}

/// Shared loading logic: partition files, parse sessions in parallel, merge agents, compute time ranges.
fn load_from_files(files: Vec<SessionFile>) -> Result<(Vec<SessionData>, GlobalDataQuality)> {
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
            let (turns, quality, metadata) = parse_session_file(&sf.path, false)
                .with_context(|| format!("failed to parse session: {}", sf.path.display()))?;
            let version = extract_version(&sf.path);
            let (first_ts, last_ts) = time_range(turns.iter().map(|t| &t.timestamp));
            Ok(ParsedMain {
                session_id: sf.session_id.clone(),
                project: sf.project.clone(),
                turns,
                version,
                first_ts,
                last_ts,
                quality,
                metadata,
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
                project: pm.project,
                turns: pm.turns,
                agent_turns: Vec::new(),
                first_timestamp: pm.first_ts,
                last_timestamp: pm.last_ts,
                version: pm.version,
                quality: pm.quality,
                metadata: pm.metadata,
            },
        );
    }

    // ── Phase 2: Parse all agent files in parallel ────────────────────────
    let parsed_agents: Vec<Result<ParsedAgent>> = agent_files
        .par_iter()
        .map(|sf| {
            let (turns, quality, _agent_meta) = parse_session_file(&sf.path, true)
                .with_context(|| format!("failed to parse agent file: {}", sf.path.display()))?;
            let target_id = sf
                .parent_session_id
                .clone()
                .unwrap_or_else(|| sf.session_id.clone());
            Ok(ParsedAgent {
                target_id,
                project: sf.project.clone(),
                turns,
                quality,
            })
        })
        .collect();

    // Merge agent results into parent sessions serially (needs mutable HashMap)
    for result in parsed_agents {
        let pa = result?;

        global_quality.total_valid_turns += pa.quality.valid_turns;
        global_quality.total_skipped += pa.quality.skipped_synthetic
            + pa.quality.skipped_sidechain
            + pa.quality.skipped_invalid
            + pa.quality.skipped_parse_error;

        if !sessions.contains_key(&pa.target_id) {
            let project = pa.project.or_else(|| Some("(orphan)".to_string()));
            sessions.insert(
                pa.target_id.clone(),
                SessionData {
                    session_id: pa.target_id.clone(),
                    project,
                    turns: Vec::new(),
                    agent_turns: Vec::new(),
                    first_timestamp: None,
                    last_timestamp: None,
                    version: None,
                    quality: DataQuality::default(),
                    metadata: SessionMetadata::default(),
                },
            );
            global_quality.orphan_agents += 1;
        }

        let parent = sessions.get_mut(&pa.target_id).unwrap();
        merge_agent_turns(parent, pa.turns, &pa.quality);
    }

    // ── Phase 3: Recompute time ranges (serial, cheap) ────────────────────
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
