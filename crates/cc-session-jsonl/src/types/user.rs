use serde::{Deserialize, Serialize};

use super::transcript_entry;

transcript_entry! {
    /// A user-authored message entry in a Claude Code session.
    pub struct UserEntry {
        pub message: Option<UserContent>,
    }
}

transcript_entry! {
    /// A system message entry in a Claude Code session.
    pub struct SystemEntry {
        pub message: Option<serde_json::Value>,
        pub subtype: Option<String>,
        pub duration_ms: Option<u64>,
    }
}

transcript_entry! {
    /// An attachment entry in a Claude Code session.
    pub struct AttachmentEntry {
        pub message: Option<serde_json::Value>,
    }
}

/// The content of a user message.
///
/// The `content` field can be either a plain string or an array of content blocks,
/// so it is represented as `serde_json::Value`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserContent {
    pub role: Option<String>,
    pub content: Option<serde_json::Value>,
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
    fn parse_system_entry_with_subtype_and_duration() {
        let json = r#"{
            "type": "system",
            "uuid": "s-001",
            "parentUuid": "u-001",
            "isSidechain": false,
            "timestamp": "2026-03-16T13:50:01.000Z",
            "sessionId": "sess-001",
            "cwd": "/tmp",
            "version": "2.0.77",
            "subtype": "tool_result",
            "durationMs": 1523,
            "message": {"role": "system", "content": [{"type": "tool_result", "tool_use_id": "toolu_01", "content": "done"}]}
        }"#;

        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.uuid.as_deref(), Some("s-001"));
        assert_eq!(entry.subtype.as_deref(), Some("tool_result"));
        assert_eq!(entry.duration_ms, Some(1523));
        assert!(entry.message.is_some());
    }

    #[test]
    fn parse_system_via_entry_enum() {
        let json = r#"{
            "type": "system",
            "uuid": "s-002",
            "sessionId": "sess-004",
            "subtype": "init",
            "message": {"role": "system", "content": "System initialized"}
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::System(s) => {
                assert_eq!(s.subtype.as_deref(), Some("init"));
            }
            other => panic!("Expected System, got: {other:?}"),
        }
    }

    #[test]
    fn parse_attachment_entry() {
        let json = r#"{
            "type": "attachment",
            "uuid": "att-001",
            "parentUuid": "u-001",
            "isSidechain": false,
            "timestamp": "2026-03-16T13:50:02.000Z",
            "sessionId": "sess-001",
            "cwd": "/tmp",
            "version": "2.0.77",
            "message": {"role": "user", "content": [{"type": "image", "source": {"type": "base64", "data": "abc123"}}]}
        }"#;

        let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.uuid.as_deref(), Some("att-001"));
        assert_eq!(entry.parent_uuid.as_deref(), Some("u-001"));
        assert!(entry.message.is_some());
        let msg = entry.message.unwrap();
        assert!(msg.is_object());
    }

    #[test]
    fn parse_attachment_via_entry_enum() {
        let json = r#"{
            "type": "attachment",
            "uuid": "att-002",
            "sessionId": "sess-005",
            "message": {"content": "file data"}
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::Attachment(a) => {
                assert_eq!(a.uuid.as_deref(), Some("att-002"));
            }
            other => panic!("Expected Attachment, got: {other:?}"),
        }
    }
}
