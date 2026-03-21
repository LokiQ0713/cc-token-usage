use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::models::{ContentBlock, DataQuality, JournalEntry, ValidatedTurn};

/// Parse a session JSONL file into validated turns and quality metrics.
///
/// Each line is validated through a pipeline: JSON parse → type filter →
/// synthetic filter → model/usage/timestamp checks → deduplication.
pub fn parse_session_file(path: &Path, is_agent: bool) -> Result<(Vec<ValidatedTurn>, DataQuality)> {
    let file =
        File::open(path).with_context(|| format!("failed to open session file: {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut quality = DataQuality::default();
    let mut turns = Vec::new();
    let mut seen_keys = HashSet::new();
    let now = Utc::now();
    let mut last_user_text: Option<String> = None;

    for line_result in reader.lines() {
        let line = line_result.with_context(|| format!("failed to read line from {}", path.display()))?;
        quality.total_lines += 1;

        // 1. Parse JSON
        let entry: JournalEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => {
                quality.skipped_parse_error += 1;
                continue;
            }
        };

        // 2. Type filter — capture user text, only process Assistant entries
        let msg = match entry {
            JournalEntry::Assistant(msg) => msg,
            JournalEntry::User(user_msg) => {
                // Extract user message text for pairing with next assistant turn
                // user_msg.message is {"role":"user","content":...} — need to get "content" first
                let content_val = user_msg.message.as_ref()
                    .and_then(|m| m.get("content"));
                if let Some(content) = content_val {
                    let text = if let Some(s) = content.as_str() {
                        // content is a plain string
                        s.to_string()
                    } else if let Some(arr) = content.as_array() {
                        // content is an array of blocks — extract text blocks, skip tool_result
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
                        String::new()
                    };

                    if !text.is_empty() {
                        let truncated = if text.len() > 500 {
                            format!("{}...", &text[..text.floor_char_boundary(500)])
                        } else {
                            text
                        };
                        last_user_text = Some(truncated);
                    }
                }
                continue;
            }
            JournalEntry::QueueOperation(_) => continue,
        };

        let api = match msg.message {
            Some(api) => api,
            None => {
                quality.skipped_invalid += 1;
                continue;
            }
        };

        // 3. Synthetic filter
        if api.model.as_deref() == Some("<synthetic>") {
            quality.skipped_synthetic += 1;
            continue;
        }

        // 4. Model existence
        let model = match api.model {
            Some(m) => m,
            None => {
                quality.skipped_invalid += 1;
                continue;
            }
        };

        // 5. Usage existence
        let usage = match api.usage {
            Some(u) => u,
            None => {
                quality.skipped_invalid += 1;
                continue;
            }
        };

        // 6. Non-zero usage validation
        let total_tokens = usage.input_tokens.unwrap_or(0)
            + usage.output_tokens.unwrap_or(0)
            + usage.cache_creation_input_tokens.unwrap_or(0)
            + usage.cache_read_input_tokens.unwrap_or(0);
        if total_tokens == 0 {
            quality.skipped_invalid += 1;
            continue;
        }

        // 7. Timestamp parsing
        let timestamp_str = match &msg.timestamp {
            Some(ts) if !ts.is_empty() => ts.as_str(),
            _ => {
                quality.skipped_invalid += 1;
                continue;
            }
        };
        let timestamp: DateTime<Utc> = match timestamp_str.parse() {
            Ok(ts) if ts <= now => ts,
            _ => {
                quality.skipped_invalid += 1;
                continue;
            }
        };

        // 8. Deduplication by uuid:requestId composite key
        let uuid = msg.uuid.unwrap_or_default();
        let dedup_key = format!("{}:{}", uuid, msg.request_id.as_deref().unwrap_or(""));
        if !seen_keys.insert(dedup_key) {
            quality.duplicate_turns += 1;
            continue;
        }

        // 9. Extract content types, assistant text, and tool names
        let mut content_types = Vec::new();
        let mut assistant_text_parts = Vec::new();
        let mut tool_names = Vec::new();

        if let Some(ref blocks) = api.content {
            for b in blocks {
                match b {
                    ContentBlock::Text { text } => {
                        content_types.push("text".to_string());
                        if let Some(t) = text {
                            assistant_text_parts.push(t.clone());
                        }
                    }
                    ContentBlock::ToolUse { name, .. } => {
                        content_types.push("tool_use".to_string());
                        if let Some(n) = name {
                            tool_names.push(n.clone());
                        }
                    }
                    ContentBlock::Other => {
                        content_types.push("other".to_string());
                    }
                }
            }
        }

        // Truncate assistant text to 500 chars
        let assistant_text = if assistant_text_parts.is_empty() {
            None
        } else {
            let full = assistant_text_parts.join("\n");
            Some(if full.len() > 500 {
                format!("{}...", &full[..full.floor_char_boundary(500)])
            } else {
                full
            })
        };

        // 10. Construct ValidatedTurn — attach user text from previous message
        turns.push(ValidatedTurn {
            uuid,
            request_id: msg.request_id,
            timestamp,
            model,
            usage,
            stop_reason: api.stop_reason,
            content_types,
            is_agent,
            agent_id: msg.agent_id,
            user_text: last_user_text.take(),
            assistant_text,
            tool_names,
        });
    }

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
}
