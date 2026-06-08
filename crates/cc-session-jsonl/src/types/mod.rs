pub mod assistant;
pub mod context;
pub mod metadata;
pub mod progress;
pub mod tracking;
pub mod user;
pub mod workflow;

pub use assistant::*;
pub use context::*;
pub use metadata::*;
pub use progress::*;
pub use tracking::*;
pub use user::*;
pub use workflow::*;

use serde::de;
use serde::{Deserialize, Serialize};

/// Macro that defines a struct with the common transcript fields shared by all
/// transcript message types (user, assistant, system, attachment), plus any
/// additional fields provided.
///
/// The common fields are: uuid, parent_uuid, logical_parent_uuid, is_sidechain,
/// timestamp, session_id, cwd, version, git_branch, user_type, entrypoint, slug,
/// agent_id, team_name, agent_name, agent_color, prompt_id.
///
/// Usage:
/// ```ignore
/// transcript_entry! {
///     /// Doc comment
///     pub struct MyEntry {
///         // extra fields here
///         pub my_field: Option<String>,
///     }
/// }
/// ```
macro_rules! transcript_entry {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                pub $field:ident : $ty:ty
            ),*
            $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, ::serde::Deserialize, ::serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            // ── Common transcript fields ──
            pub uuid: Option<String>,
            pub parent_uuid: Option<String>,
            pub logical_parent_uuid: Option<String>,
            pub is_sidechain: Option<bool>,
            pub timestamp: Option<String>,
            pub session_id: Option<String>,
            pub cwd: Option<String>,
            pub version: Option<String>,
            pub git_branch: Option<String>,
            pub user_type: Option<String>,
            pub entrypoint: Option<String>,
            pub slug: Option<String>,
            pub agent_id: Option<String>,
            pub team_name: Option<String>,
            pub agent_name: Option<String>,
            pub agent_color: Option<String>,
            pub prompt_id: Option<String>,
            // ── Entry-specific fields ──
            $(
                $(#[$field_meta])*
                pub $field : $ty,
            )*
        }
    };
}

// Make the macro available to submodules
pub(crate) use transcript_entry;

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

/// All entry types in a Claude Code session JSONL file.
///
/// Serialization uses `#[serde(tag = "type")]` (derived).  Deserialization
/// is implemented manually so that unknown/future entry types can be routed
/// to either [`Passthrough`] (DAG-continuity preserved) or [`Ignored`]
/// (no DAG fields, safe to discard).
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
// We cannot use `#[serde(other)]` with two distinct fallback variants
// (Passthrough + Ignored), so deserialization is implemented manually.
// The strategy:
//   1. Read the line as `serde_json::Value`.
//   2. Peek at the `type` field.
//   3. For known types → `serde_json::from_value` → typed variant.
//   4. For unknown types → check `uuid` + `sessionId`:
//        • Both present & non-empty → `Passthrough` (DAG continuity)
//        • Otherwise               → `Ignored`  (safe to discard)
impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let entry_type = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let entry = match entry_type {
            "user" => {
                let mut user: UserEntry =
                    serde_json::from_value(value).map_err(de::Error::custom)?;
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
            "assistant" => {
                Entry::Assistant(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "system" => {
                Entry::System(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "attachment" => {
                Entry::Attachment(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "summary" => {
                Entry::Summary(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "custom-title" => {
                Entry::CustomTitle(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "ai-title" => {
                Entry::AiTitle(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "last-prompt" => {
                Entry::LastPrompt(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "task-summary" => {
                Entry::TaskSummary(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "tag" => {
                Entry::Tag(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "agent-name" => {
                Entry::AgentName(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "agent-color" => {
                Entry::AgentColor(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "agent-setting" => {
                Entry::AgentSetting(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "pr-link" => {
                Entry::PrLink(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "mode" => {
                Entry::Mode(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "permission-mode" => {
                Entry::PermissionMode(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "progress" => {
                Entry::Progress(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "queue-operation" => {
                Entry::QueueOperation(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "speculation-accept" => {
                Entry::SpeculationAccept(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "worktree-state" => {
                Entry::WorktreeState(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "content-replacement" => {
                Entry::ContentReplacement(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "file-history-snapshot" => {
                Entry::FileHistorySnapshot(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "attribution-snapshot" => {
                Entry::AttributionSnapshot(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "marble-origami-commit" => {
                Entry::ContextCollapseCommit(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
            "marble-origami-snapshot" => {
                Entry::ContextCollapseSnapshot(serde_json::from_value(value).map_err(de::Error::custom)?)
            }
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
                    Entry::Passthrough(
                        serde_json::from_value(value).map_err(de::Error::custom)?,
                    )
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
        let json = r#"{"type":"assistant","uuid":"a1","sessionId":"s1","message":{"model":"claude-opus-4-6","role":"assistant","content":[]}}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Assistant(_)));
    }

    #[test]
    fn route_system() {
        let json = r#"{"type":"system","uuid":"sys1","sessionId":"s1","subtype":"init"}"#;
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
        let json =
            r#"{"type":"future-feature-abc","uuid":"u1","sessionId":"s1","data":"x"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Passthrough(_)));
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
