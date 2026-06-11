//! `UserEntry` — a JSONL entry where `type == "user"`.
//!
//! Despite the name, **83% of these are tool results being fed back to the
//! model**, not actual user input (see `docs/session-data-model.md` §3). The
//! `content_kind()` accessor on `UserMessage` distinguishes the two.
//!
//! v2 design notes vs the old `transcript_entry!` macro:
//!
//! - All shared transcript fields are spelled out here (no macro). The survey
//!   confirmed `promptId`/`teamName`/`agentName`/`agentColor` are *not*
//!   universal — they are user-specific or zero-cardinality on this dataset
//!   — so they don't get injected into other entry types any more.
//! - `parentUuid` is `Option<String>` here (96.3% real-data fill rate). It is
//!   `None` for session-root user entries and rewind-branch roots.
//! - New typed fields landed from v2 design (survey §3 user row): `origin`,
//!   `prompt_source`, `interrupted_message_id`, `is_compact_summary`,
//!   `is_visible_in_transcript_only`, `mcp_meta`, `image_paste_ids`.
//! - Low-cardinality enum fields (`prompt_source`, `origin.kind`) carry a
//!   `Unknown` variant so unfamiliar future values degrade just that field.
//!
//! "Mixed typed" enums (v2 design field-survey §8.3):
//!
//! - `tool_use_result` is a [`ToolUseResult`] tri-state enum: pure-string
//!   `Rejected` (8% of hits), typed [`TypedToolResult`] for the 11 recognised
//!   tool shapes (Bash, Edit, Read, Write, Task*, AskUserQuestion, WebFetch,
//!   WebSearch — together ~85% of hits), and `Other(Value)` for the long-tail
//!   ~7% of evolving / rare shapes.
//! - `image_paste_ids` element type is [`ImagePasteId`] — production emits
//!   both legacy string IDs and newer integer IDs.
//! - [`Origin`] and [`McpMeta`] carry a `#[serde(flatten)]` `extra` map so any
//!   future wire keys survive deserialisation rather than being silently dropped.

use serde::{Deserialize, Serialize};

use super::common::{DagNode, Entrypoint, OriginKind, PermissionMode, PromptSource, UserType};

/// A user-authored message entry in a Claude Code session.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserEntry {
    // ── 9 truly-universal DAG fields ──
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub cwd: Option<String>,
    pub version: Option<String>,
    pub git_branch: Option<String>,
    /// 100% present in the survey but always `"external"`. Typed for
    /// drift defence (an internal-tier role would land in `Unknown`).
    pub user_type: Option<UserType>,
    /// 100% present in the survey; `cli` or `sdk-cli`. New entrypoints
    /// land in [`Entrypoint::Unknown`].
    pub entrypoint: Option<Entrypoint>,
    pub is_sidechain: Option<bool>,

    // ── Logical-parent fallback (rare; not used by user entries today but
    //    seen on `compact_boundary` system entries; tolerate the key). ──
    pub logical_parent_uuid: Option<String>,

    // ── Identity / routing ──
    /// Identifier of the prompt the user sent (UUID-shape, 99.9% fill rate).
    pub prompt_id: Option<String>,
    /// Optional slug from the surrounding session metadata.
    pub slug: Option<String>,
    /// Sub-agent identifier (subagent trunk entries carry this).
    pub agent_id: Option<String>,

    // ── Teammates feature: deliberately not modelled. The survey
    //    confirmed `teamName`, `agentName`, `agentColor` have 0 hits in this
    //    dataset; serde silently ignores unknown keys so a future teammates-
    //    enabled session still parses cleanly. If/when teammates ships on
    //    this machine, restore these as `Option<String>` based on real
    //    observed values rather than the SDK-implied shape. ──

    // ── Message body & tool-result fallback ──
    pub message: Option<UserMessage>,
    /// Tool-result payload (38.6% fill rate). The "mixed typed" enum
    /// distinguishes pure-string rejections, 11 recognised typed shapes, and
    /// a long-tail `Other(Value)` for shapes that evolve with new tools.
    /// See [`ToolUseResult`].
    pub tool_use_result: Option<ToolUseResult>,
    /// tool-call ↔ tool-result link, capital-ID spelling (new since 2.1.71).
    #[serde(rename = "sourceToolUseID")]
    pub source_tool_use_id: Option<String>,
    /// Older spelling of the same link (still produced in some paths).
    #[serde(rename = "sourceToolAssistantUUID")]
    pub source_tool_assistant_uuid: Option<String>,

    // ── Mode / meta flags ──
    /// Inline permission-mode marker (8.5% fill rate). Typed so unknown
    /// modes degrade rather than blow up the entry.
    pub permission_mode: Option<PermissionMode>,
    /// Tool-use placeholder marker (Claude Code 2.1.104+).
    pub is_meta: Option<bool>,

    // ── v2.1.140+ ★ new fields (survey §3 user row) ──
    /// What surface generated the prompt — `text` or `slashCommand` in real
    /// data, with `Unknown` as the soft-landing for new values.
    pub prompt_source: Option<PromptSource>,
    /// Where the prompt came from (e.g. an IDE integration). Carries its own
    /// soft-landing `OriginKind::Unknown` for unfamiliar `origin.kind` values.
    pub origin: Option<Origin>,
    /// uuid of an assistant message the user interrupted.
    pub interrupted_message_id: Option<String>,
    /// Marks the synthetic user entry that introduces a compact summary on
    /// resume after context collapse.
    pub is_compact_summary: Option<bool>,
    /// Hide this entry from the rendered transcript (still in JSONL).
    pub is_visible_in_transcript_only: Option<bool>,
    /// MCP-tool metadata associated with this user turn.
    pub mcp_meta: Option<McpMeta>,
    /// Identifiers for images pasted into the prompt. Production emits both
    /// string ids (legacy) and integer ids (newer); [`ImagePasteId`] is the
    /// tagged-by-shape sum type.
    pub image_paste_ids: Option<Vec<ImagePasteId>>,
}

impl DagNode for UserEntry {
    fn uuid(&self) -> Option<&str> {
        self.uuid.as_deref()
    }
    fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    fn timestamp(&self) -> Option<&str> {
        self.timestamp.as_deref()
    }
    fn parent_uuid(&self) -> Option<&str> {
        self.parent_uuid.as_deref()
    }
    fn is_sidechain(&self) -> Option<bool> {
        self.is_sidechain
    }
}

/// The content of a user message.
///
/// The Claude Code wire format puts the actual payload under `message.content`
/// as either a plain string (legacy) or an array of content blocks. We keep
/// the raw value and expose [`UserMessage::content_kind`] for callers that
/// need to discriminate between the user's own input and tool results
/// (`docs/session-data-model.md` §3).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserMessage {
    pub role: Option<String>,
    pub content: Option<serde_json::Value>,
}

/// Discriminator for the polymorphic `content` field on [`UserMessage`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserContentKind {
    /// Legacy plain string prompt.
    String,
    /// Array containing only `tool_result` blocks — this is the API's
    /// channel for feeding tool output back to the model, not user speech.
    ToolResult,
    /// Array of `text` blocks only (multi-segment user input).
    Text,
    /// Array containing at least one image plus optional text (richer user
    /// input).
    ImageText,
    /// Anything else (mixed unexpected shapes, empty arrays, etc.).
    Mixed,
}

impl UserMessage {
    /// Classify the wire-format `content` shape — see the description on
    /// [`UserContentKind`]. Returns `None` when `content` itself is absent.
    pub fn content_kind(&self) -> Option<UserContentKind> {
        let value = self.content.as_ref()?;
        if value.is_string() {
            return Some(UserContentKind::String);
        }
        let arr = value.as_array()?;
        let mut has_tool_result = false;
        let mut has_text = false;
        let mut has_image = false;
        let mut has_other = false;
        for block in arr {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("tool_result") => has_tool_result = true,
                Some("text") => has_text = true,
                Some("image") => has_image = true,
                _ => has_other = true,
            }
        }
        let kind = match (has_tool_result, has_text, has_image, has_other) {
            (true, false, false, false) => UserContentKind::ToolResult,
            (false, true, false, false) => UserContentKind::Text,
            (false, _, true, false) => UserContentKind::ImageText,
            _ => UserContentKind::Mixed,
        };
        Some(kind)
    }
}

// ─── tool_use_result mixed-typed enum (field-survey §8.3) ────────────────

/// Element of [`UserEntry::image_paste_ids`].
///
/// Production data emits both forms:
/// - Legacy: string UUID-like identifiers (e.g. `"paste-1"`).
/// - Newer (only observed in 2026 data): small integer ordinals (e.g. `1`).
///
/// The untagged enum admits both without StructDrift, and serialisation
/// round-trips each form to its original wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ImagePasteId {
    /// Newer integer ordinal form (e.g. `1`, `3`). Listed FIRST because
    /// integers must not accidentally match the string branch — JSON
    /// distinguishes them at the lexical level so the order is correctness,
    /// not just performance.
    Integer(i64),
    /// Legacy string-id form.
    String(String),
}

/// Tool-result payload on a `tool_result`-bearing user entry.
///
/// This is a "mixed typed" enum (field-survey §8.3): pure-string rejections,
/// typed shapes for the 11 most common tool results, and a long-tail
/// `Other(serde_json::Value)` for shapes that evolve with new tools or that
/// are too rare to model. The discriminating dispatch is hand-written (not
/// `#[serde(untagged)]`) so the shape-matching rules are explicit and
/// auditable:
///
/// 1. A pure JSON string → [`ToolUseResult::Rejected`] (8% of hits — the
///    "user rejected the tool call" rejection message).
/// 2. A JSON object whose keys match one of the recognised [`TypedToolResult`]
///    shapes → [`ToolUseResult::Typed`].
/// 3. Anything else (including objects with novel keys and array-shaped
///    payloads like the `[{type, text}]` ToolSearch result) →
///    [`ToolUseResult::Other`].
///
/// Serialisation reverses the discrimination — strings serialise to bare
/// JSON strings, typed variants serialise transparently to their object
/// shape, and `Other` writes back the captured `Value`.
//
// `large_enum_variant` is permitted here for parity with the project-wide
// `Entry` enum (~similar shape, also tagged-by-content): boxing the large
// variants would scatter `Box<…>` through every call-site that pattern-matches
// and would not change peak memory because the `Typed` variant is the common
// case and almost always present.  The enum is always carried inside
// `Option<ToolUseResult>` on `UserEntry`, so the size hit is amortised.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ToolUseResult {
    /// Pure-string rejection message — 8% of all `toolUseResult` hits.
    /// Wire shape is a bare JSON string ("Error: The user doesn't want to
    /// proceed with this tool use ...").
    Rejected(String),
    /// One of the 11 recognised typed tool-result shapes.
    Typed(TypedToolResult),
    /// Long-tail (~7% of hits): novel-shape objects, array payloads (e.g.
    /// ToolSearch's `[{text, type}]`), or unrecognised future shapes.
    Other(serde_json::Value),
}

impl Serialize for ToolUseResult {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        match self {
            ToolUseResult::Rejected(s) => ser.serialize_str(s),
            ToolUseResult::Typed(t) => t.serialize(ser),
            ToolUseResult::Other(v) => v.serialize(ser),
        }
    }
}

impl<'de> Deserialize<'de> for ToolUseResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Buffer to Value first — this lets us inspect the shape without
        // depending on serde's untagged-enum dispatch order, which silently
        // matches the first variant that accepts the input. With 45+ observed
        // shapes overlapping on common keys, explicit shape detection is
        // far more reliable than ordering tricks.
        let value = serde_json::Value::deserialize(d)?;
        if let serde_json::Value::String(s) = &value {
            return Ok(ToolUseResult::Rejected(s.clone()));
        }
        // Anything other than a top-level object cannot match a typed shape;
        // array / number / bool / null all fall straight through to Other.
        let Some(obj) = value.as_object() else {
            return Ok(ToolUseResult::Other(value));
        };
        // Dispatch by required-key signature in DESCENDING order of key count
        // so the most specific shape claims its hits before a subset shape
        // does. (E.g. TaskUpdate-with-verification has 5 keys and must match
        // before plain TaskUpdate's 4 keys.)
        let typed = if has_keys(obj, &["agentId", "agentType", "content", "prompt", "status"]) {
            // TaskCompleted has agentId+agentType+content+prompt+status as the
            // distinguishing core; toolStats/totalDurationMs/totalTokens/etc.
            // are present in most hits but absent on a handful of legacy
            // entries — match on the stable core only.
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::TaskCompleted)
        } else if has_keys(obj, &["isAsync", "agentId", "status"])
            && obj.contains_key("outputFile")
        {
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::TaskAsyncLaunched)
        } else if has_keys(obj, &["statusChange", "success", "taskId", "updatedFields"]) {
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::TaskUpdate)
        } else if obj.len() == 1 && obj.contains_key("task") {
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::TaskCreate)
        } else if has_keys(obj, &["interrupted", "isImage", "noOutputExpected", "stderr", "stdout"])
        {
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::Bash)
        } else if has_keys(
            obj,
            &[
                "filePath",
                "newString",
                "oldString",
                "originalFile",
                "replaceAll",
                "structuredPatch",
                "userModified",
            ],
        ) {
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::Edit)
        } else if has_keys(
            obj,
            &[
                "content",
                "filePath",
                "originalFile",
                "structuredPatch",
                "type",
                "userModified",
            ],
        ) {
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::Write)
        } else if has_keys(obj, &["file", "type"]) && obj.len() == 2 {
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::Read)
        } else if has_keys(obj, &["answers", "questions"]) {
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::AskUserQuestion)
        } else if has_keys(obj, &["bytes", "code", "codeText", "durationMs", "result", "url"]) {
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::WebFetch)
        } else if has_keys(obj, &["durationSeconds", "query", "results"]) {
            // Both `with searchCount` and `without searchCount` flavours
            // collapse here (searchCount is optional on the struct).
            serde_json::from_value(value.clone())
                .ok()
                .map(TypedToolResult::WebSearch)
        } else {
            None
        };
        Ok(match typed {
            Some(typed) => ToolUseResult::Typed(typed),
            None => ToolUseResult::Other(value),
        })
    }
}

/// Tiny helper: does `obj` contain every key in `keys`?
fn has_keys(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> bool {
    keys.iter().all(|k| obj.contains_key(*k))
}

/// One of the 11 recognised typed tool-result shapes.
///
/// Variant ordering here doesn't affect deserialisation (the parent
/// [`ToolUseResult`] dispatches by shape), but matches the survey's
/// frequency descending order to make the most common cases scan first
/// when reading code.
//
// `large_enum_variant` is permitted: the largest variant
// (`TaskCompletedResult`, which embeds the full assistant `Usage` struct
// for sub-agent token accounting) is the one consumers most often care
// about; boxing would push a `Box<…>` through every match arm without
// reducing the steady-state memory because we keep one of these alive per
// matched user entry regardless.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum TypedToolResult {
    /// `Bash` tool result (basic + background + gitOperation variants;
    /// 34.7% of all typed hits).
    Bash(BashResult),
    /// `Edit` tool result (15.98%).
    Edit(EditResult),
    /// `Read` tool result (12.52%).
    Read(ReadResult),
    /// `Write` tool result (6.33%).
    Write(WriteResult),
    /// Sub-agent task completion summary (3.66%).
    TaskCompleted(TaskCompletedResult),
    /// `TaskUpdate` task-tracker result (3.17%; with or without verification
    /// nudge field).
    TaskUpdate(TaskUpdateResult),
    /// `AskUserQuestion` MCP tool result (2.33%).
    AskUserQuestion(AskUserQuestionResult),
    /// `TaskCreate` task-tracker creation (2.28%).
    TaskCreate(TaskCreateResult),
    /// `WebSearch` server-tool result (1.79%, with or without searchCount).
    WebSearch(WebSearchResult),
    /// `WebFetch` server-tool result (1.67%).
    WebFetch(WebFetchResult),
    /// Asynchronous `Task` launch acknowledgement (0.62%).
    TaskAsyncLaunched(TaskAsyncResult),
}

/// `Bash` tool result. Common across basic, background, and git-operation
/// invocations — `backgroundTaskId` / `gitOperation` are optional flavours.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BashResult {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub interrupted: Option<bool>,
    pub is_image: Option<bool>,
    pub no_output_expected: Option<bool>,
    /// Set for backgrounded bash invocations (~0.6% of Bash hits).
    pub background_task_id: Option<String>,
    /// Set when the command performed a git operation — nested shape varies
    /// (`{commit: {sha, kind}}`, `{push: {branch}}`, etc.), keep raw.
    pub git_operation: Option<serde_json::Value>,
}

/// `Edit` tool result (find / replace with structured patch).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    pub file_path: Option<String>,
    pub old_string: Option<String>,
    pub new_string: Option<String>,
    /// Whole-file contents prior to the edit; large blob, kept as `String`.
    pub original_file: Option<String>,
    pub replace_all: Option<bool>,
    /// Diff hunks — nested array shape is complex and rarely consumed, keep raw.
    pub structured_patch: Option<serde_json::Value>,
    pub user_modified: Option<bool>,
}

/// `Read` tool result. The nested `file` payload carries
/// `{filePath, content, ...}`; keep it raw because shape varies by file type.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResult {
    /// Nested `{filePath, content, ...}`. Shape varies, kept raw.
    pub file: Option<serde_json::Value>,
    /// `"text"` in all observed samples.
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// `Write` tool result.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    /// `"create"` or `"update"`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub file_path: Option<String>,
    pub content: Option<String>,
    pub original_file: Option<String>,
    /// Diff hunks — nested shape, kept raw.
    pub structured_patch: Option<serde_json::Value>,
    pub user_modified: Option<bool>,
}

/// Sub-agent task completion summary. `usage` mirrors
/// [`crate::types::assistant::Usage`] (re-used so token-accounting stays
/// uniform across direct and delegated work).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCompletedResult {
    pub status: Option<String>,
    pub prompt: Option<String>,
    /// Body of the sub-agent's final reply — text or richer array.
    pub content: Option<serde_json::Value>,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    /// Per-tool invocation counts collected by the sub-agent.
    pub tool_stats: Option<serde_json::Value>,
    pub total_duration_ms: Option<u64>,
    pub total_tokens: Option<u64>,
    pub total_tool_use_count: Option<u64>,
    /// Token usage rolled up from the sub-agent's transcript.
    pub usage: Option<crate::types::assistant::Usage>,
    /// Worktree path used by the sub-agent (present on worktree-style
    /// invocations, ~0.03% of TaskCompleted hits).
    pub worktree_branch: Option<String>,
    pub worktree_path: Option<String>,
}

/// Asynchronous Task launch acknowledgement (the sub-agent runs detached;
/// the launcher only sees this stub until results land).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskAsyncResult {
    pub status: Option<String>,
    pub is_async: Option<bool>,
    pub agent_id: Option<String>,
    pub can_read_output_file: Option<bool>,
    pub output_file: Option<String>,
    pub prompt: Option<String>,
    pub description: Option<String>,
}

/// `TaskCreate` task-tracker creation. The result is a single-key wrapper
/// over the task descriptor (id + subject).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCreateResult {
    pub task: TaskDescriptor,
}

/// Inner descriptor used by [`TaskCreateResult`].
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDescriptor {
    pub id: Option<String>,
    pub subject: Option<String>,
}

/// `TaskUpdate` task-tracker result.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUpdateResult {
    pub success: Option<bool>,
    pub task_id: Option<String>,
    pub updated_fields: Option<Vec<String>>,
    pub status_change: Option<StatusChange>,
    /// Present in ~0.96% of TaskUpdate hits (the "verificationNudge"
    /// flavour). Absent on the basic flavour.
    pub verification_nudge_needed: Option<bool>,
}

/// `statusChange` inner shape on [`TaskUpdateResult`].
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusChange {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// `AskUserQuestion` MCP-tool result.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserQuestionResult {
    /// Original questions array — shape varies (header, options, preview).
    pub questions: Option<serde_json::Value>,
    /// User's answers in the same order.
    pub answers: Option<serde_json::Value>,
    /// Optional annotations on individual answers (~0.64% of hits).
    pub annotations: Option<serde_json::Value>,
}

/// `WebFetch` server-tool result.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebFetchResult {
    pub url: Option<String>,
    /// HTTP status code (e.g. 200).
    pub code: Option<u16>,
    /// HTTP status reason phrase (e.g. "OK").
    pub code_text: Option<String>,
    pub bytes: Option<u64>,
    pub duration_ms: Option<u64>,
    /// Page text or LLM-summarised body. Always String in observed data.
    pub result: Option<String>,
}

/// `WebSearch` server-tool result. The `searchCount` field is present on the
/// newer flavour only.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchResult {
    pub query: Option<String>,
    /// Nested list of `{title, url, content}` per result hit.
    pub results: Option<serde_json::Value>,
    pub duration_seconds: Option<f64>,
    /// Present in ~0.96% of WebSearch hits.
    pub search_count: Option<u64>,
}

/// Origin metadata carried on some user entries (Claude Code 2.1.140+).
///
/// The richer shape (extra labelling fields) varies between integrations —
/// IDE name/version, task-notification carrier metadata, etc. The
/// [`Origin::extra`] map captures any additional keys that arrive alongside
/// `kind`, so future wire additions survive deserialisation rather than
/// being silently dropped. Real data sampled 2026-06-09 carried only `kind`
/// on every entry; the map is preserved against the next surface that
/// inevitably starts emitting more fields.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    pub kind: Option<OriginKind>,
    /// Other keys present alongside `kind`. Captured with `#[serde(flatten)]`
    /// so they survive both deserialisation and round-trip serialisation.
    /// Empty `{}` on entries that only carry `kind` (the 100% case in the
    /// surveyed dataset).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// MCP-tool metadata block referenced from a user entry.
///
/// Similar to [`Origin`], any additional keys that arrive alongside
/// `structuredContent` are captured in [`McpMeta::extra`] for forward
/// compatibility. The survey only saw `structuredContent`; the map is the
/// canary for the next field Anthropic adds.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpMeta {
    /// Free-shape structured content the MCP tool emitted. Shape varies by
    /// tool implementation, so we keep the raw value.
    pub structured_content: Option<serde_json::Value>,
    /// Other keys alongside `structuredContent`. See [`Origin::extra`] for
    /// rationale.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Entry;

    #[test]
    fn parse_user_entry_with_content_array() {
        let json = r#"{
            "type": "user",
            "parentUuid": "p-001",
            "isSidechain": false,
            "uuid": "u-001",
            "timestamp": "2026-03-16T13:50:00.000Z",
            "sessionId": "sess-001",
            "cwd": "/Users/loki/project",
            "version": "2.0.77",
            "gitBranch": "feature-x",
            "userType": "external",
            "message": {
                "role": "user",
                "content": [
                    {"type": "text", "text": "Please fix the bug in main.rs"},
                    {"type": "text", "text": "It crashes on startup"}
                ]
            }
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.uuid.as_deref(), Some("u-001"));
        assert_eq!(entry.parent_uuid.as_deref(), Some("p-001"));
        assert_eq!(entry.is_sidechain, Some(false));
        assert_eq!(entry.timestamp.as_deref(), Some("2026-03-16T13:50:00.000Z"));
        assert_eq!(entry.session_id.as_deref(), Some("sess-001"));
        assert_eq!(entry.cwd.as_deref(), Some("/Users/loki/project"));
        assert_eq!(entry.version.as_deref(), Some("2.0.77"));
        assert_eq!(entry.git_branch.as_deref(), Some("feature-x"));
        assert_eq!(entry.user_type, Some(UserType::External));

        let msg = entry.message.as_ref().unwrap();
        assert_eq!(msg.role.as_deref(), Some("user"));
        let content = msg.content.as_ref().unwrap();
        assert!(content.is_array());
        assert_eq!(content.as_array().unwrap().len(), 2);
        assert_eq!(content[0]["text"], "Please fix the bug in main.rs");
    }

    #[test]
    fn parse_user_entry_with_plain_string_content() {
        let json = r#"{
            "type": "user",
            "uuid": "u-002",
            "sessionId": "sess-002",
            "message": {
                "role": "user",
                "content": "Just a plain text prompt"
            }
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let msg = entry.message.as_ref().unwrap();
        let content = msg.content.as_ref().unwrap();
        assert!(content.is_string());
        assert_eq!(content.as_str().unwrap(), "Just a plain text prompt");
        assert_eq!(msg.content_kind(), Some(UserContentKind::String));
    }

    #[test]
    fn parse_user_via_entry_enum() {
        let json = r#"{
            "type": "user",
            "uuid": "u-003",
            "sessionId": "sess-003",
            "message": {"role": "user", "content": "hello"}
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::User(u) => {
                assert_eq!(u.uuid.as_deref(), Some("u-003"));
            }
            other => panic!("Expected User, got: {other:?}"),
        }
    }

    #[test]
    fn parse_user_with_is_meta_and_source_tool_use_id() {
        // Two-field test: `isMeta` (bool) and `sourceToolUseID` (uppercase ID).
        let json = r#"{
            "type": "user",
            "uuid": "u-meta-001",
            "sessionId": "sess-meta",
            "timestamp": "2026-05-13T10:00:00.000Z",
            "isMeta": false,
            "sourceToolUseID": "toolu_01XXXX"
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.is_meta, Some(false));
        assert_eq!(entry.source_tool_use_id.as_deref(), Some("toolu_01XXXX"));
        assert!(entry.source_tool_assistant_uuid.is_none());
        assert!(entry.permission_mode.is_none());
        assert!(entry.tool_use_result.is_none());
    }

    #[test]
    fn parse_user_with_source_tool_assistant_uuid_legacy() {
        let json = r#"{
            "type": "user",
            "uuid": "u-asst-uuid-001",
            "sessionId": "sess-asst",
            "sourceToolAssistantUUID": "asst_abcdef1234567890"
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(
            entry.source_tool_assistant_uuid.as_deref(),
            Some("asst_abcdef1234567890")
        );
        assert!(entry.source_tool_use_id.is_none());
    }

    #[test]
    fn parse_user_with_permission_mode_inline() {
        let json = r#"{
            "type": "user",
            "uuid": "u-perm-001",
            "sessionId": "sess-perm",
            "permissionMode": "bypassPermissions",
            "message": {"role": "user", "content": "run it"}
        }"#;

        let entry: Entry = serde_json::from_str(json).unwrap();
        match entry {
            Entry::User(u) => {
                assert_eq!(u.permission_mode, Some(PermissionMode::BypassPermissions));
            }
            Entry::PermissionMode(_) => {
                panic!(
                    "permissionMode inline on user entry was misclassified as PermissionMode entry"
                );
            }
            other => panic!("Expected User, got: {other:?}"),
        }
    }

    // ── Task A: tool_use_result mixed-typed dispatch ───────────────────

    #[test]
    fn tool_use_result_rejected_string() {
        // 8% of hits in the survey: pure JSON string carrying the rejection
        // message Claude Code emits when the user declined a tool call.
        let s = r#""Error: The user doesn't want to proceed with this tool use.""#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Rejected(msg) => assert!(msg.contains("doesn't want to proceed")),
            other => panic!("Expected Rejected, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_bash_typed_basic() {
        // 33.5% of hits: plain Bash invocation.
        let s = r#"{"stdout":"hello\n","stderr":"","interrupted":false,"isImage":false,"noOutputExpected":false}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::Bash(b)) => {
                assert_eq!(b.stdout.as_deref(), Some("hello\n"));
                assert_eq!(b.interrupted, Some(false));
                assert!(b.background_task_id.is_none());
                assert!(b.git_operation.is_none());
            }
            other => panic!("Expected Typed(Bash), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_bash_with_background_task_id() {
        // 0.6% of Bash hits: backgrounded invocation.
        let s = r#"{"stdout":"","stderr":"","interrupted":false,"isImage":false,"noOutputExpected":false,"backgroundTaskId":"b6xz29pmq"}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::Bash(b)) => {
                assert_eq!(b.background_task_id.as_deref(), Some("b6xz29pmq"));
            }
            other => panic!("Expected Typed(Bash) with bg id, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_bash_with_git_operation_push() {
        // gitOperation is nested {push: {branch}} — kept raw, but must
        // still dispatch to the Bash variant.
        let s = r#"{"stdout":"ok","stderr":"","interrupted":false,"isImage":false,"noOutputExpected":false,"gitOperation":{"push":{"branch":"main"}}}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::Bash(b)) => {
                let gop = b.git_operation.as_ref().unwrap();
                assert_eq!(gop["push"]["branch"], "main");
            }
            other => panic!("Expected Typed(Bash) with gitOperation, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_edit() {
        let s = r#"{
            "filePath":"/x.rs","oldString":"foo","newString":"bar",
            "originalFile":"foo body","replaceAll":false,
            "structuredPatch":[],"userModified":false
        }"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::Edit(e)) => {
                assert_eq!(e.file_path.as_deref(), Some("/x.rs"));
                assert_eq!(e.old_string.as_deref(), Some("foo"));
                assert_eq!(e.new_string.as_deref(), Some("bar"));
                assert_eq!(e.replace_all, Some(false));
                assert!(e.structured_patch.is_some());
            }
            other => panic!("Expected Typed(Edit), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_read() {
        let s = r#"{"file":{"filePath":"/x.md","content":"hi"},"type":"text"}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::Read(r)) => {
                assert_eq!(r.kind.as_deref(), Some("text"));
                let file = r.file.as_ref().unwrap();
                assert_eq!(file["filePath"], "/x.md");
            }
            other => panic!("Expected Typed(Read), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_write() {
        let s = r##"{
            "type":"create","filePath":"/n.md","content":"# title",
            "originalFile":"","structuredPatch":[],"userModified":false
        }"##;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::Write(w)) => {
                assert_eq!(w.kind.as_deref(), Some("create"));
                assert_eq!(w.file_path.as_deref(), Some("/n.md"));
                assert_eq!(w.content.as_deref(), Some("# title"));
            }
            other => panic!("Expected Typed(Write), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_task_completed() {
        let s = r#"{
            "status":"completed","prompt":"do x","content":[{"type":"text","text":"done"}],
            "agentId":"ag1","agentType":"general-purpose",
            "toolStats":{},"totalDurationMs":1234,"totalTokens":100,"totalToolUseCount":2,
            "usage":{"input_tokens":50,"output_tokens":50}
        }"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::TaskCompleted(t)) => {
                assert_eq!(t.status.as_deref(), Some("completed"));
                assert_eq!(t.agent_id.as_deref(), Some("ag1"));
                assert_eq!(t.total_duration_ms, Some(1234));
                assert_eq!(t.total_tokens, Some(100));
                let usage = t.usage.as_ref().unwrap();
                assert_eq!(usage.input_tokens, Some(50));
                assert_eq!(usage.output_tokens, Some(50));
            }
            other => panic!("Expected Typed(TaskCompleted), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_task_async_launched() {
        let s = r#"{
            "isAsync":true,"status":"async_launched",
            "agentId":"a-async","canReadOutputFile":true,
            "outputFile":"/tmp/out","prompt":"do x","description":"work"
        }"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::TaskAsyncLaunched(t)) => {
                assert_eq!(t.is_async, Some(true));
                assert_eq!(t.status.as_deref(), Some("async_launched"));
                assert_eq!(t.agent_id.as_deref(), Some("a-async"));
                assert_eq!(t.output_file.as_deref(), Some("/tmp/out"));
            }
            other => panic!("Expected Typed(TaskAsyncLaunched), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_task_create() {
        let s = r#"{"task":{"id":"1","subject":"rewrite globals.css"}}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::TaskCreate(t)) => {
                assert_eq!(t.task.id.as_deref(), Some("1"));
                assert_eq!(t.task.subject.as_deref(), Some("rewrite globals.css"));
            }
            other => panic!("Expected Typed(TaskCreate), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_task_update_basic() {
        let s = r#"{"success":true,"taskId":"1","updatedFields":["status"],"statusChange":{"from":"pending","to":"in_progress"}}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::TaskUpdate(t)) => {
                assert_eq!(t.success, Some(true));
                assert_eq!(t.task_id.as_deref(), Some("1"));
                assert!(t.verification_nudge_needed.is_none());
                let sc = t.status_change.as_ref().unwrap();
                assert_eq!(sc.from.as_deref(), Some("pending"));
                assert_eq!(sc.to.as_deref(), Some("in_progress"));
            }
            other => panic!("Expected Typed(TaskUpdate), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_task_update_with_verification_nudge() {
        let s = r#"{"success":true,"taskId":"1","updatedFields":["status"],"statusChange":{"from":"in_progress","to":"completed"},"verificationNudgeNeeded":false}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::TaskUpdate(t)) => {
                assert_eq!(t.verification_nudge_needed, Some(false));
                assert_eq!(t.success, Some(true));
            }
            other => panic!("Expected Typed(TaskUpdate) w/ nudge, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_ask_user_question() {
        let s = r#"{"questions":[{"question":"q?","options":[]}],"answers":["a"]}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::AskUserQuestion(a)) => {
                assert!(a.questions.is_some());
                assert!(a.answers.is_some());
                assert!(a.annotations.is_none());
            }
            other => panic!("Expected Typed(AskUserQuestion), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_web_fetch() {
        let s = r#"{"bytes":1234,"code":200,"codeText":"OK","durationMs":500,"result":"body","url":"https://x"}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::WebFetch(w)) => {
                assert_eq!(w.code, Some(200));
                assert_eq!(w.code_text.as_deref(), Some("OK"));
                assert_eq!(w.bytes, Some(1234));
                assert_eq!(w.url.as_deref(), Some("https://x"));
            }
            other => panic!("Expected Typed(WebFetch), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_web_search_with_count() {
        let s = r#"{"durationSeconds":2.5,"query":"rust async","results":[],"searchCount":3}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::WebSearch(w)) => {
                assert_eq!(w.query.as_deref(), Some("rust async"));
                assert_eq!(w.search_count, Some(3));
                assert_eq!(w.duration_seconds, Some(2.5));
            }
            other => panic!("Expected Typed(WebSearch), got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_web_search_without_count() {
        let s = r#"{"durationSeconds":1.5,"query":"rust","results":[]}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Typed(TypedToolResult::WebSearch(w)) => {
                assert_eq!(w.search_count, None);
            }
            other => panic!("Expected Typed(WebSearch) without count, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_other_tool_search_array_in_object() {
        // ToolSearch's `obj{matches, query, total_deferred_tools}` shape —
        // 76 hits, ~1.22% — has none of the recognised typed signatures,
        // so it must land in Other(Value).
        let s = r#"{"matches":["TaskCreate","TaskUpdate"],"query":"select:TaskCreate","total_deferred_tools":86}"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Other(v) => {
                assert_eq!(v["matches"][0], "TaskCreate");
                assert_eq!(v["total_deferred_tools"], 86);
            }
            other => panic!("Expected Other for ToolSearch shape, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_other_top_level_array() {
        // `[{"type":"text","text":"..."}]` shape — 168 hits, ~2.7% — is a
        // top-level array, not an object, so it lands in Other.
        let s = r#"[{"type":"text","text":"selected"}]"#;
        let r: ToolUseResult = serde_json::from_str(s).unwrap();
        match r {
            ToolUseResult::Other(v) => {
                assert!(v.is_array());
                assert_eq!(v[0]["text"], "selected");
            }
            other => panic!("Expected Other for array shape, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_result_round_trip_rejected() {
        let original = r#""rejection text""#;
        let r: ToolUseResult = serde_json::from_str(original).unwrap();
        let out = serde_json::to_string(&r).unwrap();
        assert_eq!(out, original);
    }

    #[test]
    fn tool_use_result_round_trip_bash() {
        // Round-tripping typed variants: emit camelCase wire keys.
        let original = r#"{"stdout":"x","stderr":"","interrupted":false,"isImage":false,"noOutputExpected":false}"#;
        let r: ToolUseResult = serde_json::from_str(original).unwrap();
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(v["stdout"], "x");
        assert_eq!(v["isImage"], false);
        assert_eq!(v["noOutputExpected"], false);
    }

    #[test]
    fn tool_use_result_round_trip_other() {
        let original = r#"{"unknownShape":true,"otherKey":42}"#;
        let r: ToolUseResult = serde_json::from_str(original).unwrap();
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(v["unknownShape"], true);
        assert_eq!(v["otherKey"], 42);
    }

    // Embedded round-trip via UserEntry (full integration through agentId
    // promotion path).
    #[test]
    fn parse_user_with_tool_use_result() {
        // Backward-compat: original test used arbitrary keys {foo, n};
        // those don't match any typed shape and must land in Other(Value).
        let json = r#"{
            "type": "user",
            "uuid": "u-tur-001",
            "sessionId": "sess-tur",
            "toolUseResult": {"foo": "bar", "n": 42}
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let result = entry
            .tool_use_result
            .as_ref()
            .expect("tool_use_result must be Some");
        match result {
            ToolUseResult::Other(v) => {
                assert!(v.is_object());
                assert_eq!(v["foo"], "bar");
                assert_eq!(v["n"], 42);
            }
            other => panic!("Expected Other(Value), got {other:?}"),
        }
    }

    #[test]
    fn parse_user_legacy_no_new_fields() {
        let json = r#"{
            "type": "user",
            "uuid": "u-legacy-001",
            "parentUuid": "p-legacy",
            "isSidechain": false,
            "timestamp": "2025-06-01T09:00:00.000Z",
            "sessionId": "sess-legacy",
            "cwd": "/home/user/project",
            "version": "2.0.77",
            "message": {"role": "user", "content": "Fix the build"}
        }"#;

        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert!(entry.is_meta.is_none());
        assert!(entry.permission_mode.is_none());
        assert!(entry.tool_use_result.is_none());
        assert!(entry.source_tool_use_id.is_none());
        assert!(entry.source_tool_assistant_uuid.is_none());
        assert!(entry.prompt_source.is_none());
        assert!(entry.origin.is_none());
        assert!(entry.interrupted_message_id.is_none());
        assert!(entry.is_compact_summary.is_none());
        assert!(entry.is_visible_in_transcript_only.is_none());
        assert!(entry.mcp_meta.is_none());
        assert!(entry.image_paste_ids.is_none());
        assert_eq!(entry.uuid.as_deref(), Some("u-legacy-001"));
        assert_eq!(entry.version.as_deref(), Some("2.0.77"));
    }

    #[test]
    fn parse_user_with_prompt_source_slash_command() {
        let json = r#"{
            "type":"user",
            "uuid":"u-ps-1",
            "sessionId":"s1",
            "promptSource":"slashCommand"
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.prompt_source, Some(PromptSource::SlashCommand));
    }

    #[test]
    fn parse_user_with_prompt_source_unknown_degrades_softly() {
        let json = r#"{
            "type":"user",
            "uuid":"u-ps-2",
            "sessionId":"s1",
            "promptSource":"future_source_42"
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.prompt_source, Some(PromptSource::Unknown));
    }

    #[test]
    fn parse_user_with_origin_and_mcp_meta() {
        let json = r#"{
            "type":"user",
            "uuid":"u-or-1",
            "sessionId":"s1",
            "origin":{"kind":"ide"},
            "mcpMeta":{"structuredContent":{"hello":"world"}}
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let origin = entry.origin.as_ref().unwrap();
        // The survey calls out `ide`-class origins; unknown shapes degrade.
        assert!(origin.kind.is_some());
        // No extra keys here → extra is empty.
        assert!(origin.extra.is_empty());
        let mcp = entry.mcp_meta.as_ref().unwrap();
        assert!(mcp.structured_content.is_some());
        let inner = mcp.structured_content.as_ref().unwrap();
        assert_eq!(inner["hello"], "world");
        assert!(mcp.extra.is_empty());
    }

    // ── Task C: Origin / McpMeta `extra` capture ───────────────────────

    #[test]
    fn parse_origin_preserves_extra_keys() {
        // Hypothetical future surface: an IDE adds `name` + `version` alongside
        // `kind`. Without the extra map these keys would be silently dropped;
        // with it, they round-trip and surface in `.extra`.
        let json = r#"{
            "type":"user",
            "uuid":"u-or-ext-1",
            "sessionId":"s1",
            "origin":{"kind":"ide","name":"vscode","version":"1.85"}
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let origin = entry.origin.as_ref().unwrap();
        assert_eq!(origin.kind, Some(OriginKind::Ide));
        assert_eq!(origin.extra.get("name").and_then(|v| v.as_str()), Some("vscode"));
        assert_eq!(origin.extra.get("version").and_then(|v| v.as_str()), Some("1.85"));
    }

    #[test]
    fn parse_origin_with_unknown_kind_real_data() {
        // Real-data observation 2026-06-09: every `origin` carried
        // `{kind: "task-notification"}` — a value not in the OriginKind enum.
        // Must soft-land in OriginKind::Unknown and leave extra empty.
        let json = r#"{
            "type":"user","uuid":"u","sessionId":"s",
            "origin":{"kind":"task-notification"}
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let origin = entry.origin.as_ref().unwrap();
        assert_eq!(origin.kind, Some(OriginKind::Unknown));
        assert!(origin.extra.is_empty());
    }

    #[test]
    fn parse_mcp_meta_preserves_extra_keys() {
        let json = r#"{
            "type":"user",
            "uuid":"u-mcp-ext-1",
            "sessionId":"s1",
            "mcpMeta":{
                "structuredContent":{"k":"v"},
                "serverName":"gmail",
                "toolName":"searchThreads"
            }
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let mcp = entry.mcp_meta.as_ref().unwrap();
        assert!(mcp.structured_content.is_some());
        assert_eq!(
            mcp.extra.get("serverName").and_then(|v| v.as_str()),
            Some("gmail")
        );
        assert_eq!(
            mcp.extra.get("toolName").and_then(|v| v.as_str()),
            Some("searchThreads")
        );
    }

    #[test]
    fn origin_round_trip_preserves_extras() {
        // Round-trip: deserialise → serialise → original keys still present.
        let json = r#"{"kind":"ide","name":"vscode"}"#;
        let origin: Origin = serde_json::from_str(json).unwrap();
        let v: serde_json::Value = serde_json::to_value(&origin).unwrap();
        assert_eq!(v["kind"], "ide");
        assert_eq!(v["name"], "vscode");
    }

    // ── Task B: image_paste_ids element-typed enum ─────────────────────

    #[test]
    fn parse_user_with_interrupted_and_compact_flags() {
        let json = r#"{
            "type":"user",
            "uuid":"u-ic-1",
            "sessionId":"s1",
            "interruptedMessageId":"a-uuid-1",
            "isCompactSummary":true,
            "isVisibleInTranscriptOnly":false,
            "imagePasteIds":["paste-1","paste-2"]
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.interrupted_message_id.as_deref(), Some("a-uuid-1"));
        assert_eq!(entry.is_compact_summary, Some(true));
        assert_eq!(entry.is_visible_in_transcript_only, Some(false));
        let ids = entry.image_paste_ids.as_ref().unwrap();
        assert_eq!(ids.len(), 2);
        match &ids[0] {
            ImagePasteId::String(s) => assert_eq!(s, "paste-1"),
            other => panic!("expected String id, got {other:?}"),
        }
    }

    #[test]
    fn parse_user_with_integer_image_paste_ids() {
        // Real-data 2026-06-09: 100% of `imagePasteIds` are integer ids.
        let json = r#"{
            "type":"user",
            "uuid":"u-int-1",
            "sessionId":"s1",
            "imagePasteIds":[1,3,4]
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let ids = entry.image_paste_ids.as_ref().unwrap();
        assert_eq!(ids.len(), 3);
        for (got, want) in ids.iter().zip([1, 3, 4]) {
            match got {
                ImagePasteId::Integer(n) => assert_eq!(*n, want),
                other => panic!("expected Integer id, got {other:?}"),
            }
        }
    }

    #[test]
    fn image_paste_ids_mixed_string_and_integer_round_trip() {
        // Mixed array: an integer followed by a string. Round-tripping
        // emits each variant in its original wire shape.
        let json = r#"{
            "type":"user","uuid":"u","sessionId":"s",
            "imagePasteIds":[1,"paste-2",3]
        }"#;
        let entry: UserEntry = serde_json::from_str(json).unwrap();
        let ids = entry.image_paste_ids.as_ref().unwrap();
        assert!(matches!(ids[0], ImagePasteId::Integer(1)));
        assert!(matches!(&ids[1], ImagePasteId::String(s) if s == "paste-2"));
        assert!(matches!(ids[2], ImagePasteId::Integer(3)));
        // Round trip: serialise the ids and ensure the array shape comes back.
        let v: serde_json::Value = serde_json::to_value(ids).unwrap();
        assert_eq!(v[0], 1);
        assert_eq!(v[1], "paste-2");
        assert_eq!(v[2], 3);
    }

    #[test]
    fn content_kind_distinguishes_user_input_from_tool_result() {
        // Real-input case: array of text blocks.
        let user_input = UserMessage {
            role: Some("user".into()),
            content: Some(serde_json::json!([{"type":"text","text":"hi"}])),
        };
        assert_eq!(user_input.content_kind(), Some(UserContentKind::Text));

        // Tool result case: tool_result blocks only.
        let tool_result = UserMessage {
            role: Some("user".into()),
            content: Some(
                serde_json::json!([{"type":"tool_result","tool_use_id":"tu1","content":"done"}]),
            ),
        };
        assert_eq!(
            tool_result.content_kind(),
            Some(UserContentKind::ToolResult)
        );

        // Image + text: a real human-input mode (e.g. paste a screenshot).
        let image = UserMessage {
            role: Some("user".into()),
            content: Some(
                serde_json::json!([{"type":"image","source":{}},{"type":"text","text":"see this"}]),
            ),
        };
        assert_eq!(image.content_kind(), Some(UserContentKind::ImageText));

        // String legacy mode.
        let string = UserMessage {
            role: Some("user".into()),
            content: Some(serde_json::Value::String("hello".into())),
        };
        assert_eq!(string.content_kind(), Some(UserContentKind::String));

        // No content at all.
        let empty = UserMessage {
            role: None,
            content: None,
        };
        assert!(empty.content_kind().is_none());
    }
}
