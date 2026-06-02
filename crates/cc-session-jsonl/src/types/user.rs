use serde::{Deserialize, Serialize};

use super::transcript_entry;

transcript_entry! {
    /// A user-authored message entry in a Claude Code session.
    pub struct UserEntry {
        pub message: Option<UserContent>,
        /// tool-use placeholder 标志（Claude Code 2.1.104+）
        pub is_meta: Option<bool>,
        /// 当前 turn 的权限模式快照（inline，Claude Code 2.1.104+）
        pub permission_mode: Option<String>,
        /// 旧版 toolResult 回填，保留原始 JSON 结构（Claude Code 2.1.104+）
        pub tool_use_result: Option<serde_json::Value>,
        /// tool 调用与结果的关联 id（新名，Claude Code 2.1.71+）
        #[serde(rename = "sourceToolUseID")]
        pub source_tool_use_id: Option<String>,
        /// tool 调用与结果的关联 id（旧名，仍在用，Claude Code 2.1.71+）
        #[serde(rename = "sourceToolAssistantUUID")]
        pub source_tool_assistant_uuid: Option<String>,
    }
}

transcript_entry! {
    /// A system message entry in a Claude Code session.
    pub struct SystemEntry {
        pub message: Option<serde_json::Value>,
        pub subtype: Option<String>,
        pub duration_ms: Option<u64>,
        /// turn_duration / local_command 等子类型携带的文本内容（Claude Code 2.1.159+）
        pub content: Option<serde_json::Value>,
        /// 标记 meta 性质的 system entry（如 turn_duration，Claude Code 2.1.159+）
        pub is_meta: Option<bool>,
        // ── 仅在 subtype = turn_duration 时出现（Claude Code 2.1.159+）──
        /// 本 turn 的消息数
        pub message_count: Option<u64>,
        /// turn 结束时尚未完成的 workflow 数量
        pub pending_workflow_count: Option<u64>,
        // ── 仅在 subtype = stop_hook_summary 时出现（Claude Code 2.1.104+）──
        /// 本次 stop hook 触发的钩子总数
        pub hook_count: Option<u64>,
        /// 每个钩子的详细信息（命令 + 耗时）
        pub hook_infos: Option<Vec<HookInfo>>,
        /// 钩子执行过程中产生的错误列表（结构未必稳定，用 Value 兜底）
        pub hook_errors: Option<Vec<serde_json::Value>>,
        /// 钩子是否阻止了 turn 继续
        pub prevented_continuation: Option<bool>,
        /// 停止原因
        pub stop_reason: Option<String>,
        /// 是否有输出
        pub has_output: Option<bool>,
        /// 日志/通知级别（如 "suggestion"）
        pub level: Option<String>,
        /// 关联的 tool_use id（注意末尾全大写 ID）
        #[serde(rename = "toolUseID")]
        pub tool_use_id: Option<String>,
    }
}

/// stop_hook_summary 中单个钩子的执行信息（Claude Code 2.1.104+）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookInfo {
    pub command: Option<String>,
    pub duration_ms: Option<u64>,
}

transcript_entry! {
    /// An attachment entry in a Claude Code session.
    pub struct AttachmentEntry {
        /// 旧版顶层字段（部分早期版本将内容放在 `message`，保留以兼容）。
        pub message: Option<serde_json::Value>,
        /// 附件实体（Claude Code 2.1.159+）。真实数据中附件内容位于顶层
        /// `attachment` 对象内，其子类型由嵌套的 `attachment.type` 标识
        /// （如 `hook_success`、`skill_listing`、`file`、`task_reminder` 等）。
        /// 结构因子类型而异，用 `Value` 兜底；可用 [`AttachmentEntry::attachment_subtype`]
        /// 读取子类型标识。
        pub attachment: Option<serde_json::Value>,
    }
}

impl AttachmentEntry {
    /// 读取附件子类型标识（嵌套 `attachment.type`），如 `hook_success`、
    /// `skill_listing`、`file`、`task_reminder`、`queued_command` 等。
    pub fn attachment_subtype(&self) -> Option<&str> {
        self.attachment.as_ref()?.get("type")?.as_str()
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

    // ── SystemEntry v2.1.159 subtypes: turn_duration / local_command / away_summary ──

    #[test]
    fn parse_system_turn_duration() {
        // Real shape: durationMs + isMeta + messageCount (+ pendingWorkflowCount sometimes).
        let json = r#"{
            "type": "system",
            "subtype": "turn_duration",
            "uuid": "s-td-001",
            "sessionId": "sess-td",
            "durationMs": 131122,
            "isMeta": false,
            "messageCount": 18,
            "pendingWorkflowCount": 2
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.subtype.as_deref(), Some("turn_duration"));
        assert_eq!(entry.duration_ms, Some(131122));
        assert_eq!(entry.is_meta, Some(false));
        assert_eq!(entry.message_count, Some(18));
        assert_eq!(entry.pending_workflow_count, Some(2));
    }

    #[test]
    fn parse_system_turn_duration_without_pending_workflow_count() {
        // pendingWorkflowCount is only present on some turns.
        let json = r#"{
            "type": "system",
            "subtype": "turn_duration",
            "uuid": "s-td-002",
            "sessionId": "sess-td",
            "durationMs": 500,
            "isMeta": false,
            "messageCount": 4
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.message_count, Some(4));
        assert!(entry.pending_workflow_count.is_none());
    }

    #[test]
    fn parse_system_local_command() {
        // Real shape: content (string) + level "info".
        let json = r#"{
            "type": "system",
            "subtype": "local_command",
            "uuid": "s-lc-001",
            "sessionId": "sess-lc",
            "content": "<command-name>/workflows</command-name>\n            <command-message>workflows</command-message>\n            <command-args></command-args>",
            "level": "info",
            "isMeta": false
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.subtype.as_deref(), Some("local_command"));
        assert_eq!(entry.level.as_deref(), Some("info"));
        assert_eq!(entry.is_meta, Some(false));
        let content = entry.content.as_ref().expect("content must be Some");
        assert!(content.is_string());
        assert!(content.as_str().unwrap().contains("/workflows"));
    }

    #[test]
    fn parse_system_away_summary() {
        // Real shape: content (string), no level.
        let json = r#"{
            "type": "system",
            "subtype": "away_summary",
            "uuid": "s-as-001",
            "sessionId": "sess-as",
            "content": "你想在 Claude Code 监控里感知 API 错误。已确认有 StopFailure hook。",
            "isMeta": false
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.subtype.as_deref(), Some("away_summary"));
        let content = entry.content.as_ref().expect("content must be Some");
        assert!(content.is_string());
        assert!(content.as_str().unwrap().contains("StopFailure"));
        // away_summary carries no level in real data
        assert!(entry.level.is_none());
    }

    // ── AttachmentEntry: nested `attachment` object + subtype helper ──

    #[test]
    fn parse_attachment_hook_success_top_level_field() {
        // Real v2.1.159 data: attachment content is in the top-level `attachment`
        // object, NOT `message`. The subtype lives at `attachment.type`.
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
        // nested fields preserved
        let att = entry.attachment.as_ref().unwrap();
        assert_eq!(att["exitCode"], 0);
        assert_eq!(att["hookEvent"], "PostToolUse");
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
        assert_eq!(entry.attachment.as_ref().unwrap()["skillCount"], 3);
    }

    #[test]
    fn attachment_subtype_none_when_no_attachment() {
        // Legacy / empty attachment entries → helper returns None, no panic.
        let json = r#"{"type":"attachment","uuid":"att-empty","sessionId":"s"}"#;
        let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
        assert!(entry.attachment.is_none());
        assert!(entry.attachment_subtype().is_none());
    }

    #[test]
    fn parse_system_legacy_no_subtype_meta_fields() {
        // A pre-2.1.159 system entry must parse with the new fields all None.
        let json = r#"{
            "type": "system",
            "uuid": "s-legacy-159",
            "sessionId": "sess-legacy",
            "subtype": "tool_result",
            "message": {"role": "system", "content": "done"}
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert!(entry.content.is_none());
        assert!(entry.is_meta.is_none());
        assert!(entry.message_count.is_none());
        assert!(entry.pending_workflow_count.is_none());
    }

    // ── Layer A: Round-trip value assertions for v2.1 new fields ──

    #[test]
    fn parse_user_with_is_meta_and_source_tool_use_id() {
        // Tests two fields simultaneously: isMeta (bool) and sourceToolUseID (renamed field).
        // The rename is critical — camelCase inference would map source_tool_use_id → sourceToolUseId
        // (lowercase 'd'), not sourceToolUseID. Explicit #[serde(rename)] is required.
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
        // Other new fields absent → None
        assert!(entry.source_tool_assistant_uuid.is_none());
        assert!(entry.permission_mode.is_none());
        assert!(entry.tool_use_result.is_none());
    }

    #[test]
    fn parse_user_with_source_tool_assistant_uuid_legacy() {
        // sourceToolAssistantUUID uses the old naming convention. Both fields can coexist.
        // This test also asserts the two ID fields are independent of each other.
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
        // The new-style sourceToolUseID should be absent
        assert!(entry.source_tool_use_id.is_none());
    }

    #[test]
    fn parse_user_with_permission_mode_inline() {
        // permissionMode as an inline field on a user entry is NOT the same as
        // type="permission-mode" (a dedicated switch entry). The inline field on UserEntry
        // records a snapshot of the active mode at turn time.
        // This test asserts the entry is still classified as Entry::User, not Entry::PermissionMode.
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
        // toolUseResult can be any JSON object. The parser must preserve the full structure.
        // Round-trip: parse → access nested field by key.
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
        // A v2.0 style user entry (no new fields) must parse fine and expose all 5 new fields as None.
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
        // All 5 v2.1 UserEntry fields must be None
        assert!(
            entry.is_meta.is_none(),
            "is_meta should be None for legacy entry"
        );
        assert!(
            entry.permission_mode.is_none(),
            "permission_mode should be None for legacy entry"
        );
        assert!(
            entry.tool_use_result.is_none(),
            "tool_use_result should be None for legacy entry"
        );
        assert!(
            entry.source_tool_use_id.is_none(),
            "source_tool_use_id should be None for legacy entry"
        );
        assert!(
            entry.source_tool_assistant_uuid.is_none(),
            "source_tool_assistant_uuid should be None for legacy entry"
        );
        // Existing fields still parse correctly
        assert_eq!(entry.uuid.as_deref(), Some("u-legacy-001"));
        assert_eq!(entry.version.as_deref(), Some("2.0.77"));
    }

    // ── System entry v2.1 new fields (Layer A) ──

    #[test]
    fn parse_system_stop_hook_summary() {
        // Exact values from spec 1.3 real sample JSON.
        // Asserts every field: hookCount, hookInfos[0].command, hookInfos[0].durationMs,
        // preventedContinuation, level, toolUseID (note: capital ID, must use explicit rename).
        let json = r#"{
            "type": "system",
            "uuid": "sys-hook-001",
            "sessionId": "sess-hook",
            "subtype": "stop_hook_summary",
            "hookCount": 1,
            "hookInfos": [
                {
                    "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/emit-event.sh",
                    "durationMs": 20
                }
            ],
            "hookErrors": [],
            "preventedContinuation": false,
            "level": "suggestion",
            "toolUseID": "e7953fc8-1234-5678-abcd-ef1234567890"
        }"#;

        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.subtype.as_deref(), Some("stop_hook_summary"));
        assert_eq!(entry.hook_count, Some(1));

        let hook_infos = entry.hook_infos.as_ref().expect("hook_infos must be Some");
        assert_eq!(hook_infos.len(), 1);
        assert_eq!(
            hook_infos[0].command.as_deref(),
            Some("bash ${CLAUDE_PLUGIN_ROOT}/hooks/emit-event.sh")
        );
        assert_eq!(hook_infos[0].duration_ms, Some(20));

        let hook_errors = entry
            .hook_errors
            .as_ref()
            .expect("hook_errors must be Some");
        assert_eq!(hook_errors.len(), 0);

        assert_eq!(entry.prevented_continuation, Some(false));
        assert_eq!(entry.level.as_deref(), Some("suggestion"));
        assert_eq!(
            entry.tool_use_id.as_deref(),
            Some("e7953fc8-1234-5678-abcd-ef1234567890")
        );
    }

    #[test]
    fn parse_system_stop_hook_summary_with_errors() {
        // hookErrors is non-empty. Asserts array length and first element structure.
        // Uses Vec<Value> because error structure is not guaranteed stable.
        let json = r#"{
            "type": "system",
            "uuid": "sys-hook-err-001",
            "sessionId": "sess-hook-err",
            "subtype": "stop_hook_summary",
            "hookCount": 2,
            "hookInfos": [
                {"command": "hook1.sh", "durationMs": 100},
                {"command": "hook2.sh", "durationMs": 50}
            ],
            "hookErrors": [{"message": "hook1.sh exited with code 1"}],
            "preventedContinuation": true,
            "level": "error",
            "toolUseID": "tool-uuid-error"
        }"#;

        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.hook_count, Some(2));

        let hook_errors = entry
            .hook_errors
            .as_ref()
            .expect("hook_errors must be Some");
        assert_eq!(hook_errors.len(), 1);
        assert_eq!(hook_errors[0]["message"], "hook1.sh exited with code 1");

        assert_eq!(entry.prevented_continuation, Some(true));
        assert_eq!(entry.level.as_deref(), Some("error"));

        let hook_infos = entry.hook_infos.as_ref().unwrap();
        assert_eq!(hook_infos.len(), 2);
        assert_eq!(hook_infos[0].command.as_deref(), Some("hook1.sh"));
        assert_eq!(hook_infos[0].duration_ms, Some(100));
        assert_eq!(hook_infos[1].command.as_deref(), Some("hook2.sh"));
        assert_eq!(hook_infos[1].duration_ms, Some(50));
    }

    #[test]
    fn parse_system_legacy_no_hook_fields() {
        // A legacy SystemEntry (only message + subtype, no hook fields) must parse fine
        // and expose all 8 new hook fields as None.
        let json = r#"{
            "type": "system",
            "uuid": "sys-legacy-001",
            "sessionId": "sess-legacy",
            "subtype": "tool_result",
            "durationMs": 500,
            "message": {"role": "system", "content": "Tool completed"}
        }"#;

        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        // All 8 v2.1 SystemEntry fields must be None
        assert!(
            entry.hook_count.is_none(),
            "hook_count should be None for legacy"
        );
        assert!(
            entry.hook_infos.is_none(),
            "hook_infos should be None for legacy"
        );
        assert!(
            entry.hook_errors.is_none(),
            "hook_errors should be None for legacy"
        );
        assert!(
            entry.prevented_continuation.is_none(),
            "prevented_continuation should be None for legacy"
        );
        assert!(
            entry.stop_reason.is_none(),
            "stop_reason should be None for legacy"
        );
        assert!(
            entry.has_output.is_none(),
            "has_output should be None for legacy"
        );
        assert!(entry.level.is_none(), "level should be None for legacy");
        assert!(
            entry.tool_use_id.is_none(),
            "tool_use_id should be None for legacy"
        );
        // Existing fields parse correctly
        assert_eq!(entry.subtype.as_deref(), Some("tool_result"));
        assert_eq!(entry.duration_ms, Some(500));
    }

    #[test]
    fn parse_system_stop_hook_summary_all_optional_fields() {
        // Verify stop_reason and has_output parse correctly (they appear in newer versions).
        let json = r#"{
            "type": "system",
            "uuid": "sys-hook-full-001",
            "sessionId": "sess-hook-full",
            "subtype": "stop_hook_summary",
            "hookCount": 1,
            "hookInfos": [{"command": "notify.sh", "durationMs": 5}],
            "hookErrors": [],
            "preventedContinuation": false,
            "stopReason": "end_turn",
            "hasOutput": true,
            "level": "info",
            "toolUseID": "tool-full-uuid"
        }"#;

        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(entry.has_output, Some(true));
        assert_eq!(entry.hook_count, Some(1));
    }

    #[test]
    fn parse_hook_info_partial_fields_command_only() {
        // 真实风险：未来 CC 版本某次 hook 调用没报告 durationMs
        let json = r#"{
            "type": "system",
            "subtype": "stop_hook_summary",
            "hookCount": 1,
            "hookInfos": [{"command": "bash hook.sh"}]
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::System(s) => {
                let hooks = s
                    .hook_infos
                    .as_ref()
                    .expect("hook_infos parses despite missing durationMs");
                assert_eq!(hooks.len(), 1);
                assert_eq!(hooks[0].command.as_deref(), Some("bash hook.sh"));
                assert!(hooks[0].duration_ms.is_none());
            }
            other => panic!("Expected System, got: {other:?}"),
        }
    }

    #[test]
    fn parse_hook_info_partial_fields_duration_only() {
        let json = r#"{
            "type": "system",
            "subtype": "stop_hook_summary",
            "hookCount": 1,
            "hookInfos": [{"durationMs": 50}]
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::System(s) => {
                let hooks = s.hook_infos.as_ref().unwrap();
                assert_eq!(hooks.len(), 1);
                assert!(hooks[0].command.is_none());
                assert_eq!(hooks[0].duration_ms, Some(50));
            }
            other => panic!("Expected System, got: {other:?}"),
        }
    }
}
