//! Types for Claude Code workflow data (Claude Code 2.1.159+).
//!
//! Workflow data lives entirely outside the main session JSONL, in a directory
//! named after the session UUID:
//!
//! ```text
//! <claude_home>/projects/<project>/<session-id>/
//! ├── workflows/wf_<runId>.json            # run state snapshot (WorkflowRunSnapshot)
//! ├── workflows/scripts/<name>-wf_<runId>.js
//! ├── subagents/workflows/wf_<runId>/agent-<id>.jsonl       # full agent transcript
//! ├── subagents/workflows/wf_<runId>/agent-<id>.meta.json   # {"agentType": "..."}
//! └── subagents/workflows/wf_<runId>/journal.jsonl          # WorkflowJournalEntry per line
//! ```
//!
//! Agent transcripts reuse the regular [`crate::types::Entry`] / `SessionReader`
//! parsing path (they are plain session JSONL files, including `usage` tokens).
//! These types only model the workflow-specific snapshot and journal files.

use serde::{Deserialize, Serialize};

/// A workflow run state snapshot, parsed from `workflows/wf_<runId>.json`.
///
/// All fields are `Option` for version compatibility. Free-form / shape-varying
/// fields (`args`, `result`, `logs`) are kept as `serde_json::Value` because they
/// differ per run (e.g. `args` may be a string or an object, `result` may be a
/// string or an object).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunSnapshot {
    /// The workflow run identifier, e.g. `wf_7c0e6255-566`.
    pub run_id: Option<String>,
    /// The task identifier this run is associated with.
    pub task_id: Option<String>,
    /// Human-readable workflow name (matches the script file prefix).
    pub workflow_name: Option<String>,
    /// ISO-8601 timestamp of the snapshot.
    pub timestamp: Option<String>,
    /// Run status, e.g. `completed`, `running`, `failed`.
    pub status: Option<String>,
    /// Full source of the workflow script (JS), inlined.
    pub script: Option<String>,
    /// Absolute path to the script file on disk.
    pub script_path: Option<String>,
    /// Arguments passed to the run. Shape varies (string or object) → `Value`.
    pub args: Option<serde_json::Value>,
    /// The default model used when an `agent()` call does not specify one.
    pub default_model: Option<String>,
    /// Start time as a Unix epoch milliseconds value.
    pub start_time: Option<u64>,
    /// Total wall-clock duration of the run, in milliseconds.
    pub duration_ms: Option<u64>,
    /// Number of `agent()` invocations in this run.
    pub agent_count: Option<u64>,
    /// Aggregate token count reported for the run.
    pub total_tokens: Option<u64>,
    /// Aggregate tool-call count reported for the run.
    pub total_tool_calls: Option<u64>,
    /// Declared phases of the workflow.
    pub phases: Option<Vec<WorkflowPhase>>,
    /// Per-step progress records (phase markers + per-agent progress).
    pub workflow_progress: Option<Vec<WorkflowProgress>>,
    /// Run-level log lines. Shape varies → `Value`.
    pub logs: Option<Vec<serde_json::Value>>,
    /// The run result. Shape varies (string or object) → `Value`.
    pub result: Option<serde_json::Value>,
    /// A short human-readable summary of the run.
    pub summary: Option<String>,
}

/// A declared phase of a workflow (from `phases[]`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPhase {
    /// The phase title.
    pub title: Option<String>,
    /// A longer description of what the phase does.
    pub detail: Option<String>,
}

/// A single progress record from `workflowProgress[]`.
///
/// Real data contains two shapes under this array:
/// 1. Phase markers: `{type: "workflow_phase", index, title}`.
/// 2. Per-agent progress: `{type: "workflow_agent", index, label, agentId, ...}`
///    (rich, with many extra fields).
///
/// This struct models the common identifying fields. Extra fields present on the
/// richer per-agent records are ignored (no `deny_unknown_fields`), keeping the
/// type forward-compatible. The richer fields most useful downstream are surfaced
/// as `Option`; anything not modelled here is simply dropped.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowProgress {
    /// The record kind, e.g. `workflow_phase` or `workflow_agent`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// 1-based index of the record within its kind.
    pub index: Option<u64>,
    /// Title (present on `workflow_phase` records).
    pub title: Option<String>,
    /// Label (present on `workflow_agent` records).
    pub label: Option<String>,
    /// The agent id (present on `workflow_agent` records; no `agent-` prefix).
    pub agent_id: Option<String>,
    /// The agent type (present on `workflow_agent` records).
    pub agent_type: Option<String>,
    /// The model used for this agent (present on `workflow_agent` records).
    pub model: Option<String>,
    /// The agent's run state, e.g. `done` (present on `workflow_agent` records).
    pub state: Option<String>,
    /// Tokens reported for this agent (present on `workflow_agent` records).
    pub tokens: Option<u64>,
    /// Tool calls reported for this agent (present on `workflow_agent` records).
    pub tool_calls: Option<u64>,
    /// Duration of this agent, in milliseconds (present on `workflow_agent` records).
    pub duration_ms: Option<u64>,
}

/// A single line from a workflow `journal.jsonl` file.
///
/// Each agent invocation produces a `started` record and (on completion) a
/// `result` record sharing the same `key`. The `result` field shape varies
/// between runs (string or object) → `serde_json::Value`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowJournalEntry {
    /// The record kind: `started` or `result`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// A content-addressed key, e.g. `v2:<sha256>`, shared by the `started`
    /// and `result` records of the same agent invocation.
    pub key: Option<String>,
    /// The agent id this record refers to (no `agent-` prefix).
    pub agent_id: Option<String>,
    /// The agent's returned result (only on `result` records). Shape varies → `Value`.
    pub result: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_run_snapshot_full() {
        // Mirrors the real wf_<runId>.json shape: args is a string, result is an object.
        let json = r#"{
            "runId": "wf_7c0e6255-566",
            "taskId": "wogtti2n7",
            "workflowName": "session-review-self",
            "timestamp": "2026-06-02T04:53:04.428Z",
            "status": "completed",
            "script": "export const meta = {}",
            "scriptPath": "/path/to/script.js",
            "args": "some string args",
            "defaultModel": "claude-opus-4-8[1m]",
            "startTime": 1780375441852,
            "durationMs": 542575,
            "agentCount": 9,
            "totalTokens": 784299,
            "totalToolCalls": 302,
            "phases": [
                {"title": "预处理", "detail": "preprocessor 解析 13M JSONL"},
                {"title": "多维分析", "detail": "7 个维度子 agent 并行分析"}
            ],
            "workflowProgress": [
                {"type": "workflow_phase", "index": 1, "title": "预处理"},
                {"type": "workflow_agent", "index": 1, "label": "preprocess", "agentId": "a4df3aac3c00e0e09", "agentType": "session-review:preprocessor", "model": "claude-sonnet-4-6", "state": "done", "tokens": 12454, "toolCalls": 8, "durationMs": 45857, "phaseIndex": 1, "phaseTitle": "预处理"}
            ],
            "logs": [],
            "result": {"final": "report text", "dimsDone": ["a", "b"]},
            "summary": "对本会话做 session-review"
        }"#;

        let snap: WorkflowRunSnapshot = serde_json::from_str(json).unwrap();
        assert_eq!(snap.run_id.as_deref(), Some("wf_7c0e6255-566"));
        assert_eq!(snap.task_id.as_deref(), Some("wogtti2n7"));
        assert_eq!(snap.workflow_name.as_deref(), Some("session-review-self"));
        assert_eq!(snap.status.as_deref(), Some("completed"));
        assert_eq!(snap.default_model.as_deref(), Some("claude-opus-4-8[1m]"));
        assert_eq!(snap.start_time, Some(1780375441852));
        assert_eq!(snap.duration_ms, Some(542575));
        assert_eq!(snap.agent_count, Some(9));
        assert_eq!(snap.total_tokens, Some(784299));
        assert_eq!(snap.total_tool_calls, Some(302));
        assert_eq!(snap.summary.as_deref(), Some("对本会话做 session-review"));

        // args is a plain string in real data
        assert!(snap.args.as_ref().unwrap().is_string());
        // result is an object in this run
        assert!(snap.result.as_ref().unwrap().is_object());
        // logs parses (empty array)
        assert_eq!(snap.logs.as_ref().unwrap().len(), 0);

        let phases = snap.phases.as_ref().unwrap();
        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0].title.as_deref(), Some("预处理"));
        assert_eq!(
            phases[0].detail.as_deref(),
            Some("preprocessor 解析 13M JSONL")
        );

        let progress = snap.workflow_progress.as_ref().unwrap();
        assert_eq!(progress.len(), 2);
        // phase marker
        assert_eq!(progress[0].kind.as_deref(), Some("workflow_phase"));
        assert_eq!(progress[0].index, Some(1));
        assert_eq!(progress[0].title.as_deref(), Some("预处理"));
        // agent record (rich fields surface; phaseIndex/phaseTitle dropped silently)
        assert_eq!(progress[1].kind.as_deref(), Some("workflow_agent"));
        assert_eq!(progress[1].label.as_deref(), Some("preprocess"));
        assert_eq!(progress[1].agent_id.as_deref(), Some("a4df3aac3c00e0e09"));
        assert_eq!(
            progress[1].agent_type.as_deref(),
            Some("session-review:preprocessor")
        );
        assert_eq!(progress[1].model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(progress[1].state.as_deref(), Some("done"));
        assert_eq!(progress[1].tokens, Some(12454));
        assert_eq!(progress[1].tool_calls, Some(8));
        assert_eq!(progress[1].duration_ms, Some(45857));
    }

    #[test]
    fn parse_run_snapshot_result_as_string() {
        // In other runs the journal/result is a plain string — must still parse.
        let json = r#"{
            "runId": "wf_x",
            "args": {"jsonl": "/path", "outdir": "/out"},
            "result": "a plain string result",
            "logs": [{"level": "info", "msg": "hi"}]
        }"#;
        let snap: WorkflowRunSnapshot = serde_json::from_str(json).unwrap();
        assert!(snap.args.as_ref().unwrap().is_object());
        assert!(snap.result.as_ref().unwrap().is_string());
        assert_eq!(snap.logs.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn parse_run_snapshot_minimal() {
        // A near-empty snapshot must parse with all fields None.
        let snap: WorkflowRunSnapshot = serde_json::from_str("{}").unwrap();
        assert!(snap.run_id.is_none());
        assert!(snap.phases.is_none());
        assert!(snap.workflow_progress.is_none());
        assert!(snap.result.is_none());
    }

    #[test]
    fn parse_journal_started() {
        let json = r#"{"type":"started","key":"v2:ca77ec26","agentId":"a4df3aac3c00e0e09"}"#;
        let entry: WorkflowJournalEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.kind.as_deref(), Some("started"));
        assert_eq!(entry.key.as_deref(), Some("v2:ca77ec26"));
        assert_eq!(entry.agent_id.as_deref(), Some("a4df3aac3c00e0e09"));
        assert!(entry.result.is_none());
    }

    #[test]
    fn parse_journal_result_string() {
        let json = r#"{"type":"result","key":"v2:ca77ec26","agentId":"a4df3aac3c00e0e09","result":"Preprocessing succeeded."}"#;
        let entry: WorkflowJournalEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.kind.as_deref(), Some("result"));
        assert!(entry.result.as_ref().unwrap().is_string());
        assert_eq!(
            entry.result.as_ref().unwrap().as_str(),
            Some("Preprocessing succeeded.")
        );
    }

    #[test]
    fn parse_journal_result_object() {
        // In some runs the result is an object — must still parse.
        let json = r#"{"type":"result","key":"v2:abc","agentId":"a1","result":{"report":"...","ok":true}}"#;
        let entry: WorkflowJournalEntry = serde_json::from_str(json).unwrap();
        assert!(entry.result.as_ref().unwrap().is_object());
        assert_eq!(entry.result.as_ref().unwrap()["ok"], true);
    }
}
