//! Independent spec-derived tests for cc-session-jsonl v2 strong-typed Entry.
//!
//! These tests are derived from the REQUIREMENT SPECIFICATION (acceptance criteria
//! A through D) and the field survey (`docs/cc-session-jsonl-v2-field-survey.md`).
//! They are NOT derived from what the code currently does — each expected value
//! comes from the spec. The goal is to catch "known type, broken shape" regressions
//! and verify the soft-landing invariants the v2 design promises.
//!
//! Requirement clauses covered:
//!   A. Per entry-type: (a) full fields, (b) required-only, (c) struct drift
//!   B. SystemBody known subtypes + Unknown soft-landing
//!      AttachmentBody known variants + Unknown soft-landing
//!   C. Low-cardinality enum fields: known variants + Unknown soft-landing
//!   D. Ghost-key defence (stop_details / container = null must NOT break parse)

use cc_session_jsonl::parse_entry;
use cc_session_jsonl::types::{
    assistant::{AssistantEntry, ContentBlock, Usage},
    attachment::{AttachmentBody, AttachmentEntry},
    common::{CacheMissReasonKind, OriginKind, PermissionMode, PromptSource, StopReason},
    progress::{ProgressData, ProgressEntry},
    system::{SystemBody, SystemEntry},
    user::{UserContentKind, UserEntry},
    Entry,
};

// ════════════════════════════════════════════════════════════════════════════
// REQUIREMENT A.1 — UserEntry
// ════════════════════════════════════════════════════════════════════════════

/// A.1(a) Full UserEntry: every optional field populated.
/// Expected values come from survey §3 user row + types/user.rs field list.
#[test]
fn spec_user_full_fields_deserialization() {
    let json = r##"{
        "type": "user",
        "uuid": "spec-u-001",
        "parentUuid": "spec-p-001",
        "sessionId": "spec-sess-001",
        "timestamp": "2026-06-01T10:00:00.000Z",
        "cwd": "/home/spec/project",
        "version": "2.1.140",
        "gitBranch": "main",
        "userType": "external",
        "entrypoint": "cli",
        "isSidechain": false,
        "logicalParentUuid": "logic-parent-1",
        "promptId": "prompt-uuid-001",
        "slug": "my-session",
        "agentId": "agent-001",
        "teamName": "team-a",
        "agentName": "builder",
        "agentColor": "#ff0000",
        "message": {"role": "user", "content": "Fix the issue"},
        "toolUseResult": {"status": "done", "output": "ok"},
        "sourceToolUseID": "toolu_spec_001",
        "sourceToolAssistantUUID": "asst-spec-001",
        "permissionMode": "bypassPermissions",
        "isMeta": false,
        "promptSource": "text",
        "origin": {"kind": "cli"},
        "interruptedMessageId": "asst-interrupted-001",
        "isCompactSummary": false,
        "isVisibleInTranscriptOnly": true,
        "mcpMeta": {"structuredContent": {"tool": "readFile"}},
        "imagePasteIds": ["img-1", "img-2"]
    }"##;

    let entry: UserEntry = serde_json::from_str(json).unwrap();
    // Verify all DAG fields (requirement: 9 universal DAG fields)
    assert_eq!(entry.uuid.as_deref(), Some("spec-u-001"));
    assert_eq!(entry.parent_uuid.as_deref(), Some("spec-p-001"));
    assert_eq!(entry.session_id.as_deref(), Some("spec-sess-001"));
    assert_eq!(entry.timestamp.as_deref(), Some("2026-06-01T10:00:00.000Z"));
    assert_eq!(entry.cwd.as_deref(), Some("/home/spec/project"));
    assert_eq!(entry.version.as_deref(), Some("2.1.140"));
    assert_eq!(entry.git_branch.as_deref(), Some("main"));
    // v2: userType / entrypoint are typed enums with `Unknown` soft-landing.
    assert_eq!(
        entry.user_type,
        Some(cc_session_jsonl::types::UserType::External)
    );
    assert_eq!(
        entry.entrypoint,
        Some(cc_session_jsonl::types::Entrypoint::Cli)
    );
    assert_eq!(entry.is_sidechain, Some(false));

    // v2 new fields (survey §3 user row)
    assert_eq!(entry.prompt_source, Some(PromptSource::Text));
    assert_eq!(
        entry.origin.as_ref().and_then(|o| o.kind),
        Some(OriginKind::Cli)
    );
    assert_eq!(
        entry.interrupted_message_id.as_deref(),
        Some("asst-interrupted-001")
    );
    assert_eq!(entry.is_compact_summary, Some(false));
    assert_eq!(entry.is_visible_in_transcript_only, Some(true));
    let mcp = entry.mcp_meta.as_ref().expect("mcp_meta must be Some");
    let sc = mcp
        .structured_content
        .as_ref()
        .expect("structuredContent must be Some");
    assert_eq!(sc["tool"], "readFile");
    let ids = entry
        .image_paste_ids
        .as_ref()
        .expect("imagePasteIds must be Some");
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0].as_str(), Some("img-1"));
}

/// A.1(b) UserEntry with ONLY required fields (all optionals omitted).
/// Must parse successfully per spec: "optional omitted → still parse success".
#[test]
fn spec_user_required_only_parses() {
    // Per survey §3: uuid, sessionId, timestamp, cwd, version, gitBranch,
    // userType, entrypoint, isSidechain, message are "REQUIRED" candidates.
    // But the Rust types keep them all Option<> for cross-version tolerance.
    // The spec guarantee is: even with truly minimal JSON it must not fail.
    let json = r#"{"type": "user", "uuid": "u-min", "sessionId": "s-min", "message": {"role": "user", "content": "hello"}}"#;

    let entry: Entry = parse_entry(json).unwrap();
    match entry {
        Entry::User(u) => {
            assert_eq!(u.uuid.as_deref(), Some("u-min"));
            // All optional fields must be None, not erroring
            assert!(u.parent_uuid.is_none());
            assert!(u.timestamp.is_none());
            assert!(u.prompt_source.is_none());
            assert!(u.origin.is_none());
            assert!(u.is_compact_summary.is_none());
            assert!(u.mcp_meta.is_none());
            assert!(u.image_paste_ids.is_none());
        }
        other => panic!("Expected User, got {other:?}"),
    }
}

/// A.1(c) StructDrift on UserEntry: uuid arrives as integer instead of string.
/// Requirement: returns ParseError::StructDrift { entry_type: "user", .. }
/// NOT ParseError::Json.
#[test]
fn spec_user_struct_drift_uuid_wrong_type() {
    // uuid should be a string; here it's a number — this is the "known type,
    // broken shape" scenario the requirement mandates returns StructDrift.
    let json = r#"{"type": "user", "uuid": 12345, "sessionId": "s1", "message": {"role":"user","content":"hi"}}"#;
    let err = parse_entry(json).unwrap_err();
    match err {
        cc_session_jsonl::ParseError::StructDrift { entry_type, .. } => {
            assert_eq!(
                entry_type, "user",
                "StructDrift must name the correct entry type"
            );
        }
        other => panic!("Expected StructDrift{{entry_type:\"user\"}}, got: {other}"),
    }
}

/// A.1(c) StructDrift on UserEntry: imagePasteIds is the wrong outer shape
/// (a bare string rather than an array). Element type is now
/// `serde_json::Value`, so individual integer/string elements both parse —
/// the drift now only triggers when the field is not an array at all.
#[test]
fn spec_user_struct_drift_image_paste_ids_wrong_type() {
    let json = r#"{"type":"user","uuid":"u1","sessionId":"s1","imagePasteIds":"not-an-array"}"#;
    let err = parse_entry(json).unwrap_err();
    match err {
        cc_session_jsonl::ParseError::StructDrift { entry_type, .. } => {
            assert_eq!(entry_type, "user");
        }
        other => panic!("Expected StructDrift for user, got: {other}"),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// REQUIREMENT A.2 — AssistantEntry
// ════════════════════════════════════════════════════════════════════════════

/// A.2(a) Full AssistantEntry: all optional fields populated including new v2 fields.
/// Golden values come from survey §3 assistant row and types/assistant.rs field list.
#[test]
fn spec_assistant_full_fields_deserialization() {
    let json = r##"{
        "type": "assistant",
        "uuid": "spec-a-001",
        "parentUuid": "spec-u-001",
        "sessionId": "spec-sess-001",
        "timestamp": "2026-06-01T10:00:01.000Z",
        "cwd": "/home/spec",
        "version": "2.1.140",
        "gitBranch": "main",
        "userType": "external",
        "entrypoint": "cli",
        "isSidechain": false,
        "requestId": "req-spec-001",
        "agentId": "agent-spec-001",
        "slug": "spec-session",
        "apiError": "rate_limit",
        "error": "err detail",
        "errorDetails": "full detail",
        "isApiErrorMessage": false,
        "isVirtual": false,
        "advisorModel": "advisor-m",
        "apiErrorStatus": 429,
        "attributionPlugin": "superpowers",
        "attributionSkill": "superpowers:brainstorming",
        "attributionAgent": "general-purpose",
        "attributionMcpServer": "plugin:fs",
        "attributionMcpTool": "read_file",
        "teamName": "team-a",
        "agentName": "builder",
        "agentColor": "#0000ff",
        "message": {
            "id": "msg_spec_001",
            "model": "claude-opus-4-6",
            "role": "assistant",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 100,
                "output_tokens": 200,
                "cache_creation_input_tokens": 50,
                "cache_read_input_tokens": 1000,
                "cache_creation": {"ephemeral_5m_input_tokens": 50, "ephemeral_1h_input_tokens": 0},
                "server_tool_use": {"web_search_requests": 1, "web_fetch_requests": 0},
                "service_tier": "standard",
                "inference_geo": "us",
                "speed": "fast"
            },
            "content": [
                {"type": "text", "text": "Here is my answer."},
                {"type": "thinking", "thinking": "Let me think...", "signature": "sig-abc"},
                {"type": "tool_use", "id": "toolu_spec", "name": "Bash", "input": {"command": "ls"}},
                {"type": "redacted_thinking", "data": "opaque"}
            ],
            "diagnostics": {
                "cache_miss_reason": {"type": "tools_changed", "cache_missed_input_tokens": 50000}
            },
            "context_management": {"type": "context_management", "details": {}}
        }
    }"##;

    let entry: AssistantEntry = serde_json::from_str(json).unwrap();
    // parentUuid is String (not Option) per v2 requirement
    assert_eq!(entry.parent_uuid, "spec-u-001");
    assert_eq!(entry.request_id.as_deref(), Some("req-spec-001"));
    assert_eq!(entry.api_error_status, Some(429));
    assert_eq!(entry.attribution_plugin.as_deref(), Some("superpowers"));
    assert_eq!(
        entry.attribution_skill.as_deref(),
        Some("superpowers:brainstorming")
    );
    assert_eq!(entry.attribution_agent.as_deref(), Some("general-purpose"));
    assert_eq!(entry.attribution_mcp_server.as_deref(), Some("plugin:fs"));
    assert_eq!(entry.attribution_mcp_tool.as_deref(), Some("read_file"));

    let msg = &entry.message;
    assert_eq!(msg.stop_reason, Some(StopReason::EndTurn));

    let usage = msg.usage.as_ref().unwrap();
    // Golden values: from the JSON above, not from running the code
    assert_eq!(usage.input_tokens, Some(100));
    assert_eq!(usage.output_tokens, Some(200));
    assert_eq!(usage.cache_creation_input_tokens, Some(50));
    assert_eq!(usage.cache_read_input_tokens, Some(1000));
    let cc = usage.cache_creation.as_ref().unwrap();
    assert_eq!(cc.ephemeral_5m_input_tokens, Some(50));
    assert_eq!(cc.ephemeral_1h_input_tokens, Some(0));
    let stu = usage.server_tool_use.as_ref().unwrap();
    assert_eq!(stu.web_search_requests, Some(1));
    assert_eq!(stu.web_fetch_requests, Some(0));

    let content = msg.content.as_ref().unwrap();
    assert_eq!(content.len(), 4);
    match &content[0] {
        ContentBlock::Text { text } => assert_eq!(text.as_deref(), Some("Here is my answer.")),
        other => panic!("expected Text, got {other:?}"),
    }
    match &content[1] {
        ContentBlock::Thinking {
            thinking,
            signature,
        } => {
            assert_eq!(thinking.as_deref(), Some("Let me think..."));
            assert_eq!(signature.as_deref(), Some("sig-abc"));
        }
        other => panic!("expected Thinking, got {other:?}"),
    }
    match &content[2] {
        ContentBlock::ToolUse { id, name, .. } => {
            assert_eq!(id.as_deref(), Some("toolu_spec"));
            assert_eq!(name.as_deref(), Some("Bash"));
        }
        other => panic!("expected ToolUse, got {other:?}"),
    }
    match &content[3] {
        ContentBlock::RedactedThinking { data } => {
            assert_eq!(data.as_deref(), Some("opaque"));
        }
        other => panic!("expected RedactedThinking, got {other:?}"),
    }

    let diag = msg.diagnostics.as_ref().unwrap();
    let cmr = diag.cache_miss_reason.as_ref().unwrap();
    assert_eq!(cmr.kind, Some(CacheMissReasonKind::ToolsChanged));
    assert_eq!(cmr.cache_missed_input_tokens, Some(50000));
    assert!(msg.context_management.is_some());
}

/// A.2(b) AssistantEntry with only the truly required field: parentUuid (a String, not Option).
/// All other fields omitted — must parse successfully.
#[test]
fn spec_assistant_required_only_parses() {
    // Per v2 design: parentUuid is String (required). All else is Option.
    let json = r#"{"type":"assistant","parentUuid":"parent-001","sessionId":"s1","message":{"model":"claude-opus-4-6","role":"assistant","content":[]}}"#;
    let entry: Entry = parse_entry(json).unwrap();
    match entry {
        Entry::Assistant(a) => {
            assert_eq!(a.parent_uuid, "parent-001");
            assert!(a.uuid.is_none());
            assert!(a.request_id.is_none());
            assert!(a.attribution_agent.is_none());
            assert!(a.api_error_status.is_none());
        }
        other => panic!("Expected Assistant, got {other:?}"),
    }
}

/// A.2(c) StructDrift on AssistantEntry: input_tokens arrives as string.
/// Requirement: ParseError::StructDrift { entry_type: "assistant", .. }
#[test]
fn spec_assistant_struct_drift_input_tokens_string() {
    let json = r#"{"type":"assistant","uuid":"a1","parentUuid":"p","sessionId":"s1","message":{"model":"m","role":"assistant","usage":{"input_tokens":"not-a-number"}}}"#;
    let err = parse_entry(json).unwrap_err();
    match err {
        cc_session_jsonl::ParseError::StructDrift { entry_type, .. } => {
            assert_eq!(entry_type, "assistant");
        }
        other => panic!("Expected StructDrift{{entry_type:\"assistant\"}}, got: {other}"),
    }
}

/// A.2(c) StructDrift on AssistantEntry: api_error_status as string instead of u16.
#[test]
fn spec_assistant_struct_drift_api_error_status_string() {
    let json = r#"{"type":"assistant","parentUuid":"p","sessionId":"s1","apiErrorStatus":"four-hundred","message":{"model":"m","role":"assistant","content":[]}}"#;
    let err = parse_entry(json).unwrap_err();
    match err {
        cc_session_jsonl::ParseError::StructDrift { entry_type, .. } => {
            assert_eq!(entry_type, "assistant");
        }
        other => panic!("Expected StructDrift for assistant api_error_status, got: {other}"),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// REQUIREMENT A.3 — SystemEntry
// ════════════════════════════════════════════════════════════════════════════

/// A.3(a) Full SystemEntry with turn_duration subtype and all optional fields.
/// Golden values from survey §4 turn_duration row.
#[test]
fn spec_system_full_turn_duration() {
    let json = r#"{
        "type": "system",
        "uuid": "spec-sys-001",
        "parentUuid": "spec-u-001",
        "sessionId": "spec-sess-001",
        "timestamp": "2026-06-01T10:00:02.000Z",
        "cwd": "/home/spec",
        "version": "2.1.140",
        "gitBranch": "main",
        "userType": "external",
        "entrypoint": "cli",
        "isSidechain": false,
        "subtype": "turn_duration",
        "durationMs": 15230,
        "messageCount": 12,
        "isMeta": true,
        "pendingWorkflowCount": 2,
        "pendingBackgroundAgentCount": 1
    }"#;

    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.subtype(), Some("turn_duration"));
    // Golden values: 15230ms duration, 12 messages — from the JSON above
    assert_eq!(entry.duration_ms(), Some(15230));
    assert_eq!(entry.message_count(), Some(12));
    assert_eq!(entry.is_meta(), Some(true));
    assert_eq!(entry.pending_workflow_count(), Some(2));
    // pending_background_agent_count is a field on TurnDuration variant
    match &entry.body {
        SystemBody::TurnDuration {
            pending_background_agent_count,
            ..
        } => {
            assert_eq!(*pending_background_agent_count, Some(1));
        }
        other => panic!("Expected TurnDuration body, got {other:?}"),
    }
}

/// A.3(b) SystemEntry with only required fields: uuid + sessionId + subtype.
/// The `subtype` field is the discriminator for the tagged enum SystemBody;
/// the survey marks it REQUIRED for all system entries (§3 system section).
/// With a recognized subtype but no subtype-specific fields — must still parse.
#[test]
fn spec_system_required_only_parses() {
    // subtype is required for SystemEntry (it's the tagged-enum discriminator).
    // An unknown subtype lands in SystemBody::Unknown — this is still a valid parse.
    let json = r#"{"type": "system", "uuid": "sys-min", "sessionId": "s-min", "subtype": "unknown_subtype_xyz"}"#;
    let entry: Entry = parse_entry(json).unwrap();
    match entry {
        Entry::System(s) => {
            assert_eq!(s.uuid.as_deref(), Some("sys-min"));
            // Unknown subtype → SystemBody::Unknown (soft landing, not an error)
            assert!(matches!(s.body, SystemBody::Unknown));
            assert!(s.subtype().is_none());
        }
        other => panic!("Expected System, got {other:?}"),
    }
}

/// A.3(b') SystemEntry with a known subtype but only the subtype discriminator field.
/// The subtype-specific fields all being absent must not cause a parse failure
/// since they are all Option<T> in the body variants.
#[test]
fn spec_system_known_subtype_no_body_fields_parses() {
    let json = r#"{"type": "system", "uuid": "sys-min2", "sessionId": "s-min", "subtype": "turn_duration"}"#;
    let entry: Entry = parse_entry(json).unwrap();
    match entry {
        Entry::System(s) => {
            assert_eq!(s.subtype(), Some("turn_duration"));
            // All TurnDuration fields are Option — absent is fine
            assert!(s.duration_ms().is_none());
            assert!(s.message_count().is_none());
            assert!(s.is_meta().is_none());
        }
        other => panic!("Expected System, got {other:?}"),
    }
}

/// A.3(c) StructDrift on SystemEntry: hookCount arrives as string instead of u64.
#[test]
fn spec_system_struct_drift_hook_count_string() {
    let json = r#"{"type":"system","uuid":"s1","sessionId":"s1","subtype":"stop_hook_summary","hookCount":"not-a-number","hookInfos":[],"hookErrors":[],"preventedContinuation":false,"level":"info","toolUseID":"t1"}"#;
    let err = parse_entry(json).unwrap_err();
    match err {
        cc_session_jsonl::ParseError::StructDrift { entry_type, .. } => {
            assert_eq!(entry_type, "system");
        }
        other => panic!("Expected StructDrift{{entry_type:\"system\"}}, got: {other}"),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// REQUIREMENT A.4 — AttachmentEntry
// ════════════════════════════════════════════════════════════════════════════

/// A.4(a) Full AttachmentEntry with hook_success subtype and all fields.
/// Golden values from survey §5 hook_success row.
#[test]
fn spec_attachment_full_hook_success() {
    let json = r#"{
        "type": "attachment",
        "uuid": "spec-att-001",
        "parentUuid": "spec-u-001",
        "sessionId": "spec-sess-001",
        "timestamp": "2026-06-01T10:00:03.000Z",
        "cwd": "/home/spec",
        "version": "2.1.140",
        "gitBranch": "main",
        "userType": "external",
        "entrypoint": "cli",
        "isSidechain": false,
        "slug": "spec-session",
        "agentId": "agent-spec",
        "attachment": {
            "type": "hook_success",
            "command": "bash /hooks/emit.sh",
            "content": "hook output here",
            "durationMs": 42,
            "exitCode": 0,
            "hookEvent": "PostToolUse",
            "hookName": "PostToolUse:Read",
            "stderr": "",
            "stdout": "ok",
            "toolUseID": "toolu_spec_att"
        }
    }"#;

    let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.uuid.as_deref(), Some("spec-att-001"));
    assert_eq!(entry.attachment_subtype(), Some("hook_success"));
    match entry.attachment.as_ref().unwrap() {
        AttachmentBody::HookSuccess {
            command,
            exit_code,
            hook_event,
            hook_name,
            duration_ms,
            stdout,
            stderr,
            tool_use_id,
            ..
        } => {
            assert_eq!(command.as_deref(), Some("bash /hooks/emit.sh"));
            assert_eq!(*exit_code, Some(0));
            assert_eq!(hook_event.as_deref(), Some("PostToolUse"));
            assert_eq!(hook_name.as_deref(), Some("PostToolUse:Read"));
            assert_eq!(*duration_ms, Some(42));
            assert_eq!(stdout.as_deref(), Some("ok"));
            assert_eq!(stderr.as_deref(), Some(""));
            assert_eq!(tool_use_id.as_deref(), Some("toolu_spec_att"));
        }
        other => panic!("Expected HookSuccess, got {other:?}"),
    }
}

/// A.4(b) AttachmentEntry with only uuid + sessionId (no attachment object).
/// Must parse successfully with attachment = None.
#[test]
fn spec_attachment_required_only_parses() {
    let json = r#"{"type": "attachment", "uuid": "att-min", "sessionId": "s-min"}"#;
    let entry: Entry = parse_entry(json).unwrap();
    match entry {
        Entry::Attachment(a) => {
            assert_eq!(a.uuid.as_deref(), Some("att-min"));
            assert!(a.attachment.is_none());
            assert!(a.parent_uuid.is_none());
            assert!(a.attachment_subtype().is_none());
        }
        other => panic!("Expected Attachment, got {other:?}"),
    }
}

/// A.4(c) StructDrift on AttachmentEntry: isSidechain arrives as string instead of bool.
#[test]
fn spec_attachment_struct_drift_is_sidechain_string() {
    let json = r#"{"type":"attachment","uuid":"att-1","sessionId":"s1","isSidechain":"yes"}"#;
    let err = parse_entry(json).unwrap_err();
    match err {
        cc_session_jsonl::ParseError::StructDrift { entry_type, .. } => {
            assert_eq!(entry_type, "attachment");
        }
        other => panic!("Expected StructDrift{{entry_type:\"attachment\"}}, got: {other}"),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// REQUIREMENT A.5 — ProgressEntry
// ════════════════════════════════════════════════════════════════════════════

/// A.5(a) Full ProgressEntry with hook_progress data and all optional fields.
#[test]
fn spec_progress_full_fields() {
    let json = r#"{
        "type": "progress",
        "uuid": "spec-prog-001",
        "parentUuid": "spec-u-001",
        "sessionId": "spec-sess-001",
        "timestamp": "2026-06-01T10:00:04.000Z",
        "cwd": "/home/spec",
        "version": "2.1.140",
        "gitBranch": "main",
        "userType": "external",
        "entrypoint": "cli",
        "isSidechain": true,
        "slug": "spec-session",
        "agentId": "agent-spec",
        "parentToolUseID": "toolu_parent_001",
        "toolUseID": "toolu_spec_001",
        "data": {
            "type": "hook_progress",
            "hookEvent": "PostToolUse",
            "hookName": "PostToolUse:Write",
            "command": "callback"
        }
    }"#;

    let entry: ProgressEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.uuid.as_deref(), Some("spec-prog-001"));
    assert_eq!(entry.is_sidechain, Some(true));
    assert_eq!(
        entry.parent_tool_use_id.as_deref(),
        Some("toolu_parent_001")
    );
    assert_eq!(entry.tool_use_id.as_deref(), Some("toolu_spec_001"));
    match entry.data.as_ref().unwrap() {
        ProgressData::HookProgress {
            hook_event,
            hook_name,
            command,
        } => {
            assert_eq!(hook_event.as_deref(), Some("PostToolUse"));
            assert_eq!(hook_name.as_deref(), Some("PostToolUse:Write"));
            assert_eq!(command.as_deref(), Some("callback"));
        }
        other => panic!("Expected HookProgress, got {other:?}"),
    }
}

/// A.5(b) ProgressEntry with only uuid + sessionId (no data field).
#[test]
fn spec_progress_required_only_parses() {
    let json = r#"{"type":"progress","uuid":"prog-min","sessionId":"s-min"}"#;
    let entry: Entry = parse_entry(json).unwrap();
    match entry {
        Entry::Progress(p) => {
            assert_eq!(p.uuid.as_deref(), Some("prog-min"));
            assert!(p.data.is_none());
            assert!(p.parent_tool_use_id.is_none());
        }
        other => panic!("Expected Progress, got {other:?}"),
    }
}

/// A.5(c) StructDrift on ProgressEntry: isSidechain as integer instead of bool.
#[test]
fn spec_progress_struct_drift_is_sidechain_integer() {
    let json = r#"{"type":"progress","uuid":"p1","sessionId":"s1","isSidechain":1}"#;
    let err = parse_entry(json).unwrap_err();
    match err {
        cc_session_jsonl::ParseError::StructDrift { entry_type, .. } => {
            assert_eq!(entry_type, "progress");
        }
        other => panic!("Expected StructDrift{{entry_type:\"progress\"}}, got: {other}"),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// REQUIREMENT B — SystemBody known subtypes + Unknown soft-landing
// ════════════════════════════════════════════════════════════════════════════

/// B.1 SystemBody::StopHookSummary — full fields.
#[test]
fn spec_system_body_stop_hook_summary_full() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-shs-001",
        "sessionId": "s1",
        "subtype": "stop_hook_summary",
        "hookCount": 3,
        "hookInfos": [{"command": "a.sh", "durationMs": 10}, {"command": "b.sh", "durationMs": 20}],
        "hookErrors": [{"message": "a.sh failed"}],
        "preventedContinuation": true,
        "stopReason": "end_turn",
        "hasOutput": true,
        "level": "error",
        "toolUseID": "toolu_stop_001"
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.subtype(), Some("stop_hook_summary"));
    assert_eq!(entry.hook_count(), Some(3));
    assert_eq!(entry.prevented_continuation(), Some(true));
    assert_eq!(entry.stop_reason(), Some("end_turn"));
    assert_eq!(entry.has_output(), Some(true));
    assert_eq!(entry.level(), Some("error"));
    assert_eq!(entry.tool_use_id(), Some("toolu_stop_001"));
    let infos = entry.hook_infos().unwrap();
    assert_eq!(infos.len(), 2);
    let errs = entry.hook_errors().unwrap();
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0]["message"], "a.sh failed");
}

/// B.2 SystemBody::AwaySummary subtype.
#[test]
fn spec_system_body_away_summary() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-as-001",
        "sessionId": "s1",
        "subtype": "away_summary",
        "content": "You wanted me to refactor the parser.",
        "isMeta": true
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.subtype(), Some("away_summary"));
    assert_eq!(entry.is_meta(), Some(true));
    let content = entry.content().expect("away_summary must have content");
    assert!(content.is_string());
    assert!(content.as_str().unwrap().contains("refactor"));
}

/// B.3 SystemBody::ApiError subtype.
/// `cause` is `Option<serde_json::Value>` to accept both shapes seen in
/// production data: a plain string (older entries) and a structured object
/// such as `{"code":"...","path":"..."}` (newer transport errors).
#[test]
fn spec_system_body_api_error_full() {
    // Case 1: legacy string-shaped cause.
    let json = r#"{
        "type": "system",
        "uuid": "sys-ae-001",
        "sessionId": "s1",
        "subtype": "api_error",
        "error": "rate_limit_exceeded",
        "level": "warn",
        "maxRetries": 5,
        "retryAttempt": 3,
        "retryInMs": 2000,
        "cause": "Too many requests"
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.subtype(), Some("api_error"));
    assert_eq!(entry.error(), Some("rate_limit_exceeded"));
    assert_eq!(entry.level(), Some("warn"));
    match &entry.body {
        SystemBody::ApiError {
            max_retries,
            retry_attempt,
            retry_in_ms,
            cause,
            ..
        } => {
            assert_eq!(*max_retries, Some(5));
            assert_eq!(*retry_attempt, Some(3));
            assert_eq!(*retry_in_ms, Some(2000.0));
            assert_eq!(
                cause.as_ref().and_then(|v| v.as_str()),
                Some("Too many requests")
            );
        }
        other => panic!("Expected ApiError, got {other:?}"),
    }

    // Case 2: production-observed object-shaped cause
    //   `{"code":"UNKNOWN_CERTIFICATE_VERIFICATION_ERROR","path":"https://..."}`
    // Must parse without StructDrift.
    let json_obj = r#"{
        "type": "system",
        "uuid": "sys-ae-002",
        "sessionId": "s1",
        "subtype": "api_error",
        "error": "transport_error",
        "level": "error",
        "cause": {
            "code": "UNKNOWN_CERTIFICATE_VERIFICATION_ERROR",
            "path": "https://api.anthropic.com/v1/messages"
        }
    }"#;
    let entry: SystemEntry = serde_json::from_str(json_obj).unwrap();
    match &entry.body {
        SystemBody::ApiError { cause, .. } => {
            let cause = cause.as_ref().expect("cause must be Some");
            assert!(cause.is_object(), "production cause shape is object");
            assert_eq!(
                cause.get("code").and_then(|v| v.as_str()),
                Some("UNKNOWN_CERTIFICATE_VERIFICATION_ERROR")
            );
            assert_eq!(
                cause.get("path").and_then(|v| v.as_str()),
                Some("https://api.anthropic.com/v1/messages")
            );
        }
        other => panic!("Expected ApiError, got {other:?}"),
    }

    // Case 3: production-observed object-shaped `error` field. Newer
    // Claude Code versions sometimes embed the full upstream response
    // payload (status, headers, nested error). Must parse without
    // StructDrift, and the convenience `entry.error()` returns None
    // because the value is no longer a plain string.
    let json_err_obj = r#"{
        "type": "system",
        "uuid": "sys-ae-003",
        "sessionId": "s1",
        "subtype": "api_error",
        "level": "error",
        "error": {
            "status": 529,
            "error": {
                "type": "overloaded_error",
                "message": "Overloaded"
            },
            "type": "overloaded_error"
        },
        "retryInMs": 597.98,
        "retryAttempt": 1,
        "maxRetries": 10
    }"#;
    let entry: SystemEntry = serde_json::from_str(json_err_obj).unwrap();
    assert_eq!(entry.error(), None, "object-shaped error is not a string");
    let raw = entry.error_raw().expect("error_raw must be Some");
    assert!(raw.is_object());
    assert_eq!(raw.get("status").and_then(|v| v.as_u64()), Some(529));
}

/// B.4 SystemBody::LocalCommand subtype.
#[test]
fn spec_system_body_local_command() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-lc-spec",
        "sessionId": "s1",
        "subtype": "local_command",
        "content": "<command-name>/doctor</command-name>",
        "level": "info",
        "isMeta": false
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.subtype(), Some("local_command"));
    assert_eq!(entry.level(), Some("info"));
    let content = entry.content().expect("local_command must have content");
    assert!(content.as_str().unwrap().contains("doctor"));
}

/// B.5 SystemBody::CompactBoundary uses logicalParentUuid not parentUuid.
/// This is a key v2 design invariant from survey §2.
#[test]
fn spec_system_body_compact_boundary_logical_parent() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-cb-spec",
        "sessionId": "s1",
        "subtype": "compact_boundary",
        "logicalParentUuid": "last-before-collapse",
        "compactMetadata": {"collapseId": "0000000000000001"},
        "content": "<collapsed>...</collapsed>",
        "level": "info"
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.subtype(), Some("compact_boundary"));
    // parentUuid must be None (compact_boundary uses logicalParentUuid)
    assert!(
        entry.parent_uuid.is_none(),
        "compact_boundary must not carry parentUuid"
    );
    assert_eq!(
        entry.logical_parent_uuid.as_deref(),
        Some("last-before-collapse")
    );
    // DagNode trait falls back to logicalParentUuid
    use cc_session_jsonl::types::common::DagNode;
    assert_eq!(entry.parent_uuid(), Some("last-before-collapse"));
}

/// B.6 SystemBody::ScheduledTaskFire subtype.
#[test]
fn spec_system_body_scheduled_task_fire() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-stf-001",
        "sessionId": "s1",
        "subtype": "scheduled_task_fire",
        "content": "task fired",
        "isMeta": true
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.subtype(), Some("scheduled_task_fire"));
    assert_eq!(entry.is_meta(), Some(true));
    let content = entry
        .content()
        .expect("scheduled_task_fire must have content");
    assert_eq!(content.as_str(), Some("task fired"));
}

/// B.7 SystemBody::BridgeStatus subtype.
#[test]
fn spec_system_body_bridge_status() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-bs-001",
        "sessionId": "s1",
        "subtype": "bridge_status",
        "content": "connected",
        "url": "https://bridge.example.com/session/123"
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.subtype(), Some("bridge_status"));
    let content = entry.content().expect("bridge_status must have content");
    assert_eq!(content.as_str(), Some("connected"));
    match &entry.body {
        SystemBody::BridgeStatus { url, .. } => {
            assert_eq!(
                url.as_deref(),
                Some("https://bridge.example.com/session/123")
            );
        }
        other => panic!("Expected BridgeStatus, got {other:?}"),
    }
}

/// B.8 SystemBody::Informational subtype.
#[test]
fn spec_system_body_informational() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-info-001",
        "sessionId": "s1",
        "subtype": "informational",
        "content": "Session started.",
        "level": "info",
        "isMeta": false
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.subtype(), Some("informational"));
    assert_eq!(entry.level(), Some("info"));
    assert_eq!(entry.is_meta(), Some(false));
}

/// B.9 SystemBody::Unknown soft-landing — new subtype must NOT error the whole entry.
/// Requirement: unknown subtype lands in Unknown variant, not a parse failure.
#[test]
fn spec_system_body_unknown_subtype_soft_landing() {
    // A future subtype that doesn't exist in the v2 enum.
    let json = r#"{
        "type": "system",
        "uuid": "sys-future-001",
        "sessionId": "s1",
        "subtype": "future_subtype_xyz_2030",
        "someNewField": "someValue"
    }"#;
    let entry: Entry = parse_entry(json).unwrap();
    match entry {
        Entry::System(s) => {
            assert!(
                matches!(s.body, SystemBody::Unknown),
                "Unknown subtype must soft-land in SystemBody::Unknown"
            );
            // The accessors return None for Unknown — correct behaviour
            assert!(s.subtype().is_none());
            assert!(s.duration_ms().is_none());
            assert!(s.hook_count().is_none());
        }
        other => panic!("Expected System, got {other:?}"),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// REQUIREMENT B — AttachmentBody known variants + Unknown soft-landing
// ════════════════════════════════════════════════════════════════════════════

/// B.10 AttachmentBody::OutputStyle variant.
#[test]
fn spec_attachment_body_output_style() {
    let json = r#"{
        "type": "attachment",
        "uuid": "att-os-spec",
        "sessionId": "s1",
        "attachment": {"type": "output_style", "style": "concise"}
    }"#;
    let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.attachment_subtype(), Some("output_style"));
    match entry.attachment.as_ref().unwrap() {
        AttachmentBody::OutputStyle { style } => assert_eq!(style.as_deref(), Some("concise")),
        other => panic!("Expected OutputStyle, got {other:?}"),
    }
}

/// B.11 AttachmentBody::TaskReminder variant.
#[test]
fn spec_attachment_body_task_reminder() {
    let json = r#"{
        "type": "attachment",
        "uuid": "att-tr-spec",
        "sessionId": "s1",
        "attachment": {"type": "task_reminder", "content": "5 tasks pending", "itemCount": 5}
    }"#;
    let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.attachment_subtype(), Some("task_reminder"));
    match entry.attachment.as_ref().unwrap() {
        AttachmentBody::TaskReminder { item_count, .. } => {
            assert_eq!(*item_count, Some(5));
        }
        other => panic!("Expected TaskReminder, got {other:?}"),
    }
}

/// B.12 AttachmentBody::DeferredToolsDelta variant.
#[test]
fn spec_attachment_body_deferred_tools_delta() {
    let json = r#"{
        "type": "attachment",
        "uuid": "att-dt-spec",
        "sessionId": "s1",
        "attachment": {
            "type": "deferred_tools_delta",
            "addedLines": [],
            "addedNames": ["Bash", "Read"],
            "removedNames": ["deprecated_tool"],
            "readdedNames": [],
            "pendingMcpServers": ["plugin:memory"]
        }
    }"#;
    let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.attachment_subtype(), Some("deferred_tools_delta"));
    match entry.attachment.as_ref().unwrap() {
        AttachmentBody::DeferredToolsDelta {
            added_names,
            removed_names,
            pending_mcp_servers,
            ..
        } => {
            let an = added_names.as_ref().unwrap();
            assert_eq!(an.len(), 2);
            assert_eq!(an[0], "Bash");
            let rn = removed_names.as_ref().unwrap();
            assert_eq!(rn[0], "deprecated_tool");
            let ms = pending_mcp_servers.as_ref().unwrap();
            assert_eq!(ms[0], "plugin:memory");
        }
        other => panic!("Expected DeferredToolsDelta, got {other:?}"),
    }
}

/// B.13 AttachmentBody::SkillListing variant.
#[test]
fn spec_attachment_body_skill_listing() {
    let json = r#"{
        "type": "attachment",
        "uuid": "att-sl-spec",
        "sessionId": "s1",
        "attachment": {
            "type": "skill_listing",
            "content": "3 skills active",
            "isInitial": false,
            "skillCount": 3,
            "names": ["skill-a", "skill-b", "skill-c"]
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
            assert_eq!(*is_initial, Some(false));
            assert_eq!(*skill_count, Some(3));
            let n = names.as_ref().unwrap();
            assert_eq!(n.len(), 3);
            assert_eq!(n[2], "skill-c");
        }
        other => panic!("Expected SkillListing, got {other:?}"),
    }
}

/// B.14 AttachmentBody::QueuedCommand variant.
#[test]
fn spec_attachment_body_queued_command() {
    let json = r#"{
        "type": "attachment",
        "uuid": "att-qc-spec",
        "sessionId": "s1",
        "attachment": {
            "type": "queued_command",
            "commandMode": "prompt",
            "prompt": "continue with the plan",
            "imagePasteIds": ["img-abc"],
            "sourceUuid": "some-uuid"
        }
    }"#;
    let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.attachment_subtype(), Some("queued_command"));
    match entry.attachment.as_ref().unwrap() {
        AttachmentBody::QueuedCommand {
            command_mode,
            prompt,
            image_paste_ids,
            source_uuid,
        } => {
            assert_eq!(command_mode.as_deref(), Some("prompt"));
            assert_eq!(prompt.as_deref(), Some("continue with the plan"));
            let ids = image_paste_ids.as_ref().unwrap();
            assert_eq!(ids[0], "img-abc");
            assert_eq!(source_uuid.as_deref(), Some("some-uuid"));
        }
        other => panic!("Expected QueuedCommand, got {other:?}"),
    }
}

/// B.15 AttachmentBody::Unknown soft-landing.
/// Requirement: long-tail attachment types (e.g. "diagnostics", "file",
/// "hook_additional_context") must NOT error — they land in Unknown.
#[test]
fn spec_attachment_body_unknown_soft_landing_file_type() {
    // "file" is a known long-tail type from the survey (n=80) but NOT modelled
    // as a typed variant — should soft-land in Unknown.
    let json = r#"{
        "type": "attachment",
        "uuid": "att-file-001",
        "sessionId": "s1",
        "attachment": {
            "type": "file",
            "filename": "main.rs",
            "displayPath": "/project/main.rs",
            "content": "fn main() {}"
        }
    }"#;
    let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert!(
        matches!(entry.attachment.as_ref().unwrap(), AttachmentBody::Unknown),
        "file attachment type must soft-land in AttachmentBody::Unknown"
    );
    assert!(
        entry.attachment_subtype().is_none(),
        "Unknown variant must return None from attachment_subtype()"
    );
}

/// B.16 AttachmentBody::Unknown soft-landing for a truly future type.
#[test]
fn spec_attachment_body_unknown_future_type() {
    let json = r#"{
        "type": "attachment",
        "uuid": "att-future-001",
        "sessionId": "s1",
        "attachment": {
            "type": "future_attachment_type_2030",
            "someField": "someValue"
        }
    }"#;
    let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert!(matches!(
        entry.attachment.as_ref().unwrap(),
        AttachmentBody::Unknown
    ));
}

/// B.17 AttachmentEntry without any attachment field at all.
/// The attachment field is Optional — must parse with attachment = None.
#[test]
fn spec_attachment_body_missing_entirely() {
    let json = r#"{"type":"attachment","uuid":"att-no-body","sessionId":"s1"}"#;
    let entry: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert!(
        entry.attachment.is_none(),
        "absent attachment field must yield None, not error"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// REQUIREMENT C — Low-cardinality enum fields
// ════════════════════════════════════════════════════════════════════════════

// ── C.1 PermissionMode ──

/// C.1(a) All known PermissionMode variants deserialize correctly.
/// Wire values from common.rs + field survey §3: bypassPermissions, acceptEdits,
/// default, plan (camelCase via rename_all).
#[test]
fn spec_permission_mode_known_variants() {
    // "bypassPermissions" → PermissionMode::BypassPermissions (camelCase)
    let bypass: PermissionMode = serde_json::from_str(r#""bypassPermissions""#).unwrap();
    assert_eq!(bypass, PermissionMode::BypassPermissions);

    // "acceptEdits" → AcceptEdits
    let accept: PermissionMode = serde_json::from_str(r#""acceptEdits""#).unwrap();
    assert_eq!(accept, PermissionMode::AcceptEdits);

    // "default" → Default
    let default: PermissionMode = serde_json::from_str(r#""default""#).unwrap();
    assert_eq!(default, PermissionMode::Default);

    // "plan" → Plan
    let plan: PermissionMode = serde_json::from_str(r#""plan""#).unwrap();
    assert_eq!(plan, PermissionMode::Plan);
}

/// C.1(b) Unknown PermissionMode value soft-lands in Unknown, not an error.
/// The requirement: unfamiliar future values degrade to Unknown.
#[test]
fn spec_permission_mode_unknown_soft_landing() {
    let unknown: PermissionMode = serde_json::from_str(r#""future_permission_mode_2030""#).unwrap();
    assert_eq!(
        unknown,
        PermissionMode::Unknown,
        "Unknown permission mode must soft-land"
    );
}

// ── C.2 StopReason ──

/// C.2(a) All known StopReason variants deserialize correctly.
/// Wire values use snake_case per types/common.rs StopReason definition.
#[test]
fn spec_stop_reason_known_variants() {
    // Survey real-data values confirmed in docs
    let end_turn: StopReason = serde_json::from_str(r#""end_turn""#).unwrap();
    assert_eq!(end_turn, StopReason::EndTurn);

    let tool_use: StopReason = serde_json::from_str(r#""tool_use""#).unwrap();
    assert_eq!(tool_use, StopReason::ToolUse);

    let stop_seq: StopReason = serde_json::from_str(r#""stop_sequence""#).unwrap();
    assert_eq!(stop_seq, StopReason::StopSequence);

    let pause: StopReason = serde_json::from_str(r#""pause_turn""#).unwrap();
    assert_eq!(pause, StopReason::PauseTurn);

    let refusal: StopReason = serde_json::from_str(r#""refusal""#).unwrap();
    assert_eq!(refusal, StopReason::Refusal);
}

/// C.2(b) Unknown StopReason value soft-lands in Unknown.
#[test]
fn spec_stop_reason_unknown_soft_landing() {
    let unknown: StopReason = serde_json::from_str(r#""future_stop_reason_xyz""#).unwrap();
    assert_eq!(unknown, StopReason::Unknown);
}

/// C.2(c) StopReason::as_str() returns the correct wire format strings.
/// This is golden: the wire format strings come from the spec (common.rs docstring).
#[test]
fn spec_stop_reason_as_str_golden_values() {
    assert_eq!(StopReason::EndTurn.as_str(), "end_turn");
    assert_eq!(StopReason::ToolUse.as_str(), "tool_use");
    assert_eq!(StopReason::StopSequence.as_str(), "stop_sequence");
    assert_eq!(StopReason::PauseTurn.as_str(), "pause_turn");
    assert_eq!(StopReason::Refusal.as_str(), "refusal");
    assert_eq!(StopReason::Unknown.as_str(), "unknown");
}

// ── C.3 PromptSource ──

/// C.3(a) Known PromptSource variants deserialize correctly.
/// Wire values are camelCase: "text", "slashCommand" per common.rs.
#[test]
fn spec_prompt_source_known_variants() {
    let text: PromptSource = serde_json::from_str(r#""text""#).unwrap();
    assert_eq!(text, PromptSource::Text);

    let slash: PromptSource = serde_json::from_str(r#""slashCommand""#).unwrap();
    assert_eq!(slash, PromptSource::SlashCommand);
}

/// C.3(b) Unknown PromptSource soft-lands in Unknown.
#[test]
fn spec_prompt_source_unknown_soft_landing() {
    let unknown: PromptSource = serde_json::from_str(r#""futureSourceType2030""#).unwrap();
    assert_eq!(unknown, PromptSource::Unknown);
}

// ── C.4 OriginKind ──

/// C.4(a) Known OriginKind variants deserialize correctly.
/// Wire values are camelCase: "cli", "ide", "web", "sdk".
#[test]
fn spec_origin_kind_known_variants() {
    let cli: OriginKind = serde_json::from_str(r#""cli""#).unwrap();
    assert_eq!(cli, OriginKind::Cli);

    let ide: OriginKind = serde_json::from_str(r#""ide""#).unwrap();
    assert_eq!(ide, OriginKind::Ide);

    let web: OriginKind = serde_json::from_str(r#""web""#).unwrap();
    assert_eq!(web, OriginKind::Web);

    let sdk: OriginKind = serde_json::from_str(r#""sdk""#).unwrap();
    assert_eq!(sdk, OriginKind::Sdk);
}

/// C.4(b) Unknown OriginKind soft-lands in Unknown.
#[test]
fn spec_origin_kind_unknown_soft_landing() {
    let unknown: OriginKind = serde_json::from_str(r#""futureIntegration2030""#).unwrap();
    assert_eq!(unknown, OriginKind::Unknown);
}

// ── C.5 CacheMissReasonKind ──

/// C.5(a) Known CacheMissReasonKind variants.
/// Wire values are snake_case: "tools_changed", "messages_changed", etc.
#[test]
fn spec_cache_miss_reason_kind_known_variants() {
    let tc: CacheMissReasonKind = serde_json::from_str(r#""tools_changed""#).unwrap();
    assert_eq!(tc, CacheMissReasonKind::ToolsChanged);

    let mc: CacheMissReasonKind = serde_json::from_str(r#""messages_changed""#).unwrap();
    assert_eq!(mc, CacheMissReasonKind::MessagesChanged);

    let sc: CacheMissReasonKind = serde_json::from_str(r#""system_prompt_changed""#).unwrap();
    assert_eq!(sc, CacheMissReasonKind::SystemPromptChanged);
}

/// C.5(b) Unknown CacheMissReasonKind soft-lands.
#[test]
fn spec_cache_miss_reason_kind_unknown_soft_landing() {
    let unknown: CacheMissReasonKind =
        serde_json::from_str(r#""future_cache_miss_reason""#).unwrap();
    assert_eq!(unknown, CacheMissReasonKind::Unknown);
}

// ── C.6 ContentBlock Other ──

/// C.6 ContentBlock::Other soft-landing for unknown block types.
/// The requirement spec says unknown block types degrade to Other, not failing the entry.
#[test]
fn spec_content_block_unknown_type_becomes_other() {
    let json = r#"{"type":"assistant","parentUuid":"p","sessionId":"s1","message":{"model":"m","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1},"content":[{"type":"server_tool_use","id":"srv_01","name":"web_search"},{"type":"text","text":"hi"}]}}"#;
    let entry: AssistantEntry = serde_json::from_str(json).unwrap();
    let content = entry.message.content.unwrap();
    assert_eq!(content.len(), 2);
    assert!(
        matches!(content[0], ContentBlock::Other),
        "server_tool_use must land in ContentBlock::Other"
    );
    assert!(matches!(content[1], ContentBlock::Text { .. }));
}

// ── C.7 ProgressData::Other soft-landing ──

/// C.7 ProgressData::Other soft-landing for unknown progress subtypes.
#[test]
fn spec_progress_data_unknown_type_becomes_other() {
    let json = r#"{"type":"progress","uuid":"p1","sessionId":"s1","data":{"type":"elicitation_progress_xyz","someField":"x"}}"#;
    let entry: ProgressEntry = serde_json::from_str(json).unwrap();
    assert!(
        matches!(entry.data, Some(ProgressData::Other)),
        "Unknown progress data type must land in ProgressData::Other"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// REQUIREMENT D — Ghost-key defence (stop_details / container = null)
// ════════════════════════════════════════════════════════════════════════════

/// D.1 Ghost keys stop_details and container (always null in real data) must NOT
/// break deserialization even though they are not modelled in AssistantEntry.
/// The v2 design spec explicitly states: "Ghost keys (message.stop_details,
/// message.container, both always null in real data) are deliberately not modelled."
/// But the requirement mandates that having them present as null must NOT error.
#[test]
fn spec_ghost_keys_null_stop_details_and_container_accepted() {
    let json = r#"{
        "type": "assistant",
        "uuid": "a-ghost-001",
        "parentUuid": "p-ghost-001",
        "sessionId": "s-ghost-001",
        "message": {
            "id": "msg_ghost",
            "model": "claude-opus-4-6",
            "role": "assistant",
            "stop_reason": "end_turn",
            "stop_details": null,
            "container": null,
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0
            },
            "content": [{"type": "text", "text": "done"}]
        }
    }"#;
    // The requirement: "must be able to successfully deserialize (not error)"
    let entry: AssistantEntry = serde_json::from_str(json)
        .expect("Ghost keys stop_details=null and container=null must NOT cause parse failure");
    assert_eq!(entry.parent_uuid, "p-ghost-001");
    let msg = entry.message;
    assert_eq!(msg.stop_reason, Some(StopReason::EndTurn));
    // Fields we DO model are still accessible
    let usage = msg.usage.unwrap();
    assert_eq!(usage.input_tokens, Some(10));
    assert_eq!(usage.output_tokens, Some(5));
}

/// D.2 Ghost keys via Entry enum parse path — same guarantee end-to-end.
#[test]
fn spec_ghost_keys_via_entry_enum_and_parse_entry() {
    let line = r#"{"type":"assistant","uuid":"a1","parentUuid":"p","sessionId":"s1","message":{"model":"m","role":"assistant","stop_reason":"tool_use","stop_details":null,"container":null,"usage":{"input_tokens":3,"output_tokens":7},"content":[]}}"#;
    let entry = parse_entry(line)
        .expect("parse_entry must succeed with stop_details=null and container=null");
    match entry {
        Entry::Assistant(a) => {
            let msg = a.message;
            assert_eq!(msg.stop_reason, Some(StopReason::ToolUse));
        }
        other => panic!("Expected Assistant, got {other:?}"),
    }
}

/// D.3 Multiple null ghost keys simultaneously (exhaustive for real-data patterns).
/// Real JSONL may carry additional unmodelled null keys besides stop_details and container.
#[test]
fn spec_extra_null_fields_beyond_known_spec() {
    let json = r#"{
        "type": "assistant",
        "parentUuid": "p",
        "sessionId": "s1",
        "message": {
            "model": "m",
            "role": "assistant",
            "stop_reason": "end_turn",
            "stop_details": null,
            "container": null,
            "usage": {"input_tokens": 1, "output_tokens": 1},
            "content": []
        }
    }"#;
    let _: AssistantEntry =
        serde_json::from_str(json).expect("Multiple null ghost keys must be tolerated");
}

// ════════════════════════════════════════════════════════════════════════════
// Additional edge cases derived from the requirement spec
// ════════════════════════════════════════════════════════════════════════════

/// AssistantEntry.parentUuid is String (not Option) in v2.
/// This is a key v2 invariant: assistant entries always reply to something.
/// If parentUuid is absent from the JSON, the parse MUST fail (it's required).
#[test]
fn spec_assistant_parent_uuid_is_required_string() {
    let json = r#"{"type":"assistant","uuid":"a1","sessionId":"s1","message":{"model":"m","role":"assistant","content":[]}}"#;
    // Without parentUuid, serde should fail because it's a non-Option String
    let result = parse_entry(json);
    assert!(
        result.is_err(),
        "AssistantEntry without parentUuid must fail (it's a required String)"
    );
}

/// UserContentKind::ToolResult — content array of only tool_result blocks.
/// This is the 83% case (survey: 12,262 of 14,491 user entries are tool results).
#[test]
fn spec_user_content_kind_tool_result_classification() {
    use cc_session_jsonl::types::user::UserMessage;
    let msg = UserMessage {
        role: Some("user".into()),
        content: Some(serde_json::json!([
            {"type": "tool_result", "tool_use_id": "toolu_01", "content": "output here"}
        ])),
    };
    assert_eq!(
        msg.content_kind(),
        Some(UserContentKind::ToolResult),
        "Array of only tool_result blocks must classify as ToolResult"
    );
}

/// UserContentKind classification: empty array → Mixed (not ToolResult or Text).
#[test]
fn spec_user_content_kind_empty_array_is_mixed() {
    use cc_session_jsonl::types::user::UserMessage;
    let msg = UserMessage {
        role: Some("user".into()),
        content: Some(serde_json::json!([])),
    };
    // Empty array has no blocks — all flags are false — falls to Mixed by spec logic
    assert_eq!(msg.content_kind(), Some(UserContentKind::Mixed));
}

/// Passthrough entry preserves all DAG continuity fields.
/// Requirement E §3: Passthrough variants must have uuid + sessionId + timestamp + isSidechain.
#[test]
fn spec_passthrough_dag_fields_preserved() {
    let json = r#"{
        "type": "future-dag-type-2030",
        "uuid": "pass-001",
        "sessionId": "sess-pass-001",
        "timestamp": "2026-06-01T12:00:00.000Z",
        "isSidechain": false,
        "parentUuid": null,
        "someExtraField": "extra"
    }"#;
    let entry = parse_entry(json).unwrap();
    match entry {
        Entry::Passthrough(p) => {
            assert_eq!(p.uuid, "pass-001");
            assert_eq!(p.session_id, "sess-pass-001");
            assert_eq!(p.timestamp.as_deref(), Some("2026-06-01T12:00:00.000Z"));
            assert_eq!(p.is_sidechain, Some(false));
            assert!(
                p.parent_uuid.is_none(),
                "parentUuid null must decode to None in Passthrough"
            );
            // entry_type field records the original type string
            assert_eq!(p.entry_type, "future-dag-type-2030");
        }
        other => panic!("Expected Passthrough, got {other:?}"),
    }
}

/// Passthrough: entry with empty-string uuid must NOT become Passthrough (falls to Ignored).
/// The requirement: both uuid AND sessionId must be present AND non-empty.
#[test]
fn spec_passthrough_empty_uuid_becomes_ignored() {
    let json = r#"{"type":"future-type","uuid":"","sessionId":"s1","data":"x"}"#;
    let entry = parse_entry(json).unwrap();
    assert!(
        matches!(entry, Entry::Ignored),
        "Empty uuid string must route to Ignored, not Passthrough"
    );
}

/// Passthrough: entry with empty sessionId must NOT become Passthrough.
#[test]
fn spec_passthrough_empty_session_id_becomes_ignored() {
    let json = r#"{"type":"future-type","uuid":"u1","sessionId":"","data":"x"}"#;
    let entry = parse_entry(json).unwrap();
    assert!(
        matches!(entry, Entry::Ignored),
        "Empty sessionId string must route to Ignored, not Passthrough"
    );
}

/// StructDrift error carries the original entry_type name.
/// The requirement: ParseError::StructDrift { entry_type, .. } where entry_type
/// identifies which entry type had the shape mismatch.
#[test]
fn spec_struct_drift_error_carries_entry_type() {
    // Test with "system" type
    let json = r#"{"type":"system","uuid":"s1","sessionId":"s1","subtype":"turn_duration","durationMs":"not-a-number"}"#;
    let err = parse_entry(json).unwrap_err();
    match err {
        cc_session_jsonl::ParseError::StructDrift {
            entry_type,
            message,
        } => {
            assert_eq!(entry_type, "system", "StructDrift must name 'system'");
            assert!(!message.is_empty(), "StructDrift message must not be empty");
        }
        other => panic!("Expected StructDrift, got: {other}"),
    }
}

/// StructDrift is distinct from malformed JSON (ParseError::Json).
/// The requirement mandates these two failure modes stay separate.
#[test]
fn spec_struct_drift_is_distinct_from_json_error() {
    // Malformed JSON → Json error, not StructDrift
    let malformed = r#"{"type":"assistant", "uuid": BROKEN"#;
    let json_err = parse_entry(malformed).unwrap_err();
    assert!(
        matches!(json_err, cc_session_jsonl::ParseError::Json(_)),
        "Malformed JSON must be ParseError::Json, not StructDrift"
    );

    // Valid JSON, known type, bad field type → StructDrift (not Json)
    let drift = r#"{"type":"assistant","parentUuid":"p","sessionId":"s1","message":{"model":"m","role":"assistant","usage":{"input_tokens":"oops"}}}"#;
    let drift_err = parse_entry(drift).unwrap_err();
    assert!(
        matches!(drift_err, cc_session_jsonl::ParseError::StructDrift { .. }),
        "Valid JSON with bad typed field must be StructDrift, not Json"
    );
}

/// PermissionMode entry type (the standalone switch, not inline field).
/// Requirement: "bypassPermissions" parses to a valid PermissionModeEntry.
#[test]
fn spec_permission_mode_entry_known_values() {
    let json =
        r#"{"type":"permission-mode","sessionId":"s1","permissionMode":"bypassPermissions"}"#;
    let entry = parse_entry(json).unwrap();
    match entry {
        Entry::PermissionMode(pm) => {
            assert_eq!(pm.permission_mode.as_deref(), Some("bypassPermissions"));
            assert_eq!(pm.session_id.as_deref(), Some("s1"));
        }
        other => panic!("Expected PermissionMode, got {other:?}"),
    }
}

/// Usage struct: all token fields can be present simultaneously.
/// Golden: input=10, output=20, cache_creation=5, cache_read=100.
#[test]
fn spec_usage_all_token_fields_golden() {
    let json = r#"{
        "input_tokens": 10,
        "output_tokens": 20,
        "cache_creation_input_tokens": 5,
        "cache_read_input_tokens": 100,
        "cache_creation": {
            "ephemeral_5m_input_tokens": 3,
            "ephemeral_1h_input_tokens": 2
        }
    }"#;
    let usage: Usage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.input_tokens, Some(10));
    assert_eq!(usage.output_tokens, Some(20));
    assert_eq!(usage.cache_creation_input_tokens, Some(5));
    assert_eq!(usage.cache_read_input_tokens, Some(100));
    let cc = usage.cache_creation.as_ref().unwrap();
    // ephemeral_5m + ephemeral_1h must match golden values
    assert_eq!(cc.ephemeral_5m_input_tokens, Some(3));
    assert_eq!(cc.ephemeral_1h_input_tokens, Some(2));
}

/// agentId promotion: when top-level agentId is absent but toolUseResult.agentId
/// is present, the UserEntry must have agent_id set to that value.
/// This is the v2 "agentId promotion" design requirement.
#[test]
fn spec_user_agent_id_promoted_from_tool_use_result() {
    let json = r#"{
        "type": "user",
        "uuid": "u-promo-001",
        "sessionId": "s1",
        "toolUseResult": {
            "status": "completed",
            "agentId": "ac5b46b9-promoted",
            "prompt": "Do the task"
        },
        "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "tu1", "content": "done"}]}
    }"#;
    let entry = parse_entry(json).unwrap();
    match entry {
        Entry::User(u) => {
            assert_eq!(
                u.agent_id.as_deref(),
                Some("ac5b46b9-promoted"),
                "agentId must be promoted from toolUseResult.agentId when top-level is absent"
            );
        }
        other => panic!("Expected User, got {other:?}"),
    }
}

/// agentId promotion: when top-level agentId IS present, it must NOT be overwritten.
#[test]
fn spec_user_agent_id_top_level_not_overwritten_by_promotion() {
    let json = r#"{
        "type": "user",
        "uuid": "u-nopromo-001",
        "sessionId": "s1",
        "agentId": "top-level-agent",
        "toolUseResult": {"status": "done", "agentId": "nested-agent"},
        "message": {"role": "user", "content": "hi"}
    }"#;
    let entry = parse_entry(json).unwrap();
    match entry {
        Entry::User(u) => {
            assert_eq!(
                u.agent_id.as_deref(),
                Some("top-level-agent"),
                "Top-level agentId must NOT be overwritten by toolUseResult.agentId"
            );
        }
        other => panic!("Expected User, got {other:?}"),
    }
}

/// DagNode trait: compact_boundary parent_uuid() returns logicalParentUuid fallback.
/// This is a critical DAG invariant from the v2 spec.
#[test]
fn spec_dag_node_compact_boundary_fallback_parent() {
    use cc_session_jsonl::types::common::DagNode;
    let json = r#"{
        "type": "system",
        "uuid": "cb-dag-001",
        "sessionId": "s1",
        "subtype": "compact_boundary",
        "logicalParentUuid": "logical-parent-abc",
        "content": "<collapsed>...</collapsed>",
        "level": "info"
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    // parentUuid is None (field absent)
    assert!(entry.parent_uuid.is_none());
    // But DagNode::parent_uuid() falls back to logicalParentUuid
    assert_eq!(
        entry.parent_uuid(),
        Some("logical-parent-abc"),
        "DagNode::parent_uuid() must return logicalParentUuid when parentUuid is absent"
    );
}

// ════════════════════════════════════════════════════════════════════════════
// iter-2 additional coverage — Value-typed fields soft-landing invariants
// ════════════════════════════════════════════════════════════════════════════
//
// iter-2 promoted two fields to `Option<serde_json::Value>` / `Vec<Value>`:
//   • ApiError.cause  — was Option<String>, now Option<Value>
//   • UserEntry.image_paste_ids — was Vec<String>, now Vec<Value>
//
// Spec philosophy §3: "value drift ≠ struct drift". Once a field is typed
// as raw Value / Vec<Value>, *any* JSON token (null, integer, array, object,
// bool) must parse without StructDrift. The tests below pin that invariant
// across all primitive shapes the spec implies are valid.

// ── iter-2 / ApiError.cause ──────────────────────────────────────────────

/// iter-2 B.3a: ApiError.cause absent (field not in JSON) → None, no error.
/// Spec: 18% fill rate means 82% of api_error entries lack cause entirely.
/// Golden: cause field absent → entry.body has cause = None (not a StructDrift).
#[test]
fn spec_iter2_api_error_cause_absent_is_none() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-ae-nocause",
        "sessionId": "s1",
        "subtype": "api_error",
        "error": "rate_limit",
        "level": "warn",
        "maxRetries": 3,
        "retryAttempt": 1,
        "retryInMs": 1000.0
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.subtype(), Some("api_error"));
    match &entry.body {
        SystemBody::ApiError { cause, .. } => {
            assert!(
                cause.is_none(),
                "absent cause field must yield None, not StructDrift"
            );
        }
        other => panic!("Expected ApiError, got {other:?}"),
    }
}

/// iter-2 B.3b: ApiError.cause = null (explicit JSON null) → None, no error.
/// Spec: `Option<serde_json::Value>` must accept explicit JSON null as None.
#[test]
fn spec_iter2_api_error_cause_explicit_null_is_none() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-ae-nullcause",
        "sessionId": "s1",
        "subtype": "api_error",
        "error": "network_error",
        "level": "error",
        "cause": null
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    match &entry.body {
        SystemBody::ApiError { cause, .. } => {
            // explicit null → serde deserializes Option<Value> as None
            assert!(cause.is_none(), "explicit JSON null cause must yield None");
        }
        other => panic!("Expected ApiError, got {other:?}"),
    }
}

/// iter-2 B.3c: ApiError.cause = integer — unusual but spec allows any Value.
/// Spec: `Option<serde_json::Value>` accepts any JSON token without StructDrift.
/// Golden: cause is Some(Value::Number(42)) — from spec, not from running code.
#[test]
fn spec_iter2_api_error_cause_integer_soft_lands() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-ae-intcause",
        "sessionId": "s1",
        "subtype": "api_error",
        "error": "mystery",
        "level": "error",
        "cause": 42
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    match &entry.body {
        SystemBody::ApiError { cause, .. } => {
            let v = cause.as_ref().expect("integer cause must be Some");
            assert!(v.is_number(), "integer cause must be a JSON number Value");
            assert_eq!(v.as_i64(), Some(42));
        }
        other => panic!("Expected ApiError, got {other:?}"),
    }
}

/// iter-2 B.3d: ApiError.cause = array — soft-lands without StructDrift.
/// Golden: cause is Some(Value::Array) with the expected elements.
#[test]
fn spec_iter2_api_error_cause_array_soft_lands() {
    let json = r#"{
        "type": "system",
        "uuid": "sys-ae-arrcause",
        "sessionId": "s1",
        "subtype": "api_error",
        "error": "multi_cause",
        "level": "error",
        "cause": ["ECONNRESET", "ETIMEDOUT"]
    }"#;
    let entry: SystemEntry = serde_json::from_str(json).unwrap();
    match &entry.body {
        SystemBody::ApiError { cause, .. } => {
            let v = cause.as_ref().expect("array cause must be Some");
            assert!(v.is_array(), "array cause must be a JSON array Value");
            let arr = v.as_array().unwrap();
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0].as_str(), Some("ECONNRESET"));
        }
        other => panic!("Expected ApiError, got {other:?}"),
    }
}

// ── iter-2 / UserEntry.image_paste_ids ───────────────────────────────────

/// iter-2 A.1d: imagePasteIds with integer elements — spec fix says
/// Vec<serde_json::Value> accepts integers without StructDrift.
/// Golden: array of 3 integers, each accessible as i64.
#[test]
fn spec_iter2_user_image_paste_ids_integer_elements_parse() {
    let json = r#"{
        "type": "user",
        "uuid": "u-int-ids",
        "sessionId": "s1",
        "imagePasteIds": [1, 3, 7]
    }"#;
    let entry = parse_entry(json).expect("integer imagePasteIds must not StructDrift");
    match entry {
        Entry::User(u) => {
            let ids = u
                .image_paste_ids
                .as_ref()
                .expect("imagePasteIds with integers must be Some");
            assert_eq!(ids.len(), 3, "must have 3 elements");
            assert_eq!(ids[0].as_i64(), Some(1), "first element must be integer 1");
            assert_eq!(ids[1].as_i64(), Some(3), "second element must be integer 3");
            assert_eq!(ids[2].as_i64(), Some(7), "third element must be integer 7");
        }
        other => panic!("Expected User, got {other:?}"),
    }
}

/// iter-2 A.1e: imagePasteIds with mixed elements (string + integer) — both parse.
/// Spec: Vec<Value> accepts heterogeneous arrays.
/// Golden: first element is string "abc", second is integer 99.
#[test]
fn spec_iter2_user_image_paste_ids_mixed_elements_parse() {
    let json = r#"{
        "type": "user",
        "uuid": "u-mixed-ids",
        "sessionId": "s1",
        "imagePasteIds": ["abc", 99]
    }"#;
    let entry = parse_entry(json).expect("mixed imagePasteIds must not StructDrift");
    match entry {
        Entry::User(u) => {
            let ids = u
                .image_paste_ids
                .as_ref()
                .expect("mixed imagePasteIds must be Some");
            assert_eq!(ids.len(), 2);
            assert_eq!(ids[0].as_str(), Some("abc"));
            assert_eq!(ids[1].as_i64(), Some(99));
        }
        other => panic!("Expected User, got {other:?}"),
    }
}

/// iter-2 → iter-3 follow-through: `AttachmentBody::QueuedCommand.image_paste_ids`
/// is now `Option<Vec<serde_json::Value>>`, matching the same widening done on
/// `UserEntry.image_paste_ids` (survey §5 queued_command row). Production sometimes
/// emits integer ids inside a queued_command attachment; without this fix the
/// whole `QueuedCommand` variant would soft-land in `AttachmentBody::Unknown`.
#[test]
fn spec_iter2_queued_command_image_paste_ids_integer_elements_parse() {
    use cc_session_jsonl::types::attachment::{AttachmentBody, AttachmentEntry};
    let json = r#"{
        "type": "attachment",
        "uuid": "att-qc-int-ids",
        "sessionId": "s1",
        "attachment": {
            "type": "queued_command",
            "commandMode": "prompt",
            "prompt": "continue",
            "imagePasteIds": [1, 2, 3]
        }
    }"#;
    let entry: AttachmentEntry = serde_json::from_str(json)
        .expect("attachment with queued_command + integer ids must parse");
    // After the fix: body must be QueuedCommand (not Unknown), with 3 elements.
    assert_eq!(
        entry.attachment_subtype(),
        Some("queued_command"),
        "queued_command with integer imagePasteIds must parse as QueuedCommand, not Unknown"
    );
    match entry.attachment.as_ref().unwrap() {
        AttachmentBody::QueuedCommand {
            image_paste_ids, ..
        } => {
            let ids = image_paste_ids.as_ref().expect("ids must be Some");
            assert_eq!(ids.len(), 3);
            assert_eq!(ids[0].as_i64(), Some(1));
            assert_eq!(ids[2].as_i64(), Some(3));
        }
        other => panic!("expected QueuedCommand variant, got {other:?}"),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// iter-3 spec-derived tests — run a66abe-1
//
// Six requirement clauses that the earlier spec_v2 suite left untested
// (confirmed by gap analysis against the code + spec). Each expected value
// comes from the requirement specification, NOT from running the code.
// ════════════════════════════════════════════════════════════════════════════

// ── Item 1 — PassthroughEntry impl DagNode: &dyn DagNode dispatch ────────

/// Item 1: PassthroughEntry must be usable as &dyn DagNode.
/// Spec §1: "Passthrough 必须能被 &dyn DagNode 当 DAG 节点访问".
/// The earlier `spec_passthrough_dag_fields_preserved` test only checks the
/// parsed struct directly. This test calls every DagNode method through a
/// trait object so we catch any impl-missing or vtable gaps.
///
/// Golden: uuid/session_id/timestamp/parent_uuid/is_sidechain return the
/// exact values supplied in the JSON — derived from the spec, not from the
/// implementation.
#[test]
fn spec_item1_passthrough_accessible_as_dyn_dag_node() {
    use cc_session_jsonl::types::common::DagNode;
    use cc_session_jsonl::types::PassthroughEntry;

    let p = PassthroughEntry {
        uuid: "pt-dag-001".into(),
        parent_uuid: Some("pt-parent-001".into()),
        session_id: "pt-sess-001".into(),
        timestamp: Some("2026-06-09T10:00:00Z".into()),
        entry_type: "future-type-abc".into(),
        is_sidechain: Some(false),
        agent_id: None,
    };

    // Coerce to trait object — this is what the spec requires
    let node: &dyn DagNode = &p;
    assert_eq!(
        node.uuid(),
        Some("pt-dag-001"),
        "DagNode::uuid() must return the passthrough uuid"
    );
    assert_eq!(
        node.session_id(),
        Some("pt-sess-001"),
        "DagNode::session_id() must return the passthrough sessionId"
    );
    assert_eq!(
        node.timestamp(),
        Some("2026-06-09T10:00:00Z"),
        "DagNode::timestamp() must return the passthrough timestamp"
    );
    assert_eq!(
        node.parent_uuid(),
        Some("pt-parent-001"),
        "DagNode::parent_uuid() must return the passthrough parentUuid"
    );
    assert_eq!(
        node.is_sidechain(),
        Some(false),
        "DagNode::is_sidechain() must return the passthrough isSidechain"
    );
}

/// Item 1: PassthroughEntry with null parentUuid — DagNode::parent_uuid() returns None.
/// Spec: "is_sidechain 都能返回" - even when parentUuid is absent (root node).
#[test]
fn spec_item1_passthrough_null_parent_uuid_via_dag_node() {
    use cc_session_jsonl::types::common::DagNode;
    use cc_session_jsonl::types::PassthroughEntry;

    let root = PassthroughEntry {
        uuid: "pt-root-001".into(),
        parent_uuid: None,
        session_id: "pt-sess-root".into(),
        timestamp: None,
        entry_type: "future-root-type".into(),
        is_sidechain: None,
        agent_id: None,
    };

    let node: &dyn DagNode = &root;
    assert_eq!(node.uuid(), Some("pt-root-001"));
    assert!(
        node.parent_uuid().is_none(),
        "DagNode::parent_uuid() on root passthrough must return None"
    );
    assert!(
        node.timestamp().is_none(),
        "DagNode::timestamp() when absent must return None via trait object"
    );
    assert!(
        node.is_sidechain().is_none(),
        "DagNode::is_sidechain() when absent must return None via trait object"
    );
}

// ── Item 2 — AssistantEntry.message is REQUIRED (non-Option) ─────────────

/// Item 2: AssistantEntry without the `message` field must yield
/// ParseError::StructDrift — NOT ParseError::Json.
///
/// Spec: "缺 message 的 assistant entry → ParseError::StructDrift"
///
/// This is the key test the implementer's suite was missing: existing tests
/// only prove "message present → OK" (a mirror test). This test proves
/// "message absent → StructDrift" (an independent oracle test).
///
/// Golden: error variant is StructDrift with entry_type == "assistant".
#[test]
fn spec_item2_assistant_missing_message_is_struct_drift() {
    // Well-formed JSON, known entry type "assistant", but `message` field
    // is entirely absent. Since message: ApiMessage (not Option), serde
    // must fail — and the v2 strict design must surface it as StructDrift.
    let json = r#"{"type":"assistant","uuid":"a1","parentUuid":"p1","sessionId":"s1"}"#;
    let err = parse_entry(json).unwrap_err();
    match err {
        cc_session_jsonl::ParseError::StructDrift { entry_type, .. } => {
            assert_eq!(
                entry_type, "assistant",
                "StructDrift must name the correct entry type 'assistant'"
            );
        }
        cc_session_jsonl::ParseError::Json(_) => {
            panic!(
                "Missing message on AssistantEntry must be StructDrift, not ParseError::Json. \
                 The strict v2 design requires known-type + bad-shape → StructDrift so that \
                 schema regressions are counted and signalled separately from malformed JSON."
            );
        }
        other => panic!("Expected StructDrift, got: {other}"),
    }
}

/// Item 2: AssistantEntry with message = null (explicit null) must also
/// yield StructDrift (null is not a valid ApiMessage value).
/// Golden: StructDrift with entry_type == "assistant".
#[test]
fn spec_item2_assistant_message_explicit_null_is_struct_drift() {
    let json =
        r#"{"type":"assistant","uuid":"a2","parentUuid":"p1","sessionId":"s1","message":null}"#;
    let err = parse_entry(json).unwrap_err();
    match err {
        cc_session_jsonl::ParseError::StructDrift { entry_type, .. } => {
            assert_eq!(entry_type, "assistant");
        }
        other => panic!(
            "message:null on AssistantEntry must be StructDrift, got: {other}"
        ),
    }
}

// ── Item 3 — 7 typed enums: known variants + Unknown soft-landing ─────────
//
// Spec: "每个 enum 必须支持 (a) 已知 variant 解析；(b) #[serde(other)] Unknown
//       软着陆未知值；(c) 跟原始 Option<String> 字段相同的解析路径不破坏"
//
// The five enums already covered in spec_v2 (PermissionMode, StopReason,
// PromptSource, OriginKind, CacheMissReasonKind) are not repeated here.
// This section adds the six enums left uncovered in spec_v2 that ARE part
// of this iteration's spec, plus the PermissionMode::Auto variant that was
// added to cover the real-data "auto" permission mode.

use cc_session_jsonl::types::common::{
    AssistantError, Entrypoint, InferenceGeo, ServiceTier, Speed, UserType,
};

// ── C-new.1 UserType ──

/// UserType(a): known variant "external" → UserType::External.
/// Golden: spec §3 states userType is present on 100% of entries, always
/// "external" on this machine. Wire value "external" → External variant.
#[test]
fn spec_item3_user_type_external_parses() {
    let v: UserType = serde_json::from_str(r#""external""#).unwrap();
    assert_eq!(v, UserType::External, "\"external\" must map to UserType::External");
}

/// UserType(b): unknown future value → UserType::Unknown (not a parse error).
/// Spec: new values from future Claude Code releases (e.g. "internal") must
/// degrade gracefully, not error the whole entry.
/// Golden: any string not in the enum → Unknown variant.
#[test]
fn spec_item3_user_type_future_value_soft_lands() {
    let v: UserType = serde_json::from_str(r#""internal_anthropic_user_2030""#).unwrap();
    assert_eq!(
        v,
        UserType::Unknown,
        "Unknown userType value must soft-land in UserType::Unknown"
    );
}

/// UserType(c): the full assistant entry with a future userType value must
/// still parse (field-level drift, not entry-level drift).
/// Spec invariant: "value drift ≠ struct drift". A novel userType string
/// lands in Unknown and the entry continues to parse.
#[test]
fn spec_item3_user_type_novel_value_in_entry_does_not_drift() {
    // "internal" is not a modelled variant — should degrade to Unknown, NOT StructDrift
    let json = r#"{
        "type":"assistant",
        "uuid":"a1",
        "parentUuid":"p",
        "sessionId":"s1",
        "userType":"internal",
        "message":{"model":"m","role":"assistant","content":[]}
    }"#;
    let entry = parse_entry(json).expect("Novel userType must not cause StructDrift");
    match entry {
        Entry::Assistant(a) => {
            assert_eq!(
                a.user_type,
                Some(UserType::Unknown),
                "Novel userType must land in UserType::Unknown, not fail the entry"
            );
        }
        other => panic!("Expected Assistant, got {other:?}"),
    }
}

// ── C-new.2 Entrypoint ──

/// Entrypoint(a): known variants "cli" and "sdk-cli" parse correctly.
/// Wire values from common.rs: kebab-case ("cli", "sdk-cli").
#[test]
fn spec_item3_entrypoint_known_variants_parse() {
    let cli: Entrypoint = serde_json::from_str(r#""cli""#).unwrap();
    assert_eq!(cli, Entrypoint::Cli, "\"cli\" must map to Entrypoint::Cli");

    let sdk: Entrypoint = serde_json::from_str(r#""sdk-cli""#).unwrap();
    assert_eq!(sdk, Entrypoint::SdkCli, "\"sdk-cli\" must map to Entrypoint::SdkCli");
}

/// Entrypoint(b): future value (e.g. "vscode-extension-2030") → Unknown.
/// Spec: new IDE integrations or IDE-specific entrypoints must not fail parse.
#[test]
fn spec_item3_entrypoint_future_value_soft_lands() {
    let v: Entrypoint = serde_json::from_str(r#""vscode-extension-2030""#).unwrap();
    assert_eq!(
        v,
        Entrypoint::Unknown,
        "Novel entrypoint value must soft-land in Entrypoint::Unknown"
    );
}

/// Entrypoint(c): novel entrypoint in a full entry must not cause StructDrift.
#[test]
fn spec_item3_entrypoint_novel_value_in_entry_does_not_drift() {
    let json = r#"{
        "type":"user",
        "uuid":"u1",
        "sessionId":"s1",
        "entrypoint":"web-ui-2030",
        "message":{"role":"user","content":"hi"}
    }"#;
    let entry = parse_entry(json).expect("Novel entrypoint must not cause StructDrift");
    match entry {
        Entry::User(u) => {
            assert_eq!(
                u.entrypoint,
                Some(Entrypoint::Unknown),
                "Novel entrypoint value must land in Entrypoint::Unknown"
            );
        }
        other => panic!("Expected User, got {other:?}"),
    }
}

// ── C-new.3 ServiceTier ──

/// ServiceTier(a): known variants parse correctly.
/// Wire values: snake_case. Real-data: "standard". Spec also models
/// "batch" and "priority" per the Anthropic Messages API spec.
#[test]
fn spec_item3_service_tier_known_variants_parse() {
    let standard: ServiceTier = serde_json::from_str(r#""standard""#).unwrap();
    assert_eq!(standard, ServiceTier::Standard);

    let batch: ServiceTier = serde_json::from_str(r#""batch""#).unwrap();
    assert_eq!(batch, ServiceTier::Batch);

    let priority: ServiceTier = serde_json::from_str(r#""priority""#).unwrap();
    assert_eq!(priority, ServiceTier::Priority);
}

/// ServiceTier(b): unknown future tier → ServiceTier::Unknown.
/// Spec: Anthropic may add new service tiers; they must not break parse.
#[test]
fn spec_item3_service_tier_future_value_soft_lands() {
    let v: ServiceTier = serde_json::from_str(r#""enterprise_2030""#).unwrap();
    assert_eq!(
        v,
        ServiceTier::Unknown,
        "Novel service_tier value must soft-land in ServiceTier::Unknown"
    );
}

/// ServiceTier(c): ServiceTier::as_str() returns correct wire-format values.
/// Golden: the wire strings come from the spec (common.rs), not from running
/// code. These are the strings that appear in real JSONL and the API docs.
#[test]
fn spec_item3_service_tier_as_str_golden_values() {
    assert_eq!(ServiceTier::Standard.as_str(), "standard");
    assert_eq!(ServiceTier::Batch.as_str(), "batch");
    assert_eq!(ServiceTier::Priority.as_str(), "priority");
    assert_eq!(ServiceTier::Unknown.as_str(), "unknown");
}

// ── C-new.4 Speed ──

/// Speed(a): known variant "standard" → Speed::Standard.
/// Real-data: ~74% of newer assistant turns carry this field.
#[test]
fn spec_item3_speed_standard_parses() {
    let v: Speed = serde_json::from_str(r#""standard""#).unwrap();
    assert_eq!(v, Speed::Standard, "\"standard\" must map to Speed::Standard");
}

/// Speed(b): unknown future speed value → Speed::Unknown.
/// Spec: speed tier surface is small but not frozen.
#[test]
fn spec_item3_speed_future_value_soft_lands() {
    let v: Speed = serde_json::from_str(r#""turbo_2030""#).unwrap();
    assert_eq!(
        v,
        Speed::Unknown,
        "Novel speed value must soft-land in Speed::Unknown"
    );
}

/// Speed(c): Speed::as_str() golden values.
#[test]
fn spec_item3_speed_as_str_golden_values() {
    assert_eq!(Speed::Standard.as_str(), "standard");
    assert_eq!(Speed::Unknown.as_str(), "unknown");
}

// ── C-new.5 InferenceGeo ──

/// InferenceGeo(a): "not_available" and "" (empty string) are distinct modelled
/// variants. The empty-string variant is critical: ~8,886 occurrences in real
/// data (survey §3). Wire value "" must map to InferenceGeo::Empty (not Unknown).
/// Golden: both values come from the spec, not from running code.
#[test]
fn spec_item3_inference_geo_known_variants_parse() {
    let na: InferenceGeo = serde_json::from_str(r#""not_available""#).unwrap();
    assert_eq!(na, InferenceGeo::NotAvailable, "\"not_available\" must map to NotAvailable");

    // The empty-string case is the critical one — thousands of real entries.
    // Without the dedicated #[serde(rename = "")] variant, "" would fall
    // through to Unknown, losing the semantic distinction.
    let empty: InferenceGeo = serde_json::from_str(r#""""#).unwrap();
    assert_eq!(
        empty,
        InferenceGeo::Empty,
        "Empty string must map to InferenceGeo::Empty (not Unknown)"
    );
}

/// InferenceGeo(b): unknown geographic tag → InferenceGeo::Unknown.
/// Spec: geo tags are not frozen; new regions must not break parse.
#[test]
fn spec_item3_inference_geo_future_value_soft_lands() {
    let v: InferenceGeo = serde_json::from_str(r#""eu-west-1-2030""#).unwrap();
    assert_eq!(
        v,
        InferenceGeo::Unknown,
        "Novel inference_geo value must soft-land in InferenceGeo::Unknown"
    );
}

/// InferenceGeo(c): as_str() golden values.
/// Golden: the wire strings come from the spec (common.rs).
#[test]
fn spec_item3_inference_geo_as_str_golden_values() {
    assert_eq!(InferenceGeo::NotAvailable.as_str(), "not_available");
    assert_eq!(InferenceGeo::Empty.as_str(), "");
    assert_eq!(InferenceGeo::Unknown.as_str(), "unknown");
}

// ── C-new.6 AssistantError ──

/// AssistantError(a): all five known string values parse to the correct variant.
/// Wire values from real data (survey §3): rate_limit, authentication_failed,
/// server_error, oauth_org_not_allowed, and the literal string "unknown"
/// (the API's own catch-all — distinct from our drift bucket).
/// Golden: all values come from the spec, not from code introspection.
#[test]
fn spec_item3_assistant_error_known_variants_parse() {
    let rl: AssistantError = serde_json::from_str(r#""rate_limit""#).unwrap();
    assert_eq!(rl, AssistantError::RateLimit);

    let af: AssistantError = serde_json::from_str(r#""authentication_failed""#).unwrap();
    assert_eq!(af, AssistantError::AuthenticationFailed);

    let se: AssistantError = serde_json::from_str(r#""server_error""#).unwrap();
    assert_eq!(se, AssistantError::ServerError);

    let oauth: AssistantError = serde_json::from_str(r#""oauth_org_not_allowed""#).unwrap();
    assert_eq!(oauth, AssistantError::OauthOrgNotAllowed);

    // The literal string "unknown" is the API's documented catch-all category,
    // observed 9 times in the survey. It maps to AssistantError::Unknown —
    // distinct from AssistantError::Other which is our drift sentinel.
    let api_unknown: AssistantError = serde_json::from_str(r#""unknown""#).unwrap();
    assert_eq!(
        api_unknown,
        AssistantError::Unknown,
        "\"unknown\" is the API's own bucket — must map to AssistantError::Unknown"
    );
    assert_eq!(api_unknown.as_str(), "unknown");
}

/// AssistantError(b): a future error category → AssistantError::Other.
/// Spec: "Drift soft-landing for error categories the parser hasn't seen yet.
/// Distinct from Unknown (which is the literal value the API emits)."
/// Golden: any string not explicitly modelled → Other (not Unknown, not error).
#[test]
fn spec_item3_assistant_error_future_value_soft_lands_in_other() {
    let v: AssistantError = serde_json::from_str(r#""future_error_category_2030""#).unwrap();
    assert_eq!(
        v,
        AssistantError::Other,
        "Novel AssistantError value must land in Other (not Unknown or error)"
    );
    // as_str() for the drift variant returns "other" — distinct from "unknown"
    assert_eq!(
        v.as_str(),
        "other",
        "AssistantError::Other.as_str() must return \"other\", not \"unknown\""
    );
}

/// AssistantError: the literal "unknown" vs drift "Other" are semantically
/// distinct — the spec models them separately. Verify they round-trip
/// differently so callers can distinguish "API said unknown" from "we haven't
/// seen this value yet".
#[test]
fn spec_item3_assistant_error_unknown_vs_other_are_distinct() {
    let api_catch_all: AssistantError = serde_json::from_str(r#""unknown""#).unwrap();
    let drift: AssistantError = serde_json::from_str(r#""brand_new_category""#).unwrap();
    assert_ne!(
        api_catch_all, drift,
        "The API's literal 'unknown' and the drift bucket 'Other' must be distinct variants"
    );
    assert_eq!(api_catch_all.as_str(), "unknown");
    assert_eq!(drift.as_str(), "other");
}

// ── C-new.7 PermissionMode::Auto ─────────────────────────────────────────

/// PermissionMode::Auto: real-data value observed alongside bypassPermissions
/// and default (not in the original survey field list, added from real-data).
/// Spec: "Observed in real data ... emitted by the CLI when the user is running
/// with --auto style flow."
/// Golden: wire value "auto" → PermissionMode::Auto (not Unknown).
#[test]
fn spec_item3_permission_mode_auto_parses() {
    use cc_session_jsonl::types::common::PermissionMode;
    let auto: PermissionMode = serde_json::from_str(r#""auto""#).unwrap();
    assert_eq!(
        auto,
        PermissionMode::Auto,
        "\"auto\" must map to PermissionMode::Auto (it's a real-data-observed variant)"
    );
    // Confirm it's NOT Unknown — if someone forgot to add Auto, it would
    // soft-land in Unknown and this test would catch it.
    assert_ne!(auto, PermissionMode::Unknown, "PermissionMode::Auto must be a distinct variant, not Unknown");
}

// ── Item 4 — QueuedCommand.image_paste_ids: mixed (string + integer) ─────

/// Item 4 (supplement): QueuedCommand.image_paste_ids with mixed string + integer
/// elements must parse — not soft-land in Unknown. The spec-v2 iter-3 test
/// `spec_iter2_queued_command_image_paste_ids_integer_elements_parse` covers
/// pure-integer arrays. This test covers the mixed case and the same user
/// `imagePasteIds` field for cross-type consistency.
///
/// Spec invariant: "同样 integer + mixed 形态在两个 entry 类型上都解析成功".
/// Golden: first element is string "abc", second is integer 99 — from spec.
#[test]
fn spec_item4_queued_command_image_paste_ids_mixed_parses() {
    use cc_session_jsonl::types::attachment::{AttachmentBody, AttachmentEntry};
    let json = r#"{
        "type": "attachment",
        "uuid": "att-qc-mixed",
        "sessionId": "s1",
        "attachment": {
            "type": "queued_command",
            "commandMode": "prompt",
            "prompt": "go",
            "imagePasteIds": ["abc", 99, "xyz"]
        }
    }"#;
    let entry: AttachmentEntry = serde_json::from_str(json)
        .expect("queued_command with mixed imagePasteIds must parse without StructDrift");
    assert_eq!(
        entry.attachment_subtype(),
        Some("queued_command"),
        "mixed imagePasteIds must not cause QueuedCommand to soft-land in Unknown"
    );
    match entry.attachment.as_ref().unwrap() {
        AttachmentBody::QueuedCommand { image_paste_ids, .. } => {
            let ids = image_paste_ids.as_ref().expect("ids must be Some");
            assert_eq!(ids.len(), 3, "must have 3 elements");
            // Golden: string, int, string
            assert_eq!(ids[0].as_str(), Some("abc"), "first element must be string \"abc\"");
            assert_eq!(ids[1].as_i64(), Some(99), "second element must be integer 99");
            assert_eq!(ids[2].as_str(), Some("xyz"), "third element must be string \"xyz\"");
        }
        other => panic!("expected QueuedCommand variant, got {other:?}"),
    }
}

// ── Item 5 — 6 zero-sample fields silently ignored ────────────────────────
//
// Spec: "真实数据出现这些字段时（哪怕零概率），entry 仍能解析（serde 默认 ignore unknown keys）"
//
// The 6 fields removed in v2 (zero hits in survey §3):
//   apiError (Option<String>), isVirtual (Option<bool>), advisorModel (Option<String>),
//   teamName (Option<String>), agentName (Option<String>), agentColor (Option<String>)
//
// They must be silently dropped, not cause StructDrift.
// These are NOT regression snapshots — the spec explicitly says serde drops
// unknown keys, so the expectation comes from the design requirement.

/// Item 5: `apiError` (old Option<String> field) present in JSON → silently
/// dropped by serde, entry parses successfully.
/// Spec: "Serde silently drops unknown keys so reappearance in future data won't
/// fail the parse".
#[test]
fn spec_item5_removed_api_error_field_silently_ignored() {
    let json = r#"{
        "type": "assistant",
        "uuid": "a-ze-001",
        "parentUuid": "p1",
        "sessionId": "s1",
        "apiError": "rate_limit",
        "message": {"model": "m", "role": "assistant", "content": []}
    }"#;
    let entry = parse_entry(json).expect(
        "Removed zero-sample field 'apiError' must be silently ignored, not cause StructDrift"
    );
    assert!(matches!(entry, Entry::Assistant(_)));
}

/// Item 5: `isVirtual` (old Option<bool> field) present → silently dropped.
#[test]
fn spec_item5_removed_is_virtual_field_silently_ignored() {
    let json = r#"{
        "type": "assistant",
        "uuid": "a-ze-002",
        "parentUuid": "p1",
        "sessionId": "s1",
        "isVirtual": true,
        "message": {"model": "m", "role": "assistant", "content": []}
    }"#;
    let entry = parse_entry(json).expect(
        "Removed zero-sample field 'isVirtual' must be silently ignored, not cause StructDrift"
    );
    assert!(matches!(entry, Entry::Assistant(_)));
}

/// Item 5: `advisorModel` (old Option<String> field) present → silently dropped.
#[test]
fn spec_item5_removed_advisor_model_field_silently_ignored() {
    let json = r#"{
        "type": "assistant",
        "uuid": "a-ze-003",
        "parentUuid": "p1",
        "sessionId": "s1",
        "advisorModel": "claude-opus-3-5",
        "message": {"model": "m", "role": "assistant", "content": []}
    }"#;
    let entry = parse_entry(json).expect(
        "Removed zero-sample field 'advisorModel' must be silently ignored, not cause StructDrift"
    );
    assert!(matches!(entry, Entry::Assistant(_)));
}

/// Item 5: `teamName` (teammates field, 0 hits in survey) present → silently dropped.
#[test]
fn spec_item5_teammates_team_name_silently_ignored() {
    let json = r#"{
        "type": "assistant",
        "uuid": "a-ze-004",
        "parentUuid": "p1",
        "sessionId": "s1",
        "teamName": "acme-corp",
        "message": {"model": "m", "role": "assistant", "content": []}
    }"#;
    let entry = parse_entry(json).expect(
        "Teammates field 'teamName' must be silently ignored even though not modelled"
    );
    assert!(matches!(entry, Entry::Assistant(_)));
}

/// Item 5: `agentName` (teammates field) present → silently dropped.
#[test]
fn spec_item5_teammates_agent_name_silently_ignored() {
    let json = r#"{
        "type": "assistant",
        "uuid": "a-ze-005",
        "parentUuid": "p1",
        "sessionId": "s1",
        "agentName": "Builder",
        "message": {"model": "m", "role": "assistant", "content": []}
    }"#;
    let entry = parse_entry(json).expect(
        "Teammates field 'agentName' must be silently ignored even though not modelled"
    );
    assert!(matches!(entry, Entry::Assistant(_)));
}

/// Item 5: `agentColor` (teammates field) present → silently dropped.
/// This test also covers the multi-field case (all three teammates fields
/// together) since real teammates sessions would emit all of them at once.
#[test]
fn spec_item5_teammates_agent_color_silently_ignored() {
    // Use r##"..."## so that the hash in the color value "#ff0000" doesn't
    // confuse the Rust raw-string delimiter parser.
    let json = r##"{
        "type": "assistant",
        "uuid": "a-ze-006",
        "parentUuid": "p1",
        "sessionId": "s1",
        "agentColor": "#ff0000",
        "message": {"model": "m", "role": "assistant", "content": []}
    }"##;
    let entry = parse_entry(json).expect(
        "Teammates field 'agentColor' must be silently ignored even though not modelled"
    );
    assert!(matches!(entry, Entry::Assistant(_)));
}

/// Item 5 (combined): All 6 zero-sample fields present simultaneously.
/// Spec: serde ignores unknown keys regardless of how many there are.
/// Golden: entry parses successfully; specific field values are absent from
/// the struct (they are simply dropped, not surfaced anywhere).
#[test]
fn spec_item5_all_six_zero_sample_fields_simultaneously_ignored() {
    let json = r##"{
        "type": "assistant",
        "uuid": "a-ze-all",
        "parentUuid": "p1",
        "sessionId": "s1",
        "apiError": "rate_limit",
        "isVirtual": false,
        "advisorModel": "claude-opus-3-5",
        "teamName": "acme-corp",
        "agentName": "Builder",
        "agentColor": "#00ff00",
        "message": {
            "model": "claude-opus-4-6",
            "role": "assistant",
            "stop_reason": "end_turn",
            "content": [{"type": "text", "text": "done"}]
        }
    }"##;
    let entry = parse_entry(json).expect(
        "All 6 zero-sample removed fields present simultaneously must not cause StructDrift"
    );
    match entry {
        Entry::Assistant(a) => {
            // The modelled fields parse correctly — the extra fields are dropped
            assert_eq!(a.parent_uuid, "p1");
            assert_eq!(a.uuid.as_deref(), Some("a-ze-all"));
        }
        other => panic!("Expected Assistant, got {other:?}"),
    }
}
