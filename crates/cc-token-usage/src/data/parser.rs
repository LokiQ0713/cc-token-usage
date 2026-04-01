use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use cc_session_jsonl::types::{
    ApiMessage, AssistantEntry, ContentBlock, Entry, UserEntry,
};

use super::models::{DataQuality, TokenUsage, ValidatedTurn};

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
                    b.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
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

    // Convert to local TokenUsage
    let usage: TokenUsage = lib_usage.into();

    // Timestamp validation
    let timestamp_str = msg.timestamp.as_deref()
        .filter(|s| !s.is_empty())
        .ok_or(FilterReason::InvalidTimestamp)?;
    let timestamp: DateTime<Utc> = timestamp_str.parse()
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
    })
}

// ─── Pipeline Stage 4: Content Extraction ──────────────────────────────────

fn extract_content(content: &Option<Vec<ContentBlock>>) -> (Vec<String>, Option<String>, Vec<String>) {
    let mut content_types = Vec::new();
    let mut text_parts = Vec::new();
    let mut tool_names = Vec::new();

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
                ContentBlock::ToolResult { .. } => {
                    content_types.push("tool_result".to_string());
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

    (content_types, assistant_text, tool_names)
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

/// Parse a session JSONL file into validated turns and quality metrics.
///
/// Pipeline: JSON parse → type filter → validation → content extraction → deduplication.
pub fn parse_session_file(path: &Path, is_agent: bool) -> Result<(Vec<ValidatedTurn>, DataQuality)> {
    let file =
        File::open(path).with_context(|| format!("failed to open session file: {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut quality = DataQuality::default();
    let mut pre_dedup_turns = Vec::new();
    let now = Utc::now();
    let mut last_user_text: Option<String> = None;

    for line_result in reader.lines() {
        let line = line_result.with_context(|| format!("failed to read line from {}", path.display()))?;
        quality.total_lines += 1;

        // Stage 1: JSON parse (via cc-session-jsonl)
        let entry = match parse_line(&line) {
            Some(e) => e,
            None => {
                quality.skipped_parse_error += 1;
                continue;
            }
        };

        // Stage 2: Type filter
        let msg = match entry {
            Entry::Assistant(msg) => msg,
            Entry::User(user_entry) => {
                if let Some(text) = extract_user_text(&user_entry) {
                    last_user_text = Some(text);
                }
                continue;
            }
            _ => continue,
        };

        // Stage 3: Validation
        let fields = match validate_assistant(msg, is_agent, now) {
            Ok(f) => f,
            Err(FilterReason::Sidechain) => { quality.skipped_sidechain += 1; continue; }
            Err(FilterReason::Synthetic) => { quality.skipped_synthetic += 1; continue; }
            Err(_) => { quality.skipped_invalid += 1; continue; }
        };

        // Stage 4: Content extraction
        let (content_types, assistant_text, tool_names) = extract_content(&fields.content);

        pre_dedup_turns.push(ValidatedTurn {
            uuid: fields.uuid,
            request_id: fields.request_id,
            timestamp: fields.timestamp,
            model: fields.model,
            usage: fields.usage,
            stop_reason: fields.stop_reason,
            content_types,
            is_agent,
            agent_id: fields.agent_id,
            user_text: last_user_text.take(),
            assistant_text,
            tool_names,
        });
    }

    // Stage 5: Streaming deduplication
    let (turns, dup_count) = dedup_by_request_id(pre_dedup_turns);
    quality.duplicate_turns = dup_count;
    quality.valid_turns = turns.len();

    Ok((turns, quality))
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
        let (turns, quality) = parse_session_file(f.path(), false).unwrap();

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
        let (turns, quality) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 0);
        assert_eq!(quality.skipped_synthetic, 1);
    }

    #[test]
    fn filters_zero_usage() {
        let zero_usage = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-03-16T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"hi"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r1"}"#;
        let f = write_jsonl(&[zero_usage]);
        let (turns, quality) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 0);
        assert_eq!(quality.skipped_invalid, 1);
    }

    #[test]
    fn deduplicates_turns() {
        let f = write_jsonl(&[VALID_ASSISTANT, VALID_ASSISTANT]);
        let (turns, quality) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 1);
        assert_eq!(quality.duplicate_turns, 1);
    }

    #[test]
    fn skips_malformed_lines() {
        let f = write_jsonl(&["not valid json at all", VALID_ASSISTANT]);
        let (turns, quality) = parse_session_file(f.path(), false).unwrap();

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
        let (turns, quality) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 1);
        assert_eq!(quality.skipped_parse_error, 0, "known entry types should not be parse errors");
        assert_eq!(quality.total_lines, 4);
    }

    #[test]
    fn parses_thinking_content_blocks() {
        let with_thinking = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-03-16T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":100,"cache_creation_input_tokens":500,"cache_read_input_tokens":10000},"content":[{"type":"thinking","thinking":"hmm","signature":"sig"},{"type":"text","text":"answer"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r1"}"#;
        let f = write_jsonl(&[with_thinking]);
        let (turns, quality) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 1);
        assert_eq!(quality.valid_turns, 1);
        assert!(turns[0].content_types.contains(&"thinking".to_string()));
        assert!(turns[0].content_types.contains(&"text".to_string()));
    }

    #[test]
    fn filters_sidechain_turns() {
        let sidechain = r#"{"type":"assistant","uuid":"u2","timestamp":"2026-03-16T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":100,"cache_creation_input_tokens":500,"cache_read_input_tokens":10000},"content":[{"type":"text","text":"abandoned"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":true,"parentUuid":"p1","requestId":"r2"}"#;
        let f = write_jsonl(&[sidechain, VALID_ASSISTANT]);
        let (turns, quality) = parse_session_file(f.path(), false).unwrap();

        assert_eq!(turns.len(), 1, "sidechain turn should be filtered out");
        assert_eq!(quality.skipped_sidechain, 1);
        assert_eq!(turns[0].uuid, "u1", "only main-chain turn should remain");
    }

    // ─── Pipeline unit tests ───────────────────────────────────────────

    #[test]
    fn dedup_preserves_last_entry() {
        let t1 = ValidatedTurn {
            uuid: "u1".into(), request_id: Some("r1".into()),
            timestamp: "2026-03-16T10:00:00Z".parse().unwrap(),
            model: "m".into(), usage: Default::default(), stop_reason: None,
            content_types: vec![], is_agent: false, agent_id: None,
            user_text: None, assistant_text: Some("first".into()), tool_names: vec![],
        };
        let t2 = ValidatedTurn {
            uuid: "u2".into(), request_id: Some("r1".into()),
            timestamp: "2026-03-16T10:00:01Z".parse().unwrap(),
            model: "m".into(), usage: Default::default(), stop_reason: None,
            content_types: vec![], is_agent: false, agent_id: None,
            user_text: None, assistant_text: Some("second".into()), tool_names: vec![],
        };
        let (result, dup) = dedup_by_request_id(vec![t1, t2]);
        assert_eq!(result.len(), 1);
        assert_eq!(dup, 1);
        assert_eq!(result[0].assistant_text.as_deref(), Some("second"));
    }

    #[test]
    fn extract_content_handles_all_types() {
        let blocks = vec![
            ContentBlock::Text { text: Some("hello".into()) },
            ContentBlock::ToolUse { id: None, name: Some("Bash".into()), input: None },
            ContentBlock::Thinking { thinking: Some("hmm".into()), signature: None },
            ContentBlock::ToolResult { tool_use_id: None, content: None, is_error: None },
            ContentBlock::Other,
        ];
        let (types, text, tools) = extract_content(&Some(blocks));
        assert_eq!(types, vec!["text", "tool_use", "thinking", "tool_result", "other"]);
        assert_eq!(text.as_deref(), Some("hello"));
        assert_eq!(tools, vec!["Bash"]);
    }
}
