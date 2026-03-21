use anyhow::{Context, Result};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::models::SessionFile;

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
pub fn scan_claude_home(claude_home: &Path) -> Result<Vec<SessionFile>> {
    let projects_dir = claude_home.join("projects");
    scan_projects_dir(&projects_dir)
}

/// Scan a projects directory for all session JSONL files.
///
/// This is the core scanner that works on any directory containing project
/// subdirectories with JSONL session files. `scan_claude_home` delegates to
/// this after appending `projects/`.
///
/// Directory structure expected:
/// ```text
/// projects_dir/
///   <project>/
///     <uuid>.jsonl              — main session
///     agent-<id>.jsonl          — legacy agent
///     <uuid>/subagents/agent-<id>.jsonl — new-style agent
/// ```
pub fn scan_projects_dir(projects_dir: &Path) -> Result<Vec<SessionFile>> {
    if !projects_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    // Iterate over project directories
    let project_entries = fs::read_dir(projects_dir)
        .with_context(|| format!("failed to read projects dir: {}", projects_dir.display()))?;

    for project_entry in project_entries {
        let project_entry = project_entry?;
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let project_name = project_entry
            .file_name()
            .to_string_lossy()
            .into_owned();

        // Iterate over entries inside each project directory
        let entries = fs::read_dir(&project_path)
            .with_context(|| format!("failed to read project dir: {}", project_path.display()))?;

        for entry in entries {
            let entry = entry?;
            let entry_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().into_owned();

            if entry_path.is_file() {
                // Skip non-jsonl files
                if !file_name.ends_with(".jsonl") {
                    continue;
                }

                let stem = file_name.trim_end_matches(".jsonl");

                if is_uuid(stem) {
                    // Type 1: Main session — <project>/<uuid>.jsonl
                    results.push(SessionFile {
                        session_id: stem.to_string(),
                        project: Some(project_name.clone()),
                        file_path: entry_path,
                        is_agent: false,
                        parent_session_id: None,
                    });
                } else if stem.starts_with("agent-") {
                    // Type 2: Legacy agent — <project>/agent-<id>.jsonl
                    results.push(SessionFile {
                        session_id: stem.to_string(),
                        project: Some(project_name.clone()),
                        file_path: entry_path,
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
                        let sub_entries = fs::read_dir(&subagents_dir).with_context(|| {
                            format!(
                                "failed to read subagents dir: {}",
                                subagents_dir.display()
                            )
                        })?;

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
                                    file_path: sub_path,
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

/// For legacy agent files that have no parent_session_id yet, read the first
/// JSON line and extract the `sessionId` field to use as parent_session_id.
pub fn resolve_agent_parents(files: &mut [SessionFile]) -> Result<()> {
    for file in files.iter_mut() {
        if !file.is_agent || file.parent_session_id.is_some() {
            continue;
        }

        // Read first line and extract sessionId
        let f = fs::File::open(&file.file_path).with_context(|| {
            format!(
                "failed to open agent file for parent resolution: {}",
                file.file_path.display()
            )
        })?;
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
        let project_dir = tmp.path().join("projects").join("-Users-testuser-myproject");
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

        assert_eq!(files.len(), 3, "should find 3 session files, found: {files:?}");

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
        let project_dir = tmp.path().join("projects").join("-Users-testuser-myproject");
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
