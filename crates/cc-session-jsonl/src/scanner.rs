//! File discovery for Claude Code session JSONL files.
//!
//! Scans `~/.claude/projects/` for session files and resolves agent parentage.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Metadata about a session JSONL file on disk.
#[derive(Debug, Clone)]
pub struct SessionFile {
    /// The session identifier (UUID for main sessions, `agent-<id>` for agents).
    pub session_id: String,
    /// The project directory name (e.g., `-Users-loki-myproject`).
    pub project: Option<String>,
    /// Full path to the JSONL file.
    pub path: PathBuf,
    /// Whether this is an agent file (vs. a main session file).
    pub is_agent: bool,
    /// The parent session ID for agent files.
    pub parent_session_id: Option<String>,
}

/// Metadata about a sub-agent, loaded from `.meta.json` sidecar files.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMeta {
    /// The type of agent (e.g., "code", "research").
    pub agent_type: Option<String>,
    /// A human-readable description of the agent's task.
    pub description: Option<String>,
    /// The worktree path associated with this agent, if any.
    pub worktree_path: Option<String>,
}

/// Check if a string looks like a UUID (8-4-4-4-12 hex pattern).
fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let expected_lens = [8, 4, 4, 4, 12];
    parts
        .iter()
        .zip(expected_lens.iter())
        .all(|(part, &len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Scan `~/.claude/projects/` for all session JSONL files and return metadata.
///
/// Finds three kinds of files:
/// 1. Main sessions: `<project>/<uuid>.jsonl`
/// 2. Legacy agents: `<project>/agent-<id>.jsonl`
/// 3. New-style agents: `<project>/<uuid>/subagents/agent-<id>.jsonl`
pub fn scan_sessions(claude_home: &Path) -> io::Result<Vec<SessionFile>> {
    let projects_dir = claude_home.join("projects");
    if !projects_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    let project_entries = fs::read_dir(&projects_dir)?;

    for project_entry in project_entries {
        let project_entry = project_entry?;
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let project_name = project_entry.file_name().to_string_lossy().into_owned();

        let entries = fs::read_dir(&project_path)?;

        for entry in entries {
            let entry = entry?;
            let entry_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().into_owned();

            if entry_path.is_file() {
                if !file_name.ends_with(".jsonl") {
                    continue;
                }

                let stem = file_name.trim_end_matches(".jsonl");

                if is_uuid(stem) {
                    // Type 1: Main session — <project>/<uuid>.jsonl
                    results.push(SessionFile {
                        session_id: stem.to_string(),
                        project: Some(project_name.clone()),
                        path: entry_path,
                        is_agent: false,
                        parent_session_id: None,
                    });
                } else if stem.starts_with("agent-") {
                    // Type 2: Legacy agent — <project>/agent-<id>.jsonl
                    results.push(SessionFile {
                        session_id: stem.to_string(),
                        project: Some(project_name.clone()),
                        path: entry_path,
                        is_agent: true,
                        parent_session_id: None,
                    });
                }
            } else if entry_path.is_dir() {
                // Skip well-known non-session directories
                if file_name == "memory" || file_name == "tool-results" {
                    continue;
                }

                // Check for new-style agents under <uuid>/subagents/
                if is_uuid(&file_name) {
                    let parent_uuid = file_name.clone();
                    let subagents_dir = entry_path.join("subagents");
                    if subagents_dir.is_dir() {
                        let sub_entries = fs::read_dir(&subagents_dir)?;

                        for sub_entry in sub_entries {
                            let sub_entry = sub_entry?;
                            let sub_path = sub_entry.path();
                            let sub_name = sub_entry.file_name().to_string_lossy().into_owned();

                            if !sub_path.is_file() || !sub_name.ends_with(".jsonl") {
                                continue;
                            }

                            let sub_stem = sub_name.trim_end_matches(".jsonl");
                            if sub_stem.starts_with("agent-") {
                                // Type 3: New-style agent
                                results.push(SessionFile {
                                    session_id: sub_stem.to_string(),
                                    project: Some(project_name.clone()),
                                    path: sub_path,
                                    is_agent: true,
                                    parent_session_id: Some(parent_uuid.clone()),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(results)
}

/// For legacy agent files that have no `parent_session_id` yet, read the first
/// JSON line and extract the `sessionId` field to use as `parent_session_id`.
pub fn resolve_agent_parents(files: &mut [SessionFile]) -> io::Result<()> {
    for file in files.iter_mut() {
        if !file.is_agent || file.parent_session_id.is_some() {
            continue;
        }

        let f = fs::File::open(&file.path)?;
        let reader = BufReader::new(f);

        if let Some(Ok(first_line)) = reader.lines().next() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&first_line) {
                if let Some(sid) = val.get("sessionId").and_then(|v| v.as_str()) {
                    file.parent_session_id = Some(sid.to_string());
                }
            }
        }
    }

    Ok(())
}

/// Load agent metadata from `.meta.json` sidecar files for a given session.
///
/// Returns a map of agent ID (the full `agent-<id>` stem) to `AgentMeta`.
pub fn load_agent_meta(session_id: &str, claude_home: &Path) -> HashMap<String, AgentMeta> {
    let mut result = HashMap::new();
    let projects_dir = claude_home.join("projects");
    if !projects_dir.exists() {
        return result;
    }

    // Search all project dirs for <session_id>/subagents/agent-*.meta.json
    let entries = match fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(_) => return result,
    };

    for entry in entries.flatten() {
        let subagents_dir = entry.path().join(session_id).join("subagents");
        if !subagents_dir.exists() {
            continue;
        }

        let sub_entries = match fs::read_dir(&subagents_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for sub_entry in sub_entries.flatten() {
            let name = sub_entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".meta.json") {
                continue;
            }

            // Extract agent ID: "agent-xyz.meta.json" -> "xyz"
            let agent_id = name
                .trim_start_matches("agent-")
                .trim_end_matches(".meta.json");

            if let Ok(content) = fs::read_to_string(sub_entry.path()) {
                if let Ok(meta) = serde_json::from_str::<AgentMeta>(&content) {
                    result.insert(agent_id.to_string(), meta);
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_claude_home() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let projects = tmp.path().join("projects");
        fs::create_dir_all(projects).unwrap();
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
            project_dir.join(format!("{main_uuid}.jsonl")),
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

        let files = scan_sessions(tmp.path()).unwrap();

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
        assert!(legacy.parent_session_id.is_none());

        let new_agent = files
            .iter()
            .find(|f| f.session_id == "agent-long-id-abcdef1234567890")
            .unwrap();
        assert!(new_agent.is_agent);
        assert_eq!(new_agent.parent_session_id.as_deref(), Some(main_uuid),);
    }

    #[test]
    fn resolve_legacy_agent_parent() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-testuser-proj");
        fs::create_dir_all(&project_dir).unwrap();

        fs::write(
            project_dir.join("agent-xyz7890.jsonl"),
            r#"{"type":"user","sessionId":"parent-sess-id","uuid":"u1"}
{"type":"assistant","sessionId":"parent-sess-id","uuid":"u2"}"#,
        )
        .unwrap();

        let mut files = scan_sessions(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].parent_session_id.is_none());

        resolve_agent_parents(&mut files).unwrap();
        assert_eq!(
            files[0].parent_session_id.as_deref(),
            Some("parent-sess-id"),
        );
    }

    #[test]
    fn ignores_non_jsonl_files() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-testuser-proj");
        fs::create_dir_all(&project_dir).unwrap();

        fs::write(project_dir.join("something.meta.json"), "{}").unwrap();

        let tool_results = project_dir.join("tool-results");
        fs::create_dir_all(&tool_results).unwrap();
        fs::write(tool_results.join("result.jsonl"), "{}").unwrap();

        let memory = project_dir.join("memory");
        fs::create_dir_all(&memory).unwrap();
        fs::write(memory.join("notes.jsonl"), "{}").unwrap();

        fs::write(project_dir.join("notes.txt"), "hello").unwrap();

        let files = scan_sessions(tmp.path()).unwrap();
        assert!(
            files.is_empty(),
            "should not find any session files, but found: {files:?}"
        );
    }

    #[test]
    fn empty_projects_dir() {
        let tmp = TempDir::new().unwrap();
        // No projects/ directory at all
        let files = scan_sessions(tmp.path()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn load_agent_meta_from_sidecar() {
        let tmp = setup_claude_home();
        let session_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let project_dir = tmp.path().join("projects").join("-Users-testuser-proj");
        let subagents_dir = project_dir.join(session_id).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        fs::write(
            subagents_dir.join("agent-abc123.meta.json"),
            r#"{"agentType":"code","description":"Write tests","worktreePath":"/tmp/wt"}"#,
        )
        .unwrap();

        let meta = load_agent_meta(session_id, tmp.path());
        assert_eq!(meta.len(), 1);
        let m = meta.get("abc123").unwrap();
        assert_eq!(m.agent_type.as_deref(), Some("code"));
        assert_eq!(m.description.as_deref(), Some("Write tests"));
        assert_eq!(m.worktree_path.as_deref(), Some("/tmp/wt"));
    }
}
