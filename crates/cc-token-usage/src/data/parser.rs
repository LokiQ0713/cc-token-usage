use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use cc_session_jsonl::types::{ApiMessage, AssistantEntry, ContentBlock, Entry, UserEntry};

use super::models::{
    AttributionData, CollapseCommit, CollapseSnapshot, DataQuality, PrLinkInfo, SessionMetadata,
    TokenUsage, ValidatedTurn,
};

// ─── Pipeline Stage 1: JSON Parse (now via cc-session-jsonl) ──────────────

fn parse_line(line: &str) -> Option<Entry> {
    cc_session_jsonl::parse_entry(line).ok()
}

// ─── Pipeline Stage 2: Type Filter + User Text Extraction ──────────────────

/// Extract user message text (truncated to 500 chars) for pairing with assistant turns.
fn extract_user_text(user_entry: &UserEntry) -> Option<String> {
    let content_val = user_entry.message.as_ref()?.content.as_ref()?;

    let text = if let Some(s) = content_val.as_str() {
        s.to_string()
    } else if let Some(arr) = content_val.as_array() {
        arr.iter()
            .filter_map(|b| {
                if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                    b.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        return None;
    };

    if text.is_empty() {
        return None;
    }

    Some(if text.len() > 500 {
        format!("{}...", &text[..text.floor_char_boundary(500)])
    } else {
        text
    })
}

// ─── Pipeline Stage 3: Validation ──────────────────────────────────────────

enum FilterReason {
    NoApiMessage,
    Sidechain,
    Synthetic,
    NoModel,
    NoUsage,
    ZeroUsage,
    InvalidTimestamp,
}

struct ValidatedFields {
    uuid: String,
    request_id: Option<String>,
    timestamp: DateTime<Utc>,
    model: String,
    usage: TokenUsage,
    stop_reason: Option<String>,
    content: Option<Vec<ContentBlock>>,
    agent_id: Option<String>,
    service_tier: Option<String>,
    speed: Option<String>,
    inference_geo: Option<String>,
    git_branch: Option<String>,
}

fn validate_assistant(
    msg: AssistantEntry,
    is_agent: bool,
    now: DateTime<Utc>,
) -> std::result::Result<ValidatedFields, FilterReason> {
    let api: ApiMessage = msg.message.ok_or(FilterReason::NoApiMessage)?;

    // Sidechain filter (skip for agent files -- they always have isSidechain=true)
    if !is_agent && msg.is_sidechain == Some(true) {
        return Err(FilterReason::Sidechain);
    }

    // Synthetic filter
    if api.model.as_deref() == Some("<synthetic>") {
        return Err(FilterReason::Synthetic);
    }

    let model = api.model.ok_or(FilterReason::NoModel)?;
    let lib_usage = api.usage.ok_or(FilterReason::NoUsage)?;

    // Non-zero usage
    let total_tokens = lib_usage.input_tokens.unwrap_or(0)
        + lib_usage.output_tokens.unwrap_or(0)
        + lib_usage.cache_creation_input_tokens.unwrap_or(0)
        + lib_usage.cache_read_input_tokens.unwrap_or(0);
    if total_tokens == 0 {
        return Err(FilterReason::ZeroUsage);
    }

    // Capture service fields before conversion consumes them
    let service_tier = lib_usage.service_tier.clone();
    let speed = lib_usage.speed.clone();
    let inference_geo = lib_usage.inference_geo.clone();

    // Convert to local TokenUsage
    let usage: TokenUsage = lib_usage.into();

    // Timestamp validation
    let timestamp_str = msg
        .timestamp
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or(FilterReason::InvalidTimestamp)?;
    let timestamp: DateTime<Utc> = timestamp_str
        .parse()
        .map_err(|_| FilterReason::InvalidTimestamp)?;
    if timestamp > now {
        return Err(FilterReason::InvalidTimestamp);
    }

    Ok(ValidatedFields {
        uuid: msg.uuid.unwrap_or_default(),
        request_id: msg.request_id,
        timestamp,
        model,
        usage,
        stop_reason: api.stop_reason,
        content: api.content,
        agent_id: msg.agent_id,
        service_tier,
        speed,
        inference_geo,
        git_branch: msg.git_branch,
    })
}

// ─── Pipeline Stage 4: Content Extraction ──────────────────────────────────

/// Extracted content info from content blocks.
struct ContentExtraction {
    content_types: Vec<String>,
    assistant_text: Option<String>,
    tool_names: Vec<String>,
    tool_error_count: usize,
}

fn extract_content(content: &Option<Vec<ContentBlock>>) -> ContentExtraction {
    let mut content_types = Vec::new();
    let mut text_parts = Vec::new();
    let mut tool_names = Vec::new();
    let mut tool_error_count = 0usize;

    if let Some(blocks) = content {
        for b in blocks {
            match b {
                ContentBlock::Text { text } => {
                    content_types.push("text".to_string());
                    if let Some(t) = text {
                        text_parts.push(t.clone());
                    }
                }
                ContentBlock::ToolUse { name, .. } => {
                    content_types.push("tool_use".to_string());
                    if let Some(n) = name {
                        tool_names.push(n.clone());
                    }
                }
                ContentBlock::Thinking { .. } => {
                    content_types.push("thinking".to_string());
                }
                ContentBlock::ToolResult { is_error, .. } => {
                    content_types.push("tool_result".to_string());
                    if *is_error == Some(true) {
                        tool_error_count += 1;
                    }
                }
                _ => {
                    content_types.push("other".to_string());
                }
            }
        }
    }

    let assistant_text = if text_parts.is_empty() {
        None
    } else {
        let full = text_parts.join("\n");
        Some(if full.len() > 500 {
            format!("{}...", &full[..full.floor_char_boundary(500)])
        } else {
            full
        })
    };

    ContentExtraction {
        content_types,
        assistant_text,
        tool_names,
        tool_error_count,
    }
}

// ─── Pipeline Stage 5: Streaming Deduplication ─────────────────────────────

fn dedup_by_request_id(turns: Vec<ValidatedTurn>) -> (Vec<ValidatedTurn>, usize) {
    let mut result = Vec::with_capacity(turns.len());
    let mut request_id_index: HashMap<String, usize> = HashMap::new();
    let mut dup_count = 0;

    for turn in turns {
        let rid = turn.request_id.clone().unwrap_or_default();
        if !rid.is_empty() {
            if let Some(&idx) = request_id_index.get(&rid) {
                result[idx] = turn;
                dup_count += 1;
                continue;
            }
            request_id_index.insert(rid, result.len());
        }
        result.push(turn);
    }

    (result, dup_count)
}

// ─── Pipeline Orchestrator ─────────────────────────────────────────────────

/// Parse a session JSONL file into validated turns, quality metrics, and session metadata.
///
/// Pipeline: JSON parse → type filter → validation → content extraction → deduplication.
/// Also collects metadata from non-assistant/user entries (titles, tags, mode, PR links, etc.).
pub fn parse_session_file(
    path: &Path,
    is_agent: bool,
) -> Result<(Vec<ValidatedTurn>, DataQuality, SessionMetadata)> {
    let file = File::open(path)
        .with_context(|| format!("failed to open session file: {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut quality = DataQuality::default();
    let mut pre_dedup_turns = Vec::new();
    let mut metadata = SessionMetadata::default();
    let now = Utc::now();
    let mut last_user_text: Option<String> = None;
    let mut ai_title: Option<String> = None;
    let mut custom_title: Option<String> = None;

    for line_result in reader.lines() {
        let line =
            line_result.with_context(|| format!("failed to read line from {}", path.display()))?;
        quality.total_lines += 1;

        // Stage 1: JSON parse (via cc-session-jsonl)
        let entry = match parse_line(&line) {
            Some(e) => e,
            None => {
                quality.skipped_parse_error += 1;
                continue;
            }
        };

        // Stage 2: Type filter + metadata collection
        let msg = match entry {
            Entry::Assistant(msg) => {
                // Count API errors even for entries that will fail validation
                if msg.api_error.is_some() || msg.error.is_some() {
                    metadata.api_error_count += 1;
                }
                msg
            }
            Entry::User(user_entry) => {
                metadata.user_prompt_count += 1;
                if let Some(text) = extract_user_text(&user_entry) {
                    last_user_text = Some(text);
                }
                continue;
            }
            Entry::AiTitle(t) => {
                if let Some(title) = t.ai_title {
                    ai_title = Some(title);
                }
                continue;
            }
            Entry::CustomTitle(t) => {
                if let Some(title) = t.custom_title {
                    custom_title = Some(title);
                }
                continue;
            }
            Entry::Tag(t) => {
                if let Some(tag) = t.tag {
                    if !metadata.tags.contains(&tag) {
                        metadata.tags.push(tag);
                    }
                }
                continue;
            }
            Entry::Mode(m) => {
                if let Some(mode) = m.mode {
                    metadata.mode = Some(mode); // last-wins
                }
                continue;
            }
            Entry::PrLink(pr) => {
                if let (Some(number), Some(url), Some(repo)) =
                    (pr.pr_number, pr.pr_url, pr.pr_repository)
                {
                    // Avoid duplicate PR links
                    if !metadata
                        .pr_links
                        .iter()
                        .any(|p| p.number == number && p.repository == repo)
                    {
                        metadata.pr_links.push(PrLinkInfo {
                            number,
                            url,
                            repository: repo,
                        });
                    }
                }
                continue;
            }
            Entry::SpeculationAccept(sa) => {
                metadata.speculation_accepts += 1;
                metadata.speculation_time_saved_ms += sa.time_saved_ms.unwrap_or(0.0);
                continue;
            }
            Entry::QueueOperation(qo) => {
                match qo.operation.as_deref() {
                    Some("enqueue") => metadata.queue_enqueues += 1,
                    Some("dequeue") => metadata.queue_dequeues += 1,
                    _ => {}
                }
                continue;
            }
            Entry::ContextCollapseCommit(cc) => {
                let collapse_id = cc.collapse_id.unwrap_or_default();
                let summary = cc.summary.unwrap_or_default();
                if !collapse_id.is_empty() || !summary.is_empty() {
                    metadata.collapse_commits.push(CollapseCommit {
                        collapse_id,
                        summary,
                    });
                }
                continue;
            }
            Entry::ContextCollapseSnapshot(cs) => {
                // last-wins semantics for snapshot
                let staged = cs.staged.unwrap_or_default();
                let staged_count = staged.len();
                let risks: Vec<f64> = staged.iter().filter_map(|s| s.risk).collect();
                let avg_risk = if risks.is_empty() {
                    0.0
                } else {
                    risks.iter().sum::<f64>() / risks.len() as f64
                };
                let max_risk = risks.iter().cloned().fold(0.0f64, f64::max);
                metadata.collapse_snapshot = Some(CollapseSnapshot {
                    staged_count,
                    avg_risk,
                    max_risk,
                    armed: cs.armed.unwrap_or(false),
                    last_spawn_tokens: cs.last_spawn_tokens.unwrap_or(0),
                });
                continue;
            }
            Entry::AttributionSnapshot(a) => {
                // last-wins semantics
                let surface = a.surface.unwrap_or_default();
                let (file_count, total_contribution) =
                    if let Some(obj) = a.file_states.as_ref().and_then(|v| v.as_object()) {
                        let fc = obj.len();
                        let tc: u64 = obj
                            .values()
                            .filter_map(|v| v.get("claudeContribution")?.as_u64())
                            .sum();
                        (fc, tc)
                    } else {
                        (0, 0)
                    };
                metadata.attribution = Some(AttributionData {
                    surface,
                    file_count,
                    total_claude_contribution: total_contribution,
                    prompt_count: a.prompt_count,
                    escape_count: a.escape_count,
                    permission_prompt_count: a.permission_prompt_count,
                });
                continue;
            }
            _ => continue,
        };

        // Stage 3: Validation
        let fields = match validate_assistant(msg, is_agent, now) {
            Ok(f) => f,
            Err(FilterReason::Sidechain) => {
                quality.skipped_sidechain += 1;
                continue;
            }
            Err(FilterReason::Synthetic) => {
                quality.skipped_synthetic += 1;
                continue;
            }
            Err(_) => {
                quality.skipped_invalid += 1;
                continue;
            }
        };

        // Stage 4: Content extraction
        let extracted = extract_content(&fields.content);

        pre_dedup_turns.push(ValidatedTurn {
            uuid: fields.uuid,
            request_id: fields.request_id,
            timestamp: fields.timestamp,
            model: fields.model,
            usage: fields.usage,
            stop_reason: fields.stop_reason,
            content_types: extracted.content_types,
            is_agent,
            agent_id: fields.agent_id,
            user_text: last_user_text.take(),
            assistant_text: extracted.assistant_text,
            tool_names: extracted.tool_names,
            service_tier: fields.service_tier,
            speed: fields.speed,
            inference_geo: fields.inference_geo,
            tool_error_count: extracted.tool_error_count,
            git_branch: fields.git_branch,
        });
    }

    // Stage 5: Streaming deduplication
    let (turns, dup_count) = dedup_by_request_id(pre_dedup_turns);
    quality.duplicate_turns = dup_count;
    quality.valid_turns = turns.len();

    // Finalize title: custom-title overrides ai-title
    metadata.title = custom_title.or(ai_title);

    Ok((turns, quality, metadata))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    const VALID_ASSISTANT: &str = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-03-16T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":100,"cache_creation_input_tokens":500,"cache_read_input_tokens":10000},"content":[{"type":"text","text":"hi"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r1"}"#;

    fn write_jsonl(lines: &[&str]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
        f.flush().unwrap();
        f
    }

    #[test]
    fn parse_valid_assistant_turn() {
        let f = write_jsonl(&[VALID_ASSISTANT]);
        let (turns, quality, _meta) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 1);
        assert_eq!(quality.valid_turns, 1);
        assert_eq!(turns[0].model, "claude-opus-4-6");
        assert_eq!(turns[0].uuid, "u1");
        assert!(!turns[0].is_agent);
        assert_eq!(turns[0].content_types, vec!["text"]);
    }

    #[test]
    fn filters_synthetic_messages() {
        let synthetic = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-03-16T10:00:00Z","message":{"model":"<synthetic>","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r1"}"#;
        let f = write_jsonl(&[synthetic]);
        let (turns, quality, _meta) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 0);
        assert_eq!(quality.skipped_synthetic, 1);
    }

    #[test]
    fn filters_zero_usage() {
        let zero_usage = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-03-16T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r1"}"#;
        let f = write_jsonl(&[zero_usage]);
        let (turns, quality, _meta) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 0);
        assert_eq!(quality.skipped_invalid, 1);
    }

    #[test]
    fn deduplicates_turns() {
        let f = write_jsonl(&[VALID_ASSISTANT, VALID_ASSISTANT]);
        let (turns, quality, _meta) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 1);
        assert_eq!(quality.duplicate_turns, 1);
    }

    #[test]
    fn skips_malformed_lines() {
        let f = write_jsonl(&["not valid json at all", VALID_ASSISTANT]);
        let (turns, quality, _meta) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 1);
        assert_eq!(quality.skipped_parse_error, 1);
    }

    #[test]
    fn non_assistant_types_not_counted_as_parse_error() {
        // Note: "progress" is not a named variant in cc-session-jsonl, it maps to Unknown
        let progress = r#"{"type":"progress","data":{"type":"hook_progress"},"uuid":"u1","timestamp":"2026-03-16T13:51:19.053Z","sessionId":"s1"}"#;
        let system = r#"{"type":"system","subtype":"turn_duration","durationMs":1234,"uuid":"u2","timestamp":"2026-03-16T13:51:19.053Z","sessionId":"s1"}"#;
        let last_prompt = r#"{"type":"last-prompt","lastPrompt":"hello","sessionId":"s1"}"#;
        let f = write_jsonl(&[progress, system, last_prompt, VALID_ASSISTANT]);
        let (turns, quality, _meta) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 1);
        assert_eq!(
            quality.skipped_parse_error, 0,
            "known entry types should not be parse errors"
        );
        assert_eq!(quality.total_lines, 4);
    }

    #[test]
    fn parses_thinking_content_blocks() {
        let with_thinking = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-03-16T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":100,"cache_creation_input_tokens":500,"cache_read_input_tokens":10000},"content":[{"type":"thinking","thinking":"hmm","signature":"sig"},{"type":"text","text":"answer"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r1"}"#;
        let f = write_jsonl(&[with_thinking]);
        let (turns, quality, _meta) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 1);
        assert_eq!(quality.valid_turns, 1);
        assert!(turns[0].content_types.contains(&"thinking".to_string()));
        assert!(turns[0].content_types.contains(&"text".to_string()));
    }

    #[test]
    fn filters_sidechain_turns() {
        let sidechain = r#"{"type":"assistant","uuid":"u2","timestamp":"2026-03-16T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":100,"cache_creation_input_tokens":500,"cache_read_input_tokens":10000},"content":[{"type":"text","text":"abandoned"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r2"}"#;
        let f = write_jsonl(&[sidechain, VALID_ASSISTANT]);
        let (turns, quality, _meta) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 1, "sidechain turn should be filtered out");
        assert_eq!(quality.skipped_sidechain, 1);
        assert_eq!(turns[0].uuid, "u1", "only main-chain turn should remain");
    }

    // ─── Pipeline unit tests ───────────────────────────────────────────

    #[test]
    fn dedup_preserves_last_entry() {
        let t1 = ValidatedTurn {
            uuid: "u1".into(),
            request_id: Some("r1".into()),
            timestamp: "2026-03-16T10:00:00Z".parse().unwrap(),
            model: "m".into(),
            usage: Default::default(),
            stop_reason: None,
            content_types: vec![],
            is_agent: false,
            agent_id: None,
            user_text: None,
            assistant_text: Some("first".into()),
            tool_names: vec![],
            service_tier: None,
            speed: None,
            inference_geo: None,
            tool_error_count: 0,
            git_branch: None,
        };
        let t2 = ValidatedTurn {
            uuid: "u2".into(),
            request_id: Some("r1".into()),
            timestamp: "2026-03-16T10:00:01Z".parse().unwrap(),
            model: "m".into(),
            usage: Default::default(),
            stop_reason: None,
            content_types: vec![],
            is_agent: false,
            agent_id: None,
            user_text: None,
            assistant_text: Some("second".into()),
            tool_names: vec![],
            service_tier: None,
            speed: None,
            inference_geo: None,
            tool_error_count: 0,
            git_branch: None,
        };
        let (result, dup) = dedup_by_request_id(vec![t1, t2]);
        assert_eq!(result.len(), 1);
        assert_eq!(dup, 1);
        assert_eq!(result[0].assistant_text.as_deref(), Some("second"));
    }

    #[test]
    fn extract_content_handles_all_types() {
        let blocks = vec![
            ContentBlock::Text {
                text: Some("hello".into()),
            },
            ContentBlock::ToolUse {
                id: None,
                name: Some("Bash".into()),
                input: None,
            },
            ContentBlock::Thinking {
                thinking: Some("hmm".into()),
                signature: None,
            },
            ContentBlock::ToolResult {
                tool_use_id: None,
                content: None,
                is_error: None,
            },
            ContentBlock::Other,
        ];
        let extracted = extract_content(&Some(blocks));
        assert_eq!(
            extracted.content_types,
            vec!["text", "tool_use", "thinking", "tool_result", "other"]
        );
        assert_eq!(extracted.assistant_text.as_deref(), Some("hello"));
        assert_eq!(extracted.tool_names, vec!["Bash"]);
        assert_eq!(extracted.tool_error_count, 0);
    }

    #[test]
    fn extract_content_counts_tool_errors() {
        let blocks = vec![
            ContentBlock::ToolResult {
                tool_use_id: None,
                content: None,
                is_error: Some(true),
            },
            ContentBlock::ToolResult {
                tool_use_id: None,
                content: None,
                is_error: Some(false),
            },
            ContentBlock::ToolResult {
                tool_use_id: None,
                content: None,
                is_error: Some(true),
            },
        ];
        let extracted = extract_content(&Some(blocks));
        assert_eq!(extracted.tool_error_count, 2);
    }

    #[test]
    fn collects_metadata_from_entries() {
        let user = r#"{"type":"user","uuid":"u0","sessionId":"s1","message":{"role":"user","content":"hello"}}"#;
        let ai_title = r#"{"type":"ai-title","sessionId":"s1","aiTitle":"AI Generated Title"}"#;
        let custom_title =
            r#"{"type":"custom-title","sessionId":"s1","customTitle":"My Custom Title"}"#;
        let tag1 = r#"{"type":"tag","sessionId":"s1","tag":"bugfix"}"#;
        let tag2 = r#"{"type":"tag","sessionId":"s1","tag":"release"}"#;
        let mode = r#"{"type":"mode","sessionId":"s1","mode":"code"}"#;
        let pr = r#"{"type":"pr-link","sessionId":"s1","prNumber":42,"prUrl":"https://github.com/user/repo/pull/42","prRepository":"user/repo"}"#;
        let spec = r#"{"type":"speculation-accept","timestamp":"2026-03-16T10:00:00Z","timeSavedMs":500.0}"#;
        let enq = r#"{"type":"queue-operation","sessionId":"s1","operation":"enqueue","timestamp":"2026-03-16T10:00:00Z"}"#;
        let deq = r#"{"type":"queue-operation","sessionId":"s1","operation":"dequeue","timestamp":"2026-03-16T10:00:01Z"}"#;

        let f = write_jsonl(&[
            user,
            ai_title,
            custom_title,
            tag1,
            tag2,
            mode,
            pr,
            spec,
            enq,
            deq,
            VALID_ASSISTANT,
        ]);
        let (_turns, _quality, meta) = parse_session_file(f.path(), false).unwrap();

        // custom-title overrides ai-title
        assert_eq!(meta.title.as_deref(), Some("My Custom Title"));
        assert_eq!(meta.tags, vec!["bugfix", "release"]);
        assert_eq!(meta.mode.as_deref(), Some("code"));
        assert_eq!(meta.pr_links.len(), 1);
        assert_eq!(meta.pr_links[0].number, 42);
        assert_eq!(meta.pr_links[0].repository, "user/repo");
        assert_eq!(meta.speculation_accepts, 1);
        assert!((meta.speculation_time_saved_ms - 500.0).abs() < f64::EPSILON);
        assert_eq!(meta.queue_enqueues, 1);
        assert_eq!(meta.queue_dequeues, 1);
        assert_eq!(meta.user_prompt_count, 1);
    }

    #[test]
    fn counts_api_errors() {
        let error_entry = r#"{"type":"assistant","uuid":"err1","timestamp":"2026-03-16T10:00:00Z","sessionId":"s1","apiError":"rate_limit","error":"Rate limited"}"#;
        let f = write_jsonl(&[error_entry, VALID_ASSISTANT]);
        let (_turns, _quality, meta) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(meta.api_error_count, 1);
    }
}
