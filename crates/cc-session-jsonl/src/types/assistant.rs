//! `AssistantEntry` — an assistant response written as one entry per
//! Anthropic `ContentBlock` (see `docs/session-data-model.md` §2.1).
//!
//! V2 changes vs the old macro:
//! - All fields enumerated explicitly; no shared `transcript_entry!`.
//! - `parent_uuid` is `String` (assistant entries are always replies — see
//!   `docs/cc-session-jsonl-v2-field-survey.md` §2). `message: ApiMessage`
//!   (not Option) — survey §3 confirms 100% present on every assistant
//!   entry including the 59 synthetic api-error entries. All other "common"
//!   fields stay `Option<…>` to tolerate cross-version JSONL.
//! - New attribution fields: `attribution_agent`, `attribution_mcp_server`,
//!   `attribution_mcp_tool` (the latter two appear together in MCP-driven
//!   turns, ~3% sample rate).
//! - `api_error_status: Option<u16>` for HTTP-code-bearing API errors.
//! - `error: Option<AssistantError>` — typed enum covering the 5 observed
//!   string values + drift soft-landing (see [`AssistantError`]).
//! - `diagnostics` is now a typed `Diagnostics` struct so the
//!   `cache_miss_reason` discriminator is a Rust enum (not a `Value`).
//! - The ghost keys `message.stop_details` and `message.container` are
//!   silently dropped (they exist in JSONL but are always `null`).
//! - Three previously-modelled zero-sample fields removed: `apiError`,
//!   `isVirtual`, `advisorModel` (0 hits across 25,429 surveyed assistant
//!   entries). Teammates fields `teamName`/`agentName`/`agentColor` are
//!   likewise omitted. Serde silently drops unknown keys so reappearance
//!   in future data won't fail the parse — restore them with a real
//!   observed type if/when they're emitted.

use serde::{Deserialize, Serialize};

use super::common::{
    AssistantError, CacheMissReasonKind, DagNode, Entrypoint, InferenceGeo, ServiceTier, Speed,
    StopReason, UserType,
};

/// An assistant response entry. One per `ContentBlock` emitted by the model.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantEntry {
    // ── 9 truly-universal DAG fields ──
    pub uuid: Option<String>,
    /// Required: assistant entries always reply to something
    /// (see survey §2). Use `String`, not `Option`.
    pub parent_uuid: String,
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub cwd: Option<String>,
    pub version: Option<String>,
    pub git_branch: Option<String>,
    /// 100% present in the survey but always `"external"`. Typed for
    /// drift defence (an internal-tier role would land in `Unknown`).
    pub user_type: Option<UserType>,
    /// 100% present in the survey; `cli` or `sdk-cli`. New entrypoints
    /// land in [`Entrypoint::Unknown`].
    pub entrypoint: Option<Entrypoint>,
    pub is_sidechain: Option<bool>,

    // ── Assistant-specific carriers ──
    /// **REQUIRED (v2 strict).** Survey §3 assistant row: 100% present on
    /// every assistant entry including the 59 synthetic api-error entries
    /// (whose model is `<synthetic>` but who still carry `message.content`
    /// with the failure text). Encoding required-ness in the type keeps the
    /// "stated invariant matches reality" discipline that drives v2.
    pub message: ApiMessage,
    pub request_id: Option<String>,
    pub agent_id: Option<String>,
    pub slug: Option<String>,

    // ── Error / status surface (synthetic and API-error scenarios) ──
    //
    // Synthetic assistant entries (model = `<synthetic>`) carry these fields
    // when the CLI failed to reach the API. Survey §3: `isApiErrorMessage`
    // 0.3%, `error` 0.1%, `apiErrorStatus` 0.1%. Real-data sample (25,601
    // assistant entries on this machine, June 2026):
    //   - `error` is present in 35 entries; always co-occurs with
    //     `isApiErrorMessage: true` and `model: "<synthetic>"`.
    //   - `isApiErrorMessage: false` appears alone in 25 entries (no `error`,
    //     no `apiErrorStatus`) — likely a synthetic non-error marker.
    //   - `apiErrorStatus` only fires when the failure was an HTTP error
    //     (401/403/429/500/529); absent for network/cert/timeout failures.
    /// Typed error category. Real-data values: `rate_limit`,
    /// `authentication_failed`, `server_error`, `oauth_org_not_allowed`,
    /// literal `"unknown"`. New strings degrade to [`AssistantError::Other`].
    pub error: Option<AssistantError>,
    /// Optional free-form companion detail. 0 hits across 25,601 assistant
    /// entries in the survey but kept because the wire shape is documented
    /// (Anthropic API error responses carry it); re-evaluate if it stays
    /// empty across the next year.
    pub error_details: Option<String>,
    /// Marker that this entry is a CLI-synthesized placeholder. When `true`,
    /// `error` is also present (35/35 surveyed cases). When `false` (25/25
    /// surveyed cases), no other error fields are set — likely a non-error
    /// synthetic marker the CLI emits for tracking.
    pub is_api_error_message: Option<bool>,
    /// HTTP status code carried by some API-error entries (~0.1% in the
    /// survey). Only present for HTTP-level failures (e.g. 401/403/429/5xx);
    /// absent for cert/network/timeout failures where `error == "unknown"`.
    pub api_error_status: Option<u16>,

    // Removed (v2 zero-sample cleanup):
    //   • `api_error` (Option<String>) — 0 hits in 25,429 surveyed entries,
    //     no callsite anywhere in the workspace. The closely-named `error`
    //     above (which IS observed) absorbs the same conceptual slot.
    //   • `is_virtual`  (Option<bool>)  — 0 hits, no callsite, no doc anchor.
    //   • `advisor_model` (Option<String>) — 0 hits, no callsite, no doc
    //     anchor. Re-introduce as `Option<T>` keyed on real observed values
    //     if/when one of these reappears in production data.

    // ── Attribution family (Claude Code 2.1.138+) ──
    /// Triggering plugin (e.g. `"superpowers"`). 3.1% of assistant entries.
    pub attribution_plugin: Option<String>,
    /// Triggering skill (e.g. `"superpowers:brainstorming"`). 12.6%.
    pub attribution_skill: Option<String>,
    /// Spawning sub-agent name (30% of assistant entries — most common
    /// attribution).
    pub attribution_agent: Option<String>,
    /// MCP server attribution (3.1%, always co-occurs with the next field).
    pub attribution_mcp_server: Option<String>,
    /// MCP tool attribution (3.1%).
    pub attribution_mcp_tool: Option<String>,
    // Teammates feature: deliberately not modelled. `teamName`, `agentName`,
    // `agentColor` have 0 hits in the surveyed dataset. Serde silently drops
    // unknown keys so future teammates-enabled sessions still parse cleanly.
}

impl DagNode for AssistantEntry {
    fn uuid(&self) -> Option<&str> {
        self.uuid.as_deref()
    }
    fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    fn timestamp(&self) -> Option<&str> {
        self.timestamp.as_deref()
    }
    fn parent_uuid(&self) -> Option<&str> {
        Some(self.parent_uuid.as_str())
    }
    fn is_sidechain(&self) -> Option<bool> {
        self.is_sidechain
    }
}

/// The inner API message returned by Claude — model info, usage, content.
///
/// `stop_details` and `container` are *deliberately omitted*: both appear in
/// every assistant entry's JSONL but are always `null` (ghost keys). The v2
/// design philosophy says "do not model fields whose value is always null".
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiMessage {
    pub id: Option<String>,
    pub model: Option<String>,
    pub role: Option<String>,
    /// Low-cardinality enum; new values land in [`StopReason::Unknown`].
    pub stop_reason: Option<StopReason>,
    /// Custom stop sequence (rare; ~0.3%). The wider JSONL ecosystem also
    /// reports `null` here, which deserializes to `None`.
    pub stop_sequence: Option<String>,
    pub usage: Option<Usage>,
    pub content: Option<Vec<ContentBlock>>,
    /// Cache diagnostics when a cache miss occurred (~5% of turns).
    pub diagnostics: Option<Diagnostics>,
    /// Context-management metadata (rare ~0.1%). Shape is API-defined and
    /// not modelled at field level — we keep the raw object so analysers can
    /// poke at it without forcing a typed struct that would break on drift.
    pub context_management: Option<serde_json::Value>,
}

/// Token usage statistics for a single API call.
///
/// All `*_tokens` fields are guaranteed present in real data for non-synthetic
/// entries (survey §3 assistant section), but stay `Option<u64>` because
/// synthetic / api_error entries omit `usage` entirely (`message.usage = None`
/// from the perspective of [`ApiMessage`]).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation: Option<CacheCreation>,
    pub server_tool_use: Option<ServerToolUse>,
    /// Anthropic API service tier; real-data values: `standard` only.
    pub service_tier: Option<ServiceTier>,
    /// Geographic region tag; real-data values: `not_available`, `""`.
    /// Empty string lands in [`InferenceGeo::Empty`] as a distinct variant
    /// (it accounts for ~35% of carrying turns).
    pub inference_geo: Option<InferenceGeo>,
    /// `iterations` appears as `[]` (legacy) or `[{...}]` arrays in real
    /// data; we keep the raw Value because the inner shape is undocumented
    /// and rarely consumed downstream.
    pub iterations: Option<serde_json::Value>,
    /// Request speed bucket; real-data values: `standard` only.
    pub speed: Option<Speed>,
}

/// Cache-creation token breakdown by TTL bucket.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheCreation {
    pub ephemeral_5m_input_tokens: Option<u64>,
    pub ephemeral_1h_input_tokens: Option<u64>,
}

/// Server-side tool usage counters (web search / fetch).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerToolUse {
    pub web_search_requests: Option<u64>,
    pub web_fetch_requests: Option<u64>,
}

/// `message.diagnostics` — cache-miss explainer.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Diagnostics {
    pub cache_miss_reason: Option<CacheMissReason>,
}

/// Inner structure of a `cache_miss_reason` payload.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheMissReason {
    #[serde(rename = "type")]
    pub kind: Option<CacheMissReasonKind>,
    pub cache_missed_input_tokens: Option<u64>,
}

/// A content block inside an API message.
///
/// Internally tagged by `type`. Unknown future block types degrade to
/// [`ContentBlock::Other`] so that the surrounding `ApiMessage` still parses.
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
            "agentId": "agent-xyz"
        }"##;

        let entry: AssistantEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.uuid.as_deref(), Some("a-uuid-001"));
        assert_eq!(entry.parent_uuid, "p-uuid-001");
        assert_eq!(entry.is_sidechain, Some(false));
        assert_eq!(entry.session_id.as_deref(), Some("sess-abc-123"));
        assert_eq!(entry.request_id.as_deref(), Some("req_001"));
        assert_eq!(entry.agent_id.as_deref(), Some("agent-xyz"));

        let msg = &entry.message;
        assert_eq!(msg.id.as_deref(), Some("msg_01XYZ"));
        assert_eq!(msg.model.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(msg.stop_reason, Some(StopReason::EndTurn));

        let usage = msg.usage.as_ref().unwrap();
        assert_eq!(usage.input_tokens, Some(3));
        assert_eq!(usage.output_tokens, Some(108));
        assert_eq!(usage.cache_creation_input_tokens, Some(1281));
        assert_eq!(usage.cache_read_input_tokens, Some(15204));

        let cache = usage.cache_creation.as_ref().unwrap();
        assert_eq!(cache.ephemeral_5m_input_tokens, Some(1281));
        assert_eq!(cache.ephemeral_1h_input_tokens, Some(0));

        let content = msg.content.as_ref().unwrap();
        assert_eq!(content.len(), 3);
        match &content[2] {
            ContentBlock::ToolUse { id, name, .. } => {
                assert_eq!(id.as_deref(), Some("toolu_01ABC"));
                assert_eq!(name.as_deref(), Some("Bash"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn parse_assistant_minimal_via_entry_enum() {
        // Parent_uuid is required — even tests that don't care about it must
        // provide one. The "always-has-a-parent" invariant is exactly what
        // v2 encodes.
        let json = r#"{
            "type":"assistant",
            "uuid":"a1",
            "parentUuid":"p",
            "sessionId":"s1",
            "message":{
                "model":"claude-opus-4-6",
                "role":"assistant",
                "stop_reason":"end_turn",
                "usage":{"input_tokens":5,"output_tokens":10},
                "content":[{"type":"text","text":"hi"}]
            }
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::Assistant(a) => {
                assert_eq!(a.parent_uuid, "p");
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn parse_diagnostics_real_shape() {
        let json = r#"{
            "type":"assistant",
            "uuid":"a1",
            "parentUuid":"p",
            "sessionId":"s1",
            "message":{
                "model":"claude-opus-4-6",
                "role":"assistant",
                "diagnostics":{"cache_miss_reason":{"type":"tools_changed","cache_missed_input_tokens":89018}},
                "usage":{"input_tokens":1,"output_tokens":1},
                "content":[]
            }
        }"#;
        let entry: AssistantEntry = serde_json::from_str(json).unwrap();
        let diag = entry.message.diagnostics.unwrap();
        let cmr = diag.cache_miss_reason.unwrap();
        assert_eq!(cmr.kind, Some(CacheMissReasonKind::ToolsChanged));
        assert_eq!(cmr.cache_missed_input_tokens, Some(89018));
    }

    #[test]
    fn parse_stop_reason_unknown_degrades_softly() {
        let json = r#"{
            "type":"assistant",
            "uuid":"a1",
            "parentUuid":"p",
            "sessionId":"s1",
            "message":{
                "model":"claude-opus-4-6",
                "role":"assistant",
                "stop_reason":"future_reason_xyz",
                "usage":{"input_tokens":1,"output_tokens":1},
                "content":[]
            }
        }"#;
        let entry: AssistantEntry = serde_json::from_str(json).unwrap();
        let msg = entry.message;
        assert_eq!(msg.stop_reason, Some(StopReason::Unknown));
    }

    #[test]
    fn ghost_keys_stop_details_container_are_dropped() {
        // Real JSONL carries these as null. Ensure we still parse cleanly
        // even when they're present. We don't expose them as fields.
        let json = r#"{
            "type":"assistant",
            "uuid":"a1",
            "parentUuid":"p",
            "sessionId":"s1",
            "message":{
                "model":"claude-opus-4-6",
                "role":"assistant",
                "stop_reason":"end_turn",
                "stop_details":null,
                "container":null,
                "usage":{"input_tokens":1,"output_tokens":1},
                "content":[]
            }
        }"#;
        let _: AssistantEntry = serde_json::from_str(json).unwrap();
    }

    #[test]
    fn parse_attribution_family() {
        let json = r#"{
            "type":"assistant",
            "uuid":"a1",
            "parentUuid":"p",
            "sessionId":"s1",
            "attributionAgent":"general-purpose",
            "attributionMcpServer":"plugin:fs",
            "attributionMcpTool":"read_file",
            "attributionPlugin":"superpowers",
            "attributionSkill":"superpowers:brainstorming",
            "apiErrorStatus":429,
            "message":{
                "model":"claude-opus-4-6",
                "role":"assistant",
                "usage":{"input_tokens":1,"output_tokens":1},
                "content":[]
            }
        }"#;
        let entry: AssistantEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.attribution_agent.as_deref(), Some("general-purpose"));
        assert_eq!(entry.attribution_mcp_server.as_deref(), Some("plugin:fs"));
        assert_eq!(entry.attribution_mcp_tool.as_deref(), Some("read_file"));
        assert_eq!(entry.attribution_plugin.as_deref(), Some("superpowers"));
        assert_eq!(
            entry.attribution_skill.as_deref(),
            Some("superpowers:brainstorming")
        );
        assert_eq!(entry.api_error_status, Some(429));
    }

    #[test]
    fn content_block_unknown_type_becomes_other() {
        let json = r#"{"type":"server_tool_use","id":"st_01","name":"web_search"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::Other => {}
            other => panic!("expected Other, got {other:?}"),
        }
    }

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
}
