pub mod assistant;
pub mod context;
pub mod metadata;
pub mod progress;
pub mod tracking;
pub mod user;

pub use assistant::*;
pub use context::*;
pub use metadata::*;
pub use progress::*;
pub use tracking::*;
pub use user::*;

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

/// All entry types in a Claude Code session JSONL file.
///
/// Uses `#[serde(tag = "type")]` for internally-tagged deserialization.
/// Unknown/future entry types fall through to `Unknown`.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    #[serde(other)]
    Unknown,
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
    fn unknown_type_becomes_unknown() {
        let json = r#"{"type":"future-feature-xyz","sessionId":"s1","data":"something"}"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, Entry::Unknown));
    }

    #[test]
    fn malformed_json_is_error() {
        let json = r#"{"type": "user", broken"#;
        let result = serde_json::from_str::<Entry>(json);
        assert!(result.is_err());
    }

    #[test]
    fn empty_object_is_error() {
        // No "type" field at all — should fail because internally-tagged requires it
        let json = r#"{}"#;
        let result = serde_json::from_str::<Entry>(json);
        assert!(result.is_err());
    }

    #[test]
    fn type_field_is_null_is_error() {
        let json = r#"{"type": null}"#;
        let result = serde_json::from_str::<Entry>(json);
        assert!(result.is_err());
    }

    #[test]
    fn type_field_is_number_is_error() {
        let json = r#"{"type": 42}"#;
        let result = serde_json::from_str::<Entry>(json);
        assert!(result.is_err());
    }
}
