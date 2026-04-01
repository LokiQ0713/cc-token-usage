use serde::{Deserialize, Serialize};

use super::transcript_entry;

transcript_entry! {
    /// An assistant response entry in a Claude Code session.
    pub struct AssistantEntry {
        pub message: Option<ApiMessage>,
        pub request_id: Option<String>,
        pub api_error: Option<String>,
        pub error: Option<String>,
        pub error_details: Option<String>,
        pub is_api_error_message: Option<bool>,
        pub is_virtual: Option<bool>,
        pub advisor_model: Option<String>,
    }
}

/// The inner API message returned by Claude, containing model info, usage, and content.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiMessage {
    pub id: Option<String>,
    pub model: Option<String>,
    pub role: Option<String>,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: Option<Usage>,
    pub content: Option<Vec<ContentBlock>>,
}

/// Token usage statistics for a single API call.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation: Option<CacheCreation>,
    pub server_tool_use: Option<ServerToolUse>,
    pub service_tier: Option<String>,
    pub inference_geo: Option<String>,
    pub iterations: Option<serde_json::Value>,
    pub speed: Option<String>,
}

/// Breakdown of cache creation tokens by TTL bucket.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheCreation {
    pub ephemeral_5m_input_tokens: Option<u64>,
    pub ephemeral_1h_input_tokens: Option<u64>,
}

/// Server-side tool usage counters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerToolUse {
    pub web_search_requests: Option<u64>,
    pub web_fetch_requests: Option<u64>,
}

/// A content block inside an API message.
///
/// Uses internally-tagged enum with `#[serde(tag = "type")]`.
/// Unknown content block types fall through to `Other`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: Option<String> },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: Option<String>,
        name: Option<String>,
        input: Option<serde_json::Value>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: Option<String>,
        content: Option<serde_json::Value>,
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: Option<String>,
        signature: Option<String>,
    },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking { data: Option<String> },
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;

    #[test]
    fn parse_complete_assistant_entry() {
        let json = r##"{
            "parentUuid": "p-uuid-001",
            "isSidechain": false,
            "type": "assistant",
            "uuid": "a-uuid-001",
            "timestamp": "2026-03-16T13:51:35.912Z",
            "message": {
                "id": "msg_01XYZ",
                "model": "claude-opus-4-6",
                "role": "assistant",
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 3,
                    "cache_creation_input_tokens": 1281,
                    "cache_read_input_tokens": 15204,
                    "cache_creation": {
                        "ephemeral_5m_input_tokens": 1281,
                        "ephemeral_1h_input_tokens": 0
                    },
                    "output_tokens": 108,
                    "service_tier": "standard",
                    "inference_geo": "us",
                    "speed": "fast"
                },
                "content": [
                    {"type": "thinking", "thinking": "Let me consider this...", "signature": "sig123"},
                    {"type": "text", "text": "Hello, I can help with that."},
                    {"type": "tool_use", "id": "toolu_01ABC", "name": "Bash", "input": {"command": "ls"}}
                ]
            },
            "sessionId": "sess-abc-123",
            "version": "2.0.77",
            "cwd": "/tmp/project",
            "gitBranch": "main",
            "userType": "external",
            "requestId": "req_001",
            "agentId": "agent-xyz",
            "teamName": "team-alpha",
            "agentName": "Builder",
            "agentColor": "#FF5733",
            "promptId": "prompt-001"
        }"##;

        let entry: AssistantEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.uuid.as_deref(), Some("a-uuid-001"));
        assert_eq!(entry.parent_uuid.as_deref(), Some("p-uuid-001"));
        assert_eq!(entry.is_sidechain, Some(false));
        assert_eq!(entry.timestamp.as_deref(), Some("2026-03-16T13:51:35.912Z"));
        assert_eq!(entry.session_id.as_deref(), Some("sess-abc-123"));
        assert_eq!(entry.version.as_deref(), Some("2.0.77"));
        assert_eq!(entry.cwd.as_deref(), Some("/tmp/project"));
        assert_eq!(entry.git_branch.as_deref(), Some("main"));
        assert_eq!(entry.user_type.as_deref(), Some("external"));
        assert_eq!(entry.request_id.as_deref(), Some("req_001"));
        assert_eq!(entry.agent_id.as_deref(), Some("agent-xyz"));
        assert_eq!(entry.team_name.as_deref(), Some("team-alpha"));
        assert_eq!(entry.agent_name.as_deref(), Some("Builder"));
        assert_eq!(entry.agent_color.as_deref(), Some("#FF5733"));
        assert_eq!(entry.prompt_id.as_deref(), Some("prompt-001"));

        let msg = entry.message.as_ref().unwrap();
        assert_eq!(msg.id.as_deref(), Some("msg_01XYZ"));
        assert_eq!(msg.model.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(msg.role.as_deref(), Some("assistant"));
        assert_eq!(msg.stop_reason.as_deref(), Some("end_turn"));

        let usage = msg.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, Some(3));
        assert_eq!(usage.output_tokens, Some(108));
        assert_eq!(usage.cache_creation_input_tokens, Some(1281));
        assert_eq!(usage.cache_read_input_tokens, Some(15204));
        assert_eq!(usage.service_tier.as_deref(), Some("standard"));
        assert_eq!(usage.inference_geo.as_deref(), Some("us"));
        assert_eq!(usage.speed.as_deref(), Some("fast"));

        let cache = usage.cache_creation.as_ref().unwrap();
        assert_eq!(cache.ephemeral_5m_input_tokens, Some(1281));
        assert_eq!(cache.ephemeral_1h_input_tokens, Some(0));

        let content = msg.content.as_ref().unwrap();
        assert_eq!(content.len(), 3);

        match &content[0] {
            ContentBlock::Thinking { thinking, signature } => {
                assert_eq!(thinking.as_deref(), Some("Let me consider this..."));
                assert_eq!(signature.as_deref(), Some("sig123"));
            }
            other => panic!("Expected Thinking, got: {other:?}"),
        }
        match &content[1] {
            ContentBlock::Text { text } => {
                assert_eq!(text.as_deref(), Some("Hello, I can help with that."));
            }
            other => panic!("Expected Text, got: {other:?}"),
        }
        match &content[2] {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id.as_deref(), Some("toolu_01ABC"));
                assert_eq!(name.as_deref(), Some("Bash"));
                assert!(input.is_some());
                assert_eq!(input.as_ref().unwrap()["command"], "ls");
            }
            other => panic!("Expected ToolUse, got: {other:?}"),
        }
    }

    #[test]
    fn parse_minimal_assistant_entry() {
        // Old-version format: no cache_creation, no inference_geo, no speed, minimal fields
        let json = r#"{
            "type": "assistant",
            "uuid": "min-uuid",
            "timestamp": "2025-06-01T10:00:00Z",
            "message": {
                "model": "claude-sonnet-4-20250514",
                "role": "assistant",
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 500,
                    "output_tokens": 100
                },
                "content": [
                    {"type": "text", "text": "OK"}
                ]
            },
            "sessionId": "sess-minimal"
        }"#;

        let entry: AssistantEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.uuid.as_deref(), Some("min-uuid"));
        assert!(entry.parent_uuid.is_none());
        assert!(entry.is_sidechain.is_none());
        assert!(entry.version.is_none());
        assert!(entry.cwd.is_none());
        assert!(entry.git_branch.is_none());
        assert!(entry.agent_id.is_none());
        assert!(entry.is_virtual.is_none());

        let msg = entry.message.as_ref().unwrap();
        assert_eq!(msg.model.as_deref(), Some("claude-sonnet-4-20250514"));

        let usage = msg.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, Some(500));
        assert_eq!(usage.output_tokens, Some(100));
        assert!(usage.cache_creation_input_tokens.is_none());
        assert!(usage.cache_read_input_tokens.is_none());
        assert!(usage.cache_creation.is_none());
        assert!(usage.inference_geo.is_none());
        assert!(usage.speed.is_none());
    }

    #[test]
    fn parse_assistant_with_api_error() {
        let json = r#"{
            "type": "assistant",
            "uuid": "err-uuid",
            "timestamp": "2026-03-16T14:00:00Z",
            "sessionId": "sess-err",
            "apiError": "rate_limit_exceeded",
            "error": "You have exceeded your rate limit",
            "errorDetails": "Please wait 30 seconds",
            "isApiErrorMessage": true
        }"#;

        let entry: AssistantEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.api_error.as_deref(), Some("rate_limit_exceeded"));
        assert_eq!(entry.error.as_deref(), Some("You have exceeded your rate limit"));
        assert_eq!(entry.error_details.as_deref(), Some("Please wait 30 seconds"));
        assert_eq!(entry.is_api_error_message, Some(true));
        assert!(entry.message.is_none());
    }

    #[test]
    fn parse_assistant_with_is_virtual() {
        let json = r#"{
            "type": "assistant",
            "uuid": "virt-uuid",
            "timestamp": "2026-03-16T14:00:00Z",
            "sessionId": "sess-virt",
            "isVirtual": true,
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "virtual response"}]
            }
        }"#;

        let entry: AssistantEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.is_virtual, Some(true));
    }

    #[test]
    fn content_block_text() {
        let json = r#"{"type": "text", "text": "Hello world"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::Text { text } => assert_eq!(text.as_deref(), Some("Hello world")),
            other => panic!("Expected Text, got: {other:?}"),
        }
    }

    #[test]
    fn content_block_tool_use() {
        let json = r#"{
            "type": "tool_use",
            "id": "toolu_01ABC",
            "name": "Read",
            "input": {"file_path": "/tmp/test.rs", "limit": 100}
        }"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id.as_deref(), Some("toolu_01ABC"));
                assert_eq!(name.as_deref(), Some("Read"));
                let inp = input.unwrap();
                assert_eq!(inp["file_path"], "/tmp/test.rs");
                assert_eq!(inp["limit"], 100);
            }
            other => panic!("Expected ToolUse, got: {other:?}"),
        }
    }

    #[test]
    fn content_block_tool_result() {
        let json = r#"{
            "type": "tool_result",
            "tool_use_id": "toolu_01ABC",
            "content": "file contents here",
            "is_error": false
        }"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                assert_eq!(tool_use_id.as_deref(), Some("toolu_01ABC"));
                assert!(content.is_some());
                assert_eq!(is_error, Some(false));
            }
            other => panic!("Expected ToolResult, got: {other:?}"),
        }
    }

    #[test]
    fn content_block_thinking() {
        let json = r#"{"type": "thinking", "thinking": "Deep analysis...", "signature": "sig_xyz"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::Thinking { thinking, signature } => {
                assert_eq!(thinking.as_deref(), Some("Deep analysis..."));
                assert_eq!(signature.as_deref(), Some("sig_xyz"));
            }
            other => panic!("Expected Thinking, got: {other:?}"),
        }
    }

    #[test]
    fn content_block_redacted_thinking() {
        let json = r#"{"type": "redacted_thinking", "data": "base64encodeddata=="}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::RedactedThinking { data } => {
                assert_eq!(data.as_deref(), Some("base64encodeddata=="));
            }
            other => panic!("Expected RedactedThinking, got: {other:?}"),
        }
    }

    #[test]
    fn content_block_unknown_type_becomes_other() {
        let json = r#"{"type": "server_tool_use", "id": "st_01", "name": "web_search"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::Other => {} // expected
            other => panic!("Expected Other, got: {other:?}"),
        }
    }

    #[test]
    fn usage_with_server_tool_use() {
        let json = r#"{
            "input_tokens": 10,
            "output_tokens": 20,
            "server_tool_use": {
                "web_search_requests": 2,
                "web_fetch_requests": 1
            }
        }"#;
        let usage: Usage = serde_json::from_str(json).unwrap();
        let stu = usage.server_tool_use.as_ref().unwrap();
        assert_eq!(stu.web_search_requests, Some(2));
        assert_eq!(stu.web_fetch_requests, Some(1));
    }

    #[test]
    fn parse_assistant_via_entry_enum() {
        let json = r#"{
            "type": "assistant",
            "uuid": "enum-test",
            "sessionId": "sess-enum",
            "message": {
                "model": "claude-opus-4-6",
                "role": "assistant",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 5, "output_tokens": 10},
                "content": [{"type": "text", "text": "Hi"}]
            }
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::Assistant(a) => {
                assert_eq!(a.uuid.as_deref(), Some("enum-test"));
                let msg = a.message.unwrap();
                assert_eq!(msg.model.as_deref(), Some("claude-opus-4-6"));
            }
            other => panic!("Expected Assistant, got: {other:?}"),
        }
    }

    #[test]
    fn parse_assistant_with_advisor_model() {
        let json = r#"{
            "type": "assistant",
            "uuid": "adv-uuid",
            "sessionId": "sess-adv",
            "advisorModel": "claude-haiku-4-20250514",
            "message": {
                "model": "claude-opus-4-6",
                "role": "assistant",
                "content": []
            }
        }"#;
        let entry: AssistantEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.advisor_model.as_deref(), Some("claude-haiku-4-20250514"));
    }

    // ── P0 missing tests (from QA review) ──

    #[test]
    fn usage_iterations_as_array() {
        let json = r#"{
            "input_tokens": 10,
            "output_tokens": 20,
            "iterations": [{"attempt": 1}, {"attempt": 2}]
        }"#;
        let usage: Usage = serde_json::from_str(json).unwrap();
        assert!(usage.iterations.as_ref().unwrap().is_array());
    }

    #[test]
    fn usage_iterations_as_null() {
        let json = r#"{"input_tokens": 10, "output_tokens": 20, "iterations": null}"#;
        let usage: Usage = serde_json::from_str(json).unwrap();
        // serde deserializes JSON null into Option::None for Option<Value>
        assert!(usage.iterations.is_none());
    }

    #[test]
    fn extra_unknown_fields_are_ignored() {
        let json = r#"{
            "type": "assistant",
            "uuid": "test-extra",
            "sessionId": "s1",
            "completelyNewField": true,
            "anotherFutureField": {"nested": "data"},
            "message": {
                "model": "claude-opus-4-6",
                "role": "assistant",
                "content": [],
                "usage": {"input_tokens": 1, "output_tokens": 1},
                "brandNewApiField": 42
            }
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::Assistant(a) => {
                assert_eq!(a.uuid.as_deref(), Some("test-extra"));
            }
            other => panic!("Expected Assistant, got: {other:?}"),
        }
    }

    #[test]
    fn all_transcript_common_fields_null() {
        let json = r#"{
            "type": "assistant",
            "uuid": null,
            "parentUuid": null,
            "logicalParentUuid": null,
            "isSidechain": null,
            "timestamp": null,
            "sessionId": null,
            "cwd": null,
            "version": null,
            "gitBranch": null,
            "userType": null,
            "entrypoint": null,
            "slug": null,
            "agentId": null,
            "teamName": null,
            "agentName": null,
            "agentColor": null,
            "promptId": null,
            "message": null
        }"#;
        let entry: AssistantEntry = serde_json::from_str(json).unwrap();
        assert!(entry.uuid.is_none());
        assert!(entry.parent_uuid.is_none());
        assert!(entry.is_sidechain.is_none());
        assert!(entry.message.is_none());
    }
}
