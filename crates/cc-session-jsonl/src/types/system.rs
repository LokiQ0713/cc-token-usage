//! `SystemEntry` — a JSONL entry where `type == "system"`.
//!
//! System entries are the most subtype-heavy class in Claude Code JSONL.
//! Survey §4 enumerates 9 known subtypes; each carries its own conditional
//! field set. v2 makes the subtype an explicit Rust enum
//! ([`SystemBody`]) instead of a single flat struct with a sea of `Option`
//! fields where "this field is only set when `subtype == X`" was carried in
//! tribal knowledge and tests.
//!
//! Backwards-compatible flat accessors (`hook_count()`, `hook_infos()`, ...)
//! are provided so the existing `cc-token-usage` aggregation path that reads
//! `sys.hook_count` etc. keeps working without a flag-day rewrite, while new
//! code can pattern-match on `SystemBody` for proper exhaustiveness.

use serde::{Deserialize, Serialize};

use super::common::DagNode;

/// A system message entry in a Claude Code session.
///
/// The 9 universal DAG fields are spelled out here. Subtype-specific fields
/// live in [`SystemBody`] which is flattened onto the wire format so the
/// JSONL retains its single top-level shape with a `subtype` discriminator.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemEntry {
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

    // ── compact_boundary fallback parent (§4 in survey) ──
    pub logical_parent_uuid: Option<String>,

    // ── Slugs / agent linkage — system entries can carry these ──
    pub slug: Option<String>,

    // ── Legacy / cross-subtype payload (some pre-v2.1.159 entries had a
    //    free-shape `message` field). Kept for back-compat.
    pub message: Option<serde_json::Value>,

    /// The subtype-discriminated body. Flattened, so the wire shape stays
    /// `{ "type": "system", "subtype": "...", ...body fields... }`.
    #[serde(flatten)]
    pub body: SystemBody,
}

impl DagNode for SystemEntry {
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
        // Survey §4: `compact_boundary` uses `logicalParentUuid` instead of
        // `parentUuid` as the linkage point. We surface either through the
        // single accessor so DAG consumers don't need to special-case it.
        self.parent_uuid
            .as_deref()
            .or(self.logical_parent_uuid.as_deref())
    }
    fn is_sidechain(&self) -> Option<bool> {
        self.is_sidechain
    }
}

/// Tagged sub-enum driven by the `subtype` discriminator.
///
/// Variants modelled here:
///
/// | subtype             | semantics                                  |
/// |---------------------|--------------------------------------------|
/// | `turn_duration`     | per-turn duration / message-count summary  |
/// | `stop_hook_summary` | results of stop hooks fired for a turn     |
/// | `away_summary`      | summary of an away-mode interaction        |
/// | `api_error`         | API call failure record                    |
/// | `local_command`     | local slash-command invocation             |
/// | `compact_boundary`  | context-collapse splice marker             |
/// | `informational`     | catch-all info message                     |
/// | `scheduled_task_fire` | scheduled-task tick                      |
/// | `bridge_status`     | bridge session status update               |
/// | `Unknown`           | soft landing for unfamiliar subtypes       |
///
/// Survey §4 lists fields per subtype; the variants below match those lists
/// (everything REQUIRED in the survey is `T` or `Vec<T>`, OPTIONAL stays
/// `Option<T>`). Cross-version tolerance keeps a few fields `Option` even
/// where the current snapshot reports 100% fill — that aligns with the
/// "required ⟺ (sample 100%) ∧ doc-cross-checked" rule in survey §0.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "subtype")]
pub enum SystemBody {
    #[serde(rename = "turn_duration")]
    #[serde(rename_all = "camelCase")]
    TurnDuration {
        /// Wall-clock duration of the turn in milliseconds.
        duration_ms: Option<u64>,
        /// Number of messages in the turn.
        message_count: Option<u64>,
        /// Whether this entry is meta (always true on real samples).
        is_meta: Option<bool>,
        /// Workflows still pending at turn end (2.1.159+).
        pending_workflow_count: Option<u64>,
        /// Background agents still pending at turn end.
        pending_background_agent_count: Option<u64>,
    },

    #[serde(rename = "stop_hook_summary")]
    #[serde(rename_all = "camelCase")]
    StopHookSummary {
        /// Total hooks that fired in this stop event.
        hook_count: Option<u64>,
        /// Detailed per-hook records.
        hook_infos: Option<Vec<HookInfo>>,
        /// Errors raised by individual hooks. Shape is hook-implementation
        /// defined — kept as raw `Value`.
        hook_errors: Option<Vec<serde_json::Value>>,
        /// Whether the hooks blocked continuation of the turn.
        prevented_continuation: Option<bool>,
        /// LLM stop reason captured alongside hook outcome.
        stop_reason: Option<String>,
        /// Whether the hooks emitted any output.
        has_output: Option<bool>,
        /// Linked tool-use identifier (capital-ID spelling on the wire).
        #[serde(rename = "toolUseID")]
        tool_use_id: Option<String>,
        /// Severity level (`"info"`, `"suggestion"`, `"error"`, ...).
        level: Option<String>,
    },

    #[serde(rename = "away_summary")]
    #[serde(rename_all = "camelCase")]
    AwaySummary {
        content: Option<serde_json::Value>,
        is_meta: Option<bool>,
    },

    #[serde(rename = "api_error")]
    #[serde(rename_all = "camelCase")]
    ApiError {
        /// Sometimes a short string (e.g. `"rate_limit_exceeded"`) and
        /// sometimes the full upstream response object (status, headers,
        /// nested error payload). Kept as raw JSON for shape tolerance.
        error: Option<serde_json::Value>,
        level: Option<String>,
        max_retries: Option<u64>,
        retry_attempt: Option<u64>,
        /// Production emits fractional values like `597.985496...` (not whole
        /// milliseconds). Use `f64` so both `1500` and `597.98` parse.
        retry_in_ms: Option<f64>,
        /// Optional cause payload (18% fill rate in survey). In real data this
        /// is sometimes a string (e.g. `"Too many requests"`) and sometimes a
        /// structured object (e.g. `{"code":"UNKNOWN_CERTIFICATE_VERIFICATION_ERROR","path":"https://..."}`).
        /// We keep it as raw JSON so both shapes parse without StructDrift.
        cause: Option<serde_json::Value>,
    },

    #[serde(rename = "local_command")]
    #[serde(rename_all = "camelCase")]
    LocalCommand {
        content: Option<serde_json::Value>,
        level: Option<String>,
        is_meta: Option<bool>,
    },

    #[serde(rename = "compact_boundary")]
    #[serde(rename_all = "camelCase")]
    CompactBoundary {
        compact_metadata: Option<serde_json::Value>,
        content: Option<serde_json::Value>,
        level: Option<String>,
    },

    #[serde(rename = "informational")]
    #[serde(rename_all = "camelCase")]
    Informational {
        content: Option<serde_json::Value>,
        level: Option<String>,
        is_meta: Option<bool>,
    },

    #[serde(rename = "scheduled_task_fire")]
    #[serde(rename_all = "camelCase")]
    ScheduledTaskFire {
        content: Option<serde_json::Value>,
        is_meta: Option<bool>,
    },

    #[serde(rename = "bridge_status")]
    #[serde(rename_all = "camelCase")]
    BridgeStatus {
        content: Option<serde_json::Value>,
        url: Option<String>,
    },

    /// Soft landing for unfamiliar `subtype` values. The raw `subtype` string
    /// is recovered from the [`SystemEntry::subtype`] accessor via the parent
    /// `Value` — when serde sees an unknown value here it lands on this
    /// variant rather than failing the whole entry.
    #[serde(other)]
    #[default]
    Unknown,
}

/// stop_hook_summary's per-hook execution record (Claude Code 2.1.104+).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookInfo {
    pub command: Option<String>,
    pub duration_ms: Option<u64>,
}

// ─── Back-compat accessors ────────────────────────────────────────────────
//
// These thin shims surface the most-read subtype-specific fields as flat
// accessors so the downstream `cc-token-usage` aggregation path keeps
// reading `sys.hook_count`-style snippets through a single deref. New code
// should match on `body` directly for full type safety.

impl SystemEntry {
    /// The subtype discriminator as it appears on the wire, e.g.
    /// `"stop_hook_summary"`. Returns `None` only for `SystemBody::Unknown`.
    pub fn subtype(&self) -> Option<&'static str> {
        match self.body {
            SystemBody::TurnDuration { .. } => Some("turn_duration"),
            SystemBody::StopHookSummary { .. } => Some("stop_hook_summary"),
            SystemBody::AwaySummary { .. } => Some("away_summary"),
            SystemBody::ApiError { .. } => Some("api_error"),
            SystemBody::LocalCommand { .. } => Some("local_command"),
            SystemBody::CompactBoundary { .. } => Some("compact_boundary"),
            SystemBody::Informational { .. } => Some("informational"),
            SystemBody::ScheduledTaskFire { .. } => Some("scheduled_task_fire"),
            SystemBody::BridgeStatus { .. } => Some("bridge_status"),
            SystemBody::Unknown => None,
        }
    }

    pub fn duration_ms(&self) -> Option<u64> {
        match &self.body {
            SystemBody::TurnDuration { duration_ms, .. } => *duration_ms,
            _ => None,
        }
    }

    pub fn message_count(&self) -> Option<u64> {
        match &self.body {
            SystemBody::TurnDuration { message_count, .. } => *message_count,
            _ => None,
        }
    }

    pub fn pending_workflow_count(&self) -> Option<u64> {
        match &self.body {
            SystemBody::TurnDuration {
                pending_workflow_count,
                ..
            } => *pending_workflow_count,
            _ => None,
        }
    }

    pub fn is_meta(&self) -> Option<bool> {
        match &self.body {
            SystemBody::TurnDuration { is_meta, .. }
            | SystemBody::AwaySummary { is_meta, .. }
            | SystemBody::LocalCommand { is_meta, .. }
            | SystemBody::Informational { is_meta, .. }
            | SystemBody::ScheduledTaskFire { is_meta, .. } => *is_meta,
            _ => None,
        }
    }

    pub fn hook_count(&self) -> Option<u64> {
        match &self.body {
            SystemBody::StopHookSummary { hook_count, .. } => *hook_count,
            _ => None,
        }
    }

    pub fn hook_infos(&self) -> Option<&Vec<HookInfo>> {
        match &self.body {
            SystemBody::StopHookSummary { hook_infos, .. } => hook_infos.as_ref(),
            _ => None,
        }
    }

    pub fn hook_errors(&self) -> Option<&Vec<serde_json::Value>> {
        match &self.body {
            SystemBody::StopHookSummary { hook_errors, .. } => hook_errors.as_ref(),
            _ => None,
        }
    }

    pub fn prevented_continuation(&self) -> Option<bool> {
        match &self.body {
            SystemBody::StopHookSummary {
                prevented_continuation,
                ..
            } => *prevented_continuation,
            _ => None,
        }
    }

    pub fn stop_reason(&self) -> Option<&str> {
        match &self.body {
            SystemBody::StopHookSummary { stop_reason, .. } => stop_reason.as_deref(),
            _ => None,
        }
    }

    pub fn has_output(&self) -> Option<bool> {
        match &self.body {
            SystemBody::StopHookSummary { has_output, .. } => *has_output,
            _ => None,
        }
    }

    pub fn tool_use_id(&self) -> Option<&str> {
        match &self.body {
            SystemBody::StopHookSummary { tool_use_id, .. } => tool_use_id.as_deref(),
            _ => None,
        }
    }

    pub fn level(&self) -> Option<&str> {
        match &self.body {
            SystemBody::StopHookSummary { level, .. }
            | SystemBody::ApiError { level, .. }
            | SystemBody::LocalCommand { level, .. }
            | SystemBody::CompactBoundary { level, .. }
            | SystemBody::Informational { level, .. } => level.as_deref(),
            _ => None,
        }
    }

    pub fn content(&self) -> Option<&serde_json::Value> {
        match &self.body {
            SystemBody::AwaySummary { content, .. }
            | SystemBody::LocalCommand { content, .. }
            | SystemBody::CompactBoundary { content, .. }
            | SystemBody::Informational { content, .. }
            | SystemBody::ScheduledTaskFire { content, .. }
            | SystemBody::BridgeStatus { content, .. } => content.as_ref(),
            _ => None,
        }
    }

    /// Returns the `ApiError.error` field as a string if it is one. In recent
    /// data the field is sometimes a structured upstream response object; use
    /// [`Self::error_raw`] to inspect that case.
    pub fn error(&self) -> Option<&str> {
        match &self.body {
            SystemBody::ApiError { error, .. } => error.as_ref().and_then(|v| v.as_str()),
            _ => None,
        }
    }

    /// Returns the `ApiError.error` field as raw JSON (string, object, …).
    pub fn error_raw(&self) -> Option<&serde_json::Value> {
        match &self.body {
            SystemBody::ApiError { error, .. } => error.as_ref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;

    #[test]
    fn parse_system_entry_with_subtype_and_duration() {
        // A pre-2.1.159-style entry that carries durationMs at top level used
        // to mean turn_duration; the field flows into the TurnDuration variant.
        let json = r#"{
            "type": "system",
            "uuid": "s-001",
            "parentUuid": "u-001",
            "isSidechain": false,
            "timestamp": "2026-03-16T13:50:01.000Z",
            "sessionId": "sess-001",
            "cwd": "/tmp",
            "version": "2.0.77",
            "subtype": "turn_duration",
            "durationMs": 1523,
            "messageCount": 4,
            "isMeta": false
        }"#;

        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.uuid.as_deref(), Some("s-001"));
        assert_eq!(entry.subtype(), Some("turn_duration"));
        assert_eq!(entry.duration_ms(), Some(1523));
    }

    #[test]
    fn parse_system_via_entry_enum() {
        let json = r#"{
            "type": "system",
            "uuid": "s-002",
            "sessionId": "sess-004",
            "subtype": "informational",
            "content": "System initialized",
            "level": "info",
            "isMeta": true
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::System(s) => {
                assert_eq!(s.subtype(), Some("informational"));
                assert_eq!(s.level(), Some("info"));
            }
            other => panic!("Expected System, got: {other:?}"),
        }
    }

    #[test]
    fn parse_system_turn_duration() {
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
        assert_eq!(entry.subtype(), Some("turn_duration"));
        assert_eq!(entry.duration_ms(), Some(131122));
        assert_eq!(entry.is_meta(), Some(false));
        assert_eq!(entry.message_count(), Some(18));
        assert_eq!(entry.pending_workflow_count(), Some(2));
    }

    #[test]
    fn parse_system_turn_duration_without_pending_workflow_count() {
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
        assert_eq!(entry.message_count(), Some(4));
        assert!(entry.pending_workflow_count().is_none());
    }

    #[test]
    fn parse_system_local_command() {
        let json = r#"{
            "type": "system",
            "subtype": "local_command",
            "uuid": "s-lc-001",
            "sessionId": "sess-lc",
            "content": "<command-name>/workflows</command-name>",
            "level": "info",
            "isMeta": false
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.subtype(), Some("local_command"));
        assert_eq!(entry.level(), Some("info"));
        assert_eq!(entry.is_meta(), Some(false));
        let content = entry.content().expect("content must be Some");
        assert!(content.is_string());
        assert!(content.as_str().unwrap().contains("/workflows"));
    }

    #[test]
    fn parse_system_away_summary() {
        let json = r#"{
            "type": "system",
            "subtype": "away_summary",
            "uuid": "s-as-001",
            "sessionId": "sess-as",
            "content": "You wanted me to ...",
            "isMeta": false
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.subtype(), Some("away_summary"));
        let content = entry.content().expect("content must be Some");
        assert!(content.is_string());
        assert!(entry.level().is_none());
    }

    #[test]
    fn parse_system_legacy_no_subtype_fields() {
        // A legacy SystemEntry where the only known subtype is a value the
        // newer enum doesn't yet model — must still parse, landing in
        // `Unknown`. (The flat accessors will report `None`.)
        let json = r#"{
            "type": "system",
            "uuid": "s-legacy-159",
            "sessionId": "sess-legacy",
            "subtype": "tool_result",
            "message": {"role": "system", "content": "done"}
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        // tool_result isn't a known subtype after the v2 refactor → Unknown.
        assert!(entry.subtype().is_none());
        assert!(entry.hook_count().is_none());
    }

    #[test]
    fn parse_system_stop_hook_summary() {
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
        assert_eq!(entry.subtype(), Some("stop_hook_summary"));
        assert_eq!(entry.hook_count(), Some(1));

        let hook_infos = entry.hook_infos().expect("hook_infos must be Some");
        assert_eq!(hook_infos.len(), 1);
        assert_eq!(
            hook_infos[0].command.as_deref(),
            Some("bash ${CLAUDE_PLUGIN_ROOT}/hooks/emit-event.sh")
        );
        assert_eq!(hook_infos[0].duration_ms, Some(20));

        let hook_errors = entry.hook_errors().expect("hook_errors must be Some");
        assert_eq!(hook_errors.len(), 0);

        assert_eq!(entry.prevented_continuation(), Some(false));
        assert_eq!(entry.level(), Some("suggestion"));
        assert_eq!(
            entry.tool_use_id(),
            Some("e7953fc8-1234-5678-abcd-ef1234567890")
        );
    }

    #[test]
    fn parse_system_stop_hook_summary_with_errors() {
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
        assert_eq!(entry.hook_count(), Some(2));

        let hook_errors = entry.hook_errors().expect("hook_errors must be Some");
        assert_eq!(hook_errors.len(), 1);
        assert_eq!(hook_errors[0]["message"], "hook1.sh exited with code 1");

        assert_eq!(entry.prevented_continuation(), Some(true));
        assert_eq!(entry.level(), Some("error"));

        let hook_infos = entry.hook_infos().unwrap();
        assert_eq!(hook_infos.len(), 2);
        assert_eq!(hook_infos[0].command.as_deref(), Some("hook1.sh"));
        assert_eq!(hook_infos[0].duration_ms, Some(100));
        assert_eq!(hook_infos[1].command.as_deref(), Some("hook2.sh"));
        assert_eq!(hook_infos[1].duration_ms, Some(50));
    }

    #[test]
    fn parse_system_stop_hook_summary_all_optional_fields() {
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
        assert_eq!(entry.stop_reason(), Some("end_turn"));
        assert_eq!(entry.has_output(), Some(true));
        assert_eq!(entry.hook_count(), Some(1));
    }

    #[test]
    fn parse_hook_info_partial_fields_command_only() {
        let json = r#"{
            "type": "system",
            "subtype": "stop_hook_summary",
            "hookCount": 1,
            "hookInfos": [{"command": "bash hook.sh"}]
        }"#;
        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::System(s) => {
                let hooks = s.hook_infos().expect("hook_infos parses");
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
                let hooks = s.hook_infos().unwrap();
                assert_eq!(hooks.len(), 1);
                assert!(hooks[0].command.is_none());
                assert_eq!(hooks[0].duration_ms, Some(50));
            }
            other => panic!("Expected System, got: {other:?}"),
        }
    }

    #[test]
    fn parse_system_unknown_subtype_lands_in_unknown() {
        let json = r#"{
            "type":"system",
            "uuid":"s-u-1",
            "sessionId":"s1",
            "subtype":"some_future_subtype_xyz"
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry.body, SystemBody::Unknown));
        assert!(entry.subtype().is_none());
    }

    #[test]
    fn parse_system_compact_boundary_uses_logical_parent() {
        let json = r#"{
            "type":"system",
            "uuid":"cb-1",
            "sessionId":"s1",
            "subtype":"compact_boundary",
            "logicalParentUuid":"prev-collapsed",
            "compactMetadata":{"x":1},
            "content":"<collapsed>...</collapsed>",
            "level":"info"
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.subtype(), Some("compact_boundary"));
        assert!(entry.parent_uuid.is_none());
        assert_eq!(entry.logical_parent_uuid.as_deref(), Some("prev-collapsed"));
        // DagNode falls back to logical_parent_uuid when parent is missing.
        assert_eq!(entry.parent_uuid(), Some("prev-collapsed"));
    }

    #[test]
    fn parse_system_api_error() {
        let json = r#"{
            "type":"system",
            "uuid":"sae-1",
            "sessionId":"s1",
            "subtype":"api_error",
            "error":"rate_limit",
            "level":"warn",
            "maxRetries":5,
            "retryAttempt":2,
            "retryInMs":1500
        }"#;
        let entry: SystemEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.subtype(), Some("api_error"));
        assert_eq!(entry.error(), Some("rate_limit"));
        assert_eq!(entry.level(), Some("warn"));
    }
}
