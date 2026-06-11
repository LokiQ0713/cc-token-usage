//! File discovery for Claude Code session JSONL files.
//!
//! Scans `~/.claude/projects/` for session files and resolves agent parentage.
//!
//! This module is `pub(crate)` — the public surface is the [`crate::loader`]
//! module which wraps these primitives behind [`crate::Session`] /
//! [`crate::Agent`] / [`crate::Workflow`].

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::types::WorkflowRunSnapshot;

/// Metadata about a session JSONL file on disk.
#[derive(Debug, Clone)]
pub(crate) struct SessionFile {
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
    /// The workflow run id (`wf_<runId>`) this agent file belongs to, if it was
    /// discovered under `<uuid>/subagents/workflows/wf_<runId>/`. `None` for
    /// ordinary (non-workflow) session and agent files.
    pub workflow_run_id: Option<String>,
}

/// Metadata about a sub-agent, loaded from `.meta.json` sidecar files.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentMeta {
    /// The type of agent (e.g., "code", "research").
    pub agent_type: Option<String>,
    /// A human-readable description of the agent's task.
    pub description: Option<String>,
    /// The worktree path associated with this agent, if any.
    pub worktree_path: Option<String>,
}

/// A single agent transcript belonging to a workflow run.
///
/// Note: `path` and `meta_path` are populated by `build_workflow_run` for
/// completeness and historical compatibility, but the loader never reads them
/// — workflow agent files reach the loader through the unified
/// `SessionFile` channel (`is_agent=true` with `workflow_run_id=Some`), so the
/// agent file paths are already in the main scan output.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct WorkflowAgentFile {
    /// The agent id, with the `agent-` prefix stripped (e.g. `a4df3aac3c00e0e09`).
    pub agent_id: String,
    /// Full path to the agent's `.jsonl` transcript.
    pub path: PathBuf,
    /// Full path to the agent's `.meta.json` sidecar, if it exists.
    pub meta_path: Option<PathBuf>,
}

/// A discovered workflow run, located under a session directory.
#[derive(Debug, Clone)]
pub(crate) struct WorkflowRun {
    /// The workflow run id, e.g. `wf_7c0e6255-566`.
    pub run_id: String,
    /// The owning session UUID.
    pub session_id: String,
    /// The project directory name (e.g. `-Users-loki-myproject`).
    pub project: Option<String>,
    /// The parsed run snapshot, if `workflows/wf_<runId>.json` existed and parsed.
    pub snapshot: Option<WorkflowRunSnapshot>,
    /// Path to the `workflows/wf_<runId>.json` snapshot file, if present.
    pub snapshot_path: Option<PathBuf>,
    /// Paths to the workflow's script source files (`workflows/scripts/*-wf_<runId>.js`).
    pub script_paths: Vec<PathBuf>,
    /// The agent transcripts produced by this run. The loader does not read
    /// this — workflow agent files come back through the main scan via
    /// `SessionFile.workflow_run_id` — but `build_workflow_run` still
    /// populates the list so the run discovery oracle (snapshot OR agents
    /// present) can short-circuit empty runs to `None`.
    #[allow(dead_code)]
    pub agent_files: Vec<WorkflowAgentFile>,
    /// Path to the run's `journal.jsonl`, if present.
    pub journal_path: Option<PathBuf>,
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
/// Finds four kinds of files:
/// 1. Main sessions: `<project>/<uuid>.jsonl`
/// 2. Legacy agents: `<project>/agent-<id>.jsonl`
/// 3. New-style agents: `<project>/<uuid>/subagents/agent-<id>.jsonl`
/// 4. Workflow agents: `<project>/<uuid>/subagents/workflows/wf_<runId>/agent-<id>.jsonl`
pub(crate) fn scan_sessions(claude_home: &Path) -> io::Result<Vec<SessionFile>> {
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
                        workflow_run_id: None,
                    });
                } else if stem.starts_with("agent-") {
                    // Type 2: Legacy agent — <project>/agent-<id>.jsonl
                    results.push(SessionFile {
                        session_id: stem.to_string(),
                        project: Some(project_name.clone()),
                        path: entry_path,
                        is_agent: true,
                        parent_session_id: None,
                        workflow_run_id: None,
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
                                    workflow_run_id: None,
                                });
                            }
                        }

                        // Type 4: Workflow agents — <uuid>/subagents/workflows/wf_<runId>/agent-*.jsonl
                        // These live one directory deeper than ordinary new-style
                        // agents and carry a workflow_run_id. They reuse the existing
                        // agent归集 channel (is_agent + parent_session_id) so their
                        // tokens are counted, but the workflow_run_id keeps them
                        // distinguishable from ordinary subagents.
                        let wf_root = subagents_dir.join("workflows");
                        if wf_root.is_dir() {
                            collect_workflow_agent_files(
                                &wf_root,
                                &parent_uuid,
                                &project_name,
                                &mut results,
                            )?;
                        }
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Walk `<uuid>/subagents/workflows/` and push every
/// `wf_<runId>/agent-*.jsonl` transcript into `results` as a workflow agent file.
///
/// Each discovered file is recorded as `is_agent = true`,
/// `parent_session_id = Some(<uuid>)` and `workflow_run_id = Some("wf_<runId>")`,
/// so it flows through the ordinary agent grouping while staying distinguishable.
fn collect_workflow_agent_files(
    wf_root: &Path,
    parent_uuid: &str,
    project_name: &str,
    results: &mut Vec<SessionFile>,
) -> io::Result<()> {
    let run_dirs = fs::read_dir(wf_root)?;
    for run_dir in run_dirs {
        let run_dir = run_dir?;
        let run_path = run_dir.path();
        if !run_path.is_dir() {
            continue;
        }
        let run_id = run_dir.file_name().to_string_lossy().into_owned();
        if !run_id.starts_with("wf_") {
            continue;
        }

        let agent_entries = fs::read_dir(&run_path)?;
        for agent_entry in agent_entries {
            let agent_entry = agent_entry?;
            let agent_path = agent_entry.path();
            let agent_name = agent_entry.file_name().to_string_lossy().into_owned();

            if !agent_path.is_file() || !agent_name.ends_with(".jsonl") {
                continue;
            }

            let agent_stem = agent_name.trim_end_matches(".jsonl");
            if agent_stem.starts_with("agent-") {
                results.push(SessionFile {
                    session_id: agent_stem.to_string(),
                    project: Some(project_name.to_string()),
                    path: agent_path,
                    is_agent: true,
                    parent_session_id: Some(parent_uuid.to_string()),
                    workflow_run_id: Some(run_id.clone()),
                });
            }
        }
    }
    Ok(())
}

/// For legacy agent files that have no `parent_session_id` yet, read the first
/// JSON line and extract the `sessionId` field to use as `parent_session_id`.
pub(crate) fn resolve_agent_parents(files: &mut [SessionFile]) -> io::Result<()> {
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
/// Returns a map of agent id (the `agent-` prefix stripped) to `AgentMeta`.
pub(crate) fn load_agent_meta(session_id: &str, claude_home: &Path) -> HashMap<String, AgentMeta> {
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

/// Build a [`WorkflowRun`] for `run_id` under `session_dir` by scanning its
/// snapshot, scripts, agent transcripts and journal. Returns `None` only if the
/// run has no discoverable files at all (neither a snapshot nor agent transcripts).
fn build_workflow_run(
    session_dir: &Path,
    session_id: &str,
    project: Option<&str>,
    run_id: &str,
) -> Option<WorkflowRun> {
    // Snapshot: workflows/wf_<runId>.json
    let snapshot_path = session_dir.join("workflows").join(format!("{run_id}.json"));
    let (snapshot, snapshot_path) = if snapshot_path.is_file() {
        let snap = fs::read_to_string(&snapshot_path)
            .ok()
            .and_then(|c| serde_json::from_str::<WorkflowRunSnapshot>(&c).ok());
        (snap, Some(snapshot_path))
    } else {
        (None, None)
    };

    // Scripts: workflows/scripts/*-wf_<runId>.js (suffix match on the run id).
    let mut script_paths = Vec::new();
    let scripts_dir = session_dir.join("workflows").join("scripts");
    if let Ok(script_entries) = fs::read_dir(&scripts_dir) {
        for script_entry in script_entries.flatten() {
            let script_path = script_entry.path();
            if !script_path.is_file() {
                continue;
            }
            let name = script_entry.file_name().to_string_lossy().into_owned();
            // File names look like `<workflow-name>-wf_<runId>.js`.
            if name.ends_with(".js") && name.trim_end_matches(".js").ends_with(run_id) {
                script_paths.push(script_path);
            }
        }
    }
    script_paths.sort();

    // Agent transcripts + journal: subagents/workflows/wf_<runId>/
    let run_dir = session_dir.join("subagents").join("workflows").join(run_id);
    let mut agent_files = Vec::new();
    let mut journal_path = None;
    if let Ok(run_entries) = fs::read_dir(&run_dir) {
        for run_entry in run_entries.flatten() {
            let path = run_entry.path();
            if !path.is_file() {
                continue;
            }
            let name = run_entry.file_name().to_string_lossy().into_owned();
            if name == "journal.jsonl" {
                journal_path = Some(path);
            } else if name.ends_with(".jsonl") && name.starts_with("agent-") {
                let agent_id = name.trim_end_matches(".jsonl").trim_start_matches("agent-");
                let meta_path = run_dir.join(format!("agent-{agent_id}.meta.json"));
                let meta_path = if meta_path.is_file() {
                    Some(meta_path)
                } else {
                    None
                };
                agent_files.push(WorkflowAgentFile {
                    agent_id: agent_id.to_string(),
                    path,
                    meta_path,
                });
            }
        }
    }
    agent_files.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));

    if snapshot_path.is_none() && agent_files.is_empty() {
        return None;
    }

    Some(WorkflowRun {
        run_id: run_id.to_string(),
        session_id: session_id.to_string(),
        project: project.map(|p| p.to_string()),
        snapshot,
        snapshot_path,
        script_paths,
        agent_files,
        journal_path,
    })
}

/// Discover the set of workflow run ids present under a session directory by
/// looking at both `workflows/wf_*.json` and `subagents/workflows/wf_*/`.
fn discover_run_ids(session_dir: &Path) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut ids: BTreeSet<String> = BTreeSet::new();

    // From workflows/wf_*.json
    if let Ok(entries) = fs::read_dir(session_dir.join("workflows")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".json") {
                if stem.starts_with("wf_") {
                    ids.insert(stem.to_string());
                }
            }
        }
    }

    // From subagents/workflows/wf_*/
    if let Ok(entries) = fs::read_dir(session_dir.join("subagents").join("workflows")) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("wf_") {
                ids.insert(name);
            }
        }
    }

    ids.into_iter().collect()
}

/// Discover all workflow runs belonging to a single session.
pub(crate) fn scan_session_workflows(
    session_id: &str,
    claude_home: &Path,
) -> io::Result<Vec<WorkflowRun>> {
    let projects_dir = claude_home.join("projects");
    if !projects_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut runs = Vec::new();
    for project_entry in fs::read_dir(&projects_dir)?.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let session_dir = project_path.join(session_id);
        if !session_dir.is_dir() {
            continue;
        }
        let project_name = project_entry.file_name().to_string_lossy().into_owned();

        for run_id in discover_run_ids(&session_dir) {
            if let Some(run) =
                build_workflow_run(&session_dir, session_id, Some(&project_name), &run_id)
            {
                runs.push(run);
            }
        }
    }

    Ok(runs)
}

/// Discover all workflow runs across every session in a Claude home directory.
#[allow(dead_code)]
pub(crate) fn scan_workflows(claude_home: &Path) -> io::Result<Vec<WorkflowRun>> {
    let projects_dir = claude_home.join("projects");
    if !projects_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut runs = Vec::new();
    for project_entry in fs::read_dir(&projects_dir)?.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let project_name = project_entry.file_name().to_string_lossy().into_owned();

        for session_entry in fs::read_dir(&project_path)?.flatten() {
            let session_path = session_entry.path();
            if !session_path.is_dir() {
                continue;
            }
            let session_id = session_entry.file_name().to_string_lossy().into_owned();
            if !is_uuid(&session_id) {
                continue;
            }

            for run_id in discover_run_ids(&session_path) {
                if let Some(run) =
                    build_workflow_run(&session_path, &session_id, Some(&project_name), &run_id)
                {
                    runs.push(run);
                }
            }
        }
    }

    Ok(runs)
}

/// Load agent metadata for a session's workflow agents from their `.meta.json`
/// sidecars under `<session_id>/subagents/workflows/wf_*/agent-*.meta.json`.
pub(crate) fn load_workflow_agent_meta(
    session_id: &str,
    claude_home: &Path,
) -> HashMap<String, AgentMeta> {
    let mut result = HashMap::new();
    let projects_dir = claude_home.join("projects");
    if !projects_dir.exists() {
        return result;
    }

    let entries = match fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(_) => return result,
    };

    for entry in entries.flatten() {
        let wf_root = entry
            .path()
            .join(session_id)
            .join("subagents")
            .join("workflows");
        if !wf_root.is_dir() {
            continue;
        }

        let run_dirs = match fs::read_dir(&wf_root) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for run_dir in run_dirs.flatten() {
            if !run_dir.path().is_dir() {
                continue;
            }
            let meta_entries = match fs::read_dir(run_dir.path()) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for meta_entry in meta_entries.flatten() {
                let name = meta_entry.file_name().to_string_lossy().to_string();
                if !name.ends_with(".meta.json") {
                    continue;
                }
                let agent_id = name
                    .trim_start_matches("agent-")
                    .trim_end_matches(".meta.json");
                if let Ok(content) = fs::read_to_string(meta_entry.path()) {
                    if let Ok(meta) = serde_json::from_str::<AgentMeta>(&content) {
                        result.insert(agent_id.to_string(), meta);
                    }
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
