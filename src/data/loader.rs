use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::models::{DataQuality, GlobalDataQuality, SessionData};
use super::parser::parse_session_file;
use super::scanner::{resolve_agent_parents, scan_claude_home, scan_projects_dir};

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
fn merge_agent_turns(parent: &mut SessionData, agent_turns: Vec<super::models::ValidatedTurn>, quality: &DataQuality) {
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
    // Step 1: Scan for all session files
    let mut files = scan_claude_home(claude_home)
        .context("failed to scan claude home for session files")?;

    // Step 2: Resolve legacy agent parent relationships
    resolve_agent_parents(&mut files)
        .context("failed to resolve agent parent sessions")?;

    // Partition into main sessions and agent files
    let (main_files, agent_files): (Vec<_>, Vec<_>) =
        files.into_iter().partition(|f| !f.is_agent);

    let mut global_quality = GlobalDataQuality {
        total_session_files: main_files.len(),
        total_agent_files: agent_files.len(),
        ..Default::default()
    };

    // Step 3a: Process all main sessions
    let mut sessions: HashMap<String, SessionData> = HashMap::new();

    for sf in &main_files {
        let (turns, quality) = parse_session_file(&sf.file_path, false)
            .with_context(|| format!("failed to parse session: {}", sf.file_path.display()))?;

        let version = extract_version(&sf.file_path);

        let (first_ts, last_ts) = time_range(turns.iter().map(|t| &t.timestamp));

        global_quality.total_valid_turns += quality.valid_turns;
        global_quality.total_skipped +=
            quality.skipped_synthetic + quality.skipped_sidechain + quality.skipped_invalid + quality.skipped_parse_error;

        let session = SessionData {
            session_id: sf.session_id.clone(),
            project: sf.project.clone(),
            turns,
            agent_turns: Vec::new(),
            first_timestamp: first_ts,
            last_timestamp: last_ts,
            version,
            quality,
        };

        sessions.insert(sf.session_id.clone(), session);
    }

    // Step 3b: Process agent files and merge into parent sessions
    for sf in &agent_files {
        let (agent_turns, quality) = parse_session_file(&sf.file_path, true)
            .with_context(|| format!("failed to parse agent file: {}", sf.file_path.display()))?;

        global_quality.total_valid_turns += quality.valid_turns;
        global_quality.total_skipped +=
            quality.skipped_synthetic + quality.skipped_sidechain + quality.skipped_invalid + quality.skipped_parse_error;

        let target_id = match &sf.parent_session_id {
            Some(parent_id) => {
                if !sessions.contains_key(parent_id) {
                    let project = sf.project.clone().or_else(|| Some("(orphan)".to_string()));
                    sessions.insert(parent_id.clone(), SessionData {
                        session_id: parent_id.clone(),
                        project,
                        turns: Vec::new(),
                        agent_turns: Vec::new(),
                        first_timestamp: None,
                        last_timestamp: None,
                        version: None,
                        quality: DataQuality::default(),
                    });
                    global_quality.orphan_agents += 1;
                }
                parent_id.clone()
            }
            None => {
                let virtual_id = sf.session_id.clone();
                if !sessions.contains_key(&virtual_id) {
                    let project = sf.project.clone().or_else(|| Some("(orphan)".to_string()));
                    sessions.insert(virtual_id.clone(), SessionData {
                        session_id: virtual_id.clone(),
                        project,
                        turns: Vec::new(),
                        agent_turns: Vec::new(),
                        first_timestamp: None,
                        last_timestamp: None,
                        version: None,
                        quality: DataQuality::default(),
                    });
                    global_quality.orphan_agents += 1;
                }
                virtual_id
            }
        };

        let parent = sessions.get_mut(&target_id).unwrap();
        merge_agent_turns(parent, agent_turns, &quality);
    }

    // Step 4: Recompute time ranges to include agent turns, and collect results
    let mut result: Vec<SessionData> = sessions.into_values().collect();

    // Compute global time range
    let mut global_min: Option<DateTime<Utc>> = None;
    let mut global_max: Option<DateTime<Utc>> = None;

    for session in &mut result {
        // Recompute session time range including agent turns
        let all_timestamps = session
            .turns
            .iter()
            .chain(session.agent_turns.iter())
            .map(|t| &t.timestamp);
        let (first_ts, last_ts) = time_range(all_timestamps);
        session.first_timestamp = first_ts;
        session.last_timestamp = last_ts;

        // Update global range
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

/// Load all session data from a projects directory directly.
///
/// Unlike `load_all` which expects a Claude home directory (and appends `projects/`),
/// this function takes the projects directory itself. Useful for loading data from
/// archive directories like `~/.config/superpowers/conversation-archive/`.
pub fn load_from_projects_dir(projects_dir: &Path) -> Result<(Vec<SessionData>, GlobalDataQuality)> {
    // Step 1: Scan for all session files directly from projects_dir
    let mut files = scan_projects_dir(projects_dir)
        .context("failed to scan projects dir for session files")?;

    // Step 2: Resolve legacy agent parent relationships
    resolve_agent_parents(&mut files)
        .context("failed to resolve agent parent sessions")?;

    // Partition into main sessions and agent files
    let (main_files, agent_files): (Vec<_>, Vec<_>) =
        files.into_iter().partition(|f| !f.is_agent);

    let mut global_quality = GlobalDataQuality {
        total_session_files: main_files.len(),
        total_agent_files: agent_files.len(),
        ..Default::default()
    };

    // Step 3a: Process all main sessions
    let mut sessions: HashMap<String, SessionData> = HashMap::new();

    for sf in &main_files {
        let (turns, quality) = parse_session_file(&sf.file_path, false)
            .with_context(|| format!("failed to parse session: {}", sf.file_path.display()))?;

        let version = extract_version(&sf.file_path);
        let (first_ts, last_ts) = time_range(turns.iter().map(|t| &t.timestamp));

        global_quality.total_valid_turns += quality.valid_turns;
        global_quality.total_skipped +=
            quality.skipped_synthetic + quality.skipped_sidechain + quality.skipped_invalid + quality.skipped_parse_error;

        let session = SessionData {
            session_id: sf.session_id.clone(),
            project: sf.project.clone(),
            turns,
            agent_turns: Vec::new(),
            first_timestamp: first_ts,
            last_timestamp: last_ts,
            version,
            quality,
        };

        sessions.insert(sf.session_id.clone(), session);
    }

    // Step 3b: Process agent files and merge into parent sessions
    for sf in &agent_files {
        let (agent_turns, quality) = parse_session_file(&sf.file_path, true)
            .with_context(|| format!("failed to parse agent file: {}", sf.file_path.display()))?;

        global_quality.total_valid_turns += quality.valid_turns;
        global_quality.total_skipped +=
            quality.skipped_synthetic + quality.skipped_sidechain + quality.skipped_invalid + quality.skipped_parse_error;

        let target_id = match &sf.parent_session_id {
            Some(parent_id) => {
                if !sessions.contains_key(parent_id) {
                    let project = sf.project.clone().or_else(|| Some("(orphan)".to_string()));
                    sessions.insert(parent_id.clone(), SessionData {
                        session_id: parent_id.clone(),
                        project,
                        turns: Vec::new(),
                        agent_turns: Vec::new(),
                        first_timestamp: None,
                        last_timestamp: None,
                        version: None,
                        quality: DataQuality::default(),
                    });
                    global_quality.orphan_agents += 1;
                }
                parent_id.clone()
            }
            None => {
                let virtual_id = sf.session_id.clone();
                if !sessions.contains_key(&virtual_id) {
                    let project = sf.project.clone().or_else(|| Some("(orphan)".to_string()));
                    sessions.insert(virtual_id.clone(), SessionData {
                        session_id: virtual_id.clone(),
                        project,
                        turns: Vec::new(),
                        agent_turns: Vec::new(),
                        first_timestamp: None,
                        last_timestamp: None,
                        version: None,
                        quality: DataQuality::default(),
                    });
                    global_quality.orphan_agents += 1;
                }
                virtual_id
            }
        };

        let parent = sessions.get_mut(&target_id).unwrap();
        merge_agent_turns(parent, agent_turns, &quality);
    }

    // Step 4: Recompute time ranges to include agent turns, and collect results
    let mut result: Vec<SessionData> = sessions.into_values().collect();

    let mut global_min: Option<DateTime<Utc>> = None;
    let mut global_max: Option<DateTime<Utc>> = None;

    for session in &mut result {
        let all_timestamps = session
            .turns
            .iter()
            .chain(session.agent_turns.iter())
            .map(|t| &t.timestamp);
        let (first_ts, last_ts) = time_range(all_timestamps);
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
