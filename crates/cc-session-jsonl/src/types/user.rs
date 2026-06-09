//! `UserEntry` — a JSONL entry where `type == "user"`.
//!
//! Despite the name, **83% of these are tool results being fed back to the
//! model**, not actual user input (see `docs/session-data-model.md` §3). The
//! `content_kind()` accessor on `UserMessage` distinguishes the two.
//!
//! v2 design notes vs the old `transcript_entry!` macro:
//!
//! - All shared transcript fields are spelled out here (no macro). The survey
//!   confirmed `promptId`/`teamName`/`agentName`/`agentColor` are *not*
//!   universal — they are user-specific or zero-cardinality on this dataset
//!   — so they don't get injected into other entry types any more.
//! - `parentUuid` is `Option<String>` here (96.3% real-data fill rate). It is
//!   `None` for session-root user entries and rewind-branch roots.
//! - New typed fields landed from v2 design (survey §3 user row): `origin`,
//!   `prompt_source`, `interrupted_message_id`, `is_compact_summary`,
//!   `is_visible_in_transcript_only`, `mcp_meta`, `image_paste_ids`.
//! - Low-cardinality enum fields (`prompt_source`, `origin.kind`) carry a
//!   `Unknown` variant so unfamiliar future values degrade just that field.

use serde::{Deserialize, Serialize};

use super::common::{DagNode, OriginKind, PromptSource};

/// A user-authored message entry in a Claude Code session.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserEntry {
    // ── 9 truly-universal DAG fields ──
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub cwd: Option<String>,
    pub version: Option<String>,
    pub git_branch: Option<String>,
    pub user_type: Option<String>,
    pub entrypoint: Option<String>,
    pub is_sidechain: Option<bool>,

    // ── Logical-parent fallback (rare; not used by user entries today but
    //    seen on `compact_boundary` system entries; tolerate the key). ──
    pub logical_parent_uuid: Option<String>,

    // ── Identity / routing ──
    /// Identifier of the prompt the user sent (UUID-shape, 99.9% fill rate).
    pub prompt_id: Option<String>,
    /// Optional slug from the surrounding session metadata.
    pub slug: Option<String>,
    /// Sub-agent identifier (subagent trunk entries carry this).
    pub agent_id: Option<String>,
    /// Teammates feature — zero hits on this machine but present in the API.
    pub team_name: Option<String>,
    pub agent_name: Option<String>,
    pub agent_color: Option<String>,

    // ── Message body & tool-result fallback ──
    pub message: Option<UserMessage>,
    /// Legacy tool result echoed at the top level (38.6% fill rate; older
    /// versions and tool-result entries from 2.1.71+). Free-shape → `Value`.
    pub tool_use_result: Option<serde_json::Value>,
    /// tool-call ↔ tool-result link, capital-ID spelling (new since 2.1.71).
    #[serde(rename = "sourceToolUseID")]
    pub source_tool_use_id: Option<String>,
    /// Older spelling of the same link (still produced in some paths).
    #[serde(rename = "sourceToolAssistantUUID")]
    pub source_tool_assistant_uuid: Option<String>,

    // ── Mode / meta flags ──
    pub permission_mode: Option<String>,
    /// Tool-use placeholder marker (Claude Code 2.1.104+).
    pub is_meta: Option<bool>,

    // ── v2.1.140+ ★ new fields (survey §3 user row) ──
    /// What surface generated the prompt — `text` or `slashCommand` in real
    /// data, with `Unknown` as the soft-landing for new values.
    pub prompt_source: Option<PromptSource>,
    /// Where the prompt came from (e.g. an IDE integration). Carries its own
    /// soft-landing `OriginKind::Unknown` for unfamiliar `origin.kind` values.
    pub origin: Option<Origin>,
    /// uuid of an assistant message the user interrupted.
    pub interrupted_message_id: Option<String>,
    /// Marks the synthetic user entry that introduces a compact summary on
    /// resume after context collapse.
    pub is_compact_summary: Option<bool>,
    /// Hide this entry from the rendered transcript (still in JSONL).
    pub is_visible_in_transcript_only: Option<bool>,
    /// MCP-tool metadata associated with this user turn.
    pub mcp_meta: Option<McpMeta>,
    /// Identifiers for images pasted into the prompt. Real data emits both
    /// string ids (legacy) and integer ids (newer); the element type stays
    /// raw JSON so either form parses without StructDrift.
    pub image_paste_ids: Option<Vec<serde_json::Value>>,
}

impl DagNode for UserEntry {
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
        self.parent_uuid.as_deref()
    }
    fn is_sidechain(&self) -> Option<bool> {
        self.is_sidechain
    }
}

/// The content of a user message.
///
/// The Claude Code wire format puts the actual payload under `message.content`
/// as either a plain string (legacy) or an array of content blocks. We keep
/// the raw value and expose [`UserMessage::content_kind`] for callers that
/// need to discriminate between the user's own input and tool results
/// (`docs/session-data-model.md` §3).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserMessage {
    pub role: Option<String>,
    pub content: Option<serde_json::Value>,
}

/// Discriminator for the polymorphic `content` field on [`UserMessage`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserContentKind {
    /// Legacy plain string prompt.
    String,
    /// Array containing only `tool_result` blocks — this is the API's
    /// channel for feeding tool output back to the model, not user speech.
    ToolResult,
    /// Array of `text` blocks only (multi-segment user input).
    Text,
    /// Array containing at least one image plus optional text (richer user
    /// input).
    ImageText,
    /// Anything else (mixed unexpected shapes, empty arrays, etc.).
    Mixed,
}

impl UserMessage {
    /// Classify the wire-format `content` shape — see the description on
    /// [`UserContentKind`]. Returns `None` when `content` itself is absent.
    pub fn content_kind(&self) -> Option<UserContentKind> {
        let value = self.content.as_ref()?;
        if value.is_string() {
            return Some(UserContentKind::String);
        }
        let arr = value.as_array()?;
        let mut has_tool_result = false;
        let mut has_text = false;
        let mut has_image = false;
        let mut has_other = false;
        for block in arr {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("tool_result") => has_tool_result = true,
                Some("text") => has_text = true,
                Some("image") => has_image = true,
                _ => has_other = true,
            }
        }
        let kind = match (has_tool_result, has_text, has_image, has_other) {
            (true, false, false, false) => UserContentKind::ToolResult,
            (false, true, false, false) => UserContentKind::Text,
            (false, _, true, false) => UserContentKind::ImageText,
            _ => UserContentKind::Mixed,
        };
        Some(kind)
    }
}

/// Origin metadata carried on some user entries (Claude Code 2.1.140+).
///
/// The richer shape (extra labelling fields) varies between integrations;
/// we model the discriminator and keep everything else off the type to
/// avoid coupling to a known-unstable surface.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    pub kind: Option<OriginKind>,
}

/// MCP-tool metadata block referenced from a user entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpMeta {
    /// Free-shape structured content the MCP tool emitted. Shape varies by
    /// tool implementation, so we keep the raw value.
    pub structured_content: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;

    #[test]
    fn parse_user_entry_with_content_array() {
        let json = r#"{
            "type": "user",
            "parentUuid": "p-001",
            "isSidechain": false,
            "uuid": "u-001",
            "timestamp": "2026-03-16T13:50:00.000Z",
            "sessionId": "sess-001",
            "cwd": "/Users/loki/project",
            "version": "2.0.77",
            "gitBranch": "feature-x",
            "userType": "external",
            "message": {
                "role": "user",
                "content": [
                    {"type": "text", "text": "Please fix the bug in main.rs"},
                    {"type": "text", "text": "It crashes on startup"}
                ]
            }
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.uuid.as_deref(), Some("u-001"));
        assert_eq!(entry.parent_uuid.as_deref(), Some("p-001"));
        assert_eq!(entry.is_sidechain, Some(false));
        assert_eq!(entry.timestamp.as_deref(), Some("2026-03-16T13:50:00.000Z"));
        assert_eq!(entry.session_id.as_deref(), Some("sess-001"));
        assert_eq!(entry.cwd.as_deref(), Some("/Users/loki/project"));
        assert_eq!(entry.version.as_deref(), Some("2.0.77"));
        assert_eq!(entry.git_branch.as_deref(), Some("feature-x"));
        assert_eq!(entry.user_type.as_deref(), Some("external"));

        let msg = entry.message.as_ref().unwrap();
        assert_eq!(msg.role.as_deref(), Some("user"));
        let content = msg.content.as_ref().unwrap();
        assert!(content.is_array());
        assert_eq!(content.as_array().unwrap().len(), 2);
        assert_eq!(content[0]["text"], "Please fix the bug in main.rs");
    }

    #[test]
    fn parse_user_entry_with_plain_string_content() {
        let json = r#"{
            "type": "user",
            "uuid": "u-002",
            "sessionId": "sess-002",
            "message": {
                "role": "user",
                "content": "Just a plain text prompt"
            }
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let msg = entry.message.as_ref().unwrap();
        let content = msg.content.as_ref().unwrap();
        assert!(content.is_string());
        assert_eq!(content.as_str().unwrap(), "Just a plain text prompt");
        assert_eq!(msg.content_kind(), Some(UserContentKind::String));
    }

    #[test]
    fn parse_user_via_entry_enum() {
        let json = r#"{
            "type": "user",
            "uuid": "u-003",
            "sessionId": "sess-003",
            "message": {"role": "user", "content": "hello"}
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::User(u) => {
                assert_eq!(u.uuid.as_deref(), Some("u-003"));
            }
            other => panic!("Expected User, got: {other:?}"),
        }
    }

    #[test]
    fn parse_user_with_is_meta_and_source_tool_use_id() {
        // Two-field test: `isMeta` (bool) and `sourceToolUseID` (uppercase ID).
        let json = r#"{
            "type": "user",
            "uuid": "u-meta-001",
            "sessionId": "sess-meta",
            "timestamp": "2026-05-13T10:00:00.000Z",
            "isMeta": false,
            "sourceToolUseID": "toolu_01XXXX"
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.is_meta, Some(false));
        assert_eq!(entry.source_tool_use_id.as_deref(), Some("toolu_01XXXX"));
        assert!(entry.source_tool_assistant_uuid.is_none());
        assert!(entry.permission_mode.is_none());
        assert!(entry.tool_use_result.is_none());
    }

    #[test]
    fn parse_user_with_source_tool_assistant_uuid_legacy() {
        let json = r#"{
            "type": "user",
            "uuid": "u-asst-uuid-001",
            "sessionId": "sess-asst",
            "sourceToolAssistantUUID": "asst_abcdef1234567890"
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(
            entry.source_tool_assistant_uuid.as_deref(),
            Some("asst_abcdef1234567890")
        );
        assert!(entry.source_tool_use_id.is_none());
    }

    #[test]
    fn parse_user_with_permission_mode_inline() {
        let json = r#"{
            "type": "user",
            "uuid": "u-perm-001",
            "sessionId": "sess-perm",
            "permissionMode": "bypassPermissions",
            "message": {"role": "user", "content": "run it"}
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::User(u) => {
                assert_eq!(u.permission_mode.as_deref(), Some("bypassPermissions"));
            }
            Entry::PermissionMode(_) => {
                panic!(
                    "permissionMode inline on user entry was misclassified as PermissionMode entry"
                );
            }
            other => panic!("Expected User, got: {other:?}"),
        }
    }

    #[test]
    fn parse_user_with_tool_use_result() {
        let json = r#"{
            "type": "user",
            "uuid": "u-tur-001",
            "sessionId": "sess-tur",
            "toolUseResult": {"foo": "bar", "n": 42}
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let result = entry
            .tool_use_result
            .as_ref()
            .expect("tool_use_result must be Some");
        assert!(result.is_object(), "toolUseResult must be Value::Object");
        assert_eq!(result["foo"], "bar");
        assert_eq!(result["n"], 42);
    }

    #[test]
    fn parse_user_legacy_no_new_fields() {
        let json = r#"{
            "type": "user",
            "uuid": "u-legacy-001",
            "parentUuid": "p-legacy",
            "isSidechain": false,
            "timestamp": "2025-06-01T09:00:00.000Z",
            "sessionId": "sess-legacy",
            "cwd": "/home/user/project",
            "version": "2.0.77",
            "message": {"role": "user", "content": "Fix the build"}
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert!(entry.is_meta.is_none());
        assert!(entry.permission_mode.is_none());
        assert!(entry.tool_use_result.is_none());
        assert!(entry.source_tool_use_id.is_none());
        assert!(entry.source_tool_assistant_uuid.is_none());
        assert!(entry.prompt_source.is_none());
        assert!(entry.origin.is_none());
        assert!(entry.interrupted_message_id.is_none());
        assert!(entry.is_compact_summary.is_none());
        assert!(entry.is_visible_in_transcript_only.is_none());
        assert!(entry.mcp_meta.is_none());
        assert!(entry.image_paste_ids.is_none());
        assert_eq!(entry.uuid.as_deref(), Some("u-legacy-001"));
        assert_eq!(entry.version.as_deref(), Some("2.0.77"));
    }

    #[test]
    fn parse_user_with_prompt_source_slash_command() {
        let json = r#"{
            "type":"user",
            "uuid":"u-ps-1",
            "sessionId":"s1",
            "promptSource":"slashCommand"
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.prompt_source, Some(PromptSource::SlashCommand));
    }

    #[test]
    fn parse_user_with_prompt_source_unknown_degrades_softly() {
        let json = r#"{
            "type":"user",
            "uuid":"u-ps-2",
            "sessionId":"s1",
            "promptSource":"future_source_42"
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.prompt_source, Some(PromptSource::Unknown));
    }

    #[test]
    fn parse_user_with_origin_and_mcp_meta() {
        let json = r#"{
            "type":"user",
            "uuid":"u-or-1",
            "sessionId":"s1",
            "origin":{"kind":"ide"},
            "mcpMeta":{"structuredContent":{"hello":"world"}}
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let origin = entry.origin.as_ref().unwrap();
        // The survey calls out `ide`-class origins; unknown shapes degrade.
        assert!(origin.kind.is_some());
        let mcp = entry.mcp_meta.as_ref().unwrap();
        assert!(mcp.structured_content.is_some());
        let inner = mcp.structured_content.as_ref().unwrap();
        assert_eq!(inner["hello"], "world");
    }

    #[test]
    fn parse_user_with_interrupted_and_compact_flags() {
        let json = r#"{
            "type":"user",
            "uuid":"u-ic-1",
            "sessionId":"s1",
            "interruptedMessageId":"a-uuid-1",
            "isCompactSummary":true,
            "isVisibleInTranscriptOnly":false,
            "imagePasteIds":["paste-1","paste-2"]
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.interrupted_message_id.as_deref(), Some("a-uuid-1"));
        assert_eq!(entry.is_compact_summary, Some(true));
        assert_eq!(entry.is_visible_in_transcript_only, Some(false));
        let ids = entry.image_paste_ids.as_ref().unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0].as_str(), Some("paste-1"));
    }

    #[test]
    fn parse_user_with_integer_image_paste_ids() {
        // Production data observed integer ids like `[1]`, `[3]`; survey
        // assumed string-only. Element type is `serde_json::Value`, so this
        // must parse without StructDrift.
        let json = r#"{
            "type":"user",
            "uuid":"u-int-1",
            "sessionId":"s1",
            "imagePasteIds":[1,3,4]
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let ids = entry.image_paste_ids.as_ref().unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0].as_i64(), Some(1));
        assert_eq!(ids[1].as_i64(), Some(3));
    }

    #[test]
    fn content_kind_distinguishes_user_input_from_tool_result() {
        // Real-input case: array of text blocks.
        let user_input = UserMessage {
            role: Some("user".into()),
            content: Some(serde_json::json!([{"type":"text","text":"hi"}])),
        };
        assert_eq!(user_input.content_kind(), Some(UserContentKind::Text));

        // Tool result case: tool_result blocks only.
        let tool_result = UserMessage {
            role: Some("user".into()),
            content: Some(
                serde_json::json!([{"type":"tool_result","tool_use_id":"tu1","content":"done"}]),
            ),
        };
        assert_eq!(
            tool_result.content_kind(),
            Some(UserContentKind::ToolResult)
        );

        // Image + text: a real human-input mode (e.g. paste a screenshot).
        let image = UserMessage {
            role: Some("user".into()),
            content: Some(
                serde_json::json!([{"type":"image","source":{}},{"type":"text","text":"see this"}]),
            ),
        };
        assert_eq!(image.content_kind(), Some(UserContentKind::ImageText));

        // String legacy mode.
        let string = UserMessage {
            role: Some("user".into()),
            content: Some(serde_json::Value::String("hello".into())),
        };
        assert_eq!(string.content_kind(), Some(UserContentKind::String));

        // No content at all.
        let empty = UserMessage {
            role: None,
            content: None,
        };
        assert!(empty.content_kind().is_none());
    }
}
