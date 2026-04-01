//! Integration tests for cc-session-jsonl.
//!
//! These tests exercise the full session lifecycle: scanning, parsing, and loading
//! using temporary directories that simulate the Claude Code directory structure.

use std::fs;
use tempfile::TempDir;

use cc_session_jsonl::parse_entry;
use cc_session_jsonl::types::Entry;

// Note: SessionReader and LenientReader are tested here via parse_entry.
// The scanner + session features require the "scanner" feature.

fn setup_claude_home() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let projects = tmp.path().join("projects");
    fs::create_dir_all(projects).unwrap();
    tmp
}

// ── parse_entry integration tests ──

#[test]
fn parse_entry_roundtrip_all_types() {
    // Verify that every entry type can be parsed through the public parse_entry function
    let test_cases = vec![
        (r#"{"type":"user","uuid":"u1","sessionId":"s1","message":{"role":"user","content":"hi"}}"#, "user"),
        (r#"{"type":"assistant","uuid":"a1","sessionId":"s1","message":{"model":"claude-opus-4-6","role":"assistant","content":[{"type":"text","text":"ok"}]}}"#, "assistant"),
        (r#"{"type":"system","uuid":"sys1","sessionId":"s1","subtype":"init"}"#, "system"),
        (r#"{"type":"attachment","uuid":"att1","sessionId":"s1"}"#, "attachment"),
        (r#"{"type":"summary","leafUuid":"l1","summary":"done"}"#, "summary"),
        (r#"{"type":"custom-title","sessionId":"s1","customTitle":"t"}"#, "custom-title"),
        (r#"{"type":"ai-title","sessionId":"s1","aiTitle":"t"}"#, "ai-title"),
        (r#"{"type":"last-prompt","sessionId":"s1","lastPrompt":"p"}"#, "last-prompt"),
        (r#"{"type":"task-summary","sessionId":"s1","summary":"s","timestamp":"2026-01-01T00:00:00Z"}"#, "task-summary"),
        (r#"{"type":"tag","sessionId":"s1","tag":"t"}"#, "tag"),
        (r#"{"type":"agent-name","sessionId":"s1","agentName":"n"}"#, "agent-name"),
        (r##"{"type":"agent-color","sessionId":"s1","agentColor":"#f00"}"##, "agent-color"),
        (r#"{"type":"agent-setting","sessionId":"s1","agentSetting":"default"}"#, "agent-setting"),
        (r#"{"type":"pr-link","sessionId":"s1","prNumber":1}"#, "pr-link"),
        (r#"{"type":"mode","sessionId":"s1","mode":"code"}"#, "mode"),
        (r#"{"type":"queue-operation","sessionId":"s1","operation":"enqueue"}"#, "queue-operation"),
        (r#"{"type":"speculation-accept","timestamp":"2026-01-01T00:00:00Z","timeSavedMs":500}"#, "speculation-accept"),
        (r#"{"type":"worktree-state","sessionId":"s1","worktreeSession":null}"#, "worktree-state"),
        (r#"{"type":"content-replacement","sessionId":"s1","replacements":[]}"#, "content-replacement"),
        (r#"{"type":"file-history-snapshot","messageId":"m1","snapshot":{},"isSnapshotUpdate":false}"#, "file-history-snapshot"),
        (r#"{"type":"attribution-snapshot","messageId":"m1","surface":"cli","fileStates":{}}"#, "attribution-snapshot"),
        (r#"{"type":"marble-origami-commit","sessionId":"s1","collapseId":"0001","summaryUuid":"su1","summaryContent":"x","summary":"x","firstArchivedUuid":"f1","lastArchivedUuid":"l1"}"#, "marble-origami-commit"),
        (r#"{"type":"marble-origami-snapshot","sessionId":"s1","staged":[],"armed":false,"lastSpawnTokens":0}"#, "marble-origami-snapshot"),
    ];

    for (json, type_name) in &test_cases {
        let result = parse_entry(json);
        assert!(result.is_ok(), "Failed to parse {type_name}: {:?}", result.err());
    }

    // Unknown type
    let unknown = parse_entry(r#"{"type":"future-type","data":1}"#).unwrap();
    assert!(matches!(unknown, Entry::Unknown));
}

#[test]
fn parse_realistic_assistant_turn() {
    let json = r#"{"parentUuid":"abc","isSidechain":false,"type":"assistant","uuid":"def","timestamp":"2026-03-16T13:51:35.912Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"cache_creation_input_tokens":1281,"cache_read_input_tokens":15204,"cache_creation":{"ephemeral_5m_input_tokens":1281,"ephemeral_1h_input_tokens":0},"output_tokens":108,"service_tier":"standard"},"content":[{"type":"text","text":"Hello"}]},"sessionId":"abc-123","version":"2.0.77","cwd":"/tmp","gitBranch":"main","userType":"external","requestId":"req_1"}"#;

    let entry = parse_entry(json).unwrap();
    match entry {
        Entry::Assistant(a) => {
            assert_eq!(a.parent_uuid.as_deref(), Some("abc"));
            assert_eq!(a.is_sidechain, Some(false));
            assert_eq!(a.uuid.as_deref(), Some("def"));
            let msg = a.message.as_ref().unwrap();
            assert_eq!(msg.model.as_deref(), Some("claude-opus-4-6"));
            let usage = msg.usage.as_ref().unwrap();
            assert_eq!(usage.input_tokens, Some(3));
            assert_eq!(usage.cache_creation_input_tokens, Some(1281));
            assert_eq!(usage.cache_read_input_tokens, Some(15204));
            assert_eq!(usage.output_tokens, Some(108));
            let cache = usage.cache_creation.as_ref().unwrap();
            assert_eq!(cache.ephemeral_5m_input_tokens, Some(1281));
            assert_eq!(cache.ephemeral_1h_input_tokens, Some(0));
        }
        other => panic!("Expected Assistant, got: {other:?}"),
    }
}

// ── Scanner + Session integration tests (require "scanner" feature) ──

#[cfg(feature = "scanner")]
mod scanner_tests {
    use super::*;
    use cc_session_jsonl::load_all_sessions;

    #[test]
    fn full_session_lifecycle() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-tester-myproject");
        let session_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let subagents_dir = project_dir.join(session_id).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        // Main session JSONL with mixed entry types
        let main_content = format!(
            r#"{{"type":"user","uuid":"u1","parentUuid":null,"isSidechain":false,"timestamp":"2026-03-16T10:00:00Z","sessionId":"{session_id}","cwd":"/tmp/myproject","version":"2.0.77","gitBranch":"main","userType":"external","message":{{"role":"user","content":"Write tests for the parser"}}}}
{{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"timestamp":"2026-03-16T10:00:05Z","sessionId":"{session_id}","cwd":"/tmp/myproject","version":"2.0.77","gitBranch":"main","userType":"external","requestId":"req_1","message":{{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{{"input_tokens":1500,"output_tokens":800,"cache_creation_input_tokens":500,"cache_read_input_tokens":1000}},"content":[{{"type":"thinking","thinking":"I need to write tests...","signature":"sig1"}},{{"type":"text","text":"I will write comprehensive tests."}}]}}}}
{{"type":"system","uuid":"s1","parentUuid":"a1","isSidechain":false,"timestamp":"2026-03-16T10:00:06Z","sessionId":"{session_id}","subtype":"tool_result","durationMs":234,"message":{{"role":"system","content":"Tool executed successfully"}}}}
{{"type":"ai-title","sessionId":"{session_id}","aiTitle":"Parser Test Suite"}}
{{"type":"tag","sessionId":"{session_id}","tag":"tests"}}
{{"type":"mode","sessionId":"{session_id}","mode":"code"}}"#
        );

        fs::write(
            project_dir.join(format!("{session_id}.jsonl")),
            &main_content,
        )
        .unwrap();

        // Agent file
        let agent_content = format!(
            r#"{{"type":"user","uuid":"au1","sessionId":"{session_id}","timestamp":"2026-03-16T10:01:00Z","message":{{"role":"user","content":"Agent task: analyze file"}}}}
{{"type":"assistant","uuid":"aa1","parentUuid":"au1","sessionId":"{session_id}","timestamp":"2026-03-16T10:01:05Z","message":{{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{{"input_tokens":200,"output_tokens":100}},"content":[{{"type":"text","text":"File analyzed."}}]}}}}"#
        );

        fs::write(
            subagents_dir.join("agent-test-agent-001.jsonl"),
            &agent_content,
        )
        .unwrap();

        // Agent meta
        fs::write(
            subagents_dir.join("agent-test-agent-001.meta.json"),
            r#"{"agentType":"code","description":"Test writing agent","worktreePath":"/tmp/wt"}"#,
        )
        .unwrap();

        // Load via load_all_sessions
        let sessions = load_all_sessions(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 1);

        let session = &sessions[0];
        assert_eq!(session.id, session_id);
        assert_eq!(session.project.as_deref(), Some("-Users-tester-myproject"));
        assert_eq!(session.main_entries.len(), 6);
        assert_eq!(session.titles, vec!["Parser Test Suite"]);
        assert_eq!(session.tags, vec!["tests"]);
        assert_eq!(session.mode.as_deref(), Some("code"));

        // Verify agent
        assert_eq!(session.agent_files.len(), 1);
        let agent = &session.agent_files[0];
        assert_eq!(agent.agent_id, "agent-test-agent-001");
        assert_eq!(agent.entries.len(), 2);
        assert!(agent.meta.is_some());
        let meta = agent.meta.as_ref().unwrap();
        assert_eq!(meta.agent_type.as_deref(), Some("code"));
        assert_eq!(meta.description.as_deref(), Some("Test writing agent"));

        // Verify specific entry types
        assert!(matches!(&session.main_entries[0], Entry::User(_)));
        assert!(matches!(&session.main_entries[1], Entry::Assistant(_)));
        assert!(matches!(&session.main_entries[2], Entry::System(_)));
        assert!(matches!(&session.main_entries[3], Entry::AiTitle(_)));
        assert!(matches!(&session.main_entries[4], Entry::Tag(_)));
        assert!(matches!(&session.main_entries[5], Entry::Mode(_)));

        // Verify assistant entry contents
        if let Entry::Assistant(a) = &session.main_entries[1] {
            let msg = a.message.as_ref().unwrap();
            let usage = msg.usage.as_ref().unwrap();
            assert_eq!(usage.input_tokens, Some(1500));
            assert_eq!(usage.output_tokens, Some(800));
            let content = msg.content.as_ref().unwrap();
            assert_eq!(content.len(), 2);
        } else {
            panic!("Expected Assistant entry at index 1");
        }
    }

    #[test]
    fn load_multiple_projects_and_sessions() {
        let tmp = setup_claude_home();

        // Project 1 with 2 sessions
        let proj1_dir = tmp.path().join("projects").join("-Users-tester-proj1");
        fs::create_dir_all(&proj1_dir).unwrap();

        let sess1 = "11111111-1111-1111-1111-111111111111";
        let sess2 = "22222222-2222-2222-2222-222222222222";

        fs::write(
            proj1_dir.join(format!("{sess1}.jsonl")),
            format!(r#"{{"type":"user","uuid":"u1","sessionId":"{sess1}","message":{{"role":"user","content":"proj1 sess1"}}}}"#),
        )
        .unwrap();

        fs::write(
            proj1_dir.join(format!("{sess2}.jsonl")),
            format!(r#"{{"type":"user","uuid":"u2","sessionId":"{sess2}","message":{{"role":"user","content":"proj1 sess2"}}}}"#),
        )
        .unwrap();

        // Project 2 with 1 session
        let proj2_dir = tmp.path().join("projects").join("-Users-tester-proj2");
        fs::create_dir_all(&proj2_dir).unwrap();

        let sess3 = "33333333-3333-3333-3333-333333333333";
        fs::write(
            proj2_dir.join(format!("{sess3}.jsonl")),
            format!(r#"{{"type":"user","uuid":"u3","sessionId":"{sess3}","message":{{"role":"user","content":"proj2 sess1"}}}}"#),
        )
        .unwrap();

        let sessions = load_all_sessions(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 3);

        let proj1_sessions: Vec<_> = sessions
            .iter()
            .filter(|s| s.project.as_deref() == Some("-Users-tester-proj1"))
            .collect();
        assert_eq!(proj1_sessions.len(), 2);

        let proj2_sessions: Vec<_> = sessions
            .iter()
            .filter(|s| s.project.as_deref() == Some("-Users-tester-proj2"))
            .collect();
        assert_eq!(proj2_sessions.len(), 1);
    }

    #[test]
    fn load_session_with_unparseable_lines_gracefully() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-tester-proj");
        fs::create_dir_all(&project_dir).unwrap();

        let session_id = "44444444-4444-4444-4444-444444444444";
        // Mix of valid entries, garbage, and empty lines
        let content = format!(
            r#"{{"type":"user","uuid":"u1","sessionId":"{session_id}","message":{{"role":"user","content":"first"}}}}
GARBAGE LINE HERE
{{"malformed json
{{"type":"assistant","uuid":"a1","sessionId":"{session_id}","message":{{"model":"claude-opus-4-6","role":"assistant","content":[{{"type":"text","text":"reply"}}]}}}}

{{"type":"ai-title","sessionId":"{session_id}","aiTitle":"Test"}}"#
        );

        fs::write(
            project_dir.join(format!("{session_id}.jsonl")),
            &content,
        )
        .unwrap();

        let sessions = load_all_sessions(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 1);
        // Should have 3 valid entries (user, assistant, ai-title), skipping 2 bad lines and 1 empty
        assert_eq!(sessions[0].main_entries.len(), 3);
        assert_eq!(sessions[0].titles, vec!["Test"]);
    }
}
