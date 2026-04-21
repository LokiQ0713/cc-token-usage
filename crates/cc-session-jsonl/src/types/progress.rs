use serde::{Deserialize, Serialize};

use super::transcript_entry;

transcript_entry! {
    /// A progress event entry recording hook/agent/bash/mcp/search execution progress.
    ///
    /// Introduced in Claude Code v2.1.x. Progress events are streamed during tool
    /// execution and share the common transcript fields (`parentUuid`, `uuid`,
    /// `sessionId`, etc.) with user/assistant entries, plus tool-use tracking IDs
    /// and a discriminated [`ProgressData`] payload whose variant matches the
    /// progress subtype (hook, agent, bash, mcp, search, query).
    ///
    /// Note: the JSON keys `parentToolUseID` and `toolUseID` use uppercase `ID`
    /// (not the camelCase `Id` pattern used by the surrounding transcript fields),
    /// so they are spelled out with explicit `#[serde(rename = ...)]` overrides.
    pub struct ProgressEntry {
        #[serde(rename = "parentToolUseID")]
        pub parent_tool_use_id: Option<String>,

        #[serde(rename = "toolUseID")]
        pub tool_use_id: Option<String>,

        pub data: Option<ProgressData>,
    }
}

/// The progress payload, discriminated by its own inner `type` field.
///
/// Uses internally-tagged enum; unknown future variants fall through to `Other`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressData {
    /// A lifecycle hook firing before/after a tool call.
    #[serde(rename_all = "camelCase")]
    HookProgress {
        hook_event: Option<String>,
        hook_name: Option<String>,
        command: Option<String>,
    },

    /// A Task-launched subagent streaming its inner transcript turn-by-turn.
    ///
    /// `message` is an entire nested transcript entry (user or assistant, with
    /// its own `role`, `content`, and possibly `usage`). Kept as
    /// `serde_json::Value` to avoid recursive type definitions and to stay
    /// forward-compatible with nested-format drift.
    #[serde(rename_all = "camelCase")]
    AgentProgress {
        agent_id: Option<String>,
        prompt: Option<String>,
        message: Option<serde_json::Value>,
    },

    /// A Bash tool streaming output update.
    #[serde(rename_all = "camelCase")]
    BashProgress {
        task_id: Option<String>,
        output: Option<String>,
        full_output: Option<String>,
        total_bytes: Option<u64>,
        total_lines: Option<u64>,
        timeout_ms: Option<u64>,
        elapsed_time_seconds: Option<f64>,
    },

    /// An MCP tool-call status update.
    #[serde(rename_all = "camelCase")]
    McpProgress {
        server_name: Option<String>,
        tool_name: Option<String>,
        status: Option<String>,
        elapsed_time_ms: Option<u64>,
    },

    /// Server-side search returned results.
    #[serde(rename_all = "camelCase")]
    SearchResultsReceived {
        query: Option<String>,
        result_count: Option<u64>,
    },

    /// Server-side search query update / streaming.
    QueryUpdate { query: Option<String> },

    /// Unknown future progress variant.
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;

    #[test]
    fn route_progress_via_entry_enum() {
        let json = r#"{
            "type": "progress",
            "uuid": "p-uuid",
            "sessionId": "s1",
            "toolUseID": "toolu_01",
            "parentToolUseID": "toolu_parent",
            "data": {"type": "hook_progress", "hookEvent": "PostToolUse", "hookName": "PostToolUse:Read", "command": "callback"}
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Progress(_)));
    }

    #[test]
    fn parse_hook_progress_real_sample() {
        let json = r#"{"parentUuid":"da3d097b-92cd-49cb-bc60-697f0e43e9f9","isSidechain":true,"userType":"external","cwd":"/Users/loki/AndroidStudioProjects/MyApplication2","sessionId":"848b538f-a0a3-4a31-862a-0aeba387f26c","version":"2.1.27","gitBranch":"","agentId":"a5d0a13","slug":"gentle-fluttering-token","type":"progress","data":{"type":"hook_progress","hookEvent":"PostToolUse","hookName":"PostToolUse:Read","command":"callback"},"parentToolUseID":"toolu_014UfTZLiS1fm4D3LPkEBxA9","toolUseID":"toolu_014UfTZLiS1fm4D3LPkEBxA9","timestamp":"2026-01-31T13:00:24.898Z","uuid":"d30afb65-fd99-47a7-9c7b-307c28dae091"}"#;
        let entry: ProgressEntry = serde_json::from_str(json).unwrap();
        assert_eq!(
            entry.uuid.as_deref(),
            Some("d30afb65-fd99-47a7-9c7b-307c28dae091")
        );
        assert_eq!(entry.is_sidechain, Some(true));
        assert_eq!(entry.agent_id.as_deref(), Some("a5d0a13"));
        assert_eq!(
            entry.tool_use_id.as_deref(),
            Some("toolu_014UfTZLiS1fm4D3LPkEBxA9")
        );
        assert_eq!(
            entry.parent_tool_use_id.as_deref(),
            Some("toolu_014UfTZLiS1fm4D3LPkEBxA9")
        );
        match entry.data.unwrap() {
            ProgressData::HookProgress {
                hook_event,
                hook_name,
                command,
            } => {
                assert_eq!(hook_event.as_deref(), Some("PostToolUse"));
                assert_eq!(hook_name.as_deref(), Some("PostToolUse:Read"));
                assert_eq!(command.as_deref(), Some("callback"));
            }
            other => panic!("expected HookProgress, got {other:?}"),
        }
    }

    #[test]
    fn parse_agent_progress_real_sample() {
        let json = r#"{"parentUuid":"x","isSidechain":false,"type":"progress","data":{"type":"agent_progress","agentId":"a4dce3b","prompt":"Research X","message":{"type":"user","message":{"role":"user","content":[]},"uuid":"nested-1","timestamp":"2026-03-27T14:55:27.089Z"}},"toolUseID":"agent_msg_014","parentToolUseID":"toolu_018","uuid":"p-outer","timestamp":"2026-03-27T14:55:27.089Z","sessionId":"s1","version":"2.1.84"}"#;
        let entry: ProgressEntry = serde_json::from_str(json).unwrap();
        match entry.data.unwrap() {
            ProgressData::AgentProgress {
                agent_id,
                prompt,
                message,
            } => {
                assert_eq!(agent_id.as_deref(), Some("a4dce3b"));
                assert_eq!(prompt.as_deref(), Some("Research X"));
                assert!(message.is_some());
            }
            other => panic!("expected AgentProgress, got {other:?}"),
        }
    }

    #[test]
    fn parse_bash_progress_real_sample() {
        let json = r#"{"type":"progress","data":{"type":"bash_progress","output":"out\n","fullOutput":"out\n","elapsedTimeSeconds":3,"totalLines":5,"totalBytes":0,"taskId":"bvz77nv0h"},"toolUseID":"bash-progress-0","parentToolUseID":"toolu_019","uuid":"u1","sessionId":"s1"}"#;
        let entry: ProgressEntry = serde_json::from_str(json).unwrap();
        match entry.data.unwrap() {
            ProgressData::BashProgress {
                task_id,
                output,
                elapsed_time_seconds,
                total_lines,
                total_bytes,
                timeout_ms,
                full_output,
            } => {
                assert_eq!(task_id.as_deref(), Some("bvz77nv0h"));
                assert_eq!(output.as_deref(), Some("out\n"));
                assert_eq!(full_output.as_deref(), Some("out\n"));
                assert!((elapsed_time_seconds.unwrap() - 3.0).abs() < f64::EPSILON);
                assert_eq!(total_lines, Some(5));
                assert_eq!(total_bytes, Some(0));
                assert!(timeout_ms.is_none());
            }
            other => panic!("expected BashProgress, got {other:?}"),
        }
    }

    #[test]
    fn parse_mcp_progress_real_sample() {
        let json = r#"{"type":"progress","data":{"type":"mcp_progress","status":"started","serverName":"plugin:episodic-memory:episodic-memory","toolName":"search"},"toolUseID":"toolu_01G","parentToolUseID":"toolu_01G","uuid":"u1","sessionId":"s1"}"#;
        let entry: ProgressEntry = serde_json::from_str(json).unwrap();
        match entry.data.unwrap() {
            ProgressData::McpProgress {
                server_name,
                tool_name,
                status,
                elapsed_time_ms,
            } => {
                assert_eq!(
                    server_name.as_deref(),
                    Some("plugin:episodic-memory:episodic-memory")
                );
                assert_eq!(tool_name.as_deref(), Some("search"));
                assert_eq!(status.as_deref(), Some("started"));
                assert!(elapsed_time_ms.is_none());
            }
            other => panic!("expected McpProgress, got {other:?}"),
        }
    }

    #[test]
    fn parse_search_results_received_real_sample() {
        let json = r#"{"type":"progress","data":{"type":"search_results_received","resultCount":10,"query":"rust"},"toolUseID":"srvtoolu_01","parentToolUseID":"toolu_01","uuid":"u1","sessionId":"s1"}"#;
        let entry: ProgressEntry = serde_json::from_str(json).unwrap();
        match entry.data.unwrap() {
            ProgressData::SearchResultsReceived {
                query,
                result_count,
            } => {
                assert_eq!(query.as_deref(), Some("rust"));
                assert_eq!(result_count, Some(10));
            }
            other => panic!("expected SearchResultsReceived, got {other:?}"),
        }
    }

    #[test]
    fn parse_query_update_real_sample() {
        let json = r#"{"type":"progress","data":{"type":"query_update","query":"rust"},"toolUseID":"search-progress-1","parentToolUseID":"toolu_01","uuid":"u1","sessionId":"s1"}"#;
        let entry: ProgressEntry = serde_json::from_str(json).unwrap();
        match entry.data.unwrap() {
            ProgressData::QueryUpdate { query } => {
                assert_eq!(query.as_deref(), Some("rust"));
            }
            other => panic!("expected QueryUpdate, got {other:?}"),
        }
    }

    #[test]
    fn unknown_progress_subtype_becomes_other() {
        let json = r#"{"type":"progress","data":{"type":"elicitation_progress","somefield":"x"},"uuid":"u1","sessionId":"s1"}"#;
        let entry: ProgressEntry = serde_json::from_str(json).unwrap();
        match entry.data.unwrap() {
            ProgressData::Other => {}
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn progress_without_data_is_ok() {
        let json = r#"{"type":"progress","uuid":"u1","sessionId":"s1"}"#;
        let entry: ProgressEntry = serde_json::from_str(json).unwrap();
        assert!(entry.data.is_none());
    }
}
