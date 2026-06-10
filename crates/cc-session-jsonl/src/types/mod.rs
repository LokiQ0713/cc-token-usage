//! cc-session-jsonl v2 — strongly-typed Claude Code session entries.
//!
//! v2 design philosophy (see `docs/cc-session-jsonl-v2-field-survey.md`):
//!
//! - Every entry type owns its own field list — the legacy `transcript_entry!`
//!   macro is gone. Repeating the 9 truly-universal DAG fields by hand across
//!   ~15 structs is the deliberate price of letting each type model only what
//!   it really has.
//! - Low-cardinality enum-shaped fields (e.g. `stopReason`, `permissionMode`,
//!   `entrypoint`, `promptSource`, `origin.kind`) are Rust enums with
//!   `#[serde(other)] Unknown` so an unfamiliar future value degrades the
//!   single field rather than the whole entry.
//! - `parentUuid` is `Option<String>` everywhere except `AssistantEntry`,
//!   where it is `String` (assistant entries are always replies to something).
//! - `SystemEntry.body` and `AttachmentEntry.body` are tagged sub-enums
//!   driven by the inner `subtype` / `attachment.type` discriminator.
//! - Ghost keys (`message.stop_details`, `message.container`, both always
//!   `null` in real data) are deliberately not modelled.
//! - Three new entry types observed in the field — `started`, `result`,
//!   `bridge-session` — have no `uuid` and per the survey settle to `Ignored`.
//!   The default `Entry::deserialize` already routes them there because the
//!   types are not enumerated; no extra wiring is needed.

pub mod assistant;
pub mod attachment;
pub mod common;
pub mod context;
pub mod metadata;
pub mod progress;
pub mod system;
pub mod tracking;
pub mod user;
pub mod workflow;

pub use assistant::*;
pub use attachment::*;
pub use common::*;
pub use context::*;
pub use metadata::*;
pub use progress::*;
pub use system::*;
pub use tracking::*;
pub use user::*;
pub use workflow::*;

use serde::de;
use serde::{Deserialize, Serialize};

/// A structurally-preserved entry of an unrecognized JSONL entry type that
/// nonetheless carries DAG-continuity fields (`uuid` + `sessionId`).
///
/// Corresponds to claude-code-log's `PassthroughTranscriptEntry`: kept so
/// parent–child chains don't break, but carries no typed content beyond the
/// minimal graph fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassthroughEntry {
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub session_id: String,
    pub timestamp: Option<String>,
    /// The original `type` value (e.g. `"progress"`, `"agent-setting"`),
    /// recorded so callers can distinguish passthrough sub-flavours if needed.
    #[serde(rename = "type")]
    pub entry_type: String,
    pub is_sidechain: Option<bool>,
    pub agent_id: Option<String>,
}

impl common::DagNode for PassthroughEntry {
    fn uuid(&self) -> Option<&str> {
        Some(self.uuid.as_str())
    }
    fn session_id(&self) -> Option<&str> {
        Some(self.session_id.as_str())
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

/// All entry types in a Claude Code session JSONL file.
///
/// Serialization uses `#[serde(tag = "type")]` (derived). Deserialization
/// is implemented manually so that:
///   1. Known types whose payload fails to decode return
///      `ParseError::StructDrift` instead of a plain JSON error — this is
///      the "shape changed under us" signal the strict layer cares about.
///   2. Unknown/future entry types route to [`Entry::Passthrough`] when they
///      still carry DAG keys, or [`Entry::Ignored`] when they don't.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum Entry {
    #[serde(rename = "user")]
    User(UserEntry),
    #[serde(rename = "assistant")]
    Assistant(AssistantEntry),
    #[serde(rename = "system")]
    System(SystemEntry),
    #[serde(rename = "attachment")]
    Attachment(AttachmentEntry),
    #[serde(rename = "summary")]
    Summary(SummaryMessage),
    #[serde(rename = "custom-title")]
    CustomTitle(CustomTitleMessage),
    #[serde(rename = "ai-title")]
    AiTitle(AiTitleMessage),
    #[serde(rename = "last-prompt")]
    LastPrompt(LastPromptMessage),
    #[serde(rename = "task-summary")]
    TaskSummary(TaskSummaryMessage),
    #[serde(rename = "tag")]
    Tag(TagMessage),
    #[serde(rename = "agent-name")]
    AgentName(AgentNameMessage),
    #[serde(rename = "agent-color")]
    AgentColor(AgentColorMessage),
    #[serde(rename = "agent-setting")]
    AgentSetting(AgentSettingMessage),
    #[serde(rename = "pr-link")]
    PrLink(PrLinkMessage),
    #[serde(rename = "mode")]
    Mode(ModeEntry),
    #[serde(rename = "permission-mode")]
    PermissionMode(PermissionModeEntry),
    #[serde(rename = "progress")]
    Progress(ProgressEntry),
    #[serde(rename = "queue-operation")]
    QueueOperation(QueueOperationMessage),
    #[serde(rename = "speculation-accept")]
    SpeculationAccept(SpeculationAcceptMessage),
    #[serde(rename = "worktree-state")]
    WorktreeState(WorktreeStateEntry),
    #[serde(rename = "content-replacement")]
    ContentReplacement(ContentReplacementEntry),
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot(FileHistorySnapshotMessage),
    #[serde(rename = "attribution-snapshot")]
    AttributionSnapshot(AttributionSnapshotMessage),
    #[serde(rename = "marble-origami-commit")]
    ContextCollapseCommit(ContextCollapseCommitEntry),
    #[serde(rename = "marble-origami-snapshot")]
    ContextCollapseSnapshot(ContextCollapseSnapshotEntry),
    /// Unknown entry type that carries DAG-continuity fields (`uuid` +
    /// `sessionId`). Preserved so parent→child chains don't break. A
    /// non-zero count is an early signal of Claude Code JSONL format drift.
    #[serde(rename = "__passthrough")]
    Passthrough(PassthroughEntry),
    /// Unknown entry type without DAG fields (missing `uuid` or
    /// `sessionId`). Pure metadata that is safe to discard — does not
    /// participate in the DAG.
    #[serde(rename = "__ignored")]
    Ignored,
}

// ── Custom Deserialization ──────────────────────────────────────────
//
// The hand-written Deserialize implementation enforces three strict rules:
//
//   1. Known `type` strings are dispatched to their typed payloads. If the
//      typed payload fails (e.g. a `usage.input_tokens` arrives as a string),
//      the deserializer **records the `type` and the inner error** via
//      [`StructDriftMarker`]; the parser layer reads that marker and emits
//      `ParseError::StructDrift`. We deliberately do not soft-degrade —
//      "known type, broken shape" is the canary the lenient reader counts.
//
//   2. Unknown `type` strings are inspected for DAG keys (`uuid` +
//      `sessionId`, both present and non-empty). Both present → `Passthrough`
//      (so the DAG stays connected). Either missing → `Ignored`.
//
//   3. The legacy `agentId` promotion (top-level `agentId` is sometimes
//      missing on subagent trunk entries — the real id is buried in
//      `toolUseResult.agentId`) is preserved here so downstream consumers
//      read one uniform field.

/// Sentinel error carried out of the `Deserialize` impl when a known type's
/// payload fails to decode. The outer `parse_entry` recovers the original
/// `type` name from this marker and turns it into [`crate::parser::ParseError::StructDrift`].
///
/// Encoded into a serde error message as `__cc_struct_drift__<type>:<inner>`
/// so it round-trips through `D::Error::custom` cleanly.
#[doc(hidden)]
pub(crate) const STRUCT_DRIFT_PREFIX: &str = "__cc_struct_drift__";

impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        // Capture as String so the borrow doesn't fight `from_value(value)`
        // moves inside the `known!` macro below.
        let entry_type: String = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let entry_type_ref: &str = entry_type.as_str();

        // Helper macro: turn a typed-payload deserialization failure for a
        // known `type` discriminator into the STRUCT_DRIFT_PREFIX sentinel
        // string. `parse_entry` reads that prefix in `From<serde_json::Error>`
        // and lifts it back to `ParseError::StructDrift`.
        macro_rules! known {
            ($variant:ident) => {
                match serde_json::from_value(value) {
                    Ok(v) => Entry::$variant(v),
                    Err(e) => {
                        return Err(de::Error::custom(format!(
                            "{}{}:{}",
                            STRUCT_DRIFT_PREFIX, entry_type, e
                        )));
                    }
                }
            };
        }

        let entry = match entry_type_ref {
            "user" => {
                let mut user: UserEntry = match serde_json::from_value(value) {
                    Ok(v) => v,
                    Err(e) => {
                        return Err(de::Error::custom(format!(
                            "{}{}:{}",
                            STRUCT_DRIFT_PREFIX, &entry_type, e
                        )));
                    }
                };
                // ── agentId promotion ──
                // Trunk entries carry the subagent identifier inside
                // `toolUseResult.agentId`, not at top level.  Promote
                // it so downstream code reads one field uniformly.
                if user.agent_id.is_none() {
                    if let Some(ref tur) = user.tool_use_result {
                        if let Some(aid) = tur.get("agentId").and_then(|v| v.as_str()) {
                            if !aid.is_empty() {
                                user.agent_id = Some(aid.to_string());
                            }
                        }
                    }
                }
                Entry::User(user)
            }
            "assistant" => known!(Assistant),
            "system" => known!(System),
            "attachment" => known!(Attachment),
            "summary" => known!(Summary),
            "custom-title" => known!(CustomTitle),
            "ai-title" => known!(AiTitle),
            "last-prompt" => known!(LastPrompt),
            "task-summary" => known!(TaskSummary),
            "tag" => known!(Tag),
            "agent-name" => known!(AgentName),
            "agent-color" => known!(AgentColor),
            "agent-setting" => known!(AgentSetting),
            "pr-link" => known!(PrLink),
            "mode" => known!(Mode),
            "permission-mode" => known!(PermissionMode),
            "progress" => known!(Progress),
            "queue-operation" => known!(QueueOperation),
            "speculation-accept" => known!(SpeculationAccept),
            "worktree-state" => known!(WorktreeState),
            "content-replacement" => known!(ContentReplacement),
            "file-history-snapshot" => known!(FileHistorySnapshot),
            "attribution-snapshot" => known!(AttributionSnapshot),
            "marble-origami-commit" => known!(ContextCollapseCommit),
            "marble-origami-snapshot" => known!(ContextCollapseSnapshot),
            // ── Unknown types — classify by DAG fields ──
            _ => {
                let has_uuid = value
                    .get("uuid")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false);
                let has_sid = value
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false);

                if has_uuid && has_sid {
                    match serde_json::from_value(value) {
                        Ok(p) => Entry::Passthrough(p),
                        // Even Passthrough is best-effort: if for some reason
                        // we can't decode the minimal shape, fall through to
                        // Ignored rather than fail the whole line.
                        Err(_) => Entry::Ignored,
                    }
                } else {
                    Entry::Ignored
                }
            }
        };

        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Verify serde tag routing for each entry type ──

    #[test]
    fn route_user() {
        let json = r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"hi"}}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::User(_)));
    }

    #[test]
    fn route_assistant() {
        let json = r#"{"type":"assistant","uuid":"a1","parentUuid":"p","sessionId":"s1","message":{"model":"claude-opus-4-6","role":"assistant","content":[]}}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Assistant(_)));
    }

    #[test]
    fn route_system() {
        let json = r#"{"type":"system","uuid":"sys1","sessionId":"s1","subtype":"informational","content":"x","level":"info","isMeta":false}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::System(_)));
    }

    #[test]
    fn route_attachment() {
        let json = r#"{"type":"attachment","uuid":"att1","sessionId":"s1"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Attachment(_)));
    }

    #[test]
    fn route_summary() {
        let json = r#"{"type":"summary","leafUuid":"l1","summary":"done"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Summary(_)));
    }

    #[test]
    fn route_custom_title() {
        let json = r#"{"type":"custom-title","sessionId":"s1","customTitle":"My Title"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::CustomTitle(_)));
    }

    #[test]
    fn route_ai_title() {
        let json = r#"{"type":"ai-title","sessionId":"s1","aiTitle":"AI Title"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::AiTitle(_)));
    }

    #[test]
    fn route_last_prompt() {
        let json = r#"{"type":"last-prompt","sessionId":"s1","lastPrompt":"fix it"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::LastPrompt(_)));
    }

    #[test]
    fn route_task_summary() {
        let json = r#"{"type":"task-summary","sessionId":"s1","summary":"all done","timestamp":"2026-01-01T00:00:00Z"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::TaskSummary(_)));
    }

    #[test]
    fn route_tag() {
        let json = r#"{"type":"tag","sessionId":"s1","tag":"important"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Tag(_)));
    }

    #[test]
    fn route_agent_name() {
        let json = r#"{"type":"agent-name","sessionId":"s1","agentName":"Builder"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::AgentName(_)));
    }

    #[test]
    fn route_agent_color() {
        let json = r##"{"type":"agent-color","sessionId":"s1","agentColor":"#f00"}"##;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::AgentColor(_)));
    }

    #[test]
    fn route_agent_setting() {
        let json = r#"{"type":"agent-setting","sessionId":"s1","agentSetting":"custom-def"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::AgentSetting(_)));
    }

    #[test]
    fn route_pr_link() {
        let json = r#"{"type":"pr-link","sessionId":"s1","prNumber":1,"prUrl":"http://x","prRepository":"r"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::PrLink(_)));
    }

    #[test]
    fn route_mode() {
        let json = r#"{"type":"mode","sessionId":"s1","mode":"code"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Mode(_)));
    }

    #[test]
    fn route_permission_mode() {
        let json =
            r#"{"type":"permission-mode","sessionId":"s1","permissionMode":"bypassPermissions"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::PermissionMode(_)));
    }

    #[test]
    fn route_progress() {
        let json = r#"{"type":"progress","uuid":"u1","sessionId":"s1","data":{"type":"hook_progress","hookEvent":"PostToolUse"}}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Progress(_)));
    }

    #[test]
    fn route_queue_operation() {
        let json = r#"{"type":"queue-operation","sessionId":"s1","operation":"enqueue"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::QueueOperation(_)));
    }

    #[test]
    fn route_speculation_accept() {
        let json = r#"{"type":"speculation-accept","timestamp":"2026-01-01T00:00:00Z","timeSavedMs":100.0}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::SpeculationAccept(_)));
    }

    #[test]
    fn route_worktree_state() {
        let json = r#"{"type":"worktree-state","sessionId":"s1"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::WorktreeState(_)));
    }

    #[test]
    fn route_content_replacement() {
        let json = r#"{"type":"content-replacement","sessionId":"s1","replacements":[]}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::ContentReplacement(_)));
    }

    #[test]
    fn route_file_history_snapshot() {
        let json = r#"{"type":"file-history-snapshot","sessionId":"s1","snapshot":{}}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::FileHistorySnapshot(_)));
    }

    #[test]
    fn route_attribution_snapshot() {
        let json = r#"{"type":"attribution-snapshot","sessionId":"s1","snapshot":{}}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::AttributionSnapshot(_)));
    }

    #[test]
    fn route_marble_origami_commit() {
        let json = r#"{"type":"marble-origami-commit","sessionId":"s1","collapseId":"0001","summaryUuid":"su1","summaryContent":"<collapsed>x</collapsed>","summary":"x","firstArchivedUuid":"f1","lastArchivedUuid":"l1"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::ContextCollapseCommit(_)));
    }

    #[test]
    fn route_marble_origami_snapshot() {
        let json = r#"{"type":"marble-origami-snapshot","sessionId":"s1","staged":[],"armed":false,"lastSpawnTokens":0}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::ContextCollapseSnapshot(_)));
    }

    // ── Edge cases ──

    #[test]
    fn unknown_type_without_dag_fields_is_ignored() {
        let json = r#"{"type":"future-feature-xyz","sessionId":"s1","data":"something"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Ignored));
    }

    #[test]
    fn unknown_type_with_dag_fields_is_passthrough() {
        let json = r#"{"type":"future-feature-abc","uuid":"u1","sessionId":"s1","data":"x"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Passthrough(_)));
    }

    #[test]
    fn new_started_type_without_uuid_is_ignored() {
        // Survey §6: `started` carries no uuid/sessionId. Falls through to Ignored.
        let json = r#"{"type":"started","key":"v2:abc","agentId":"x"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Ignored));
    }

    #[test]
    fn new_result_type_without_uuid_is_ignored() {
        let json = r#"{"type":"result","key":"v2:abc","agentId":"x","result":{}}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Ignored));
    }

    #[test]
    fn new_bridge_session_without_uuid_is_ignored() {
        let json = r#"{"type":"bridge-session","sessionId":"s1","bridgeSessionId":"b1","lastSequenceNum":1}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        // bridge-session has sessionId but no uuid → Ignored per survey §6.
        assert!(matches!(entry, Entry::Ignored));
    }

    #[test]
    fn malformed_json_is_error() {
        let json = r#"{"type": "user", broken"#;
        let result = serde_json::from_str::<Entry>(json);
        assert!(result.is_err());
    }

    #[test]
    fn empty_object_is_ignored() {
        // No "type" field and no DAG fields → Ignored
        let json = r#"{}"#;
        let entry = serde_json::from_str::<Entry>(json).unwrap();
        assert!(matches!(entry, Entry::Ignored));
    }

    #[test]
    fn type_field_is_null_is_ignored() {
        let json = r#"{"type": null}"#;
        let entry = serde_json::from_str::<Entry>(json).unwrap();
        assert!(matches!(entry, Entry::Ignored));
    }

    #[test]
    fn type_field_is_number_is_ignored() {
        let json = r#"{"type": 42}"#;
        let entry = serde_json::from_str::<Entry>(json).unwrap();
        assert!(matches!(entry, Entry::Ignored));
    }

    #[test]
    fn agent_id_promoted_from_tool_use_result() {
        // trunk entry: agentId is nested inside toolUseResult, NOT at top level
        let json = r#"{
            "type":"user","uuid":"u1","sessionId":"s1",
            "toolUseResult":{"status":"completed","agentId":"ac5b46b9d216674b4","prompt":"audit"},
            "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tu1","content":"done"}]}
        }"#;
        let entry = serde_json::from_str::<Entry>(json).unwrap();
        match entry {
            Entry::User(u) => {
                assert_eq!(u.agent_id.as_deref(), Some("ac5b46b9d216674b4"));
                assert!(u.tool_use_result.is_some());
            }
            other => panic!("Expected User, got: {other:?}"),
        }
    }

    #[test]
    fn passthrough_implements_dag_node_trait() {
        // PassthroughEntry's whole reason for existing is keeping parent→child
        // chains intact across unknown types; if it doesn't implement DagNode,
        // any DAG consumer iterating `&dyn DagNode` will silently drop these.
        use crate::types::common::DagNode;
        let p = PassthroughEntry {
            uuid: "u-passthrough".into(),
            parent_uuid: Some("u-parent".into()),
            session_id: "sess-1".into(),
            timestamp: Some("2026-06-09T00:00:00Z".into()),
            entry_type: "future-feature-xyz".into(),
            is_sidechain: Some(false),
            agent_id: None,
        };
        assert_eq!(p.uuid(), Some("u-passthrough"));
        assert_eq!(p.session_id(), Some("sess-1"));
        assert_eq!(p.timestamp(), Some("2026-06-09T00:00:00Z"));
        assert_eq!(p.parent_uuid(), Some("u-parent"));
        assert_eq!(p.is_sidechain(), Some(false));

        // parent_uuid is Option<String> on the struct: when None, the trait
        // accessor returns None (root of a passthrough chain).
        let root = PassthroughEntry {
            uuid: "u-root".into(),
            parent_uuid: None,
            session_id: "sess-1".into(),
            timestamp: None,
            entry_type: "future-feature-xyz".into(),
            is_sidechain: None,
            agent_id: None,
        };
        assert!(root.parent_uuid().is_none());
        assert!(root.timestamp().is_none());
        assert!(root.is_sidechain().is_none());
    }

    #[test]
    fn agent_id_top_level_not_overwritten() {
        let json = r#"{
            "type":"user","uuid":"u1","sessionId":"s1","agentId":"already-here",
            "message":{"role":"user","content":"hello"}
        }"#;
        let entry = serde_json::from_str::<Entry>(json).unwrap();
        match entry {
            Entry::User(u) => {
                assert_eq!(u.agent_id.as_deref(), Some("already-here"));
            }
            other => panic!("Expected User, got: {other:?}"),
        }
    }
}
