use chrono::{DateTime, Utc};
use serde::Deserialize;

// ─── Re-exports from cc-session-jsonl ───────────────────────────────────────

/// Re-export the session file metadata type from cc-session-jsonl.
///
/// Note: field name is `path` (not `file_path`). All call-sites have been
/// updated accordingly.
pub use cc_session_jsonl::scanner::SessionFile;

// ─── Local Token/Usage Types (kept to avoid changing analysis/pricing/output) ─

/// Token usage statistics for a single API call.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation: Option<CacheCreationDetail>,
    pub server_tool_use: Option<ServerToolUse>,
    pub service_tier: Option<String>,
    pub speed: Option<String>,
    pub inference_geo: Option<String>,
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

// ─── Conversions from cc-session-jsonl types ─────────────────────────────────

impl From<cc_session_jsonl::types::Usage> for TokenUsage {
    fn from(u: cc_session_jsonl::types::Usage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cache_creation_input_tokens: u.cache_creation_input_tokens,
            cache_read_input_tokens: u.cache_read_input_tokens,
            cache_creation: u.cache_creation.map(|c| CacheCreationDetail {
                ephemeral_5m_input_tokens: c.ephemeral_5m_input_tokens,
                ephemeral_1h_input_tokens: c.ephemeral_1h_input_tokens,
            }),
            server_tool_use: u.server_tool_use.map(|s| ServerToolUse {
                web_search_requests: s.web_search_requests,
                web_fetch_requests: s.web_fetch_requests,
            }),
            service_tier: u.service_tier,
            inference_geo: u.inference_geo,
            speed: u.speed,
        }
    }
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
    pub service_tier: Option<String>,
    pub speed: Option<String>,
    pub inference_geo: Option<String>,
    pub tool_error_count: usize,         // ToolResult blocks with is_error=true
    pub git_branch: Option<String>,      // from the assistant entry's gitBranch field
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
    pub metadata: SessionMetadata,
}

// ─── Session Metadata ───────────────────────────────────────────────────────

/// PR link info extracted from pr-link entries.
#[derive(Debug, Clone)]
pub struct PrLinkInfo {
    pub number: u64,
    pub url: String,
    pub repository: String,
}

/// Metadata collected from non-assistant/user entries during parsing.
#[derive(Debug, Default)]
pub struct SessionMetadata {
    pub title: Option<String>,              // custom-title > ai-title
    pub tags: Vec<String>,
    pub mode: Option<String>,               // last-wins
    pub pr_links: Vec<PrLinkInfo>,
    pub speculation_accepts: usize,
    pub speculation_time_saved_ms: f64,
    pub queue_enqueues: usize,
    pub queue_dequeues: usize,
    pub api_error_count: usize,             // assistant entries with api_error/error
    pub user_prompt_count: usize,           // count of user entries
}

impl SessionData {
    /// All API responses (main + agent), sorted by timestamp.
    pub fn all_responses(&self) -> Vec<&ValidatedTurn> {
        let mut all: Vec<&ValidatedTurn> = self.turns.iter()
            .chain(self.agent_turns.iter())
            .collect();
        all.sort_by_key(|r| r.timestamp);
        all
    }

    /// Total number of API responses (main + agent).
    pub fn total_turn_count(&self) -> usize {
        self.turns.len() + self.agent_turns.len()
    }

    /// Number of agent API responses.
    pub fn agent_turn_count(&self) -> usize {
        self.agent_turns.len()
    }
}

/// Quality metrics for a single session file.
#[derive(Debug, Default)]
pub struct DataQuality {
    pub total_lines: usize,
    pub valid_turns: usize,
    pub skipped_synthetic: usize,
    pub skipped_sidechain: usize,
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

        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();

        match entry {
            cc_session_jsonl::types::Entry::Assistant(msg) => {
                assert_eq!(msg.uuid.as_deref(), Some("def"));
                assert_eq!(msg.session_id.as_deref(), Some("abc-123"));
                assert_eq!(msg.request_id.as_deref(), Some("req_1"));
                assert_eq!(msg.parent_uuid.as_deref(), Some("abc"));
                assert_eq!(msg.is_sidechain, Some(false));

                let api = msg.message.unwrap();
                assert_eq!(api.model.as_deref(), Some("claude-opus-4-6"));
                assert_eq!(api.stop_reason.as_deref(), Some("end_turn"));

                let usage: TokenUsage = api.usage.unwrap().into();
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
                    cc_session_jsonl::types::ContentBlock::Text { text } => {
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

        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();

        match entry {
            cc_session_jsonl::types::Entry::User(msg) => {
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

        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();

        match entry {
            cc_session_jsonl::types::Entry::QueueOperation(val) => {
                assert_eq!(val.operation.as_deref(), Some("dequeue"));
                assert_eq!(val.session_id.as_deref(), Some("abc"));
            }
            _ => panic!("expected QueueOperation variant"),
        }
    }

    #[test]
    fn test_parse_progress_entry() {
        // progress is not a named variant in cc-session-jsonl; it maps to Unknown
        let json = r#"{"type":"progress","data":{"type":"hook_progress"},"uuid":"u1","timestamp":"2026-03-16T13:51:19.053Z","sessionId":"s1"}"#;
        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, cc_session_jsonl::types::Entry::Unknown));
    }

    #[test]
    fn test_parse_system_entry() {
        let json = r#"{"type":"system","subtype":"turn_duration","durationMs":1234,"uuid":"u1","timestamp":"2026-03-16T13:51:19.053Z","sessionId":"s1"}"#;
        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, cc_session_jsonl::types::Entry::System(_)));
    }

    #[test]
    fn test_parse_unknown_entry_type() {
        let json = r#"{"type":"some-future-type","data":"whatever","uuid":"u1","timestamp":"2026-03-16T13:51:19.053Z"}"#;
        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, cc_session_jsonl::types::Entry::Unknown));
    }

    #[test]
    fn test_parse_thinking_content_block() {
        let json = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-03-16T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":100,"cache_creation_input_tokens":500,"cache_read_input_tokens":10000},"content":[{"type":"thinking","thinking":"Let me analyze this...","signature":"abc123"},{"type":"text","text":"Here is my answer."}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null,"requestId":"r1"}"#;
        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();
        match entry {
            cc_session_jsonl::types::Entry::Assistant(msg) => {
                let content = msg.message.unwrap().content.unwrap();
                assert_eq!(content.len(), 2);
                assert!(matches!(&content[0], cc_session_jsonl::types::ContentBlock::Thinking { thinking: Some(t), .. } if t.contains("analyze")));
                assert!(matches!(&content[1], cc_session_jsonl::types::ContentBlock::Text { .. }));
            }
            _ => panic!("expected Assistant variant"),
        }
    }

    #[test]
    fn test_parse_synthetic_message() {
        let json = r#"{"type":"assistant","uuid":"x","timestamp":"2026-03-16T00:00:00Z","message":{"model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"error"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":null}"#;

        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();

        match entry {
            cc_session_jsonl::types::Entry::Assistant(msg) => {
                let api = msg.message.unwrap();
                assert_eq!(api.model.as_deref(), Some("<synthetic>"));
                assert_eq!(api.stop_reason.as_deref(), Some("stop_sequence"));

                let usage: TokenUsage = api.usage.unwrap().into();
                assert_eq!(usage.input_tokens, Some(0));
                assert_eq!(usage.output_tokens, Some(0));

                // synthetic messages typically lack cache_creation detail
                assert!(usage.cache_creation.is_none());
            }
            _ => panic!("expected Assistant variant"),
        }
    }

    #[test]
    fn test_token_usage_from_conversion() {
        let lib_usage = cc_session_jsonl::types::Usage {
            input_tokens: Some(100),
            output_tokens: Some(200),
            cache_creation_input_tokens: Some(50),
            cache_read_input_tokens: Some(300),
            cache_creation: Some(cc_session_jsonl::types::CacheCreation {
                ephemeral_5m_input_tokens: Some(30),
                ephemeral_1h_input_tokens: Some(20),
            }),
            server_tool_use: Some(cc_session_jsonl::types::ServerToolUse {
                web_search_requests: Some(2),
                web_fetch_requests: Some(1),
            }),
            service_tier: Some("standard".into()),
            inference_geo: Some("us".into()),  // dropped in conversion
            iterations: None,                  // dropped in conversion
            speed: Some("fast".into()),
        };

        let local: TokenUsage = lib_usage.into();
        assert_eq!(local.input_tokens, Some(100));
        assert_eq!(local.output_tokens, Some(200));
        assert_eq!(local.cache_creation_input_tokens, Some(50));
        assert_eq!(local.cache_read_input_tokens, Some(300));
        assert_eq!(local.service_tier.as_deref(), Some("standard"));
        assert_eq!(local.speed.as_deref(), Some("fast"));

        let cache = local.cache_creation.unwrap();
        assert_eq!(cache.ephemeral_5m_input_tokens, Some(30));
        assert_eq!(cache.ephemeral_1h_input_tokens, Some(20));

        let stu = local.server_tool_use.unwrap();
        assert_eq!(stu.web_search_requests, Some(2));
        assert_eq!(stu.web_fetch_requests, Some(1));
    }
}
