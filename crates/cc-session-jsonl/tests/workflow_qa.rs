//! QA tests for workflow support (cc-session-jsonl layer).
//!
//! Covers:
//! - WorkflowRunSnapshot / WorkflowPhase / WorkflowProgress / WorkflowJournalEntry parsing
//! - scanner Type4 workflow agent discovery
//! - scan_session_workflows / scan_workflows / load_workflow_agent_meta
//! - SessionFile.workflow_run_id propagation
//! - AttachmentEntry.attachment field (top-level, not in `message`)
//! - SystemEntry new subtypes: turn_duration with pendingWorkflowCount, local_command, away_summary
//!
//! All tests are independent of the builder's existing tests.

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

    // args = string
    let s: WorkflowRunSnapshot = serde_json::from_str(r#"{"args": "some string"}"#).unwrap();
    assert!(s.args.as_ref().unwrap().is_string());

    // args = object
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

    // Also parse WorkflowPhase standalone
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
    assert!(prog[0].agent_id.is_none()); // not present on phase marker

    assert_eq!(prog[1].kind.as_deref(), Some("workflow_agent"));
    assert_eq!(prog[1].label.as_deref(), Some("worker"));
    assert_eq!(prog[1].agent_id.as_deref(), Some("agent-aaa"));
    assert_eq!(prog[1].agent_type.as_deref(), Some("demo:type"));
    assert_eq!(prog[1].model.as_deref(), Some("claude-opus-4-8"));
    assert_eq!(prog[1].state.as_deref(), Some("done"));
    assert_eq!(prog[1].tokens, Some(5000));
    assert_eq!(prog[1].tool_calls, Some(12));
    assert_eq!(prog[1].duration_ms, Some(30000));
    // extra unknown field `phaseIndex` and `extraFuture` silently ignored
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
    // Completely empty — must not panic
    let e: WorkflowJournalEntry = serde_json::from_str("{}").unwrap();
    assert!(e.kind.is_none());
    assert!(e.key.is_none());
    assert!(e.agent_id.is_none());
    assert!(e.result.is_none());
}

#[test]
fn journal_entry_unknown_fields_ignored() {
    use cc_session_jsonl::types::WorkflowJournalEntry;
    // Future-compat: unknown fields should be silently ignored
    let json = r#"{"type":"started","key":"v2:abc","agentId":"x","futureField":"ignored","nested":{"a":1}}"#;
    let e: WorkflowJournalEntry = serde_json::from_str(json).unwrap();
    assert_eq!(e.kind.as_deref(), Some("started"));
}

// ─── Layer 1: SystemEntry new subtype fields (independent coverage) ───────────

#[test]
fn system_turn_duration_pending_workflow_count_boundary_zero() {
    use cc_session_jsonl::types::Entry;
    // pendingWorkflowCount = 0 is a valid value (not None)
    let json = r#"{
        "type":"system","subtype":"turn_duration",
        "durationMs":100,"messageCount":2,"isMeta":false,"pendingWorkflowCount":0
    }"#;
    let entry: Entry = serde_json::from_str(json).unwrap();
    match entry {
        Entry::System(s) => {
            assert_eq!(s.pending_workflow_count, Some(0));
            assert_eq!(s.message_count, Some(2));
        }
        _ => panic!("expected System"),
    }
}

#[test]
fn system_turn_duration_large_values() {
    use cc_session_jsonl::types::Entry;
    // Large u64 values — must not overflow
    let json = r#"{
        "type":"system","subtype":"turn_duration",
        "durationMs":999999999,"messageCount":9999,"pendingWorkflowCount":50
    }"#;
    let entry: Entry = serde_json::from_str(json).unwrap();
    match entry {
        Entry::System(s) => {
            assert_eq!(s.duration_ms, Some(999999999));
            assert_eq!(s.message_count, Some(9999));
            assert_eq!(s.pending_workflow_count, Some(50));
        }
        _ => panic!("expected System"),
    }
}

#[test]
fn system_local_command_content_is_value_not_string() {
    use cc_session_jsonl::types::Entry;
    // content field is Value — can hold string or object depending on CC version
    let json = r#"{
        "type":"system","subtype":"local_command",
        "content":"<command-name>/workflows</command-name>","isMeta":false
    }"#;
    let entry: Entry = serde_json::from_str(json).unwrap();
    match entry {
        Entry::System(s) => {
            let c = s.content.as_ref().expect("content must be Some");
            assert!(c.is_string());
            assert!(c.as_str().unwrap().contains("workflows"));
        }
        _ => panic!("expected System"),
    }
}

#[test]
fn system_local_command_content_as_object() {
    use cc_session_jsonl::types::Entry;
    // Future-proof: content may be an object
    let json =
        r#"{"type":"system","subtype":"local_command","content":{"cmd":"/workflows","args":[]}}"#;
    let entry: Entry = serde_json::from_str(json).unwrap();
    match entry {
        Entry::System(s) => {
            let c = s.content.as_ref().unwrap();
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
            assert_eq!(s.subtype.as_deref(), Some("away_summary"));
            assert!(s.level.is_none());
            assert!(s.content.as_ref().unwrap().is_string());
        }
        _ => panic!("expected System"),
    }
}

// ─── Layer 1: AttachmentEntry.attachment field ────────────────────────────────

#[test]
fn attachment_entry_top_level_field_present() {
    use cc_session_jsonl::types::AttachmentEntry;
    let json = r#"{
        "type":"attachment","uuid":"att-1","sessionId":"s1",
        "attachment":{"type":"hook_success","command":"run.sh","exitCode":0}
    }"#;
    let e: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert!(
        e.message.is_none(),
        "real v2.1.159 data has no `message` field"
    );
    let att = e.attachment.as_ref().expect("attachment must be Some");
    assert_eq!(att["type"], "hook_success");
    assert_eq!(att["exitCode"], 0);
    assert_eq!(e.attachment_subtype(), Some("hook_success"));
}

#[test]
fn attachment_entry_with_both_message_and_attachment() {
    use cc_session_jsonl::types::AttachmentEntry;
    // Forward-compat: both fields can coexist
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
    // attachment exists but has no "type" key
    assert!(e.attachment.is_some());
    assert!(e.attachment_subtype().is_none());
}

#[test]
fn attachment_subtype_skill_listing() {
    use cc_session_jsonl::types::AttachmentEntry;
    let json = r#"{"type":"attachment","uuid":"att-sl","sessionId":"s","attachment":{"type":"skill_listing","skills":["a","b"]}}"#;
    let e: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert_eq!(e.attachment_subtype(), Some("skill_listing"));
    assert_eq!(e.attachment.as_ref().unwrap()["skills"][0], "a");
}

#[test]
fn attachment_subtype_file() {
    use cc_session_jsonl::types::AttachmentEntry;
    let json = r#"{"type":"attachment","uuid":"att-file","sessionId":"s","attachment":{"type":"file","path":"/a/b.txt","content":"hello"}}"#;
    let e: AttachmentEntry = serde_json::from_str(json).unwrap();
    assert_eq!(e.attachment_subtype(), Some("file"));
    assert_eq!(e.attachment.as_ref().unwrap()["path"], "/a/b.txt");
}

// ─── Layer 2 (scanner): Type4 workflow agent discovery ───────────────────────

#[cfg(feature = "scanner")]
mod scanner_qa {
    use super::*;
    use cc_session_jsonl::scanner::{
        load_workflow_agent_meta, scan_session_workflows, scan_sessions, scan_workflows,
    };

    // Build a minimal tree: main session + one ordinary subagent + one workflow run
    // with two agents, a journal, and a snapshot. No script.
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

        let files = scan_sessions(tmp.path()).unwrap();

        // main + ordinary subagent + 2 workflow agents = 4
        assert_eq!(files.len(), 4, "files: {files:?}");

        let ordinary = files
            .iter()
            .find(|f| f.session_id == "agent-ordinary")
            .expect("ordinary subagent must be found");
        assert!(ordinary.is_agent);
        assert!(
            ordinary.workflow_run_id.is_none(),
            "ordinary subagent must NOT have a workflow_run_id"
        );
        assert_eq!(ordinary.parent_session_id.as_deref(), Some(session_uuid));

        let wf_agents: Vec<_> = files
            .iter()
            .filter(|f| f.workflow_run_id.as_deref() == Some("wf_qa01"))
            .collect();
        assert_eq!(wf_agents.len(), 2, "expected 2 workflow agent files");
        for wf in &wf_agents {
            assert!(wf.is_agent);
            assert_eq!(
                wf.parent_session_id.as_deref(),
                Some(session_uuid),
                "workflow agent must have the correct parent session UUID"
            );
            assert!(
                wf.session_id.starts_with("agent-wqa0"),
                "unexpected session_id: {}",
                wf.session_id
            );
        }
    }

    #[test]
    fn type3_does_not_enter_workflow_subdir_as_regular_agent() {
        // Critical: Type3 scan must NOT pick up workflow agents from the `workflows/`
        // subdirectory as ordinary subagents. Type4 is the sole discoverer.
        // Verify by checking that workflow agent session_ids are only found with
        // a non-None workflow_run_id.
        let tmp = make_claude_home();
        let session_uuid = make_session_uuid();
        build_minimal_workflow_tree(tmp.path(), "-Users-qa-proj2", session_uuid, "wf_qa01");

        let files = scan_sessions(tmp.path()).unwrap();

        // Every agent file found in the workflows subdir must have workflow_run_id set
        for f in &files {
            if f.session_id.starts_with("agent-wqa") {
                assert!(
                    f.workflow_run_id.is_some(),
                    "agent '{}' was picked up by Type3 (no workflow_run_id) — double discovery!",
                    f.session_id
                );
            }
        }
    }

    #[test]
    fn ordinary_subagent_and_workflow_agent_counts_are_independent() {
        let tmp = make_claude_home();
        let session_uuid = make_session_uuid();
        build_minimal_workflow_tree(tmp.path(), "-Users-qa-proj3", session_uuid, "wf_qa01");

        let files = scan_sessions(tmp.path()).unwrap();

        let ordinary_count = files
            .iter()
            .filter(|f| f.is_agent && f.workflow_run_id.is_none())
            .count();
        let wf_count = files.iter().filter(|f| f.workflow_run_id.is_some()).count();

        assert_eq!(ordinary_count, 1, "only 1 ordinary subagent");
        assert_eq!(wf_count, 2, "exactly 2 workflow agents");
    }

    #[test]
    fn scan_session_workflows_returns_correct_run_metadata() {
        let tmp = make_claude_home();
        let session_uuid = make_session_uuid();
        build_minimal_workflow_tree(tmp.path(), "-Users-qa-proj4", session_uuid, "wf_qa01");

        let runs = scan_session_workflows(session_uuid, tmp.path()).unwrap();
        assert_eq!(runs.len(), 1);

        let run = &runs[0];
        assert_eq!(run.run_id, "wf_qa01");
        assert_eq!(run.session_id, session_uuid);
        assert_eq!(run.project.as_deref(), Some("-Users-qa-proj4"));
        assert!(run.snapshot_path.is_some(), "snapshot file must be found");
        assert!(run.journal_path.is_some(), "journal must be found");
        assert_eq!(run.agent_files.len(), 2);

        // Snapshot parses correctly
        let snap = run.snapshot.as_ref().expect("snapshot must parse");
        assert_eq!(snap.run_id.as_deref(), Some("wf_qa01"));
        assert_eq!(snap.workflow_name.as_deref(), Some("qa-test"));
        assert_eq!(snap.status.as_deref(), Some("completed"));
        assert_eq!(snap.agent_count, Some(2));
        assert_eq!(snap.total_tokens, Some(10000));

        // agent_files: IDs are stripped of "agent-" prefix, sorted
        let ids: Vec<&str> = run
            .agent_files
            .iter()
            .map(|a| a.agent_id.as_str())
            .collect();
        assert!(ids.contains(&"wqa01"), "wqa01 must be present");
        assert!(ids.contains(&"wqa02"), "wqa02 must be present");

        // agent-wqa01 has a meta sidecar; agent-wqa02 does not
        let wqa01 = run
            .agent_files
            .iter()
            .find(|a| a.agent_id == "wqa01")
            .unwrap();
        let wqa02 = run
            .agent_files
            .iter()
            .find(|a| a.agent_id == "wqa02")
            .unwrap();
        assert!(wqa01.meta_path.is_some(), "wqa01 must have meta sidecar");
        assert!(
            wqa02.meta_path.is_none(),
            "wqa02 must NOT have meta sidecar"
        );
    }

    #[test]
    fn scan_session_workflows_empty_when_no_workflow_dir() {
        let tmp = make_claude_home();
        let session_uuid = make_session_uuid();
        // Only a main session, no workflow directory
        let proj = tmp.path().join("projects").join("-Users-qa-plain");
        fs::create_dir_all(&proj).unwrap();
        fs::write(
            proj.join(format!("{}.jsonl", session_uuid)),
            r#"{"type":"user","uuid":"u1","sessionId":"s"}"#,
        )
        .unwrap();

        let runs = scan_session_workflows(session_uuid, tmp.path()).unwrap();
        assert!(
            runs.is_empty(),
            "no workflow runs expected for plain session"
        );
    }

    #[test]
    fn scan_session_workflows_missing_snapshot_still_finds_agents() {
        // Missing wf_*.json does NOT cause scan_session_workflows to return an empty list —
        // it still discovers agents from the subagents/workflows/wf_*/ directory.
        let tmp = make_claude_home();
        let session_uuid = make_session_uuid();
        let proj = tmp.path().join("projects").join("-Users-qa-nosnapshot");
        let wf_run = proj
            .join(session_uuid)
            .join("subagents")
            .join("workflows")
            .join("wf_nosnapshot");
        fs::create_dir_all(&wf_run).unwrap();

        // Main session
        fs::write(
            proj.join(format!("{}.jsonl", session_uuid)),
            r#"{"type":"user","uuid":"u1"}"#,
        )
        .unwrap();

        // Agent transcript (no snapshot file)
        fs::write(
            wf_run.join("agent-solo.jsonl"),
            r#"{"type":"user","uuid":"x"}"#,
        )
        .unwrap();

        let runs = scan_session_workflows(session_uuid, tmp.path()).unwrap();
        // build_workflow_run returns None only when BOTH snapshot AND agent_files are absent.
        // Here agent_files has one entry, so it must return Some.
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, "wf_nosnapshot");
        assert!(
            runs[0].snapshot.is_none(),
            "no snapshot json → snapshot is None"
        );
        assert!(runs[0].snapshot_path.is_none());
        assert_eq!(runs[0].agent_files.len(), 1);
    }

    #[test]
    fn scan_session_workflows_missing_journal_is_ok() {
        // Journal is optional — should not block discovery
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
        // No journal.jsonl

        let runs = scan_session_workflows(session_uuid, tmp.path()).unwrap();
        assert_eq!(runs.len(), 1);
        assert!(
            runs[0].journal_path.is_none(),
            "journal_path must be None when journal absent"
        );
        assert_eq!(runs[0].agent_files.len(), 1);
    }

    #[test]
    fn scan_session_workflows_multiple_runs_in_same_session() {
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

        let runs = scan_session_workflows(session_uuid, tmp.path()).unwrap();
        assert_eq!(runs.len(), 3, "three distinct runs must be discovered");
        let ids: Vec<&str> = runs.iter().map(|r| r.run_id.as_str()).collect();
        assert!(ids.contains(&"wf_run001"));
        assert!(ids.contains(&"wf_run002"));
        assert!(ids.contains(&"wf_run003"));
    }

    #[test]
    fn scan_workflows_global_finds_all_across_projects() {
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

        let runs = scan_workflows(tmp.path()).unwrap();
        assert_eq!(runs.len(), 2, "must find one run per project");
        let rids: Vec<&str> = runs.iter().map(|r| r.run_id.as_str()).collect();
        assert!(rids.contains(&"wf_pa"));
        assert!(rids.contains(&"wf_pb"));
    }

    #[test]
    fn load_workflow_agent_meta_correct_map() {
        let tmp = make_claude_home();
        let session_uuid = make_session_uuid();
        build_minimal_workflow_tree(tmp.path(), "-Users-qa-meta", session_uuid, "wf_qa01");

        let meta_map = load_workflow_agent_meta(session_uuid, tmp.path());

        // wqa01 has a meta sidecar with agentType = "qa-worker", description = "QA agent one"
        let meta = meta_map.get("wqa01").expect("wqa01 meta must be present");
        assert_eq!(meta.agent_type.as_deref(), Some("qa-worker"));
        assert_eq!(meta.description.as_deref(), Some("QA agent one"));

        // wqa02 has no sidecar → not in map
        assert!(
            meta_map.get("wqa02").is_none(),
            "wqa02 has no meta sidecar — must not appear in map"
        );
    }

    #[test]
    fn load_workflow_agent_meta_returns_empty_for_nonexistent_session() {
        let tmp = make_claude_home();
        let meta_map = load_workflow_agent_meta("nonexistent-session-id", tmp.path());
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

        // Type 1: main session
        fs::write(
            proj.join(format!("{}.jsonl", session_uuid)),
            r#"{"type":"user"}"#,
        )
        .unwrap();
        // Type 2: legacy agent
        fs::write(
            proj.join("agent-leg001.jsonl"),
            r#"{"type":"user","sessionId":"parent"}"#,
        )
        .unwrap();
        // Type 3: new-style subagent
        fs::write(subagents.join("agent-new001.jsonl"), r#"{"type":"user"}"#).unwrap();

        let files = scan_sessions(tmp.path()).unwrap();
        assert_eq!(files.len(), 3);
        for f in &files {
            assert!(
                f.workflow_run_id.is_none(),
                "file '{}' should not have workflow_run_id",
                f.session_id
            );
        }
    }

    #[test]
    fn wf_dir_with_no_agent_files_but_has_snapshot_only() {
        // A wf_*.json with no agents in subagents/workflows/wf_*/ but the snapshot file
        // exists → should still produce a WorkflowRun (not None), with empty agent_files.
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
        // No subagents/workflows/wf_snaponly/ directory at all

        let runs = scan_session_workflows(session_uuid, tmp.path()).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, "wf_snaponly");
        assert!(runs[0].snapshot.is_some());
        assert!(runs[0].agent_files.is_empty());
    }

    #[test]
    fn wf_dir_completely_empty_returns_no_run() {
        // A wf_* directory under subagents/workflows that contains no agent files AND
        // no snapshot → build_workflow_run returns None → not included in results.
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
        // wf_empty dir is empty: no agents, no snapshot in workflows/

        let runs = scan_session_workflows(session_uuid, tmp.path()).unwrap();
        // wf_empty has a subdir but no snapshot and no agent files → returns None → empty
        assert!(
            runs.is_empty(),
            "completely empty wf dir should yield no run"
        );
    }

    #[test]
    fn scan_sessions_ignores_non_wf_dirs_under_workflows() {
        // Only `wf_*` subdirectory names should be picked up — other dirs are ignored
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

        let files = scan_sessions(tmp.path()).unwrap();
        // Only the main session; agent under `not-wf-dir/` must be ignored
        assert_eq!(
            files.len(),
            1,
            "non-wf_ dir must not produce workflow agent files"
        );
    }

    // ─── Real data (Layer 3) ─────────────────────────────────────────────────

    /// Real session ae289b37 contains 3 workflow runs (wf_7c0e6255-566,
    /// wf_81719e41-156, wf_c210842b-3d9). Verify the scanner finds them all,
    /// each has a snapshot, agents and a journal.
    /// See `crates/cc-token-usage/tests/workflow_qa.rs` for the same pattern
    /// and the rationale: `REQUIRE_REAL_DATA=1` turns silent-skip into panic.
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
        let runs = scan_session_workflows(session_id, &claude_home).unwrap();

        if runs.is_empty() {
            if require_real_data() {
                panic!(
                    "REQUIRE_REAL_DATA=1 but reference session {} not present locally",
                    session_id
                );
            }
            eprintln!("Skipping: session {} not present locally", session_id);
            return;
        }

        assert_eq!(runs.len(), 3, "expected 3 workflow runs for {}", session_id);
        for r in &runs {
            assert!(
                r.snapshot.is_some(),
                "{} must have parsed snapshot",
                r.run_id
            );
            assert!(!r.agent_files.is_empty(), "{} must have agents", r.run_id);
            assert!(r.journal_path.is_some(), "{} must have journal", r.run_id);
        }
    }
}
