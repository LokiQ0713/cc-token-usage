//! Session loading: combines scanner results with parsed JSONL entries
//! into a structured `RawSession` representation.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use crate::parser::SessionReader;
use crate::scanner::{self, AgentMeta, SessionFile};
use crate::types::Entry;

/// A raw session with all its entries and metadata, ready for downstream analysis.
#[derive(Debug, Clone)]
pub struct RawSession {
    /// The main session UUID.
    pub id: String,
    /// The project directory name.
    pub project: Option<String>,
    /// All entries from the main session JSONL file.
    pub main_entries: Vec<Entry>,
    /// Agent files associated with this session.
    pub agent_files: Vec<AgentFile>,
    /// Titles extracted from `ai-title` and `custom-title` entries.
    pub titles: Vec<String>,
    /// Tags extracted from `tag` entries.
    pub tags: Vec<String>,
    /// The last mode set via a `mode` entry.
    pub mode: Option<String>,
}

/// A sub-agent's entries and metadata within a session.
#[derive(Debug, Clone)]
pub struct AgentFile {
    /// The agent identifier (e.g., `agent-abc123`).
    pub agent_id: String,
    /// All entries from this agent's JSONL file.
    pub entries: Vec<Entry>,
    /// Metadata loaded from the `.meta.json` sidecar file, if available.
    pub meta: Option<AgentMeta>,
}

/// Load entries from a single JSONL file, skipping unparseable lines.
fn load_entries(path: &Path) -> io::Result<Vec<Entry>> {
    let reader = SessionReader::open(path)?;
    Ok(reader.lenient().collect())
}

/// Extract metadata (titles, tags, mode) from a list of entries.
fn extract_metadata(entries: &[Entry]) -> (Vec<String>, Vec<String>, Option<String>) {
    let mut titles = Vec::new();
    let mut tags = Vec::new();
    let mut mode = None;

    for entry in entries {
        match entry {
            Entry::AiTitle(msg) => {
                if let Some(ref t) = msg.ai_title {
                    titles.push(t.clone());
                }
            }
            Entry::CustomTitle(msg) => {
                if let Some(ref t) = msg.custom_title {
                    titles.push(t.clone());
                }
            }
            Entry::Tag(msg) => {
                if let Some(ref t) = msg.tag {
                    tags.push(t.clone());
                }
            }
            Entry::Mode(msg) => {
                if let Some(ref m) = msg.mode {
                    mode = Some(m.clone());
                }
            }
            _ => {}
        }
    }

    (titles, tags, mode)
}

/// Load a single session from its `SessionFile` metadata and associated agent files.
///
/// `agent_session_files` should contain only the agent files that belong to this session.
/// `agent_meta_map` provides metadata loaded from `.meta.json` sidecar files.
pub fn load_session(
    main_file: &SessionFile,
    agent_session_files: &[&SessionFile],
    agent_meta_map: &HashMap<String, AgentMeta>,
) -> io::Result<RawSession> {
    let main_entries = load_entries(&main_file.path)?;
    let (titles, tags, mode) = extract_metadata(&main_entries);

    let mut agent_files = Vec::new();
    for agent_sf in agent_session_files {
        let entries = load_entries(&agent_sf.path)?;
        agent_files.push(AgentFile {
            agent_id: agent_sf.session_id.clone(),
            entries,
            meta: agent_meta_map
                .get(agent_sf.session_id.trim_start_matches("agent-"))
                .cloned(),
        });
    }

    Ok(RawSession {
        id: main_file.session_id.clone(),
        project: main_file.project.clone(),
        main_entries,
        agent_files,
        titles,
        tags,
        mode,
    })
}

/// Scan and load all sessions from a Claude home directory.
///
/// This is a convenience function that combines `scan_sessions`,
/// `resolve_agent_parents`, `load_agent_meta`, and `load_session`.
pub fn load_all_sessions(claude_home: &Path) -> io::Result<Vec<RawSession>> {
    let mut files = scanner::scan_sessions(claude_home)?;
    scanner::resolve_agent_parents(&mut files)?;

    // Group agent files by parent session ID
    let mut agent_map: HashMap<String, Vec<usize>> = HashMap::new();
    let mut main_indices: Vec<usize> = Vec::new();

    for (idx, file) in files.iter().enumerate() {
        if file.is_agent {
            if let Some(ref parent) = file.parent_session_id {
                agent_map.entry(parent.clone()).or_default().push(idx);
            }
        } else {
            main_indices.push(idx);
        }
    }

    let mut sessions = Vec::new();
    for main_idx in main_indices {
        let main_file = &files[main_idx];
        let meta_map = scanner::load_agent_meta(&main_file.session_id, claude_home);

        let agent_refs: Vec<&SessionFile> = agent_map
            .get(&main_file.session_id)
            .map(|indices| indices.iter().map(|&i| &files[i]).collect())
            .unwrap_or_default();

        match load_session(main_file, &agent_refs, &meta_map) {
            Ok(session) => sessions.push(session),
            Err(_) => continue, // Skip sessions that fail to load
        }
    }

    Ok(sessions)
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
    fn load_simple_session() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-test-proj");
        fs::create_dir_all(&project_dir).unwrap();

        let session_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let content = format!(
            r#"{{"type":"user","sessionId":"{session_id}","uuid":"u1","timestamp":"2026-01-01T00:00:00Z","message":{{"role":"user","content":"hello"}}}}
{{"type":"assistant","sessionId":"{session_id}","uuid":"u2","timestamp":"2026-01-01T00:00:01Z","message":{{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{{"input_tokens":10,"output_tokens":20}},"content":[{{"type":"text","text":"Hi there"}}]}}}}
{{"type":"ai-title","sessionId":"{session_id}","aiTitle":"Test Session"}}
{{"type":"tag","sessionId":"{session_id}","tag":"test"}}
{{"type":"mode","sessionId":"{session_id}","mode":"code"}}"#
        );

        fs::write(
            project_dir.join(format!("{session_id}.jsonl")),
            &content,
        )
        .unwrap();

        let sessions = load_all_sessions(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 1);

        let session = &sessions[0];
        assert_eq!(session.id, session_id);
        assert_eq!(session.main_entries.len(), 5);
        assert_eq!(session.titles, vec!["Test Session"]);
        assert_eq!(session.tags, vec!["test"]);
        assert_eq!(session.mode.as_deref(), Some("code"));
        assert!(session.agent_files.is_empty());
    }

    #[test]
    fn load_session_with_agents() {
        let tmp = setup_claude_home();
        let project_dir = tmp.path().join("projects").join("-Users-test-proj");
        let session_id = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let subagents_dir = project_dir.join(session_id).join("subagents");
        fs::create_dir_all(&subagents_dir).unwrap();

        // Main session
        fs::write(
            project_dir.join(format!("{session_id}.jsonl")),
            format!(r#"{{"type":"user","sessionId":"{session_id}","uuid":"u1","timestamp":"2026-01-01T00:00:00Z"}}"#),
        )
        .unwrap();

        // Agent file
        fs::write(
            subagents_dir.join("agent-abc123.jsonl"),
            format!(r#"{{"type":"user","sessionId":"agent-abc123","uuid":"a1","timestamp":"2026-01-01T00:00:02Z"}}"#),
        )
        .unwrap();

        // Agent meta
        fs::write(
            subagents_dir.join("agent-abc123.meta.json"),
            r#"{"agentType":"code","description":"Write tests"}"#,
        )
        .unwrap();

        let sessions = load_all_sessions(tmp.path()).unwrap();
        assert_eq!(sessions.len(), 1);

        let session = &sessions[0];
        assert_eq!(session.agent_files.len(), 1);
        assert_eq!(session.agent_files[0].agent_id, "agent-abc123");
        assert!(session.agent_files[0].meta.is_some());
        let meta = session.agent_files[0].meta.as_ref().unwrap();
        assert_eq!(meta.agent_type.as_deref(), Some("code"));
    }

    #[test]
    fn load_empty_home() {
        let tmp = TempDir::new().unwrap();
        let sessions = load_all_sessions(tmp.path()).unwrap();
        assert!(sessions.is_empty());
    }
}
