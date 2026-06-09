//! Cross-cutting helpers shared by individual entry types.
//!
//! V2 design philosophy (see `docs/cc-session-jsonl-v2-field-survey.md`):
//!
//! - Each entry type owns its own field list — the legacy `transcript_entry!`
//!   macro is gone. Repeating the 9 truly-universal fields by hand across
//!   ~15 structs is the deliberate cost of letting each type model only what
//!   it actually has.
//! - Low-cardinality enum-shaped fields (e.g. `stopReason`, `permissionMode`,
//!   `entrypoint`) use Rust enums with `#[serde(other)] Unknown` so a brand
//!   new value drifting in from a future Claude Code release degrades to
//!   `Unknown` rather than blowing up the entire entry.
//! - `parentUuid` is `Option<String>` on every type except `AssistantEntry`,
//!   where it is `String` (assistant entries are always replies to *something*).

use serde::{Deserialize, Serialize};

/// Read-only graph keys exposed by every entry type that participates in the
/// JSONL DAG (user / assistant / system / attachment / progress).
///
/// Sparse metadata entries (`ai-title`, `tag`, `permission-mode`, ...) do
/// not implement this trait: they carry no `uuid` and live outside the
/// parent→child chain. The semantic data model document explains the rule.
pub trait DagNode {
    fn uuid(&self) -> Option<&str>;
    fn session_id(&self) -> Option<&str>;
    fn timestamp(&self) -> Option<&str>;
    fn parent_uuid(&self) -> Option<&str>;
    fn is_sidechain(&self) -> Option<bool>;
}

// ─── Low-cardinality enum fields ───────────────────────────────────────────

/// User-set permission mode at turn time. Mirrors the inline `permissionMode`
/// field on user entries and the standalone `permission-mode` switch entries.
///
/// Unknown future values fall through to [`PermissionMode::Unknown`]: serde
/// `#[serde(other)]` only accepts unit variants here, so callers can detect
/// unknown values by matching but cannot recover the original string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    BypassPermissions,
    Plan,
    #[serde(other)]
    Unknown,
}

/// Origin of a user prompt (Claude Code 2.1.140+, `user.promptSource`).
///
/// Real-data examples: `slashCommand`, `text`. New string-valued sources
/// degrade to `Unknown` instead of failing the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptSource {
    Text,
    SlashCommand,
    #[serde(other)]
    Unknown,
}

/// LLM stop reason on the inner `message.stop_reason` field.
///
/// Observed real-data values are limited to `end_turn`, `tool_use`,
/// `stop_sequence`, `pause_turn`, `refusal`. New values degrade to `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    StopSequence,
    PauseTurn,
    Refusal,
    #[serde(other)]
    Unknown,
}

impl StopReason {
    /// Stable wire-format identifier for this stop reason, matching the
    /// JSONL representation. `Unknown` collapses to the literal `"unknown"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            StopReason::EndTurn => "end_turn",
            StopReason::ToolUse => "tool_use",
            StopReason::StopSequence => "stop_sequence",
            StopReason::PauseTurn => "pause_turn",
            StopReason::Refusal => "refusal",
            StopReason::Unknown => "unknown",
        }
    }
}

/// Cache-miss reason on `message.diagnostics.cache_miss_reason.type`.
///
/// Real samples: `tools_changed`, `messages_changed`. The numeric
/// `cache_missed_input_tokens` companion field is preserved as a `u64`
/// alongside this discriminator (see [`crate::types::assistant::Diagnostics`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheMissReasonKind {
    ToolsChanged,
    MessagesChanged,
    SystemPromptChanged,
    #[serde(other)]
    Unknown,
}

/// Discriminator for a user prompt's `origin.kind` field.
///
/// The full list of integrations (CLI, IDE, web, etc.) is not stable across
/// Claude Code versions, so unknown values degrade to [`OriginKind::Unknown`]
/// instead of refusing the parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OriginKind {
    Cli,
    Ide,
    Web,
    Sdk,
    #[serde(other)]
    Unknown,
}
