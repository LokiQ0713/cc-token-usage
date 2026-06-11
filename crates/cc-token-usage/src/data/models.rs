use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Local Token/Usage Types (kept to avoid changing analysis/pricing/output) ─

/// Token usage statistics for a single API call.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation: Option<CacheCreationDetail>,
    pub server_tool_use: Option<ServerToolUse>,
    pub service_tier: Option<String>,
    pub speed: Option<String>,
    pub inference_geo: Option<String>,
}

/// Breakdown of cache creation tokens by TTL bucket.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CacheCreationDetail {
    pub ephemeral_5m_input_tokens: Option<u64>,
    pub ephemeral_1h_input_tokens: Option<u64>,
}

/// Server-side tool usage counters.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerToolUse {
    pub web_search_requests: Option<u64>,
    pub web_fetch_requests: Option<u64>,
}

// ─── Conversions from cc-session-jsonl types ─────────────────────────────────

impl From<cc_session_jsonl::types::Usage> for TokenUsage {
    fn from(u: cc_session_jsonl::types::Usage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cache_creation_input_tokens: u.cache_creation_input_tokens,
            cache_read_input_tokens: u.cache_read_input_tokens,
            cache_creation: u.cache_creation.map(|c| CacheCreationDetail {
                ephemeral_5m_input_tokens: c.ephemeral_5m_input_tokens,
                ephemeral_1h_input_tokens: c.ephemeral_1h_input_tokens,
            }),
            server_tool_use: u.server_tool_use.map(|s| ServerToolUse {
                web_search_requests: s.web_search_requests,
                web_fetch_requests: s.web_fetch_requests,
            }),
            // cc-session-jsonl v2 promoted these to typed enums. The analysis
            // layer continues to carry them as plain strings (it never
            // dispatches on them — they end up in HashMap<String, _> buckets
            // and JSON output verbatim), so project through `.as_str()`.
            service_tier: u.service_tier.map(|t| t.as_str().to_string()),
            inference_geo: u.inference_geo.map(|g| g.as_str().to_string()),
            speed: u.speed.map(|s| s.as_str().to_string()),
        }
    }
}

// ─── Validated Data Layer ────────────────────────────────────────────────────

/// A single validated assistant turn, ready for analysis.
#[derive(Debug, Clone)]
pub struct ValidatedTurn {
    pub uuid: String,
    pub request_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub usage: TokenUsage,
    pub stop_reason: Option<String>,
    pub content_types: Vec<String>,
    pub is_agent: bool,
    pub agent_id: Option<String>,
    pub user_text: Option<String>,      // 对应的用户消息文本（截断）
    pub assistant_text: Option<String>, // assistant 回复文本（截断）
    pub tool_names: Vec<String>,        // 使用的工具名列表
    pub service_tier: Option<String>,
    pub speed: Option<String>,
    pub inference_geo: Option<String>,
    pub tool_error_count: usize, // ToolResult blocks with is_error=true
    pub git_branch: Option<String>, // from the assistant entry's gitBranch field
    /// 触发本 turn 的 plugin 名（如 "superpowers"，Claude Code 2.1.138+）
    pub attribution_plugin: Option<String>,
    /// 触发本 turn 的 skill 名（如 "superpowers:brainstorming"，Claude Code 2.1.138+）
    pub attribution_skill: Option<String>,
}

/// A subagent invocation within a session, grouped from one agent JSONL file.
///
/// Each `Subagent` corresponds to one `agent-<id>.jsonl` file under a parent
/// session. The struct itself is *not* `Serialize` because it embeds the
/// internal `ValidatedTurn` analysis type; the JSON output layer builds its
/// own purpose-shaped `SubagentJson` (see `output/json.rs`).
#[derive(Debug, Clone)]
pub struct Subagent {
    /// Agent ID extracted from the agent JSONL file name (e.g. `agent-abc123`).
    pub agent_id: String,
    /// Agent type from `.meta.json`, e.g. "general-purpose".
    pub agent_type: Option<String>,
    /// Human-readable task description from `.meta.json`.
    pub description: Option<String>,
    /// All turns from this subagent, sorted by timestamp.
    pub turns: Vec<ValidatedTurn>,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub last_timestamp: Option<DateTime<Utc>>,
    /// The workflow run id (`wf_<runId>`) this subagent belongs to, if it was
    /// discovered under `<uuid>/subagents/workflows/wf_<runId>/`. `None` for
    /// ordinary (non-workflow) subagents. Used to distinguish workflow-spawned
    /// agents from regular Task-tool subagents in analysis and output.
    pub workflow_run_id: Option<String>,
}

/// Per-plugin aggregation for one session.
///
/// Plugin name comes from `attributionPlugin` on assistant entries
/// (Claude Code 2.1.138+).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUsage {
    pub plugin: String,
    pub turns: u64,
    pub cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Per-skill aggregation for one session.
///
/// Skill name comes from `attributionSkill` on assistant entries
/// (Claude Code 2.1.138+).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUsage {
    pub skill: String,
    pub turns: u64,
    pub cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Per-hook aggregation for one session.
///
/// Hooks come from `system` entries with `subtype == "stop_hook_summary"`
/// (Claude Code 2.1.104+). Grouped by `hookInfos[].command`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookUsage {
    pub command: String,
    pub invocations: u64,
    pub total_duration_ms: u64,
    pub error_count: u64,
    pub prevented_continuation_count: u64,
}

/// Aggregated data from a single session.
#[derive(Debug, Clone)]
pub struct SessionData {
    pub session_id: String,
    pub project: Option<String>,
    pub turns: Vec<ValidatedTurn>,
    /// Subagent groups for this session. Each entry corresponds to one
    /// `agent-<id>.jsonl` file. Empty for sessions without subagents.
    pub subagents: Vec<Subagent>,
    /// Plugins used in this session (aggregated from main session turns'
    /// `attributionPlugin`). Empty for pre-2.1.138 sessions.
    pub plugins: Vec<PluginUsage>,
    /// Skills used in this session (aggregated from main session turns'
    /// `attributionSkill`). Empty for pre-2.1.138 sessions.
    pub skills: Vec<SkillUsage>,
    /// Hooks triggered in this session (from `stop_hook_summary` system
    /// entries). Empty for sessions without hooks.
    pub hooks: Vec<HookUsage>,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub version: Option<String>,
    pub quality: DataQuality,
    pub metadata: SessionMetadata,
    /// Orphan session: the parent main session `.jsonl` was deleted, but
    /// subagent files under `<uuid>/subagents/` still exist. Scanner picked
    /// them up and the loader created this session as a placeholder so the
    /// subagent data isn't lost. Turn / token / cost totals still include
    /// these sessions; this flag only marks them for separate display.
    pub is_orphan: bool,
}

/// Per-`agent_type` rollup of all subagent invocations within one session.
///
/// One session may invoke the same agent type (e.g. `builder`) many times;
/// each invocation produces its own `agent-<id>.jsonl` file and one
/// `Subagent` instance. This struct groups those instances by `agent_type`
/// so the UI can render a single chip per type with a call count.
///
/// Subagents whose `agent_type` is `None` (no `.meta.json` sidecar) are
/// grouped under the literal type `"unknown"` rather than dropped.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentTypeAggregate {
    pub agent_type: String,
    pub count: u64,
    pub total_turns: u64,
    pub total_cost: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    /// Descriptions from each invocation of this agent type, in deterministic
    /// order (sorted by agent_id). Empty strings are omitted.
    pub descriptions: Vec<String>,
}

// ─── Session Metadata ───────────────────────────────────────────────────────

/// PR link info extracted from pr-link entries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PrLinkInfo {
    pub number: u64,
    pub url: String,
    pub repository: String,
}

/// A committed context collapse event.
#[derive(Debug, Clone)]
pub struct CollapseCommit {
    pub collapse_id: String,
    pub summary: String,
}

/// Snapshot of context collapse risk state.
#[derive(Debug, Clone)]
pub struct CollapseSnapshot {
    pub staged_count: usize,
    pub avg_risk: f64,
    pub max_risk: f64,
    pub armed: bool,
    pub last_spawn_tokens: u64,
}

/// Attribution data extracted from attribution-snapshot entries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AttributionData {
    pub surface: String,
    pub file_count: usize,
    pub total_claude_contribution: u64,
    pub prompt_count: Option<u64>,
    pub escape_count: Option<u64>,
    pub permission_prompt_count: Option<u64>,
}

/// Metadata collected from non-assistant/user entries during parsing.
#[derive(Debug, Default, Clone)]
pub struct SessionMetadata {
    pub title: Option<String>, // custom-title > ai-title
    pub tags: Vec<String>,
    pub mode: Option<String>, // last-wins
    pub pr_links: Vec<PrLinkInfo>,
    pub speculation_accepts: usize,
    pub speculation_time_saved_ms: f64,
    pub queue_enqueues: usize,
    pub queue_dequeues: usize,
    pub api_error_count: usize,   // assistant entries with api_error/error
    pub user_prompt_count: usize, // count of user entries
    pub collapse_commits: Vec<CollapseCommit>,
    pub collapse_snapshot: Option<CollapseSnapshot>,
    pub attribution: Option<AttributionData>,
}

impl SessionData {
    /// All API responses (main + every subagent), sorted by timestamp.
    pub fn all_responses(&self) -> Vec<&ValidatedTurn> {
        let mut all: Vec<&ValidatedTurn> = self
            .turns
            .iter()
            .chain(self.subagents.iter().flat_map(|s| s.turns.iter()))
            .collect();
        all.sort_by_key(|r| r.timestamp);
        all
    }

    /// Total number of API responses (main + all subagent turns).
    pub fn total_turn_count(&self) -> usize {
        self.turns.len() + self.subagents.iter().map(|s| s.turns.len()).sum::<usize>()
    }

    /// Total number of agent API responses (sum across all subagents).
    pub fn agent_turn_count(&self) -> usize {
        self.subagents.iter().map(|s| s.turns.len()).sum::<usize>()
    }

    /// Group this session's subagents by `agent_type` for chip rendering.
    ///
    /// Each output entry corresponds to one `agent_type` string. Subagents
    /// with `agent_type = None` are grouped under the literal type
    /// `"unknown"` (data is preserved, never dropped). Output is sorted by
    /// `agent_type` ascending for deterministic JSON serialization.
    ///
    /// Token / cost totals are summed across each subagent's turns; the
    /// `count` field counts the number of `Subagent` instances per type
    /// (i.e. how many times that type was invoked in this session).
    pub fn subagent_type_aggregates(
        &self,
        calc: &crate::pricing::calculator::PricingCalculator,
    ) -> Vec<SubagentTypeAggregate> {
        use std::collections::BTreeMap;

        // Sort subagents by agent_id so descriptions land in deterministic order.
        let mut sorted: Vec<&Subagent> = self.subagents.iter().collect();
        sorted.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));

        let mut acc: BTreeMap<String, SubagentTypeAggregate> = BTreeMap::new();
        for sa in sorted {
            let key = sa
                .agent_type
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unknown".to_string());
            let mut sa_input: u64 = 0;
            let mut sa_output: u64 = 0;
            let mut sa_cost: f64 = 0.0;
            for t in &sa.turns {
                sa_input += t.usage.input_tokens.unwrap_or(0);
                sa_output += t.usage.output_tokens.unwrap_or(0);
                sa_cost += calc.calculate_turn_cost(&t.model, &t.usage).total;
            }
            let entry = acc
                .entry(key.clone())
                .or_insert_with(|| SubagentTypeAggregate {
                    agent_type: key,
                    count: 0,
                    total_turns: 0,
                    total_cost: 0.0,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    descriptions: Vec::new(),
                });
            entry.count += 1;
            entry.total_turns += sa.turns.len() as u64;
            entry.total_cost += sa_cost;
            entry.total_input_tokens += sa_input;
            entry.total_output_tokens += sa_output;
            if let Some(desc) = sa.description.as_ref().filter(|d| !d.is_empty()) {
                entry.descriptions.push(desc.clone());
            }
        }
        acc.into_values().collect()
    }
}

/// Quality metrics for a single session file.
#[derive(Debug, Default, Clone)]
pub struct DataQuality {
    pub total_lines: usize,
    pub valid_turns: usize,
    pub skipped_synthetic: usize,
    pub skipped_sidechain: usize,
    pub skipped_invalid: usize,
    pub skipped_parse_error: usize,
    pub duplicate_turns: usize,
}

/// Quality metrics aggregated across all session files.
#[derive(Debug, Default, Clone, Serialize)]
pub struct GlobalDataQuality {
    pub total_session_files: usize,
    pub total_agent_files: usize,
    pub orphan_agents: usize,
    pub total_valid_turns: usize,
    pub total_skipped: usize,
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_assistant_message() {
        let json = r#"{"parentUuid":"abc","isSidechain":false,"type":"assistant","uuid":"def","timestamp":"2026-03-16T13:51:35.912Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"cache_creation_input_tokens":1281,"cache_read_input_tokens":15204,"cache_creation":{"ephemeral_5m_input_tokens":1281,"ephemeral_1h_input_tokens":0},"output_tokens":108,"service_tier":"standard"},"content":[{"type":"text","text":"Hello"}]},"sessionId":"abc-123","version":"2.0.77","cwd":"/tmp","gitBranch":"main","userType":"external","requestId":"req_1"}"#;

        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();

        match entry {
            cc_session_jsonl::types::Entry::Assistant(msg) => {
                assert_eq!(msg.uuid.as_deref(), Some("def"));
                assert_eq!(msg.session_id.as_deref(), Some("abc-123"));
                assert_eq!(msg.request_id.as_deref(), Some("req_1"));
                assert_eq!(msg.parent_uuid.as_str(), "abc");
                assert_eq!(msg.is_sidechain, Some(false));

                let api = msg.message;
                assert_eq!(api.model.as_deref(), Some("claude-opus-4-6"));
                assert_eq!(
                    api.stop_reason,
                    Some(cc_session_jsonl::types::StopReason::EndTurn)
                );

                let usage: TokenUsage = api.usage.unwrap().into();
                assert_eq!(usage.input_tokens, Some(3));
                assert_eq!(usage.output_tokens, Some(108));
                assert_eq!(usage.cache_creation_input_tokens, Some(1281));
                assert_eq!(usage.cache_read_input_tokens, Some(15204));
                assert_eq!(usage.service_tier.as_deref(), Some("standard"));

                let cache = usage.cache_creation.unwrap();
                assert_eq!(cache.ephemeral_5m_input_tokens, Some(1281));
                assert_eq!(cache.ephemeral_1h_input_tokens, Some(0));

                let content = api.content.unwrap();
                assert_eq!(content.len(), 1);
                match &content[0] {
                    cc_session_jsonl::types::ContentBlock::Text { text } => {
                        assert_eq!(text.as_deref(), Some("Hello"));
                    }
                    _ => panic!("expected Text content block"),
                }
            }
            _ => panic!("expected Assistant variant"),
        }
    }

    #[test]
    fn test_parse_user_message() {
        // V2 keeps UserEntry.parent_uuid as Option<String> (96.3% fill rate
        // in real data — session-root user entries are None).
        let json = r#"{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":[{"type":"text","text":"hello"}]},"uuid":"u1","timestamp":"2026-03-16T13:51:19.053Z","sessionId":"s1","version":"2.1.80","cwd":"/tmp","gitBranch":"main","userType":"external"}"#;

        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();

        match entry {
            cc_session_jsonl::types::Entry::User(msg) => {
                assert_eq!(msg.uuid.as_deref(), Some("u1"));
                assert_eq!(msg.session_id.as_deref(), Some("s1"));
                assert_eq!(msg.version.as_deref(), Some("2.1.80"));
                assert_eq!(msg.cwd.as_deref(), Some("/tmp"));
                assert_eq!(msg.git_branch.as_deref(), Some("main"));
                assert!(msg.parent_uuid.is_none());
            }
            _ => panic!("expected User variant"),
        }
    }

    #[test]
    fn test_parse_queue_operation() {
        let json = r#"{"type":"queue-operation","operation":"dequeue","timestamp":"2026-03-16T13:51:19.041Z","sessionId":"abc"}"#;

        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();

        match entry {
            cc_session_jsonl::types::Entry::QueueOperation(val) => {
                assert_eq!(val.operation.as_deref(), Some("dequeue"));
                assert_eq!(val.session_id.as_deref(), Some("abc"));
            }
            _ => panic!("expected QueueOperation variant"),
        }
    }

    #[test]
    fn test_parse_progress_entry() {
        let json = r#"{"type":"progress","data":{"type":"hook_progress","hookEvent":"PostToolUse","hookName":"PostToolUse:Read","command":"callback"},"toolUseID":"toolu_01","parentToolUseID":"toolu_01","uuid":"u1","timestamp":"2026-03-16T13:51:19.053Z","sessionId":"s1"}"#;
        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();
        match entry {
            cc_session_jsonl::types::Entry::Progress(p) => {
                assert_eq!(p.tool_use_id.as_deref(), Some("toolu_01"));
                match p.data.unwrap() {
                    cc_session_jsonl::types::ProgressData::HookProgress {
                        hook_event,
                        hook_name,
                        command,
                    } => {
                        assert_eq!(hook_event.as_deref(), Some("PostToolUse"));
                        assert_eq!(hook_name.as_deref(), Some("PostToolUse:Read"));
                        assert_eq!(command.as_deref(), Some("callback"));
                    }
                    other => panic!("expected HookProgress, got {other:?}"),
                }
            }
            other => panic!("expected Progress, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_system_entry() {
        let json = r#"{"type":"system","subtype":"turn_duration","durationMs":1234,"uuid":"u1","timestamp":"2026-03-16T13:51:19.053Z","sessionId":"s1"}"#;
        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, cc_session_jsonl::types::Entry::System(_)));
    }

    #[test]
    fn test_parse_unknown_entry_type() {
        let json = r#"{"type":"some-future-type","data":"whatever","uuid":"u1","timestamp":"2026-03-16T13:51:19.053Z"}"#;
        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, cc_session_jsonl::types::Entry::Ignored));
    }

    #[test]
    fn test_parse_thinking_content_block() {
        let json = r#"{"type":"assistant","uuid":"u1","timestamp":"2026-03-16T10:00:00Z","message":{"model":"claude-opus-4-6","role":"assistant","stop_reason":"end_turn","usage":{"input_tokens":3,"output_tokens":100,"cache_creation_input_tokens":500,"cache_read_input_tokens":10000},"content":[{"type":"thinking","thinking":"Let me analyze this...","signature":"abc123"},{"type":"text","text":"Here is my answer."}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":"p1","requestId":"r1"}"#;
        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();
        match entry {
            cc_session_jsonl::types::Entry::Assistant(msg) => {
                let content = msg.message.content.unwrap();
                assert_eq!(content.len(), 2);
                assert!(
                    matches!(&content[0], cc_session_jsonl::types::ContentBlock::Thinking { thinking: Some(t), .. } if t.contains("analyze"))
                );
                assert!(matches!(
                    &content[1],
                    cc_session_jsonl::types::ContentBlock::Text { .. }
                ));
            }
            _ => panic!("expected Assistant variant"),
        }
    }

    #[test]
    fn test_parse_synthetic_message() {
        let json = r#"{"type":"assistant","uuid":"x","timestamp":"2026-03-16T00:00:00Z","message":{"model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","usage":{"input_tokens":0,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"text","text":"error"}]},"sessionId":"s1","cwd":"/tmp","gitBranch":"","userType":"external","isSidechain":false,"parentUuid":"p1"}"#;

        let entry: cc_session_jsonl::types::Entry = serde_json::from_str(json).unwrap();

        match entry {
            cc_session_jsonl::types::Entry::Assistant(msg) => {
                let api = msg.message;
                assert_eq!(api.model.as_deref(), Some("<synthetic>"));
                assert_eq!(
                    api.stop_reason,
                    Some(cc_session_jsonl::types::StopReason::StopSequence)
                );

                let usage: TokenUsage = api.usage.unwrap().into();
                assert_eq!(usage.input_tokens, Some(0));
                assert_eq!(usage.output_tokens, Some(0));

                // synthetic messages typically lack cache_creation detail
                assert!(usage.cache_creation.is_none());
            }
            _ => panic!("expected Assistant variant"),
        }
    }

    #[test]
    fn test_token_usage_from_conversion() {
        let lib_usage = cc_session_jsonl::types::Usage {
            input_tokens: Some(100),
            output_tokens: Some(200),
            cache_creation_input_tokens: Some(50),
            cache_read_input_tokens: Some(300),
            cache_creation: Some(cc_session_jsonl::types::CacheCreation {
                ephemeral_5m_input_tokens: Some(30),
                ephemeral_1h_input_tokens: Some(20),
            }),
            server_tool_use: Some(cc_session_jsonl::types::ServerToolUse {
                web_search_requests: Some(2),
                web_fetch_requests: Some(1),
            }),
            // v2: service_tier / inference_geo / speed are typed enums.
            service_tier: Some(cc_session_jsonl::types::ServiceTier::Standard),
            inference_geo: Some(cc_session_jsonl::types::InferenceGeo::NotAvailable),
            iterations: None, // dropped in conversion
            speed: Some(cc_session_jsonl::types::Speed::Standard),
        };

        let local: TokenUsage = lib_usage.into();
        assert_eq!(local.input_tokens, Some(100));
        assert_eq!(local.output_tokens, Some(200));
        assert_eq!(local.cache_creation_input_tokens, Some(50));
        assert_eq!(local.cache_read_input_tokens, Some(300));
        assert_eq!(local.service_tier.as_deref(), Some("standard"));
        assert_eq!(local.inference_geo.as_deref(), Some("not_available"));
        assert_eq!(local.speed.as_deref(), Some("standard"));

        let cache = local.cache_creation.unwrap();
        assert_eq!(cache.ephemeral_5m_input_tokens, Some(30));
        assert_eq!(cache.ephemeral_1h_input_tokens, Some(20));

        let stu = local.server_tool_use.unwrap();
        assert_eq!(stu.web_search_requests, Some(2));
        assert_eq!(stu.web_fetch_requests, Some(1));
    }
}
