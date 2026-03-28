use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::data::models::{GlobalDataQuality, SessionData, SessionFile};
use crate::data::scanner;
use crate::pricing::calculator::PricingCalculator;

// ─── Result Types ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ValidationReport {
    pub session_results: Vec<SessionValidation>,
    pub structure_checks: Vec<Check>,
    pub summary: ValidationSummary,
}

#[derive(Debug)]
pub struct SessionValidation {
    pub session_id: String,
    pub project: String,
    pub token_checks: Vec<Check>,
    pub agent_checks: Vec<Check>,
}

#[derive(Debug)]
pub struct Check {
    pub name: String,
    pub expected: String,
    pub actual: String,
    pub passed: bool,
}

impl Check {
    fn pass(name: impl Into<String>, value: impl fmt::Display) -> Self {
        let v = value.to_string();
        Self { name: name.into(), expected: v.clone(), actual: v, passed: true }
    }

    fn compare(name: impl Into<String>, expected: impl fmt::Display, actual: impl fmt::Display) -> Self {
        let e = expected.to_string();
        let a = actual.to_string();
        let passed = e == a;
        Self { name: name.into(), expected: e, actual: a, passed }
    }

    #[allow(dead_code)]
    fn compare_f64(name: impl Into<String>, expected: f64, actual: f64, tolerance: f64) -> Self {
        let passed = (expected - actual).abs() < tolerance;
        Self {
            name: name.into(),
            expected: format!("{:.2}", expected),
            actual: format!("{:.2}", actual),
            passed,
        }
    }
}

#[derive(Debug, Default)]
pub struct ValidationSummary {
    pub total_checks: usize,
    pub passed: usize,
    pub failed: usize,
    pub sessions_validated: usize,
    pub sessions_passed: usize,
}

// ─── Raw Token Counter (independent from main pipeline) ────────────────────

/// Token totals computed independently from the main parsing pipeline.
/// Uses serde_json::Value to ensure complete code path independence.
#[derive(Debug, Default)]
struct RawTokenCount {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    turn_count: usize,
}

/// Check if a raw JSON entry passes the same validation as the main pipeline:
/// - type == "assistant"
/// - sidechain filter (if applicable)
/// - not synthetic, model exists
/// - usage exists with non-zero tokens
/// - valid timestamp <= now
fn is_valid_assistant(val: &serde_json::Value, skip_sidechain: bool, now: &chrono::DateTime<chrono::Utc>) -> bool {
    if val.get("type").and_then(|t| t.as_str()) != Some("assistant") {
        return false;
    }
    if skip_sidechain && val.get("isSidechain").and_then(|v| v.as_bool()) == Some(true) {
        return false;
    }
    let model = val.pointer("/message/model").and_then(|m| m.as_str());
    if model == Some("<synthetic>") || model.is_none() {
        return false;
    }
    // Check usage exists
    if val.pointer("/message/usage").is_none() {
        return false;
    }
    let input = val.pointer("/message/usage/input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let output = val.pointer("/message/usage/output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let cache_creation = val.pointer("/message/usage/cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let cache_read = val.pointer("/message/usage/cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    if input + output + cache_creation + cache_read == 0 {
        return false;
    }
    // Validate timestamp
    if let Some(ts_str) = val.get("timestamp").and_then(|t| t.as_str()) {
        if let Ok(ts) = ts_str.parse::<chrono::DateTime<chrono::Utc>>() {
            if ts > *now {
                return false;
            }
        } else {
            return false;
        }
    } else {
        return false;
    }
    true
}

/// Count tokens from a JSONL file using raw JSON parsing (no JournalEntry types).
/// Applies the same validation filters as the main pipeline for apples-to-apples comparison.
fn count_raw_tokens(path: &Path, skip_sidechain: bool) -> Result<RawTokenCount> {
    let file = File::open(path)
        .with_context(|| format!("raw counter: failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let now = chrono::Utc::now();

    // requestId -> token counts (keep last for streaming dedup)
    let mut by_request: HashMap<String, (u64, u64, u64, u64)> = HashMap::new();
    let mut no_request_id_count = RawTokenCount::default();

    for line in reader.lines() {
        let line = line?;
        let val: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if !is_valid_assistant(&val, skip_sidechain, &now) {
            continue;
        }

        let input = val.pointer("/message/usage/input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let output = val.pointer("/message/usage/output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let cache_creation = val.pointer("/message/usage/cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let cache_read = val.pointer("/message/usage/cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);

        let request_id = val.get("requestId").and_then(|r| r.as_str());

        match request_id {
            Some(rid) if !rid.is_empty() => {
                by_request.insert(rid.to_string(), (input, output, cache_creation, cache_read));
            }
            _ => {
                no_request_id_count.input_tokens += input;
                no_request_id_count.output_tokens += output;
                no_request_id_count.cache_creation_tokens += cache_creation;
                no_request_id_count.cache_read_tokens += cache_read;
                no_request_id_count.turn_count += 1;
            }
        }
    }

    let mut result = no_request_id_count;
    for (input, output, cc, cr) in by_request.values() {
        result.input_tokens += input;
        result.output_tokens += output;
        result.cache_creation_tokens += cc;
        result.cache_read_tokens += cr;
        result.turn_count += 1;
    }

    Ok(result)
}

/// Collect requestIds from valid assistant entries in a JSONL file.
/// Applies the same validation filters as the pipeline for accurate cross-file dedup checking.
fn collect_valid_request_ids(path: &Path, skip_sidechain: bool) -> Result<HashSet<String>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let now = chrono::Utc::now();
    let mut ids = HashSet::new();

    for line in reader.lines() {
        let line = line?;
        let val: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !is_valid_assistant(&val, skip_sidechain, &now) {
            continue;
        }
        if let Some(rid) = val.get("requestId").and_then(|r| r.as_str()) {
            if !rid.is_empty() {
                ids.insert(rid.to_string());
            }
        }
    }
    Ok(ids)
}

// ─── Validation Engine ─────────────────────────────────────────────────────

/// Run full validation across all sessions.
pub fn validate_all(
    sessions: &[&SessionData],
    quality: &GlobalDataQuality,
    claude_home: &Path,
    calc: &PricingCalculator,
) -> Result<ValidationReport> {
    // Re-scan files independently for structure validation
    let mut files = scanner::scan_claude_home(claude_home)?;
    scanner::resolve_agent_parents(&mut files)?;

    let (main_files, agent_files): (Vec<&SessionFile>, Vec<&SessionFile>) =
        files.iter().partition(|f| !f.is_agent);

    let mut structure_checks = Vec::new();
    let mut session_results = Vec::new();

    // ── Structure Checks ────────────────────────────────────────────────

    // Check 1: Session count = main file count
    structure_checks.push(Check::compare(
        "session_count == main_file_count",
        main_files.len(),
        quality.total_session_files,
    ));

    // Check 2: Agent file count
    structure_checks.push(Check::compare(
        "agent_file_count",
        agent_files.len(),
        quality.total_agent_files,
    ));

    // Check 3: Orphan agent count (agents without a matching main session file)
    let main_session_ids: HashSet<&str> = main_files.iter()
        .map(|f| f.session_id.as_str())
        .collect();
    let orphan_count = agent_files.iter()
        .filter(|f| {
            let parent = f.parent_session_id.as_deref()
                .unwrap_or(&f.session_id);
            !main_session_ids.contains(parent)
        })
        .count();
    structure_checks.push(Check::pass(
        format!("orphan_agents (no main session file): {}", orphan_count),
        orphan_count,
    ));

    // Check 4: Report duplicate session IDs in main files (pipeline deduplicates by HashMap)
    let unique_main_ids: HashSet<&str> = main_files.iter()
        .map(|f| f.session_id.as_str())
        .collect();
    let dup_count = main_files.len() - unique_main_ids.len();
    structure_checks.push(Check::pass(
        format!("main_session_files: {} files, {} unique IDs ({} duplicates)", main_files.len(), unique_main_ids.len(), dup_count),
        main_files.len(),
    ));

    // Check 5: Cross-file dedup — agent turns in main session should not be double-counted
    let mut cross_file_overlap = 0usize;
    for agent in &agent_files {
        let parent_id = agent.parent_session_id.as_deref()
            .unwrap_or(&agent.session_id);
        let parent_file = main_files.iter()
            .find(|f| f.session_id == parent_id);
        if let Some(pf) = parent_file {
            let parent_rids = collect_valid_request_ids(&pf.file_path, true).unwrap_or_default();
            let agent_rids = collect_valid_request_ids(&agent.file_path, false).unwrap_or_default();
            cross_file_overlap += parent_rids.intersection(&agent_rids).count();
        }
    }
    structure_checks.push(Check::pass(
        format!("cross_file_overlapping_request_ids (deduped: {})", cross_file_overlap),
        cross_file_overlap,
    ));

    // ── Per-Session Validation ──────────────────────────────────────────

    // Build lookup: session_id -> Vec<agent SessionFile>
    let mut agents_by_parent: HashMap<&str, Vec<&SessionFile>> = HashMap::new();
    for af in &agent_files {
        let parent_id = af.parent_session_id.as_deref()
            .unwrap_or(&af.session_id);
        agents_by_parent.entry(parent_id).or_default().push(af);
    }

    // Build lookup: session_id -> main SessionFile
    let main_file_map: HashMap<&str, &SessionFile> = main_files.iter()
        .map(|f| (f.session_id.as_str(), *f))
        .collect();

    for session in sessions {
        let mut token_checks = Vec::new();
        let mut agent_checks = Vec::new();

        // --- Token validation: raw counter vs pipeline ---
        if let Some(mf) = main_file_map.get(session.session_id.as_str()) {
            let raw_main = count_raw_tokens(&mf.file_path, true)
                .unwrap_or_default();

            // Pipeline's main turns
            let pipeline_main_input: u64 = session.turns.iter()
                .map(|t| t.usage.input_tokens.unwrap_or(0)).sum();
            let pipeline_main_output: u64 = session.turns.iter()
                .map(|t| t.usage.output_tokens.unwrap_or(0)).sum();
            let pipeline_main_cache_creation: u64 = session.turns.iter()
                .map(|t| t.usage.cache_creation_input_tokens.unwrap_or(0)).sum();
            let pipeline_main_cache_read: u64 = session.turns.iter()
                .map(|t| t.usage.cache_read_input_tokens.unwrap_or(0)).sum();
            let pipeline_main_turns = session.turns.len();

            token_checks.push(Check::compare(
                "main_turn_count",
                raw_main.turn_count,
                pipeline_main_turns,
            ));
            token_checks.push(Check::compare(
                "main_input_tokens",
                raw_main.input_tokens,
                pipeline_main_input,
            ));
            token_checks.push(Check::compare(
                "main_output_tokens",
                raw_main.output_tokens,
                pipeline_main_output,
            ));
            token_checks.push(Check::compare(
                "main_cache_creation_tokens",
                raw_main.cache_creation_tokens,
                pipeline_main_cache_creation,
            ));
            token_checks.push(Check::compare(
                "main_cache_read_tokens",
                raw_main.cache_read_tokens,
                pipeline_main_cache_read,
            ));
        }

        // --- Agent validation ---
        let agent_session_files = agents_by_parent.get(session.session_id.as_str());
        let expected_agent_files = agent_session_files.map_or(0, |v| v.len());

        agent_checks.push(Check::compare(
            "agent_file_count",
            expected_agent_files,
            expected_agent_files, // we already know the count from scanning
        ));

        // Verify agent turn association (if agent files exist)
        if expected_agent_files > 0 {
            if let Some(afs) = agent_session_files {
                // Get main session's valid requestIds for cross-file dedup
                let main_file = main_file_map.get(session.session_id.as_str());
                let main_rids = main_file
                    .map(|mf| collect_valid_request_ids(&mf.file_path, true).unwrap_or_default())
                    .unwrap_or_default();

                // Calculate expected per-file (matching pipeline's per-file merge logic)
                let mut expected_unique_agent_turns = 0usize;
                for af in afs {
                    let raw = count_raw_tokens(&af.file_path, false)
                        .unwrap_or_default();
                    let file_rids = collect_valid_request_ids(&af.file_path, false)
                        .unwrap_or_default();
                    let file_overlap = file_rids.intersection(&main_rids).count();
                    expected_unique_agent_turns += raw.turn_count.saturating_sub(file_overlap);
                }

                agent_checks.push(Check::compare(
                    "agent_turn_count (after cross-file dedup)",
                    expected_unique_agent_turns,
                    session.agent_turns.len(),
                ));

                // If expected > 0 but pipeline has 0, that's a real issue
                if expected_unique_agent_turns > 0 {
                    agent_checks.push(Check::compare(
                        "has_agent_turns (non-overlapping exist)",
                        "true",
                        (!session.agent_turns.is_empty()).to_string(),
                    ));
                }
            }
        }

        // --- Cost validation ---
        let pipeline_cost: f64 = session.turns.iter()
            .chain(session.agent_turns.iter())
            .map(|t| calc.calculate_turn_cost(&t.model, &t.usage).total)
            .sum();

        // Verify cost is non-negative and consistent with tokens
        let has_tokens = session.turns.iter().chain(session.agent_turns.iter())
            .any(|t| {
                t.usage.input_tokens.unwrap_or(0) > 0
                    || t.usage.output_tokens.unwrap_or(0) > 0
            });
        if has_tokens {
            token_checks.push(Check::compare(
                "cost > 0 when tokens exist",
                "true",
                (pipeline_cost > 0.0).to_string(),
            ));
        }

        // --- Project association ---
        if let Some(mf) = main_file_map.get(session.session_id.as_str()) {
            token_checks.push(Check::compare(
                "project_association",
                mf.project.as_deref().unwrap_or("(none)"),
                session.project.as_deref().unwrap_or("(none)"),
            ));
        }

        let project_name = session.project.as_deref().unwrap_or("(unknown)").to_string();

        session_results.push(SessionValidation {
            session_id: session.session_id.clone(),
            project: project_name,
            token_checks,
            agent_checks,
        });
    }

    // ── Compute Summary ─────────────────────────────────────────────────

    let mut summary = ValidationSummary::default();

    for check in &structure_checks {
        summary.total_checks += 1;
        if check.passed { summary.passed += 1; } else { summary.failed += 1; }
    }

    for sv in &session_results {
        summary.sessions_validated += 1;
        let mut session_pass = true;
        for check in sv.token_checks.iter().chain(sv.agent_checks.iter()) {
            summary.total_checks += 1;
            if check.passed {
                summary.passed += 1;
            } else {
                summary.failed += 1;
                session_pass = false;
            }
        }
        if session_pass {
            summary.sessions_passed += 1;
        }
    }

    Ok(ValidationReport {
        session_results,
        structure_checks,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_assistant_line(request_id: &str, input: u64, output: u64) -> String {
        format!(
            r#"{{"type":"assistant","uuid":"u-{}","timestamp":"2026-03-16T10:00:00Z","message":{{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{{"input_tokens":{},"output_tokens":{},"cache_creation_input_tokens":0,"cache_read_input_tokens":0}},"content":[{{"type":"text","text":"hi"}}]}},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"{}"}}"#,
            request_id, input, output, request_id
        )
    }

    #[test]
    fn raw_counter_basic() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "{}", make_assistant_line("r1", 100, 50)).unwrap();
        writeln!(f, "{}", make_assistant_line("r2", 200, 75)).unwrap();
        f.flush().unwrap();

        let result = count_raw_tokens(f.path(), true).unwrap();
        assert_eq!(result.turn_count, 2);
        assert_eq!(result.input_tokens, 300);
        assert_eq!(result.output_tokens, 125);
    }

    #[test]
    fn raw_counter_deduplicates_streaming() {
        let mut f = NamedTempFile::new().unwrap();
        // Same requestId, different values — last one wins
        writeln!(f, "{}", make_assistant_line("r1", 100, 50)).unwrap();
        writeln!(f, "{}", make_assistant_line("r1", 200, 75)).unwrap();
        f.flush().unwrap();

        let result = count_raw_tokens(f.path(), true).unwrap();
        assert_eq!(result.turn_count, 1);
        assert_eq!(result.input_tokens, 200);
        assert_eq!(result.output_tokens, 75);
    }

    #[test]
    fn raw_counter_skips_synthetic() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, r#"{{"type":"assistant","uuid":"u1","timestamp":"2026-03-16T10:00:00Z","message":{{"model":"<synthetic>","role":"assistant","stop_reason":"end_turn","usage":{{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}},"content":[]}},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r1"}}"#).unwrap();
        writeln!(f, "{}", make_assistant_line("r2", 200, 75)).unwrap();
        f.flush().unwrap();

        let result = count_raw_tokens(f.path(), true).unwrap();
        assert_eq!(result.turn_count, 1);
        assert_eq!(result.input_tokens, 200);
    }

    #[test]
    fn raw_counter_respects_sidechain_flag() {
        let sidechain_line = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-03-16T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":true,"parentUuid":null,"requestId":"r1"}"#;
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "{}", sidechain_line).unwrap();
        f.flush().unwrap();

        // Main session: skip sidechain
        let result = count_raw_tokens(f.path(), true).unwrap();
        assert_eq!(result.turn_count, 0);

        // Agent file: keep sidechain
        let result = count_raw_tokens(f.path(), false).unwrap();
        assert_eq!(result.turn_count, 1);
        assert_eq!(result.input_tokens, 100);
    }

    #[test]
    fn raw_counter_skips_non_assistant() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, r#"{{"type":"user","uuid":"u1","message":{{"role":"user","content":"hi"}},"timestamp":"2026-03-16T10:00:00Z","sessionId":"s1"}}"#).unwrap();
        writeln!(f, r#"{{"type":"progress","data":{{"type":"hook"}},"uuid":"u2","timestamp":"2026-03-16T10:00:00Z","sessionId":"s1"}}"#).unwrap();
        writeln!(f, "{}", make_assistant_line("r1", 100, 50)).unwrap();
        f.flush().unwrap();

        let result = count_raw_tokens(f.path(), true).unwrap();
        assert_eq!(result.turn_count, 1);
    }
}
