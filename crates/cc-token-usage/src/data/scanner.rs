//! Session file discovery — thin wrapper around cc-session-jsonl's scanner.
//!
//! Re-exports the core scanner functions and types. The `load_agent_meta`
//! function converts cc-session-jsonl's `AgentMeta` into the (String, String)
//! tuple format expected by the existing analysis layer.

use std::path::Path;

pub use cc_session_jsonl::scanner::{
    resolve_agent_parents, scan_sessions as scan_sessions_raw, SessionFile,
};

/// Scan `~/.claude/projects/` for all session JSONL files and return metadata.
///
/// This is a thin wrapper around `cc_session_jsonl::scan_sessions` that converts
/// the `io::Result` into `anyhow::Result` for compatibility with the rest of the codebase.
pub fn scan_claude_home(claude_home: &Path) -> anyhow::Result<Vec<SessionFile>> {
    Ok(cc_session_jsonl::scanner::scan_sessions(claude_home)?)
}

/// Load agent metadata from .meta.json files for a given session.
/// Returns a map of agent_id (e.g., "agent-abc123") -> (agentType, description).
///
/// This wraps cc-session-jsonl's `load_agent_meta` and converts `AgentMeta`
/// into the tuple format used by the existing session analysis code.
pub fn load_agent_meta(
    session_id: &str,
    claude_home: &Path,
) -> std::collections::HashMap<String, (String, String)> {
    cc_session_jsonl::scanner::load_agent_meta(session_id, claude_home)
        .into_iter()
        .map(|(k, meta)| {
            let agent_type = meta.agent_type.unwrap_or_else(|| "unknown".to_string());
            let description = meta.description.unwrap_or_default();
            (k, (agent_type, description))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a minimal Claude home with projects structure.
    fn setup_claude_home() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let projects = tmp.path().join("projects");
        fs::create_dir_all(&projects).unwrap();
        tmp
    }

    #[test]
    fn scan_finds_all_session_types() {
        let tmp = setup_claude_home();
        let project_dir = tmp
            .path()
            .join("projects")
            .join("-Users-testuser-myproject");
        fs::create_dir_all(&project_dir).unwrap();

        // Type 1: main session
        let main_uuid = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        fs::write(
            project_dir.join(format!("{}.jsonl", main_uuid)),
            r#"{"type":"user","sessionId":"a1b2c3d4-e5f6-7890-abcd-ef1234567890"}"#,
        )
        .unwrap();

        // Type 2: legacy agent
        fs::write(
            project_dir.join("agent-abc1234.jsonl"),
            r#"{"type":"user","sessionId":"parent-session-id-here"}"#,
        )
        .unwrap();

        // Type 3: new-style agent
        let subagents_dir = project_dir.join(main_uuid).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();
        fs::write(
            subagents_dir.join("agent-long-id-abcdef1234567890.jsonl"),
            r#"{"type":"user","sessionId":"sub-session"}"#,
        )
        .unwrap();

        let files = scan_claude_home(tmp.path()).unwrap();

        assert_eq!(
            files.len(),
            3,
            "should find 3 session files, found: {files:?}"
        );

        let main = files.iter().find(|f| f.session_id == main_uuid).unwrap();
        assert!(!main.is_agent);
        assert!(main.parent_session_id.is_none());

        let legacy = files
            .iter()
            .find(|f| f.session_id == "agent-abc1234")
            .unwrap();
        assert!(legacy.is_agent);
        assert!(legacy.parent_session_id.is_none()); // not resolved yet

        let new_agent = files
            .iter()
            .find(|f| f.session_id == "agent-long-id-abcdef1234567890")
            .unwrap();
        assert!(new_agent.is_agent);
        assert_eq!(
            new_agent.parent_session_id.as_deref(),
            Some(main_uuid),
            "new-style agent should have parent_session_id from directory name"
        );
    }

    #[test]
    fn agent_has_parent_session_id() {
        let tmp = setup_claude_home();
        let project_dir = tmp
            .path()
            .join("projects")
            .join("-Users-testuser-myproject");
        let parent_uuid = "11111111-2222-3333-4444-555555555555";
        let subagents_dir = project_dir.join(parent_uuid).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        fs::write(
            subagents_dir.join("agent-newstyle-001.jsonl"),
            r#"{"type":"user","sessionId":"agent-newstyle-001"}"#,
        )
        .unwrap();

        let files = scan_claude_home(tmp.path()).unwrap();

        assert_eq!(files.len(), 1);
        let agent = &files[0];
        assert!(agent.is_agent);
        assert_eq!(
            agent.parent_session_id.as_deref(),
            Some(parent_uuid),
            "new-style agent parent_session_id must match the UUID directory"
        );
    }

    #[test]
    fn ignores_non_jsonl_files() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-testuser-proj");
        fs::create_dir_all(&project_dir).unwrap();

        // .meta.json — should be ignored
        fs::write(project_dir.join("something.meta.json"), "{}").unwrap();

        // tool-results directory — should be ignored
        let tool_results = project_dir.join("tool-results");
        fs::create_dir_all(&tool_results).unwrap();
        fs::write(tool_results.join("result.jsonl"), "{}").unwrap();

        // memory directory — should be ignored
        let memory = project_dir.join("memory");
        fs::create_dir_all(&memory).unwrap();
        fs::write(memory.join("notes.jsonl"), "{}").unwrap();

        // A random .txt file — should be ignored
        fs::write(project_dir.join("notes.txt"), "hello").unwrap();

        let files = scan_claude_home(tmp.path()).unwrap();
        assert!(
            files.is_empty(),
            "should not find any session files, but found: {files:?}"
        );
    }

    #[test]
    fn resolve_legacy_agent_parent() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-testuser-proj");
        fs::create_dir_all(&project_dir).unwrap();

        let agent_file = project_dir.join("agent-xyz7890.jsonl");
        fs::write(
            &agent_file,
            r#"{"type":"user","sessionId":"parent-sess-id","uuid":"u1"}
{"type":"assistant","sessionId":"parent-sess-id","uuid":"u2"}"#,
        )
        .unwrap();

        let mut files = scan_claude_home(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].parent_session_id.is_none());

        resolve_agent_parents(&mut files).unwrap();
        assert_eq!(
            files[0].parent_session_id.as_deref(),
            Some("parent-sess-id"),
            "legacy agent parent_session_id should come from first line's sessionId"
        );
    }
}
