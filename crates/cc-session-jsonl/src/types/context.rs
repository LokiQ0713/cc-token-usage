use serde::{Deserialize, Serialize};

/// A context collapse commit entry (marble-origami-commit).
///
/// Records a committed context collapse — the boundary UUIDs and summary placeholder
/// needed to reconstruct the splice on resume.
///
/// Reference: Claude Code `logs.ts:255-269`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCollapseCommitEntry {
    pub session_id: Option<String>,
    pub collapse_id: Option<String>,
    pub summary_uuid: Option<String>,
    pub summary_content: Option<String>,
    pub summary: Option<String>,
    pub first_archived_uuid: Option<String>,
    pub last_archived_uuid: Option<String>,
}

/// A context collapse snapshot entry (marble-origami-snapshot).
///
/// Snapshot of staged queue and spawn trigger state. Last-wins on restore.
///
/// Reference: Claude Code `logs.ts:282-295`
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCollapseSnapshotEntry {
    pub session_id: Option<String>,
    pub staged: Option<Vec<StagedSpan>>,
    pub armed: Option<bool>,
    pub last_spawn_tokens: Option<u64>,
}

/// A span of conversation turns that has been staged for context collapse.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedSpan {
    pub start_uuid: Option<String>,
    pub end_uuid: Option<String>,
    pub summary: Option<String>,
    pub risk: Option<f64>,
    pub staged_at: Option<u64>,
}

#[cfg(test)]
mod tests {
    use crate::types::Entry;

    #[test]
    fn parse_context_collapse_commit() {
        let json = r#"{
            "type": "marble-origami-commit",
            "sessionId": "sess-cc-001",
            "collapseId": "0000000000000001",
            "summaryUuid": "sum-uuid-001",
            "summaryContent": "<collapsed id=\"0000000000000001\">User asked about lifetimes</collapsed>",
            "summary": "User asked about lifetimes",
            "firstArchivedUuid": "arch-first-001",
            "lastArchivedUuid": "arch-last-001"
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::ContextCollapseCommit(cc) => {
                assert_eq!(cc.session_id.as_deref(), Some("sess-cc-001"));
                assert_eq!(cc.collapse_id.as_deref(), Some("0000000000000001"));
                assert_eq!(cc.summary_uuid.as_deref(), Some("sum-uuid-001"));
                assert!(cc.summary_content.as_ref().unwrap().contains("collapsed"));
                assert_eq!(cc.summary.as_deref(), Some("User asked about lifetimes"));
                assert_eq!(cc.first_archived_uuid.as_deref(), Some("arch-first-001"));
                assert_eq!(cc.last_archived_uuid.as_deref(), Some("arch-last-001"));
            }
            other => panic!("Expected ContextCollapseCommit, got: {other:?}"),
        }
    }

    #[test]
    fn parse_context_collapse_snapshot() {
        let json = r#"{
            "type": "marble-origami-snapshot",
            "sessionId": "sess-cc-002",
            "staged": [
                {
                    "startUuid": "span-start-1",
                    "endUuid": "span-end-1",
                    "summary": "Collapsed context about lifetimes.",
                    "risk": 0.15,
                    "stagedAt": 1710601200000
                }
            ],
            "armed": true,
            "lastSpawnTokens": 50000
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::ContextCollapseSnapshot(cs) => {
                assert_eq!(cs.session_id.as_deref(), Some("sess-cc-002"));
                assert_eq!(cs.armed, Some(true));
                assert_eq!(cs.last_spawn_tokens, Some(50000));
                let spans = cs.staged.as_ref().unwrap();
                assert_eq!(spans.len(), 1);
                assert_eq!(spans[0].start_uuid.as_deref(), Some("span-start-1"));
                assert!((spans[0].risk.unwrap() - 0.15).abs() < f64::EPSILON);
            }
            other => panic!("Expected ContextCollapseSnapshot, got: {other:?}"),
        }
    }

    #[test]
    fn parse_context_collapse_snapshot_empty_staged() {
        let json = r#"{
            "type": "marble-origami-snapshot",
            "sessionId": "sess-cc-003",
            "staged": [],
            "armed": false,
            "lastSpawnTokens": 0
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::ContextCollapseSnapshot(cs) => {
                assert!(cs.staged.as_ref().unwrap().is_empty());
                assert_eq!(cs.armed, Some(false));
                assert_eq!(cs.last_spawn_tokens, Some(0));
            }
            other => panic!("Expected ContextCollapseSnapshot, got: {other:?}"),
        }
    }
}
