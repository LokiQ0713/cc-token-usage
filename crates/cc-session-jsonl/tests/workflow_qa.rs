//! QA tests for workflow support (cc-session-jsonl layer).
//!
//! Covers:
//! - WorkflowRunSnapshot / WorkflowPhase / WorkflowProgress / WorkflowJournalEntry parsing
//! - Loader-level workflow agent discovery (via the public `load_all_sessions`
//!   and `load_session` surface)
//! - Agent.workflow_run_id propagation
//! - AttachmentEntry.attachment field (top-level, not in `message`)
//! - SystemEntry new subtypes: turn_duration with pendingWorkflowCount, local_command, away_summary

use std::fs;
use tempfile::TempDir;

// ── helpers ──────────────────────────────────────────────────────────────────

fn make_claude_home() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("projects")).unwrap();
    tmp
}

fn make_session_uuid() -> &'static str {
    "cc111111-2222-3333-4444-555566667777"
}

// ─── Layer 1: WorkflowRunSnapshot deserialization ────────────────────────────

#[test]
fn snapshot_all_scalar_fields_round_trip() {
    use cc_session_jsonl::types::WorkflowRunSnapshot;
    let json = r#"{
        "runId":       "wf_deadbeef-001",
        "taskId":      "task-alpha",
        "workflowName":"qa-workflow",
        "timestamp":   "2026-06-01T00:00:00.000Z",
        "status":      "completed",
        "script":      "export const meta = {}",
        "scriptPath":  "/home/user/wf.js",
        "defaultModel":"claude-opus-4-8[1m]",
        "startTime":   1748745600000,
        "durationMs":  99999,
        "agentCount":  7,
        "totalTokens": 1234567,
        "totalToolCalls": 42,
        "summary":     "QA test run"
    }"#;
    let snap: WorkflowRunSnapshot = serde_json::from_str(json).unwrap();
    assert_eq!(snap.run_id.as_deref(), Some("wf_deadbeef-001"));
    assert_eq!(snap.task_id.as_deref(), Some("task-alpha"));
    assert_eq!(snap.workflow_name.as_deref(), Some("qa-workflow"));
    assert_eq!(snap.timestamp.as_deref(), Some("2026-06-01T00:00:00.000Z"));
    assert_eq!(snap.status.as_deref(), Some("completed"));
    assert_eq!(snap.script.as_deref(), Some("export const meta = {}"));
    assert_eq!(snap.script_path.as_deref(), Some("/home/user/wf.js"));
    assert_eq!(snap.default_model.as_deref(), Some("claude-opus-4-8[1m]"));
    assert_eq!(snap.start_time, Some(1748745600000));
    assert_eq!(snap.duration_ms, Some(99999));
    assert_eq!(snap.agent_count, Some(7));
    assert_eq!(snap.total_tokens, Some(1234567));
    assert_eq!(snap.total_tool_calls, Some(42));
    assert_eq!(snap.summary.as_deref(), Some("QA test run"));
    // optional arrays absent
    assert!(snap.phases.is_none());
    assert!(snap.workflow_progress.is_none());
    assert!(snap.logs.is_none());
    assert!(snap.result.is_none());
    assert!(snap.args.is_none());
}

#[test]
fn snapshot_args_can_be_string_or_object() {
    use cc_session_jsonl::types::WorkflowRunSnapshot;

    let s: WorkflowRunSnapshot = serde_json::from_str(r#"{"args": "some string"}"#).unwrap();
    assert!(s.args.as_ref().unwrap().is_string());

    let s: WorkflowRunSnapshot =
        serde_json::from_str(r#"{"args": {"key": "value", "n": 3}}"#).unwrap();
    assert!(s.args.as_ref().unwrap().is_object());
    assert_eq!(s.args.as_ref().unwrap()["n"], 3);
}

#[test]
fn snapshot_result_can_be_string_or_object() {
    use cc_session_jsonl::types::WorkflowRunSnapshot;

    let s: WorkflowRunSnapshot = serde_json::from_str(r#"{"result": "plain result"}"#).unwrap();
    assert!(s.result.as_ref().unwrap().is_string());

    let s: WorkflowRunSnapshot =
        serde_json::from_str(r#"{"result": {"ok": true, "score": 9.5}}"#).unwrap();
    assert!(s.result.as_ref().unwrap().is_object());
    assert_eq!(s.result.as_ref().unwrap()["ok"], true);
}

#[test]
fn snapshot_logs_can_be_empty_or_populated() {
    use cc_session_jsonl::types::WorkflowRunSnapshot;

    let s: WorkflowRunSnapshot = serde_json::from_str(r#"{"logs": []}"#).unwrap();
    assert_eq!(s.logs.as_ref().unwrap().len(), 0);

    let s: WorkflowRunSnapshot = serde_json::from_str(
        r#"{"logs": [{"level":"info","msg":"started"}, {"level":"warn","msg":"slow"}]}"#,
    )
    .unwrap();
    assert_eq!(s.logs.as_ref().unwrap().len(), 2);
}

#[test]
fn snapshot_phases_round_trip() {
    use cc_session_jsonl::types::{WorkflowPhase, WorkflowRunSnapshot};

    let json = r#"{"phases":[
        {"title":"Phase A","detail":"Do something"},
        {"title":"Phase B"}
    ]}"#;
    let s: WorkflowRunSnapshot = serde_json::from_str(json).unwrap();
    let phases = s.phases.as_ref().unwrap();
    assert_eq!(phases.len(), 2);
    assert_eq!(phases[0].title.as_deref(), Some("Phase A"));
    assert_eq!(phases[0].detail.as_deref(), Some("Do something"));
    assert_eq!(phases[1].title.as_deref(), Some("Phase B"));
    assert!(phases[1].detail.is_none());

    let p: WorkflowPhase = serde_json::from_str(r#"{"title":"solo"}"#).unwrap();
    assert_eq!(p.title.as_deref(), Some("solo"));
    assert!(p.detail.is_none());
}

#[test]
fn snapshot_workflow_progress_phase_marker_and_agent_record() {
    use cc_session_jsonl::types::WorkflowRunSnapshot;

    let json = r#"{"workflowProgress":[
        {"type":"workflow_phase","index":1,"title":"prep"},
        {"type":"workflow_agent","index":1,"label":"worker","agentId":"agent-aaa","agentType":"demo:type","model":"claude-opus-4-8","state":"done","tokens":5000,"toolCalls":12,"durationMs":30000,"phaseIndex":1,"extraFuture":true}
    ]}"#;
    let s: WorkflowRunSnapshot = serde_json::from_str(json).unwrap();
    let prog = s.workflow_progress.as_ref().unwrap();

    assert_eq!(prog[0].kind.as_deref(), Some("workflow_phase"));
    assert_eq!(prog[0].index, Some(1));
    assert_eq!(prog[0].title.as_deref(), Some("prep"));
    assert!(prog[0].agent_id.is_none());

    assert_eq!(prog[1].kind.as_deref(), Some("workflow_agent"));
    assert_eq!(prog[1].label.as_deref(), Some("worker"));
    assert_eq!(prog[1].agent_id.as_deref(), Some("agent-aaa"));
    assert_eq!(prog[1].agent_type.as_deref(), Some("demo:type"));
    assert_eq!(prog[1].model.as_deref(), Some("claude-opus-4-8"));
    assert_eq!(prog[1].state.as_deref(), Some("done"));
    assert_eq!(prog[1].tokens, Some(5000));
    assert_eq!(prog[1].tool_calls, Some(12));
    assert_eq!(prog[1].duration_ms, Some(30000));
}

#[test]
fn snapshot_fully_empty_still_parses() {
    use cc_session_jsonl::types::WorkflowRunSnapshot;
    let s: WorkflowRunSnapshot = serde_json::from_str("{}").unwrap();
    assert!(s.run_id.is_none());
    assert!(s.agent_count.is_none());
    assert!(s.phases.is_none());
    assert!(s.result.is_none());
}

// ─── Layer 1: WorkflowJournalEntry deserialization ───────────────────────────

#[test]
fn journal_started_entry_all_fields() {
    use cc_session_jsonl::types::WorkflowJournalEntry;
    let json = r#"{"type":"started","key":"v2:0a1b2c3d","agentId":"fa001"}"#;
    let e: WorkflowJournalEntry = serde_json::from_str(json).unwrap();
    assert_eq!(e.kind.as_deref(), Some("started"));
    assert_eq!(e.key.as_deref(), Some("v2:0a1b2c3d"));
    assert_eq!(e.agent_id.as_deref(), Some("fa001"));
    assert!(e.result.is_none());
}

#[test]
fn journal_result_entry_with_string_result() {
    use cc_session_jsonl::types::WorkflowJournalEntry;
    let json = r#"{"type":"result","key":"v2:abc","agentId":"fa002","result":"succeeded"}"#;
    let e: WorkflowJournalEntry = serde_json::from_str(json).unwrap();
    assert_eq!(e.kind.as_deref(), Some("result"));
    assert!(e.result.as_ref().unwrap().is_string());
    assert_eq!(e.result.as_ref().unwrap().as_str(), Some("succeeded"));
}

#[test]
fn journal_result_entry_with_object_result() {
    use cc_session_jsonl::types::WorkflowJournalEntry;
    let json =
        r#"{"type":"result","key":"v2:xyz","agentId":"fa003","result":{"ok":true,"data":42}}"#;
    let e: WorkflowJournalEntry = serde_json::from_str(json).unwrap();
    assert!(e.result.as_ref().unwrap().is_object());
    assert_eq!(e.result.as_ref().unwrap()["ok"], true);
    assert_eq!(e.result.as_ref().unwrap()["data"], 42);
}

#[test]
fn journal_entry_minimal_no_panic() {
    use cc_session_jsonl::types::WorkflowJournalEntry;
    let e: WorkflowJournalEntry = serde_json::from_str("{}").unwrap();
    assert!(e.kind.is_none());
    assert!(e.key.is_none());
    assert!(e.agent_id.is_none());
    assert!(e.result.is_none());
}

#[test]
fn journal_entry_unknown_fields_ignored() {
    use cc_session_jsonl::types::WorkflowJournalEntry;
    let json = r#"{"type":"started","key":"v2:abc","agentId":"x","futureField":"ignored","nested":{"a":1}}"#;
    let e: WorkflowJournalEntry = serde_json::from_str(json).unwrap();
    assert_eq!(e.kind.as_deref(), Some("started"));
}

// ─── Layer 1: SystemEntry new subtype fields (independent coverage) ───────────

#[test]
fn system_turn_duration_pending_workflow_count_boundary_zero() {
    use cc_session_jsonl::types::Entry;
    let json = r#"{
        "type":"system","subtype":"turn_duration",
        "durationMs":100,"messageCount":2,"isMeta":false,"pendingWorkflowCount":0
    }"#;
    let entry: Entry = serde_json::from_str(json).unwrap();
    match entry {
        Entry::System(s) => {
            assert_eq!(s.pending_workflow_count(), Some(0));
            assert_eq!(s.message_count(), Some(2));
        }
        _ => panic!("expected System"),
    }
}

#[test]
fn system_turn_duration_large_values() {
    use cc_session_jsonl::types::Entry;
    let json = r#"{
        "type":"system","subtype":"turn_duration",
        "durationMs":999999999,"messageCount":9999,"pendingWorkflowCount":50
    }"#;
    let entry: Entry = serde_json::from_str(json).unwrap();
    match entry {
        Entry::System(s) => {
            assert_eq!(s.duration_ms(), Some(999999999));
            assert_eq!(s.message_count(), Some(9999));
            assert_eq!(s.pending_workflow_count(), Some(50));
        }
        _ => panic!("expected System"),
    }
}

#[test]
fn system_local_command_content_is_value_not_string() {
    use cc_session_jsonl::types::Entry;
    let json = r#"{
        "type":"system","subtype":"local_command",
        "content":"<command-name>/workflows</command-name>","isMeta":false
    }"#;
    let entry: Entry = serde_json::from_str(json).unwrap();
    match entry {
        Entry::System(s) => {
            let c = s.content().expect("content must be Some");
            assert!(c.is_string());
            assert!(c.as_str().unwrap().contains("workflows"));
        }
        _ => panic!("expected System"),
    }
}

#[test]
fn system_local_command_content_as_object() {
    use cc_session_jsonl::types::Entry;
    let json =
        r#"{"type":"system","subtype":"local_command","content":{"cmd":"/workflows","args":[]}}"#;
    let entry: Entry = serde_json::from_str(json).unwrap();
    match entry {
        Entry::System(s) => {
            let c = s.content().unwrap();
            assert!(c.is_object());
            assert_eq!(c["cmd"], "/workflows");
        }
        _ => panic!("expected System"),
    }
}

#[test]
fn system_away_summary_no_level() {
    use cc_session_jsonl::types::Entry;
    let json = r#"{
        "type":"system","subtype":"away_summary",
        "content":"Summary of away period","isMeta":false
    }"#;
    let entry: Entry = serde_json::from_str(json).unwrap();
    match entry {
        Entry::System(s) => {
            assert_eq!(s.subtype(), Some("away_summary"));
            assert!(s.level().is_none());
            assert!(s.content().unwrap().is_string());
        }
        _ => panic!("expected System"),
    }
}

// ─── Layer 1: AttachmentEntry.attachment field ────────────────────────────────

#[test]
fn attachment_entry_top_level_field_present() {
    use cc_session_jsonl::types::{AttachmentBody, AttachmentEntry};
    let json = r#"{
        "type":"attachment","uuid":"att-1","sessionId":"s1",
        "attachment":{"type":"hook_success","command":"run.sh","exitCode":0}
    }"#;
    let e: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert!(
        e.message.is_none(),
        "real v2.1.159 data has no `message` field"
    );
    let body = e.attachment.as_ref().expect("attachment must be Some");
    match body {
        AttachmentBody::HookSuccess {
            command, exit_code, ..
        } => {
            assert_eq!(command.as_deref(), Some("run.sh"));
            assert_eq!(*exit_code, Some(0));
        }
        other => panic!("expected HookSuccess, got {other:?}"),
    }
    assert_eq!(e.attachment_subtype(), Some("hook_success"));
}

#[test]
fn attachment_entry_with_both_message_and_attachment() {
    use cc_session_jsonl::types::AttachmentEntry;
    let json = r#"{
        "type":"attachment","uuid":"att-2","sessionId":"s1",
        "message":{"role":"user","content":"legacy"},
        "attachment":{"type":"task_reminder","content":"reminder text"}
    }"#;
    let e: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert!(e.message.is_some());
    assert!(e.attachment.is_some());
    assert_eq!(e.attachment_subtype(), Some("task_reminder"));
}

#[test]
fn attachment_subtype_returns_none_when_type_key_absent() {
    use cc_session_jsonl::types::AttachmentEntry;
    let json = r#"{"type":"attachment","uuid":"att-3","sessionId":"s1","attachment":{"data":"no-type-field"}}"#;
    let e: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert!(e.attachment.is_some());
    assert!(e.attachment_subtype().is_none());
}

#[test]
fn attachment_subtype_skill_listing() {
    use cc_session_jsonl::types::{AttachmentBody, AttachmentEntry};
    let json = r#"{"type":"attachment","uuid":"att-sl","sessionId":"s","attachment":{"type":"skill_listing","names":["a","b"]}}"#;
    let e: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert_eq!(e.attachment_subtype(), Some("skill_listing"));
    match e.attachment.as_ref().unwrap() {
        AttachmentBody::SkillListing { names, .. } => {
            assert_eq!(names.as_ref().unwrap()[0], "a");
        }
        other => panic!("expected SkillListing, got {other:?}"),
    }
}

#[test]
fn attachment_subtype_file() {
    use cc_session_jsonl::types::{AttachmentBody, AttachmentEntry};
    let json = r#"{"type":"attachment","uuid":"att-file","sessionId":"s","attachment":{"type":"file","path":"/a/b.txt","content":"hello"}}"#;
    let e: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert!(matches!(
        e.attachment.as_ref().unwrap(),
        AttachmentBody::Unknown
    ));
    assert!(e.attachment_subtype().is_none());
}

// ─── Layer 2 (loader): workflow discovery via the public surface ─────────────

use cc_session_jsonl::{load_agent_metadata, load_all_sessions, load_session};

/// Build a minimal tree: main session + one ordinary subagent + one workflow run
/// with two agents, a journal, and a snapshot. No script.
fn build_minimal_workflow_tree(
    claude_home: &std::path::Path,
    project_name: &str,
    session_uuid: &str,
    run_id: &str,
) {
    let proj = claude_home.join("projects").join(project_name);
    let subagents = proj.join(session_uuid).join("subagents");
    let wf_run = subagents.join("workflows").join(run_id);
    let workflows = proj.join(session_uuid).join("workflows");
    fs::create_dir_all(&subagents).unwrap();
    fs::create_dir_all(&wf_run).unwrap();
    fs::create_dir_all(&workflows).unwrap();

    // Main session
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        format!(
            r#"{{"type":"user","uuid":"u1","sessionId":"{}"}}"#,
            session_uuid
        ),
    )
    .unwrap();

    // Ordinary subagent (no workflow_run_id)
    fs::write(
        subagents.join("agent-ordinary.jsonl"),
        r#"{"type":"user","uuid":"oa1","sessionId":"sub"}"#,
    )
    .unwrap();

    // Workflow snapshot
    fs::write(
        workflows.join(format!("{}.json", run_id)),
        r#"{"runId":"wf_qa01","workflowName":"qa-test","status":"completed","agentCount":2,"totalTokens":10000}"#,
    )
    .unwrap();

    // Two workflow agents
    fs::write(
        wf_run.join("agent-wqa01.jsonl"),
        r#"{"type":"user","uuid":"wa1","sessionId":"wf-sub"}"#,
    )
    .unwrap();
    fs::write(
        wf_run.join("agent-wqa01.meta.json"),
        r#"{"agentType":"qa-worker","description":"QA agent one"}"#,
    )
    .unwrap();
    fs::write(
        wf_run.join("agent-wqa02.jsonl"),
        r#"{"type":"user","uuid":"wa2","sessionId":"wf-sub"}"#,
    )
    .unwrap();
    // No meta for agent-wqa02 (verify None path)

    // Journal
    fs::write(
        wf_run.join("journal.jsonl"),
        "{\"type\":\"started\",\"key\":\"v2:a1\",\"agentId\":\"wqa01\"}\n{\"type\":\"result\",\"key\":\"v2:a1\",\"agentId\":\"wqa01\",\"result\":\"done\"}",
    )
    .unwrap();
}

#[test]
fn type4_workflow_agents_discovered_and_tagged_with_run_id() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    build_minimal_workflow_tree(tmp.path(), "-Users-qa-proj", session_uuid, "wf_qa01");

    let sessions = load_all_sessions(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];

    // 1 ordinary + 2 workflow agents = 3
    assert_eq!(s.agents.len(), 3, "agents: {:?}", s.agents);

    let ordinary = s
        .agents
        .iter()
        .find(|a| a.agent_id == "agent-ordinary")
        .expect("ordinary subagent must be found");
    assert!(
        ordinary.workflow_run_id.is_none(),
        "ordinary subagent must NOT have a workflow_run_id"
    );
    assert_eq!(ordinary.parent_session_id, session_uuid);

    let wf_agents: Vec<_> = s
        .agents
        .iter()
        .filter(|a| a.workflow_run_id.as_deref() == Some("wf_qa01"))
        .collect();
    assert_eq!(wf_agents.len(), 2, "expected 2 workflow agent files");
    for wf in &wf_agents {
        assert_eq!(
            wf.parent_session_id, session_uuid,
            "workflow agent must have the correct parent session UUID"
        );
        assert!(
            wf.agent_id.starts_with("agent-wqa0"),
            "unexpected agent_id: {}",
            wf.agent_id
        );
    }
}

#[test]
fn ordinary_subagent_and_workflow_agent_counts_are_independent() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    build_minimal_workflow_tree(tmp.path(), "-Users-qa-proj3", session_uuid, "wf_qa01");

    let sessions = load_all_sessions(tmp.path()).unwrap();
    let s = &sessions[0];

    let ordinary_count = s
        .agents
        .iter()
        .filter(|a| a.workflow_run_id.is_none())
        .count();
    let wf_count = s
        .agents
        .iter()
        .filter(|a| a.workflow_run_id.is_some())
        .count();

    assert_eq!(ordinary_count, 1, "only 1 ordinary subagent");
    assert_eq!(wf_count, 2, "exactly 2 workflow agents");
}

#[test]
fn load_all_sessions_returns_correct_workflow_metadata() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    build_minimal_workflow_tree(tmp.path(), "-Users-qa-proj4", session_uuid, "wf_qa01");

    let sessions = load_all_sessions(tmp.path()).unwrap();
    let s = &sessions[0];
    assert_eq!(s.workflows.len(), 1);

    let wf = &s.workflows[0];
    assert_eq!(wf.run_id, "wf_qa01");
    assert_eq!(wf.session_id, session_uuid);
    assert_eq!(wf.project.as_deref(), Some("-Users-qa-proj4"));
    assert!(wf.snapshot_path.is_some(), "snapshot file must be found");
    assert!(wf.journal_path.is_some(), "journal must be found");

    // Snapshot parses correctly
    let snap = wf.snapshot.as_ref().expect("snapshot must parse");
    assert_eq!(snap.run_id.as_deref(), Some("wf_qa01"));
    assert_eq!(snap.workflow_name.as_deref(), Some("qa-test"));
    assert_eq!(snap.status.as_deref(), Some("completed"));
    assert_eq!(snap.agent_count, Some(2));
    assert_eq!(snap.total_tokens, Some(10000));

    // Journal eagerly parsed
    let journal = wf.journal.as_ref().expect("journal must parse");
    assert_eq!(journal.len(), 2);
    assert_eq!(journal[0].kind.as_deref(), Some("started"));
    assert_eq!(journal[1].kind.as_deref(), Some("result"));
}

#[test]
fn load_all_sessions_empty_when_no_workflow_dir() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    let proj = tmp.path().join("projects").join("-Users-qa-plain");
    fs::create_dir_all(&proj).unwrap();
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        r#"{"type":"user","uuid":"u1","sessionId":"s"}"#,
    )
    .unwrap();

    let sessions = load_all_sessions(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0].workflows.is_empty());
}

#[test]
fn load_all_sessions_missing_snapshot_still_finds_agents() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    let proj = tmp.path().join("projects").join("-Users-qa-nosnapshot");
    let wf_run = proj
        .join(session_uuid)
        .join("subagents")
        .join("workflows")
        .join("wf_nosnapshot");
    fs::create_dir_all(&wf_run).unwrap();

    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        r#"{"type":"user","uuid":"u1"}"#,
    )
    .unwrap();
    fs::write(
        wf_run.join("agent-solo.jsonl"),
        r#"{"type":"user","uuid":"x"}"#,
    )
    .unwrap();

    let sessions = load_all_sessions(tmp.path()).unwrap();
    let s = &sessions[0];
    assert_eq!(s.workflows.len(), 1);
    assert_eq!(s.workflows[0].run_id, "wf_nosnapshot");
    assert!(s.workflows[0].snapshot.is_none());
    assert!(s.workflows[0].snapshot_path.is_none());
    assert_eq!(s.agents.len(), 1);
    assert_eq!(
        s.agents[0].workflow_run_id.as_deref(),
        Some("wf_nosnapshot")
    );
}

#[test]
fn load_all_sessions_missing_journal_is_ok() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    let proj = tmp.path().join("projects").join("-Users-qa-nojournal");
    let workflows_dir = proj.join(session_uuid).join("workflows");
    let wf_run = proj
        .join(session_uuid)
        .join("subagents")
        .join("workflows")
        .join("wf_nojournal");
    fs::create_dir_all(&workflows_dir).unwrap();
    fs::create_dir_all(&wf_run).unwrap();

    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        r#"{"type":"user"}"#,
    )
    .unwrap();
    fs::write(
        workflows_dir.join("wf_nojournal.json"),
        r#"{"runId":"wf_nojournal","status":"running"}"#,
    )
    .unwrap();
    fs::write(
        wf_run.join("agent-x.jsonl"),
        r#"{"type":"user","uuid":"x"}"#,
    )
    .unwrap();

    let sessions = load_all_sessions(tmp.path()).unwrap();
    let s = &sessions[0];
    assert_eq!(s.workflows.len(), 1);
    assert!(s.workflows[0].journal_path.is_none());
    assert!(s.workflows[0].journal.is_none());
}

#[test]
fn load_all_sessions_multiple_runs_in_same_session() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    let proj = tmp.path().join("projects").join("-Users-qa-multiruns");
    let subagents = proj.join(session_uuid).join("subagents");
    let workflows_dir = proj.join(session_uuid).join("workflows");

    for run_id in &["wf_run001", "wf_run002", "wf_run003"] {
        let wf_run = subagents.join("workflows").join(run_id);
        fs::create_dir_all(&wf_run).unwrap();
        fs::create_dir_all(&workflows_dir).unwrap();
        fs::write(
            workflows_dir.join(format!("{}.json", run_id)),
            format!(r#"{{"runId":"{run_id}","status":"completed","agentCount":1}}"#),
        )
        .unwrap();
        fs::write(
            wf_run.join("agent-x.jsonl"),
            r#"{"type":"user","uuid":"x"}"#,
        )
        .unwrap();
    }
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        r#"{"type":"user"}"#,
    )
    .unwrap();

    let sessions = load_all_sessions(tmp.path()).unwrap();
    let s = &sessions[0];
    assert_eq!(s.workflows.len(), 3);
    let ids: Vec<&str> = s.workflows.iter().map(|r| r.run_id.as_str()).collect();
    assert!(ids.contains(&"wf_run001"));
    assert!(ids.contains(&"wf_run002"));
    assert!(ids.contains(&"wf_run003"));
}

#[test]
fn load_all_sessions_finds_workflows_across_projects() {
    let tmp = make_claude_home();
    let uuid_a = "aaaaaaaa-bbbb-cccc-dddd-000000000001";
    let uuid_b = "aaaaaaaa-bbbb-cccc-dddd-000000000002";

    for (uuid, proj, run_id) in &[
        (uuid_a, "-Users-qa-proj-x", "wf_pa"),
        (uuid_b, "-Users-qa-proj-y", "wf_pb"),
    ] {
        let p = tmp.path().join("projects").join(proj);
        let wf_run = p
            .join(uuid)
            .join("subagents")
            .join("workflows")
            .join(run_id);
        let wf_dir = p.join(uuid).join("workflows");
        fs::create_dir_all(&wf_run).unwrap();
        fs::create_dir_all(&wf_dir).unwrap();
        fs::write(p.join(format!("{}.jsonl", uuid)), r#"{"type":"user"}"#).unwrap();
        fs::write(
            wf_dir.join(format!("{}.json", run_id)),
            format!(r#"{{"runId":"{run_id}"}}"#),
        )
        .unwrap();
        fs::write(wf_run.join("agent-x.jsonl"), r#"{"type":"user"}"#).unwrap();
    }

    let sessions = load_all_sessions(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 2);
    let mut all_runs: Vec<&str> = sessions
        .iter()
        .flat_map(|s| s.workflows.iter().map(|r| r.run_id.as_str()))
        .collect();
    all_runs.sort();
    assert_eq!(all_runs, vec!["wf_pa", "wf_pb"]);
}

#[test]
fn load_agent_metadata_correct_map() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    build_minimal_workflow_tree(tmp.path(), "-Users-qa-meta", session_uuid, "wf_qa01");

    let meta_map = load_agent_metadata(tmp.path(), session_uuid);

    // wqa01 has a meta sidecar with agentType = "qa-worker", description = "QA agent one"
    let meta = meta_map.get("wqa01").expect("wqa01 meta must be present");
    assert_eq!(meta.agent_type.as_deref(), Some("qa-worker"));
    assert_eq!(meta.description.as_deref(), Some("QA agent one"));

    // wqa02 has no sidecar → not in map
    assert!(
        !meta_map.contains_key("wqa02"),
        "wqa02 has no meta sidecar — must not appear in map"
    );
}

#[test]
fn load_agent_metadata_returns_empty_for_nonexistent_session() {
    let tmp = make_claude_home();
    let meta_map = load_agent_metadata(tmp.path(), "nonexistent-session-id");
    assert!(meta_map.is_empty());
}

#[test]
fn workflow_run_id_absent_for_ordinary_scan_types() {
    // Types 1, 2, 3 must never get a workflow_run_id
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    let proj = tmp.path().join("projects").join("-Users-qa-classic");
    let subagents = proj.join(session_uuid).join("subagents");
    fs::create_dir_all(&subagents).unwrap();

    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        r#"{"type":"user"}"#,
    )
    .unwrap();
    fs::write(
        proj.join("agent-leg001.jsonl"),
        format!(r#"{{"type":"user","sessionId":"{}"}}"#, session_uuid),
    )
    .unwrap();
    fs::write(subagents.join("agent-new001.jsonl"), r#"{"type":"user"}"#).unwrap();

    let sessions = load_all_sessions(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    for a in &sessions[0].agents {
        assert!(
            a.workflow_run_id.is_none(),
            "agent '{}' should not have workflow_run_id",
            a.agent_id
        );
    }
}

#[test]
fn wf_dir_with_no_agent_files_but_has_snapshot_only() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    let proj = tmp.path().join("projects").join("-Users-qa-snapshotonly");
    let wf_dir = proj.join(session_uuid).join("workflows");
    fs::create_dir_all(&wf_dir).unwrap();
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        r#"{"type":"user"}"#,
    )
    .unwrap();
    fs::write(
        wf_dir.join("wf_snaponly.json"),
        r#"{"runId":"wf_snaponly","status":"failed","agentCount":0}"#,
    )
    .unwrap();

    let sessions = load_all_sessions(tmp.path()).unwrap();
    let s = &sessions[0];
    assert_eq!(s.workflows.len(), 1);
    assert_eq!(s.workflows[0].run_id, "wf_snaponly");
    assert!(s.workflows[0].snapshot.is_some());
    // No workflow agent files on disk → no workflow agents in the session.
    let wf_agents: Vec<_> = s
        .agents
        .iter()
        .filter(|a| a.workflow_run_id.as_deref() == Some("wf_snaponly"))
        .collect();
    assert!(wf_agents.is_empty());
}

#[test]
fn wf_dir_completely_empty_returns_no_run() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    let proj = tmp.path().join("projects").join("-Users-qa-emptyrun");
    let wf_run = proj
        .join(session_uuid)
        .join("subagents")
        .join("workflows")
        .join("wf_empty");
    fs::create_dir_all(&wf_run).unwrap();
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        r#"{"type":"user"}"#,
    )
    .unwrap();

    let sessions = load_all_sessions(tmp.path()).unwrap();
    let s = &sessions[0];
    assert!(s.workflows.is_empty());
}

#[test]
fn non_wf_dirs_under_workflows_subdir_are_ignored() {
    let tmp = make_claude_home();
    let session_uuid = make_session_uuid();
    let proj = tmp.path().join("projects").join("-Users-qa-notwf");
    let subagents = proj.join(session_uuid).join("subagents");
    let fake_wf = subagents.join("workflows").join("not-wf-dir");
    fs::create_dir_all(&fake_wf).unwrap();
    fs::write(
        proj.join(format!("{}.jsonl", session_uuid)),
        r#"{"type":"user"}"#,
    )
    .unwrap();
    fs::write(fake_wf.join("agent-x.jsonl"), r#"{"type":"user"}"#).unwrap();

    let sessions = load_all_sessions(tmp.path()).unwrap();
    assert_eq!(sessions.len(), 1);
    // No workflow agents and no workflow runs — `not-wf-dir` is filtered.
    assert!(sessions[0].workflows.is_empty());
    assert!(sessions[0].agents.is_empty());
}

// ─── Real data (Layer 3) ─────────────────────────────────────────────────

fn require_real_data() -> bool {
    std::env::var("REQUIRE_REAL_DATA").as_deref() == Ok("1")
}

#[test]
#[ignore]
fn real_session_ae289b37_workflow_scan() {
    let home = match std::env::var("HOME").ok() {
        Some(h) => std::path::PathBuf::from(h),
        None => {
            if require_real_data() {
                panic!("REQUIRE_REAL_DATA=1 but $HOME is unset");
            }
            return;
        }
    };
    let claude_home = home.join(".claude");
    if !claude_home.is_dir() {
        if require_real_data() {
            panic!("REQUIRE_REAL_DATA=1 but {:?} not found", claude_home);
        }
        eprintln!("Skipping: ~/.claude not found");
        return;
    }

    let session_id = "ae289b37-f19a-4797-b14c-52b5ada582ed";
    let s = match load_session(&claude_home, session_id) {
        Ok(s) => s,
        Err(_) => {
            if require_real_data() {
                panic!(
                    "REQUIRE_REAL_DATA=1 but reference session {} not present locally",
                    session_id
                );
            }
            eprintln!("Skipping: session {} not present locally", session_id);
            return;
        }
    };

    assert_eq!(
        s.workflows.len(),
        3,
        "expected 3 workflow runs for {}",
        session_id
    );
    for r in &s.workflows {
        assert!(
            r.snapshot.is_some(),
            "{} must have parsed snapshot",
            r.run_id
        );
        let wf_agents: Vec<_> = s
            .agents
            .iter()
            .filter(|a| a.workflow_run_id.as_deref() == Some(r.run_id.as_str()))
            .collect();
        assert!(
            !wf_agents.is_empty(),
            "{} must have at least one workflow agent",
            r.run_id
        );
        assert!(r.journal_path.is_some(), "{} must have journal", r.run_id);
    }
}
