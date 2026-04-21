use serde::{Deserialize, Serialize};

/// A summary entry that stores a summarized version of a conversation branch.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryMessage {
    pub leaf_uuid: Option<String>,
    pub summary: Option<String>,
}

/// A user-set custom title for the session.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTitleMessage {
    pub session_id: Option<String>,
    pub custom_title: Option<String>,
}

/// An AI-generated title for the session.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTitleMessage {
    pub session_id: Option<String>,
    pub ai_title: Option<String>,
}

/// The last prompt sent in the session.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastPromptMessage {
    pub session_id: Option<String>,
    pub last_prompt: Option<String>,
}

/// A task summary entry with timestamp.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummaryMessage {
    pub session_id: Option<String>,
    pub summary: Option<String>,
    pub timestamp: Option<String>,
}

/// A tag applied to a session.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagMessage {
    pub session_id: Option<String>,
    pub tag: Option<String>,
}

/// The display name for an agent in a session.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentNameMessage {
    pub session_id: Option<String>,
    pub agent_name: Option<String>,
}

/// The color assigned to an agent in a session.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentColorMessage {
    pub session_id: Option<String>,
    pub agent_color: Option<String>,
}

/// A setting applied to an agent.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingMessage {
    pub session_id: Option<String>,
    pub agent_setting: Option<String>,
}

/// A pull request link associated with a session.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrLinkMessage {
    pub session_id: Option<String>,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    pub pr_repository: Option<String>,
    pub timestamp: Option<String>,
}

/// The operating mode of a session (e.g., "plan", "code").
///
/// **Legacy entry type.** Claude Code v2.1 and later write
/// `permission-mode` entries ([`PermissionModeEntry`]) instead. This type is
/// kept for backward compatibility with JSONL files produced by older
/// versions and is never emitted by recent Claude Code releases.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeEntry {
    pub session_id: Option<String>,
    pub mode: Option<String>,
}

/// The permission mode applied to a session (e.g., `"bypassPermissions"`,
/// `"acceptEdits"`, `"plan"`, `"default"`).
///
/// Introduced in Claude Code v2.1 as the replacement for the legacy `mode`
/// entry. Recorded whenever the user switches permission mode mid-session.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionModeEntry {
    pub session_id: Option<String>,
    pub permission_mode: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::types::Entry;

    #[test]
    fn parse_summary() {
        let json = r#"{
            "type": "summary",
            "leafUuid": "leaf-001",
            "summary": "The user asked to fix a bug and the assistant resolved it."
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::Summary(s) => {
                assert_eq!(s.leaf_uuid.as_deref(), Some("leaf-001"));
                assert!(s.summary.as_ref().unwrap().contains("fix a bug"));
            }
            other => panic!("Expected Summary, got: {other:?}"),
        }
    }

    #[test]
    fn parse_custom_title() {
        let json = r#"{
            "type": "custom-title",
            "sessionId": "sess-001",
            "customTitle": "My Custom Session Title"
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::CustomTitle(ct) => {
                assert_eq!(ct.session_id.as_deref(), Some("sess-001"));
                assert_eq!(ct.custom_title.as_deref(), Some("My Custom Session Title"));
            }
            other => panic!("Expected CustomTitle, got: {other:?}"),
        }
    }

    #[test]
    fn parse_ai_title() {
        let json = r#"{
            "type": "ai-title",
            "sessionId": "sess-002",
            "aiTitle": "Debugging memory leak in parser"
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::AiTitle(at) => {
                assert_eq!(at.session_id.as_deref(), Some("sess-002"));
                assert_eq!(
                    at.ai_title.as_deref(),
                    Some("Debugging memory leak in parser")
                );
            }
            other => panic!("Expected AiTitle, got: {other:?}"),
        }
    }

    #[test]
    fn parse_last_prompt() {
        let json = r#"{
            "type": "last-prompt",
            "sessionId": "sess-003",
            "lastPrompt": "Refactor the authentication module"
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::LastPrompt(lp) => {
                assert_eq!(lp.session_id.as_deref(), Some("sess-003"));
                assert_eq!(
                    lp.last_prompt.as_deref(),
                    Some("Refactor the authentication module")
                );
            }
            other => panic!("Expected LastPrompt, got: {other:?}"),
        }
    }

    #[test]
    fn parse_task_summary() {
        let json = r#"{
            "type": "task-summary",
            "sessionId": "sess-004",
            "summary": "Completed all unit tests for the parser module.",
            "timestamp": "2026-03-16T15:00:00Z"
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::TaskSummary(ts) => {
                assert_eq!(ts.session_id.as_deref(), Some("sess-004"));
                assert!(ts.summary.as_ref().unwrap().contains("unit tests"));
                assert_eq!(ts.timestamp.as_deref(), Some("2026-03-16T15:00:00Z"));
            }
            other => panic!("Expected TaskSummary, got: {other:?}"),
        }
    }

    #[test]
    fn parse_tag() {
        let json = r#"{
            "type": "tag",
            "sessionId": "sess-005",
            "tag": "bugfix"
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::Tag(t) => {
                assert_eq!(t.session_id.as_deref(), Some("sess-005"));
                assert_eq!(t.tag.as_deref(), Some("bugfix"));
            }
            other => panic!("Expected Tag, got: {other:?}"),
        }
    }

    #[test]
    fn parse_agent_name() {
        let json = r#"{
            "type": "agent-name",
            "sessionId": "sess-006",
            "agentName": "Reviewer"
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::AgentName(an) => {
                assert_eq!(an.session_id.as_deref(), Some("sess-006"));
                assert_eq!(an.agent_name.as_deref(), Some("Reviewer"));
            }
            other => panic!("Expected AgentName, got: {other:?}"),
        }
    }

    #[test]
    fn parse_agent_color() {
        let json = r##"{
            "type": "agent-color",
            "sessionId": "sess-007",
            "agentColor": "#3498db"
        }"##;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::AgentColor(ac) => {
                assert_eq!(ac.session_id.as_deref(), Some("sess-007"));
                assert_eq!(ac.agent_color.as_deref(), Some("#3498db"));
            }
            other => panic!("Expected AgentColor, got: {other:?}"),
        }
    }

    #[test]
    fn parse_agent_setting() {
        let json = r#"{
            "type": "agent-setting",
            "sessionId": "sess-008",
            "agentSetting": "custom-agent-definition"
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::AgentSetting(asetting) => {
                assert_eq!(asetting.session_id.as_deref(), Some("sess-008"));
                assert_eq!(
                    asetting.agent_setting.as_deref(),
                    Some("custom-agent-definition")
                );
            }
            other => panic!("Expected AgentSetting, got: {other:?}"),
        }
    }

    #[test]
    fn parse_pr_link() {
        let json = r#"{
            "type": "pr-link",
            "sessionId": "sess-009",
            "prNumber": 42,
            "prUrl": "https://github.com/user/repo/pull/42",
            "prRepository": "user/repo",
            "timestamp": "2026-03-16T16:00:00Z"
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::PrLink(pr) => {
                assert_eq!(pr.session_id.as_deref(), Some("sess-009"));
                assert_eq!(pr.pr_number, Some(42));
                assert_eq!(
                    pr.pr_url.as_deref(),
                    Some("https://github.com/user/repo/pull/42")
                );
                assert_eq!(pr.pr_repository.as_deref(), Some("user/repo"));
                assert_eq!(pr.timestamp.as_deref(), Some("2026-03-16T16:00:00Z"));
            }
            other => panic!("Expected PrLink, got: {other:?}"),
        }
    }

    #[test]
    fn parse_mode() {
        let json = r#"{
            "type": "mode",
            "sessionId": "sess-010",
            "mode": "plan"
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::Mode(m) => {
                assert_eq!(m.session_id.as_deref(), Some("sess-010"));
                assert_eq!(m.mode.as_deref(), Some("plan"));
            }
            other => panic!("Expected Mode, got: {other:?}"),
        }
    }
}
