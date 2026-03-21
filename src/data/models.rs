use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::PathBuf;

// ─── JSONL Deserialization Layer ─────────────────────────────────────────────

/// Top-level tagged union for each line in the JSONL session file.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum JournalEntry {
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "queue-operation")]
    QueueOperation(serde_json::Value),
}

/// A user-authored message entry.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessage {
    pub uuid: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub cwd: Option<String>,
    pub version: Option<String>,
    pub git_branch: Option<String>,
    pub message: Option<serde_json::Value>,
    pub parent_uuid: Option<String>,
    pub is_sidechain: Option<bool>,
    pub user_type: Option<String>,
}

/// An assistant response entry.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessage {
    pub uuid: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub request_id: Option<String>,
    pub agent_id: Option<String>,
    pub message: Option<ApiMessage>,
    pub cwd: Option<String>,
    pub version: Option<String>,
    pub git_branch: Option<String>,
    pub parent_uuid: Option<String>,
    pub is_sidechain: Option<bool>,
    pub user_type: Option<String>,
}

/// The inner API message returned by Claude.
#[derive(Debug, Deserialize)]
pub struct ApiMessage {
    pub model: Option<String>,
    pub role: Option<String>,
    pub stop_reason: Option<String>,
    pub usage: Option<TokenUsage>,
    pub content: Option<Vec<ContentBlock>>,
}

/// Token usage statistics for a single API call.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation: Option<CacheCreationDetail>,
    pub server_tool_use: Option<ServerToolUse>,
    pub service_tier: Option<String>,
    pub speed: Option<String>,
}

/// Breakdown of cache creation tokens by TTL bucket.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CacheCreationDetail {
    pub ephemeral_5m_input_tokens: Option<u64>,
    pub ephemeral_1h_input_tokens: Option<u64>,
}

/// Server-side tool usage counters.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerToolUse {
    pub web_search_requests: Option<u64>,
    pub web_fetch_requests: Option<u64>,
}

/// A content block inside a message. Only `text` and `tool_use` are parsed;
/// everything else is captured as `Other`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        text: Option<String>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: Option<String>,
        name: Option<String>,
        input: Option<serde_json::Value>,
    },
    #[serde(other)]
    Other,
}

// ─── Validated Data Layer ────────────────────────────────────────────────────

/// A single validated assistant turn, ready for analysis.
#[derive(Debug)]
pub struct ValidatedTurn {
    pub uuid: String,
    pub request_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub usage: TokenUsage,
    pub stop_reason: Option<String>,
    pub content_types: Vec<String>,
    pub is_agent: bool,
    pub agent_id: Option<String>,
    pub user_text: Option<String>,       // 对应的用户消息文本（截断）
    pub assistant_text: Option<String>,  // assistant 回复文本（截断）
    pub tool_names: Vec<String>,         // 使用的工具名列表
}

/// Metadata about a session JSONL file on disk.
#[derive(Debug)]
pub struct SessionFile {
    pub session_id: String,
    pub project: Option<String>,
    pub file_path: PathBuf,
    pub is_agent: bool,
    pub parent_session_id: Option<String>,
}

/// Aggregated data from a single session.
#[derive(Debug)]
pub struct SessionData {
    pub session_id: String,
    pub project: Option<String>,
    pub turns: Vec<ValidatedTurn>,
    pub agent_turns: Vec<ValidatedTurn>,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub quality: DataQuality,
}

/// Quality metrics for a single session file.
#[derive(Debug, Default)]
pub struct DataQuality {
    pub total_lines: usize,
    pub valid_turns: usize,
    pub skipped_synthetic: usize,
    pub skipped_invalid: usize,
    pub skipped_parse_error: usize,
    pub duplicate_turns: usize,
}

/// Quality metrics aggregated across all session files.
#[derive(Debug, Default, Clone)]
pub struct GlobalDataQuality {
    pub total_session_files: usize,
    pub total_agent_files: usize,
    pub orphan_agents: usize,
    pub total_valid_turns: usize,
    pub total_skipped: usize,
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_assistant_message() {
        let json = r#"{"parentUuid":"abc","isSidechain":false,"type":"assistant","uuid":"def","timestamp":"2026-03-16T13:51:35.912Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"cache_creation_input_tokens":1281,"cache_read_input_tokens":15204,"cache_creation":{"ephemeral_5m_input_tokens":1281,"ephemeral_1h_input_tokens":0},"output_tokens":108,"service_tier":"standard"},"content":[{"type":"text","text":"Hello"}]},"sessionId":"abc-123","version":"2.0.77","cwd":"/tmp","gitBranch":"main","userType":"external","requestId":"req_1"}"#;

        let entry: JournalEntry = serde_json::from_str(json).unwrap();

        match entry {
            JournalEntry::Assistant(msg) => {
                assert_eq!(msg.uuid.as_deref(), Some("def"));
                assert_eq!(msg.session_id.as_deref(), Some("abc-123"));
                assert_eq!(msg.request_id.as_deref(), Some("req_1"));
                assert_eq!(msg.parent_uuid.as_deref(), Some("abc"));
                assert_eq!(msg.is_sidechain, Some(false));

                let api = msg.message.unwrap();
                assert_eq!(api.model.as_deref(), Some("claude-opus-4-6"));
                assert_eq!(api.stop_reason.as_deref(), Some("end_turn"));

                let usage = api.usage.unwrap();
                assert_eq!(usage.input_tokens, Some(3));
                assert_eq!(usage.output_tokens, Some(108));
                assert_eq!(usage.cache_creation_input_tokens, Some(1281));
                assert_eq!(usage.cache_read_input_tokens, Some(15204));
                assert_eq!(usage.service_tier.as_deref(), Some("standard"));

                let cache = usage.cache_creation.unwrap();
                assert_eq!(cache.ephemeral_5m_input_tokens, Some(1281));
                assert_eq!(cache.ephemeral_1h_input_tokens, Some(0));

                let content = api.content.unwrap();
                assert_eq!(content.len(), 1);
                match &content[0] {
                    ContentBlock::Text { text } => {
                        assert_eq!(text.as_deref(), Some("Hello"));
                    }
                    _ => panic!("expected Text content block"),
                }
            }
            _ => panic!("expected Assistant variant"),
        }
    }

    #[test]
    fn test_parse_user_message() {
        let json = r#"{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":[{"type":"text","text":"hello"}]},"uuid":"u1","timestamp":"2026-03-16T13:51:19.053Z","sessionId":"s1","version":"2.1.80","cwd":"/tmp","gitBranch":"main","userType":"external"}"#;

        let entry: JournalEntry = serde_json::from_str(json).unwrap();

        match entry {
            JournalEntry::User(msg) => {
                assert_eq!(msg.uuid.as_deref(), Some("u1"));
                assert_eq!(msg.session_id.as_deref(), Some("s1"));
                assert_eq!(msg.version.as_deref(), Some("2.1.80"));
                assert_eq!(msg.cwd.as_deref(), Some("/tmp"));
                assert_eq!(msg.git_branch.as_deref(), Some("main"));
                assert!(msg.parent_uuid.is_none());
            }
            _ => panic!("expected User variant"),
        }
    }

    #[test]
    fn test_parse_queue_operation() {
        let json = r#"{"type":"queue-operation","operation":"dequeue","timestamp":"2026-03-16T13:51:19.041Z","sessionId":"abc"}"#;

        let entry: JournalEntry = serde_json::from_str(json).unwrap();

        match entry {
            JournalEntry::QueueOperation(val) => {
                assert_eq!(val.get("operation").and_then(|v| v.as_str()), Some("dequeue"));
                assert_eq!(val.get("sessionId").and_then(|v| v.as_str()), Some("abc"));
            }
            _ => panic!("expected QueueOperation variant"),
        }
    }

    #[test]
    fn test_parse_synthetic_message() {
        let json = r#"{"type":"assistant","uuid":"x","timestamp":"2026-03-16T00:00:00Z","message":{"model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"error"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null}"#;

        let entry: JournalEntry = serde_json::from_str(json).unwrap();

        match entry {
            JournalEntry::Assistant(msg) => {
                let api = msg.message.unwrap();
                assert_eq!(api.model.as_deref(), Some("<synthetic>"));
                assert_eq!(api.stop_reason.as_deref(), Some("stop_sequence"));

                let usage = api.usage.unwrap();
                assert_eq!(usage.input_tokens, Some(0));
                assert_eq!(usage.output_tokens, Some(0));

                // synthetic messages typically lack cache_creation detail
                assert!(usage.cache_creation.is_none());
            }
            _ => panic!("expected Assistant variant"),
        }
    }
}
