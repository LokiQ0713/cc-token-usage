//! `AttachmentEntry` — a JSONL entry where `type == "attachment"`.
//!
//! Attachments carry the richest nested-discriminator surface in the JSONL
//! format. The wire payload nests its discriminator one level down at
//! `attachment.type`; survey §5 enumerates 23 known sub-types.
//!
//! Design choice (v2): the survey's top-6 by volume (≥ ~90% coverage) are
//! modelled as typed variants of [`AttachmentBody`]; the long tail lands in
//! [`AttachmentBody::Unknown`] which preserves the raw value so downstream
//! consumers can still inspect it.

use serde::{Deserialize, Serialize};

use super::common::DagNode;

/// An attachment entry in a Claude Code session.
///
/// Carries the 9 universal DAG fields plus an `attachment` object whose
/// inner `type` discriminates the body shape (see [`AttachmentBody`]).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentEntry {
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

    // ── Optional contextual fields (survey §3 attachment row) ──
    pub slug: Option<String>,
    pub agent_id: Option<String>,

    /// Legacy / fallback payload — some pre-2.1.159 attachment entries
    /// stored content under `message` instead of the new top-level
    /// `attachment` object. Kept as `Value` for back-compat.
    pub message: Option<serde_json::Value>,

    /// The typed sub-body of the attachment (v2.1.159+).
    ///
    /// `attachment` lands as `None` for pre-2.1.159 entries that lacked the
    /// top-level field entirely; on modern entries it carries the typed
    /// payload discriminated by `attachment.type`. An object that lacks
    /// the inner `type` discriminator (and so can't match a tagged variant)
    /// still lands as `Some(AttachmentBody::Unknown)` — see the custom
    /// deserializer below.
    #[serde(deserialize_with = "deserialize_attachment_body_opt", default)]
    pub attachment: Option<AttachmentBody>,
}

/// Custom deserializer for `attachment` so an object without an inner
/// `type` discriminator (or with an unrecognised discriminator) lands in
/// [`AttachmentBody::Unknown`] instead of erroring the whole entry.
fn deserialize_attachment_body_opt<'de, D>(
    deserializer: D,
) -> Result<Option<AttachmentBody>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    Ok(
        opt.map(|v| match serde_json::from_value::<AttachmentBody>(v) {
            Ok(body) => body,
            // Any decoding failure of the inner body — missing `type`,
            // unrecognised value, partial shape — soft-lands in Unknown so
            // the long tail doesn't break the parse.
            Err(_) => AttachmentBody::Unknown,
        }),
    )
}

impl DagNode for AttachmentEntry {
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

/// Tagged sub-enum driven by the nested `attachment.type` discriminator.
///
/// Survey §5 enumerates 23 known values; the top-6 by volume (covering ~95%
/// of real data) are modelled here, the long tail lands in
/// [`AttachmentBody::Unknown`] with the raw value preserved.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum AttachmentBody {
    #[serde(rename = "output_style")]
    #[serde(rename_all = "camelCase")]
    OutputStyle { style: Option<String> },

    #[serde(rename = "hook_success")]
    #[serde(rename_all = "camelCase")]
    HookSuccess {
        command: Option<String>,
        content: Option<serde_json::Value>,
        duration_ms: Option<u64>,
        exit_code: Option<i64>,
        hook_event: Option<String>,
        hook_name: Option<String>,
        stderr: Option<String>,
        stdout: Option<String>,
        #[serde(rename = "toolUseID")]
        tool_use_id: Option<String>,
    },

    #[serde(rename = "task_reminder")]
    #[serde(rename_all = "camelCase")]
    TaskReminder {
        content: Option<serde_json::Value>,
        item_count: Option<u64>,
    },

    #[serde(rename = "deferred_tools_delta")]
    #[serde(rename_all = "camelCase")]
    DeferredToolsDelta {
        added_lines: Option<serde_json::Value>,
        added_names: Option<Vec<String>>,
        removed_names: Option<Vec<String>>,
        readded_names: Option<Vec<String>>,
        pending_mcp_servers: Option<Vec<String>>,
    },

    #[serde(rename = "skill_listing")]
    #[serde(rename_all = "camelCase")]
    SkillListing {
        content: Option<serde_json::Value>,
        is_initial: Option<bool>,
        skill_count: Option<u64>,
        names: Option<Vec<String>>,
    },

    #[serde(rename = "queued_command")]
    #[serde(rename_all = "camelCase")]
    QueuedCommand {
        command_mode: Option<String>,
        prompt: Option<String>,
        image_paste_ids: Option<Vec<String>>,
        source_uuid: Option<String>,
    },

    /// Long-tail subtypes (17+ rare ones — `file`, `diagnostics`,
    /// `edited_text_file`, `nested_memory`, etc.). The raw value is kept
    /// intact so consumers can still inspect fields by JSON key.
    #[serde(other)]
    Unknown,
}

impl AttachmentEntry {
    /// Read the attachment's inner subtype discriminator, e.g.
    /// `"hook_success"`. Returns `None` when the entry has no `attachment`
    /// payload (legacy entries) or the body lands in `Unknown`.
    pub fn attachment_subtype(&self) -> Option<&'static str> {
        match self.attachment.as_ref()? {
            AttachmentBody::OutputStyle { .. } => Some("output_style"),
            AttachmentBody::HookSuccess { .. } => Some("hook_success"),
            AttachmentBody::TaskReminder { .. } => Some("task_reminder"),
            AttachmentBody::DeferredToolsDelta { .. } => Some("deferred_tools_delta"),
            AttachmentBody::SkillListing { .. } => Some("skill_listing"),
            AttachmentBody::QueuedCommand { .. } => Some("queued_command"),
            AttachmentBody::Unknown => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;

    #[test]
    fn parse_attachment_entry_legacy_with_message() {
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
        // No nested `attachment` object → subtype helper returns None.
        assert!(entry.attachment_subtype().is_none());
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

    #[test]
    fn parse_attachment_hook_success_top_level_field() {
        let json = r#"{
            "type": "attachment",
            "uuid": "att-hs-001",
            "sessionId": "sess-att",
            "attachment": {
                "type": "hook_success",
                "command": "bash hook.sh",
                "hookEvent": "PostToolUse",
                "hookName": "emit-event",
                "exitCode": 0,
                "durationMs": 20,
                "stdout": "ok",
                "stderr": "",
                "content": "done",
                "toolUseID": "toolu_01"
            }
        }"#;
        let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
        assert!(
            entry.message.is_none(),
            "real data uses `attachment`, not `message`"
        );
        assert!(entry.attachment.is_some());
        assert_eq!(entry.attachment_subtype(), Some("hook_success"));
        match entry.attachment.as_ref().unwrap() {
            AttachmentBody::HookSuccess {
                command,
                exit_code,
                hook_event,
                duration_ms,
                ..
            } => {
                assert_eq!(command.as_deref(), Some("bash hook.sh"));
                assert_eq!(*exit_code, Some(0));
                assert_eq!(hook_event.as_deref(), Some("PostToolUse"));
                assert_eq!(*duration_ms, Some(20));
            }
            other => panic!("Expected HookSuccess, got {other:?}"),
        }
    }

    #[test]
    fn parse_attachment_skill_listing_subtype() {
        let json = r#"{
            "type": "attachment",
            "uuid": "att-sl-001",
            "sessionId": "sess-att",
            "attachment": {
                "type": "skill_listing",
                "isInitial": true,
                "skillCount": 3,
                "names": ["a", "b", "c"],
                "content": "skills"
            }
        }"#;
        let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.attachment_subtype(), Some("skill_listing"));
        match entry.attachment.as_ref().unwrap() {
            AttachmentBody::SkillListing {
                is_initial,
                skill_count,
                names,
                ..
            } => {
                assert_eq!(*is_initial, Some(true));
                assert_eq!(*skill_count, Some(3));
                assert_eq!(names.as_ref().unwrap().len(), 3);
            }
            other => panic!("Expected SkillListing, got {other:?}"),
        }
    }

    #[test]
    fn attachment_subtype_none_when_no_attachment() {
        let json = r#"{"type":"attachment","uuid":"att-empty","sessionId":"s"}"#;
        let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
        assert!(entry.attachment.is_none());
        assert!(entry.attachment_subtype().is_none());
    }

    #[test]
    fn parse_attachment_long_tail_lands_in_unknown() {
        let json = r#"{
            "type":"attachment",
            "uuid":"att-x-001",
            "sessionId":"s",
            "attachment":{
                "type":"file",
                "filename":"main.rs",
                "displayPath":"/x/y/main.rs",
                "content":"fn main() {}"
            }
        }"#;
        let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
        assert!(matches!(
            entry.attachment.as_ref().unwrap(),
            AttachmentBody::Unknown
        ));
        // subtype helper returns None for Unknown; consumers that want the
        // long-tail discriminator can still pull it from a raw `Value` path.
        assert!(entry.attachment_subtype().is_none());
    }

    #[test]
    fn parse_attachment_output_style() {
        let json = r#"{
            "type":"attachment",
            "uuid":"att-os-1",
            "sessionId":"s",
            "attachment":{"type":"output_style","style":"verbose"}
        }"#;
        let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.attachment_subtype(), Some("output_style"));
        match entry.attachment.as_ref().unwrap() {
            AttachmentBody::OutputStyle { style } => {
                assert_eq!(style.as_deref(), Some("verbose"))
            }
            other => panic!("Expected OutputStyle, got {other:?}"),
        }
    }

    #[test]
    fn parse_attachment_queued_command() {
        let json = r#"{
            "type":"attachment",
            "uuid":"att-qc-1",
            "sessionId":"s",
            "attachment":{
                "type":"queued_command",
                "commandMode":"prompt",
                "prompt":"continue",
                "imagePasteIds":["p1"]
            }
        }"#;
        let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.attachment_subtype(), Some("queued_command"));
        match entry.attachment.as_ref().unwrap() {
            AttachmentBody::QueuedCommand { prompt, .. } => {
                assert_eq!(prompt.as_deref(), Some("continue"));
            }
            other => panic!("Expected QueuedCommand, got {other:?}"),
        }
    }

    #[test]
    fn parse_attachment_task_reminder() {
        let json = r#"{
            "type":"attachment",
            "uuid":"att-tr-1",
            "sessionId":"s",
            "attachment":{
                "type":"task_reminder",
                "content":"3 tasks open",
                "itemCount":3
            }
        }"#;
        let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.attachment_subtype(), Some("task_reminder"));
    }

    #[test]
    fn parse_attachment_deferred_tools_delta() {
        let json = r#"{
            "type":"attachment",
            "uuid":"att-dt-1",
            "sessionId":"s",
            "attachment":{
                "type":"deferred_tools_delta",
                "addedLines":[],
                "addedNames":["fs.read"],
                "removedNames":[],
                "readdedNames":[]
            }
        }"#;
        let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.attachment_subtype(), Some("deferred_tools_delta"));
    }
}
