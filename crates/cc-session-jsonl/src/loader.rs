//! Session loading — the sole public entry point for session aggregation.
//!
//! Wraps the (now-private) scanner and reader layers and emits a single
//! [`Session`] struct per session UUID, complete with its main entries,
//! all subagent transcripts (ordinary and workflow), and discovered
//! workflow runs.
//!
//! Three public functions cover every loading need:
//! - [`load_all_sessions`] — scan + load every session under a Claude home.
//! - [`load_session`]      — scan + load a single session by id.
//! - [`load_agent_metadata`] — read agent `.meta.json` sidecars for one
//!   session (both ordinary `subagents/` and workflow `subagents/workflows/`).

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::parser::SessionReader;
use crate::scanner;
use crate::types::{Entry, WorkflowJournalEntry, WorkflowRunSnapshot};

/// One aggregated session: main entries + every associated agent transcript
/// + every discovered workflow run.
///
/// `main_entries` is a flat `Vec<Entry>` (no wrapper struct) — consumers walk it
/// directly. Agent transcripts (ordinary and workflow) all live in `agents`;
/// the `workflow_run_id` field distinguishes the two kinds.
#[derive(Debug, Clone)]
pub struct Session {
    /// The session UUID (main file stem).
    pub id: String,
    /// The project directory name (e.g. `-Users-loki-myproject`).
    pub project: Option<String>,
    /// Absolute path to the main `<sid>.jsonl` file on disk. For synthetic
    /// orphan sessions (agents without a main file) this points at the
    /// would-be location (`<projects>/<parent>/<parent>.jsonl`) which need
    /// not exist on disk — `main_entries` will simply be empty.
    pub main_path: PathBuf,
    /// Entries from the main session JSONL file, in file order.
    pub main_entries: Vec<Entry>,
    /// Every agent transcript that belongs to this session.
    ///
    /// Includes ordinary subagents (`subagents/agent-*.jsonl` and
    /// `subagents/<id>.jsonl`) **and** workflow agents
    /// (`subagents/workflows/wf_*/agent-*.jsonl`). The two are distinguished
    /// by `Agent.workflow_run_id`.
    pub agents: Vec<Agent>,
    /// Workflow runs discovered under `<session>/workflows/` and/or
    /// `<session>/subagents/workflows/wf_*/`. Empty for pre-2.1.159 sessions.
    pub workflows: Vec<Workflow>,
}

/// One agent transcript file, parsed and grouped under its parent session.
#[derive(Debug, Clone)]
pub struct Agent {
    /// The agent identifier (the JSONL file stem, e.g. `agent-abc123`).
    pub agent_id: String,
    /// The parent session UUID this agent belongs to.
    pub parent_session_id: String,
    /// The workflow run id (`wf_<runId>`) when the agent was discovered under
    /// `subagents/workflows/wf_<runId>/`. `None` for ordinary subagents.
    pub workflow_run_id: Option<String>,
    /// The project directory name.
    pub project: Option<String>,
    /// Full path to the agent JSONL file on disk. Lets consumers correlate
    /// `Agent`s back to source files without re-running the scanner.
    pub path: PathBuf,
    /// Parsed entries from the agent transcript.
    pub entries: Vec<Entry>,
    /// Metadata from the agent's `.meta.json` sidecar, if present.
    pub meta: Option<AgentMetadata>,
}

/// One discovered workflow run.
#[derive(Debug, Clone)]
pub struct Workflow {
    /// The workflow run id, e.g. `wf_7c0e6255-566`.
    pub run_id: String,
    /// The owning session UUID.
    pub session_id: String,
    /// The project directory name.
    pub project: Option<String>,
    /// Parsed run snapshot, if the `workflows/wf_<runId>.json` file existed
    /// and parsed successfully. Malformed snapshots collapse to `None` (the
    /// `snapshot_path` still points at the file).
    pub snapshot: Option<WorkflowRunSnapshot>,
    /// Path to the `workflows/wf_<runId>.json` snapshot file, if present.
    pub snapshot_path: Option<PathBuf>,
    /// Paths to the workflow's script source files
    /// (`workflows/scripts/<name>-wf_<runId>.js`).
    pub script_paths: Vec<PathBuf>,
    /// Eagerly-parsed contents of the run's `journal.jsonl`, if present and
    /// parseable. Collapse to `None` on parse failure (silent, matching the
    /// snapshot fall-through).
    pub journal: Option<Vec<WorkflowJournalEntry>>,
    /// Path to the run's `journal.jsonl`, if present.
    pub journal_path: Option<PathBuf>,
}

/// Metadata loaded from an agent's `.meta.json` sidecar.
///
/// Fields mirror the on-disk JSON keys (camelCase via serde rename).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMetadata {
    /// The type of agent (e.g. `code`, `research`).
    pub agent_type: Option<String>,
    /// A human-readable description of the agent's task.
    pub description: Option<String>,
    /// The worktree path associated with this agent, if any.
    pub worktree_path: Option<String>,
}

// ── Public entry points ──────────────────────────────────────────────────────

/// Scan a Claude home and return every session aggregated by id.
pub fn load_all_sessions(claude_home: &Path) -> io::Result<Vec<Session>> {
    let mut files = scanner::scan_sessions(claude_home)?;
    scanner::resolve_agent_parents(&mut files)?;

    // Partition by parent session id.
    let mut main_files: Vec<scanner::SessionFile> = Vec::new();
    let mut agent_files_by_parent: HashMap<String, Vec<scanner::SessionFile>> = HashMap::new();
    for file in files {
        if file.is_agent {
            let parent = file
                .parent_session_id
                .clone()
                .unwrap_or_else(|| file.session_id.clone());
            agent_files_by_parent.entry(parent).or_default().push(file);
        } else {
            main_files.push(file);
        }
    }

    let mut sessions = Vec::with_capacity(main_files.len());
    for main_file in main_files {
        let agent_files = agent_files_by_parent
            .remove(&main_file.session_id)
            .unwrap_or_default();
        let meta = load_meta_for(&main_file.session_id, claude_home);
        let workflows = workflows_for_session(
            &main_file.session_id,
            main_file.project.as_deref(),
            claude_home,
        );
        match assemble(&main_file, agent_files, &meta, workflows) {
            Ok(session) => sessions.push(session),
            Err(_) => continue,
        }
    }

    // Orphan agents: agent files whose parent main session is missing on
    // disk. Surface them as standalone sessions so consumers (validators,
    // analyzers) can still see them. main_path points to the non-existent
    // expected location; consumers test `.is_file()` to detect orphans.
    let mut leftover_parents: Vec<String> = agent_files_by_parent.keys().cloned().collect();
    leftover_parents.sort();
    for parent_sid in leftover_parents {
        let agent_files = agent_files_by_parent
            .remove(&parent_sid)
            .unwrap_or_default();
        if agent_files.is_empty() {
            continue;
        }
        let project = agent_files.first().and_then(|sf| sf.project.clone());
        let meta = load_meta_for(&parent_sid, claude_home);
        let workflows = workflows_for_session(&parent_sid, project.as_deref(), claude_home);
        // Synthesize an expected (non-existent) main_path so downstream tools
        // can attempt-then-skip on disk presence.
        let synthetic_main = match &project {
            Some(p) => claude_home
                .join("projects")
                .join(p)
                .join(format!("{parent_sid}.jsonl")),
            None => claude_home.join(format!("{parent_sid}.jsonl")),
        };
        let synthetic_main_file = scanner::SessionFile {
            session_id: parent_sid.clone(),
            project: project.clone(),
            path: synthetic_main,
            is_agent: false,
            parent_session_id: None,
            workflow_run_id: None,
        };
        if let Ok(session) = assemble_orphan(&synthetic_main_file, agent_files, &meta, workflows) {
            sessions.push(session);
        }
    }

    Ok(sessions)
}

/// Assemble a session whose main JSONL file does not exist on disk.
/// `main_entries` is empty; `main_path` points to where the file would live.
fn assemble_orphan(
    main_file: &scanner::SessionFile,
    agent_files: Vec<scanner::SessionFile>,
    meta_map: &HashMap<String, AgentMetadata>,
    workflows: Vec<Workflow>,
) -> io::Result<Session> {
    let mut agents = Vec::with_capacity(agent_files.len());
    for sf in agent_files {
        let entries = match load_entries(&sf.path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let parent_session_id = sf
            .parent_session_id
            .clone()
            .unwrap_or_else(|| sf.session_id.clone());
        let meta_key = sf
            .session_id
            .strip_prefix("agent-")
            .unwrap_or(&sf.session_id)
            .to_string();
        let meta = meta_map.get(&meta_key).cloned();
        agents.push(Agent {
            agent_id: sf.session_id,
            parent_session_id,
            workflow_run_id: sf.workflow_run_id,
            project: sf.project,
            path: sf.path,
            entries,
            meta,
        });
    }

    Ok(Session {
        id: main_file.session_id.clone(),
        project: main_file.project.clone(),
        main_path: main_file.path.clone(),
        main_entries: Vec::new(),
        agents,
        workflows,
    })
}

/// Load a single session by its UUID. Returns an error if the main session
/// file can't be read; agent / metadata errors are non-fatal (Agent collected
/// only when its JSONL parses).
pub fn load_session(claude_home: &Path, sid: &str) -> io::Result<Session> {
    let mut files = scanner::scan_sessions(claude_home)?;
    scanner::resolve_agent_parents(&mut files)?;

    let mut main_file: Option<scanner::SessionFile> = None;
    let mut agent_files: Vec<scanner::SessionFile> = Vec::new();
    for file in files {
        if file.is_agent {
            let parent = file.parent_session_id.as_deref().unwrap_or(&file.session_id);
            if parent == sid {
                agent_files.push(file);
            }
        } else if file.session_id == sid {
            main_file = Some(file);
        }
    }

    let main_file = main_file.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("session {sid} not found under {}", claude_home.display()),
        )
    })?;

    let meta = load_meta_for(&main_file.session_id, claude_home);
    let workflows = workflows_for_session(
        &main_file.session_id,
        main_file.project.as_deref(),
        claude_home,
    );
    assemble(&main_file, agent_files, &meta, workflows)
}

/// Read every agent `.meta.json` sidecar for a session.
///
/// Searches both `subagents/agent-*.meta.json` (ordinary subagents) and
/// `subagents/workflows/wf_*/agent-*.meta.json` (workflow agents). Keys are
/// agent ids with the `agent-` prefix stripped. On id collisions the
/// ordinary subagent entry wins.
///
/// Malformed sidecars are silently skipped (consistent with the rest of the
/// loader — sidecar files are best-effort enrichment).
pub fn load_agent_metadata(claude_home: &Path, sid: &str) -> HashMap<String, AgentMetadata> {
    load_meta_for(sid, claude_home)
}

/// Discover every workflow run for a single session, eagerly parsing each
/// snapshot and journal.
///
/// Same coverage as the `Session.workflows` field returned by
/// [`load_all_sessions`] / [`load_session`], but scoped to one session id so
/// callers can resolve workflows lazily (e.g. per-session report rendering)
/// without re-scanning every other session in the Claude home.
///
/// Returns an empty `Vec` when the session has no `workflows/` or
/// `subagents/workflows/` directories. The session does not need to exist in
/// the main session file list; this is purely a per-session workflow scan.
pub fn load_workflows_for_session(claude_home: &Path, sid: &str) -> Vec<Workflow> {
    workflows_for_session(sid, None, claude_home)
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Load metadata for a session by walking both the ordinary and workflow
/// sidecar locations. Ordinary entries win on key collisions.
fn load_meta_for(sid: &str, claude_home: &Path) -> HashMap<String, AgentMetadata> {
    let ordinary = scanner::load_agent_meta(sid, claude_home);
    let workflow = scanner::load_workflow_agent_meta(sid, claude_home);
    let mut out: HashMap<String, AgentMetadata> = HashMap::with_capacity(ordinary.len());
    for (k, v) in ordinary {
        out.insert(k, agent_meta_to_metadata(v));
    }
    for (k, v) in workflow {
        out.entry(k).or_insert_with(|| agent_meta_to_metadata(v));
    }
    out
}

fn agent_meta_to_metadata(m: scanner::AgentMeta) -> AgentMetadata {
    AgentMetadata {
        agent_type: m.agent_type,
        description: m.description,
        worktree_path: m.worktree_path,
    }
}

fn workflows_for_session(
    sid: &str,
    project: Option<&str>,
    claude_home: &Path,
) -> Vec<Workflow> {
    let runs = scanner::scan_session_workflows(sid, claude_home).unwrap_or_default();
    runs.into_iter()
        .map(|run| run_to_workflow(run, project))
        .collect()
}

/// Convert a scanner `WorkflowRun` into the public `Workflow`, eagerly parsing
/// the journal if a journal_path is set.
fn run_to_workflow(run: scanner::WorkflowRun, project_fallback: Option<&str>) -> Workflow {
    let journal = run.journal_path.as_ref().and_then(|p| read_journal(p));
    Workflow {
        run_id: run.run_id,
        session_id: run.session_id,
        // `WorkflowRun.project` is already populated by the scanner; fall back
        // to the caller-supplied project name only if the run didn't carry one.
        project: run.project.or_else(|| project_fallback.map(|p| p.to_string())),
        snapshot: run.snapshot,
        snapshot_path: run.snapshot_path,
        script_paths: run.script_paths,
        journal,
        journal_path: run.journal_path,
    }
}

/// Read a `journal.jsonl` file line-by-line, returning the parsed entries.
/// Returns `None` on any I/O failure (silently — journals are auxiliary
/// observability, never load-blocking). Individual unparseable lines are
/// dropped; the function returns `Some(Vec)` whenever the file opens.
fn read_journal(path: &Path) -> Option<Vec<WorkflowJournalEntry>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<WorkflowJournalEntry>(&line) {
            out.push(entry);
        }
    }
    Some(out)
}

/// Stitch a main session and its agent files into a `Session`.
fn assemble(
    main_file: &scanner::SessionFile,
    agent_files: Vec<scanner::SessionFile>,
    meta_map: &HashMap<String, AgentMetadata>,
    workflows: Vec<Workflow>,
) -> io::Result<Session> {
    let main_entries = load_entries(&main_file.path)?;

    let mut agents = Vec::with_capacity(agent_files.len());
    for sf in agent_files {
        let entries = match load_entries(&sf.path) {
            Ok(e) => e,
            Err(_) => continue, // skip individual unreadable agents
        };
        let parent_session_id = sf
            .parent_session_id
            .clone()
            .unwrap_or_else(|| sf.session_id.clone());
        // .meta.json key strips the "agent-" prefix.
        let meta_key = sf
            .session_id
            .strip_prefix("agent-")
            .unwrap_or(&sf.session_id)
            .to_string();
        let meta = meta_map.get(&meta_key).cloned();
        agents.push(Agent {
            agent_id: sf.session_id,
            parent_session_id,
            workflow_run_id: sf.workflow_run_id,
            project: sf.project,
            path: sf.path,
            entries,
            meta,
        });
    }

    Ok(Session {
        id: main_file.session_id.clone(),
        project: main_file.project.clone(),
        main_path: main_file.path.clone(),
        main_entries,
        agents,
        workflows,
    })
}

/// Parse a JSONL file leniently — unparseable lines are skipped, structurally
/// valid entries are kept (the same contract `RawSession` previously had).
fn load_entries(path: &Path) -> io::Result<Vec<Entry>> {
    let reader = SessionReader::open(path)?;
    Ok(reader.lenient().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_claude_home() -> TempDir {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("projects")).unwrap();
        tmp
    }

    #[test]
    fn load_simple_session() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project_dir).unwrap();

        let session_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let content = format!(
            r#"{{"type":"user","sessionId":"{session_id}","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","message":{{"role":"user","content":"hello"}}}}
{{"type":"assistant","sessionId":"{session_id}","uuid":"u2","parentUuid":"u1","timestamp":"2026-01-01T00:00:01Z","message":{{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{{"input_tokens":10,"output_tokens":20}},"content":[{{"type":"text","text":"Hi there"}}]}}}}
{{"type":"ai-title","sessionId":"{session_id}","aiTitle":"Test Session"}}
{{"type":"tag","sessionId":"{session_id}","tag":"test"}}
{{"type":"mode","sessionId":"{session_id}","mode":"code"}}"#
        );

        fs::write(project_dir.join(format!("{session_id}.jsonl")), &content).unwrap();

        let sessions = load_all_sessions(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 1);

        let session = &sessions[0];
        assert_eq!(session.id, session_id);
        assert_eq!(session.main_entries.len(), 5);
        assert!(session.agents.is_empty());
        assert!(session.workflows.is_empty());
    }

    #[test]
    fn load_session_with_agents_and_meta() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-test-proj");
        let session_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let subagents_dir = project_dir.join(session_id).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        fs::write(
            project_dir.join(format!("{session_id}.jsonl")),
            format!(r#"{{"type":"user","sessionId":"{session_id}","uuid":"u1","timestamp":"2026-01-01T00:00:00Z"}}"#),
        )
        .unwrap();
        fs::write(
            subagents_dir.join("agent-abc123.jsonl"),
            r#"{"type":"user","sessionId":"agent-abc123","uuid":"a1","timestamp":"2026-01-01T00:00:02Z"}"#,
        )
        .unwrap();
        fs::write(
            subagents_dir.join("agent-abc123.meta.json"),
            r#"{"agentType":"code","description":"Write tests","worktreePath":"/tmp/wt"}"#,
        )
        .unwrap();

        let sessions = load_all_sessions(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 1);

        let session = &sessions[0];
        assert_eq!(session.agents.len(), 1);
        let agent = &session.agents[0];
        assert_eq!(agent.agent_id, "agent-abc123");
        assert_eq!(agent.parent_session_id, session_id);
        assert!(agent.workflow_run_id.is_none());
        assert_eq!(agent.path, subagents_dir.join("agent-abc123.jsonl"));
        let meta = agent.meta.as_ref().unwrap();
        assert_eq!(meta.agent_type.as_deref(), Some("code"));
        assert_eq!(meta.description.as_deref(), Some("Write tests"));
        assert_eq!(meta.worktree_path.as_deref(), Some("/tmp/wt"));
    }

    #[test]
    fn load_empty_home() {
        let tmp = TempDir::new().unwrap();
        let sessions = load_all_sessions(tmp.path()).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn load_session_by_id() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project_dir).unwrap();
        let sid = "11111111-2222-3333-4444-555555555555";
        fs::write(
            project_dir.join(format!("{sid}.jsonl")),
            format!(r#"{{"type":"user","sessionId":"{sid}","uuid":"u1"}}"#),
        )
        .unwrap();

        let session = load_session(tmp.path(), sid).unwrap();
        assert_eq!(session.id, sid);
        assert_eq!(session.main_entries.len(), 1);
    }

    #[test]
    fn load_session_missing_returns_not_found() {
        let tmp = setup_claude_home();
        let err = load_session(tmp.path(), "nonexistent-session-id").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn load_agent_metadata_merges_ordinary_and_workflow() {
        let tmp = setup_claude_home();
        let sid = "22222222-2222-3333-4444-555555555555";
        let proj = tmp.path().join("projects").join("-Users-test-meta");
        let subagents = proj.join(sid).join("subagents");
        let wf_run = subagents.join("workflows").join("wf_meta01");
        fs::create_dir_all(&subagents).unwrap();
        fs::create_dir_all(&wf_run).unwrap();

        // Ordinary
        fs::write(
            subagents.join("agent-ord.meta.json"),
            r#"{"agentType":"ordinary","description":"o"}"#,
        )
        .unwrap();
        // Workflow
        fs::write(
            wf_run.join("agent-wf.meta.json"),
            r#"{"agentType":"workflow","description":"w"}"#,
        )
        .unwrap();

        let map = load_agent_metadata(tmp.path(), sid);
        assert_eq!(map.len(), 2);
        assert_eq!(map["ord"].agent_type.as_deref(), Some("ordinary"));
        assert_eq!(map["wf"].agent_type.as_deref(), Some("workflow"));
    }

    #[test]
    fn workflow_agents_grouped_under_session_and_tagged() {
        let tmp = setup_claude_home();
        let sid = "33333333-2222-3333-4444-555555555555";
        let proj = tmp.path().join("projects").join("-Users-test-wf");
        let wf_run = proj
            .join(sid)
            .join("subagents")
            .join("workflows")
            .join("wf_run01");
        let wf_dir = proj.join(sid).join("workflows");
        fs::create_dir_all(&wf_run).unwrap();
        fs::create_dir_all(&wf_dir).unwrap();

        fs::write(
            proj.join(format!("{sid}.jsonl")),
            format!(r#"{{"type":"user","sessionId":"{sid}","uuid":"u1"}}"#),
        )
        .unwrap();
        fs::write(
            wf_dir.join("wf_run01.json"),
            r#"{"runId":"wf_run01","workflowName":"demo","agentCount":1}"#,
        )
        .unwrap();
        fs::write(
            wf_run.join("agent-wf-a.jsonl"),
            r#"{"type":"user","sessionId":"sub","uuid":"x"}"#,
        )
        .unwrap();
        fs::write(
            wf_run.join("journal.jsonl"),
            r#"{"type":"started","key":"k","agentId":"wf-a"}"#,
        )
        .unwrap();

        let sessions = load_all_sessions(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(s.agents.len(), 1);
        let agent = &s.agents[0];
        assert_eq!(agent.workflow_run_id.as_deref(), Some("wf_run01"));
        assert_eq!(agent.parent_session_id, sid);

        assert_eq!(s.workflows.len(), 1);
        let wf = &s.workflows[0];
        assert_eq!(wf.run_id, "wf_run01");
        assert_eq!(wf.session_id, sid);
        assert!(wf.snapshot.is_some());
        // Journal eagerly parsed.
        let journal = wf.journal.as_ref().unwrap();
        assert_eq!(journal.len(), 1);
        assert_eq!(journal[0].kind.as_deref(), Some("started"));
    }

    #[test]
    fn malformed_journal_collapses_to_empty_vec_not_none() {
        // The journal file exists and opens, but every line is garbage —
        // read_journal returns Some(empty) (Some because the file opened).
        let tmp = setup_claude_home();
        let sid = "44444444-2222-3333-4444-555555555555";
        let proj = tmp.path().join("projects").join("-Users-test-badjournal");
        let wf_run = proj
            .join(sid)
            .join("subagents")
            .join("workflows")
            .join("wf_bj");
        fs::create_dir_all(&wf_run).unwrap();

        fs::write(
            proj.join(format!("{sid}.jsonl")),
            format!(r#"{{"type":"user","sessionId":"{sid}","uuid":"u1"}}"#),
        )
        .unwrap();
        fs::write(
            wf_run.join("agent-x.jsonl"),
            r#"{"type":"user","uuid":"x"}"#,
        )
        .unwrap();
        fs::write(wf_run.join("journal.jsonl"), b"garbage line\n{also bad\n").unwrap();

        let sessions = load_all_sessions(tmp.path()).unwrap();
        let s = &sessions[0];
        let wf = &s.workflows[0];
        assert!(wf.journal.is_some());
        assert!(wf.journal.as_ref().unwrap().is_empty());
    }

    #[test]
    fn missing_journal_yields_none() {
        let tmp = setup_claude_home();
        let sid = "55555555-2222-3333-4444-555555555555";
        let proj = tmp.path().join("projects").join("-Users-test-nojournal");
        let wf_run = proj
            .join(sid)
            .join("subagents")
            .join("workflows")
            .join("wf_nj");
        fs::create_dir_all(&wf_run).unwrap();

        fs::write(
            proj.join(format!("{sid}.jsonl")),
            format!(r#"{{"type":"user","sessionId":"{sid}","uuid":"u1"}}"#),
        )
        .unwrap();
        fs::write(
            wf_run.join("agent-x.jsonl"),
            r#"{"type":"user","uuid":"x"}"#,
        )
        .unwrap();
        // No journal.jsonl.

        let sessions = load_all_sessions(tmp.path()).unwrap();
        let wf = &sessions[0].workflows[0];
        assert!(wf.journal.is_none());
        assert!(wf.journal_path.is_none());
    }
}
