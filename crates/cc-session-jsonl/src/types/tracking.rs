use serde::{Deserialize, Serialize};

/// A queue operation entry (enqueue/dequeue/remove/popAll).
///
/// Reference: Claude Code `messageQueueManager.ts:30-36`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueOperationMessage {
    pub session_id: Option<String>,
    pub operation: Option<String>,
    pub timestamp: Option<String>,
    pub content: Option<String>,
}

/// A speculation accept entry, recorded when speculative execution is accepted.
///
/// Reference: Claude Code `logs.ts:233-237`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeculationAcceptMessage {
    pub timestamp: Option<String>,
    pub time_saved_ms: Option<f64>,
}

/// A worktree state entry, tracking persisted worktree session state.
/// `worktree_session` is nullable: `null` means exited worktree, `None` means field absent.
///
/// Reference: Claude Code `logs.ts:167-171`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeStateEntry {
    pub session_id: Option<String>,
    pub worktree_session: Option<PersistedWorktreeSession>,
}

/// A persisted worktree session record.
///
/// Reference: Claude Code `logs.ts:149-159`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWorktreeSession {
    pub original_cwd: Option<String>,
    pub worktree_path: Option<String>,
    pub worktree_name: Option<String>,
    pub worktree_branch: Option<String>,
    pub original_branch: Option<String>,
    pub original_head_commit: Option<String>,
    pub session_id: Option<String>,
    pub tmux_session_name: Option<String>,
    pub hook_based: Option<bool>,
}

/// A content replacement entry, recording when large tool results were replaced with stubs.
///
/// Reference: Claude Code `logs.ts:181-186`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentReplacementEntry {
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub replacements: Option<Vec<ContentReplacementRecord>>,
}

/// A single content replacement record.
///
/// Reference: Claude Code `toolResultStorage.ts`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentReplacementRecord {
    pub kind: Option<String>,
    pub tool_use_id: Option<String>,
    pub replacement: Option<String>,
}

/// A file history snapshot entry.
///
/// Reference: Claude Code `logs.ts:188-193`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistorySnapshotMessage {
    pub message_id: Option<String>,
    pub snapshot: Option<serde_json::Value>,
    pub is_snapshot_update: Option<bool>,
}

/// An attribution snapshot entry, tracking Claude's character contributions.
///
/// Reference: Claude Code `logs.ts:208-219`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionSnapshotMessage {
    pub message_id: Option<String>,
    pub surface: Option<String>,
    pub file_states: Option<serde_json::Value>,
    pub prompt_count: Option<u64>,
    pub prompt_count_at_last_commit: Option<u64>,
    pub permission_prompt_count: Option<u64>,
    pub permission_prompt_count_at_last_commit: Option<u64>,
    pub escape_count: Option<u64>,
    pub escape_count_at_last_commit: Option<u64>,
}

#[cfg(test)]
mod tests {
    use crate::types::Entry;

    #[test]
    fn parse_queue_operation_enqueue_with_content() {
        let json = r#"{
            "type": "queue-operation",
            "sessionId": "sess-q-001",
            "operation": "enqueue",
            "timestamp": "2026-03-16T19:00:00Z",
            "content": "fix the bug"
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::QueueOperation(qo) => {
                assert_eq!(qo.session_id.as_deref(), Some("sess-q-001"));
                assert_eq!(qo.operation.as_deref(), Some("enqueue"));
                assert_eq!(qo.content.as_deref(), Some("fix the bug"));
            }
            other => panic!("Expected QueueOperation, got: {other:?}"),
        }
    }

    #[test]
    fn parse_queue_operation_dequeue_no_content() {
        let json = r#"{
            "type": "queue-operation",
            "sessionId": "sess-q-002",
            "operation": "dequeue",
            "timestamp": "2026-03-16T19:01:00Z"
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::QueueOperation(qo) => {
                assert_eq!(qo.operation.as_deref(), Some("dequeue"));
                assert!(qo.content.is_none());
            }
            other => panic!("Expected QueueOperation, got: {other:?}"),
        }
    }

    #[test]
    fn parse_speculation_accept() {
        let json = r#"{
            "type": "speculation-accept",
            "timestamp": "2026-03-16T19:10:00Z",
            "timeSavedMs": 1234.5
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::SpeculationAccept(sa) => {
                assert_eq!(sa.timestamp.as_deref(), Some("2026-03-16T19:10:00Z"));
                assert!((sa.time_saved_ms.unwrap() - 1234.5).abs() < f64::EPSILON);
            }
            other => panic!("Expected SpeculationAccept, got: {other:?}"),
        }
    }

    #[test]
    fn parse_worktree_state_with_session() {
        let json = r#"{
            "type": "worktree-state",
            "sessionId": "sess-wt-001",
            "worktreeSession": {
                "originalCwd": "/Users/loki/project",
                "worktreePath": "/tmp/worktree-a",
                "worktreeName": "wt-feature",
                "worktreeBranch": "feature-a",
                "originalBranch": "main",
                "originalHeadCommit": "abc123",
                "sessionId": "wt-sess-a",
                "tmuxSessionName": "tmux-wt",
                "hookBased": false
            }
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::WorktreeState(ws) => {
                assert_eq!(ws.session_id.as_deref(), Some("sess-wt-001"));
                let wt = ws.worktree_session.as_ref().unwrap();
                assert_eq!(wt.original_cwd.as_deref(), Some("/Users/loki/project"));
                assert_eq!(wt.worktree_path.as_deref(), Some("/tmp/worktree-a"));
                assert_eq!(wt.worktree_name.as_deref(), Some("wt-feature"));
                assert_eq!(wt.worktree_branch.as_deref(), Some("feature-a"));
                assert_eq!(wt.original_branch.as_deref(), Some("main"));
                assert_eq!(wt.original_head_commit.as_deref(), Some("abc123"));
                assert_eq!(wt.tmux_session_name.as_deref(), Some("tmux-wt"));
                assert_eq!(wt.hook_based, Some(false));
            }
            other => panic!("Expected WorktreeState, got: {other:?}"),
        }
    }

    #[test]
    fn parse_worktree_state_null_session() {
        let json = r#"{
            "type": "worktree-state",
            "sessionId": "sess-wt-002",
            "worktreeSession": null
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::WorktreeState(ws) => {
                assert!(ws.worktree_session.is_none());
            }
            other => panic!("Expected WorktreeState, got: {other:?}"),
        }
    }

    #[test]
    fn parse_content_replacement() {
        let json = r#"{
            "type": "content-replacement",
            "sessionId": "sess-cr-001",
            "agentId": "agent-abc",
            "replacements": [
                {
                    "kind": "tool-result",
                    "toolUseId": "toolu_123",
                    "replacement": "[content stored externally]"
                }
            ]
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::ContentReplacement(cr) => {
                assert_eq!(cr.session_id.as_deref(), Some("sess-cr-001"));
                assert_eq!(cr.agent_id.as_deref(), Some("agent-abc"));
                let repls = cr.replacements.as_ref().unwrap();
                assert_eq!(repls.len(), 1);
                assert_eq!(repls[0].kind.as_deref(), Some("tool-result"));
                assert_eq!(repls[0].tool_use_id.as_deref(), Some("toolu_123"));
                assert_eq!(
                    repls[0].replacement.as_deref(),
                    Some("[content stored externally]")
                );
            }
            other => panic!("Expected ContentReplacement, got: {other:?}"),
        }
    }

    #[test]
    fn parse_file_history_snapshot() {
        let json = r#"{
            "type": "file-history-snapshot",
            "messageId": "msg-fh-001",
            "snapshot": {"trackedFileBackups": {}},
            "isSnapshotUpdate": true
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::FileHistorySnapshot(fh) => {
                assert_eq!(fh.message_id.as_deref(), Some("msg-fh-001"));
                assert_eq!(fh.is_snapshot_update, Some(true));
                assert!(fh.snapshot.is_some());
            }
            other => panic!("Expected FileHistorySnapshot, got: {other:?}"),
        }
    }

    #[test]
    fn parse_attribution_snapshot() {
        let json = r#"{
            "type": "attribution-snapshot",
            "messageId": "msg-as-001",
            "surface": "cli",
            "fileStates": {"/tmp/main.rs": {"contentHash": "abc", "claudeContribution": 500, "mtime": 1710601200}},
            "promptCount": 15,
            "promptCountAtLastCommit": 10,
            "permissionPromptCount": 3,
            "permissionPromptCountAtLastCommit": 2,
            "escapeCount": 1,
            "escapeCountAtLastCommit": 0
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::AttributionSnapshot(a) => {
                assert_eq!(a.message_id.as_deref(), Some("msg-as-001"));
                assert_eq!(a.surface.as_deref(), Some("cli"));
                assert!(a.file_states.is_some());
                assert_eq!(a.prompt_count, Some(15));
                assert_eq!(a.prompt_count_at_last_commit, Some(10));
                assert_eq!(a.permission_prompt_count, Some(3));
                assert_eq!(a.permission_prompt_count_at_last_commit, Some(2));
                assert_eq!(a.escape_count, Some(1));
                assert_eq!(a.escape_count_at_last_commit, Some(0));
            }
            other => panic!("Expected AttributionSnapshot, got: {other:?}"),
        }
    }
}
