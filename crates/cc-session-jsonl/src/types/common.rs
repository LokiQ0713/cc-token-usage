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
/// Real-data variants observed on this machine: `bypassPermissions`, `auto`,
/// `default`, `plan`. `acceptEdits` is documented in the Claude Code CLI but
/// has 0 hits in the surveyed dataset; it is kept for spec completeness.
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
    /// Observed in real data alongside `bypassPermissions` and `default`;
    /// not documented in the field survey but emitted by the CLI when the
    /// user is running with `--auto` style flow.
    Auto,
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

/// `userType` discriminator carried on user/assistant entries (100% fill rate
/// in the survey).
///
/// Real data on the surveyed machine shows only `external`. Other values are
/// possible (e.g. an `internal` Anthropic-only path) but unobserved here, so
/// new strings degrade to [`UserType::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UserType {
    External,
    #[serde(other)]
    Unknown,
}

/// `entrypoint` discriminator carried on user/assistant entries (100% fill
/// rate in the survey).
///
/// Real-data values: `cli` (dominant), `sdk-cli` (rare; the Anthropic SDK
/// command-line shim). Future entrypoints (IDE plug-ins, web) degrade to
/// [`Entrypoint::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Entrypoint {
    Cli,
    SdkCli,
    #[serde(other)]
    Unknown,
}

/// `usage.service_tier` — Anthropic API service tier the request ran against.
///
/// Real-data values on this machine: `standard` only. The Messages API spec
/// also defines `batch` for batched inference and `priority` for the priority
/// tier; both are kept so a switchover doesn't blow up the parse. New tier
/// names land in [`ServiceTier::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    Standard,
    Batch,
    Priority,
    #[serde(other)]
    Unknown,
}

/// `usage.speed` — request speed bucket reported by the API (Claude Code
/// 2.1.x+, ~74% fill rate; only the newer assistant turns carry it).
///
/// Real-data values: `standard` only. Anthropic's speed-tier surface is
/// still small; new buckets degrade to [`Speed::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Speed {
    Standard,
    #[serde(other)]
    Unknown,
}

/// `usage.inference_geo` — geographic region tag emitted alongside the
/// usage block.
///
/// Real-data values observed: `not_available` (~65% of carrying turns),
/// empty-string `""` (~35%), and `null`/absent (~0.2%). The empty-string
/// case gets a dedicated [`InferenceGeo::Empty`] variant via `rename = ""`
/// so a sizeable real-data value doesn't get conflated with "unknown to us";
/// truly novel strings still fall through to [`InferenceGeo::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceGeo {
    NotAvailable,
    /// The API sometimes reports an empty string when the geo cannot be
    /// determined. Surveyed at ~8,886 occurrences in one user's data —
    /// large enough to model explicitly rather than fold into `Unknown`.
    #[serde(rename = "")]
    Empty,
    #[serde(other)]
    Unknown,
}

impl InferenceGeo {
    /// Stable wire-format identifier, matching the JSONL representation.
    /// `Unknown` collapses to the literal `"unknown"` for diagnostic output.
    pub fn as_str(&self) -> &'static str {
        match self {
            InferenceGeo::NotAvailable => "not_available",
            InferenceGeo::Empty => "",
            InferenceGeo::Unknown => "unknown",
        }
    }
}

impl ServiceTier {
    /// Stable wire-format identifier, matching the JSONL representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceTier::Standard => "standard",
            ServiceTier::Batch => "batch",
            ServiceTier::Priority => "priority",
            ServiceTier::Unknown => "unknown",
        }
    }
}

impl Speed {
    /// Stable wire-format identifier, matching the JSONL representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Speed::Standard => "standard",
            Speed::Unknown => "unknown",
        }
    }
}

/// Top-level `error` discriminator on assistant entries flagged
/// `isApiErrorMessage: true` (synthetic entries the CLI writes when the API
/// call failed; see `AssistantEntry.error`).
///
/// Real-data values: `rate_limit`, `authentication_failed`, `server_error`,
/// `oauth_org_not_allowed`, plus the literal string `"unknown"` which is the
/// API's own catch-all category. To avoid a name clash with `serde(other)`,
/// the catch-all soft-landing variant is named [`AssistantError::Other`]:
///
///   - the literal string `"unknown"` deserializes to
///     [`AssistantError::Unknown`] (the API's documented unknown bucket)
///   - any other unseen string deserializes to [`AssistantError::Other`]
///     (our drift bucket — caller didn't know about it yet)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantError {
    RateLimit,
    AuthenticationFailed,
    ServerError,
    OauthOrgNotAllowed,
    /// API-reported unknown error — matches the literal string `"unknown"`,
    /// which appeared 9 times in the surveyed assistant entries.
    Unknown,
    /// Drift soft-landing for error categories the parser hasn't seen yet.
    /// Distinct from `Unknown` (which is the literal value the API emits).
    #[serde(other)]
    Other,
}

impl AssistantError {
    /// Stable wire-format identifier, matching the JSONL representation.
    /// `Other` collapses to `"other"` — distinct from the literal `"unknown"`
    /// bucket so diagnostic logs can tell them apart.
    pub fn as_str(&self) -> &'static str {
        match self {
            AssistantError::RateLimit => "rate_limit",
            AssistantError::AuthenticationFailed => "authentication_failed",
            AssistantError::ServerError => "server_error",
            AssistantError::OauthOrgNotAllowed => "oauth_org_not_allowed",
            AssistantError::Unknown => "unknown",
            AssistantError::Other => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── PermissionMode ──
    #[test]
    fn permission_mode_known_variants_parse() {
        let bp: PermissionMode = serde_json::from_str("\"bypassPermissions\"").unwrap();
        assert_eq!(bp, PermissionMode::BypassPermissions);
        let auto: PermissionMode = serde_json::from_str("\"auto\"").unwrap();
        assert_eq!(auto, PermissionMode::Auto);
    }

    #[test]
    fn permission_mode_unknown_soft_lands() {
        let v: PermissionMode = serde_json::from_str("\"future_mode_xyz\"").unwrap();
        assert_eq!(v, PermissionMode::Unknown);
    }

    // ── UserType ──
    #[test]
    fn user_type_external_parses() {
        let v: UserType = serde_json::from_str("\"external\"").unwrap();
        assert_eq!(v, UserType::External);
    }

    #[test]
    fn user_type_unknown_soft_lands() {
        let v: UserType = serde_json::from_str("\"internal\"").unwrap();
        assert_eq!(v, UserType::Unknown);
    }

    // ── Entrypoint ──
    #[test]
    fn entrypoint_known_variants_parse() {
        let v: Entrypoint = serde_json::from_str("\"cli\"").unwrap();
        assert_eq!(v, Entrypoint::Cli);
        let v: Entrypoint = serde_json::from_str("\"sdk-cli\"").unwrap();
        assert_eq!(v, Entrypoint::SdkCli);
    }

    #[test]
    fn entrypoint_unknown_soft_lands() {
        let v: Entrypoint = serde_json::from_str("\"vscode-extension\"").unwrap();
        assert_eq!(v, Entrypoint::Unknown);
    }

    // ── ServiceTier ──
    #[test]
    fn service_tier_known_variants_parse() {
        let v: ServiceTier = serde_json::from_str("\"standard\"").unwrap();
        assert_eq!(v, ServiceTier::Standard);
    }

    #[test]
    fn service_tier_unknown_soft_lands() {
        let v: ServiceTier = serde_json::from_str("\"futuretier\"").unwrap();
        assert_eq!(v, ServiceTier::Unknown);
    }

    // ── Speed ──
    #[test]
    fn speed_standard_parses() {
        let v: Speed = serde_json::from_str("\"standard\"").unwrap();
        assert_eq!(v, Speed::Standard);
    }

    #[test]
    fn speed_unknown_soft_lands() {
        let v: Speed = serde_json::from_str("\"turbo\"").unwrap();
        assert_eq!(v, Speed::Unknown);
    }

    // ── InferenceGeo ──
    #[test]
    fn inference_geo_known_variants_parse() {
        let v: InferenceGeo = serde_json::from_str("\"not_available\"").unwrap();
        assert_eq!(v, InferenceGeo::NotAvailable);
    }

    #[test]
    fn inference_geo_empty_string_is_typed_variant() {
        // Real data has 8886 instances of `""` for inference_geo. Cleanest
        // representation is a dedicated `Empty` variant via #[serde(rename = "")].
        let v: InferenceGeo = serde_json::from_str("\"\"").unwrap();
        assert_eq!(v, InferenceGeo::Empty);
    }

    #[test]
    fn inference_geo_unknown_soft_lands() {
        let v: InferenceGeo = serde_json::from_str("\"us-west\"").unwrap();
        assert_eq!(v, InferenceGeo::Unknown);
    }

    // ── AssistantError ──
    #[test]
    fn assistant_error_known_variants_parse() {
        let v: AssistantError = serde_json::from_str("\"rate_limit\"").unwrap();
        assert_eq!(v, AssistantError::RateLimit);
        let v: AssistantError = serde_json::from_str("\"authentication_failed\"").unwrap();
        assert_eq!(v, AssistantError::AuthenticationFailed);
        let v: AssistantError = serde_json::from_str("\"server_error\"").unwrap();
        assert_eq!(v, AssistantError::ServerError);
        let v: AssistantError = serde_json::from_str("\"oauth_org_not_allowed\"").unwrap();
        assert_eq!(v, AssistantError::OauthOrgNotAllowed);
    }

    #[test]
    fn assistant_error_literal_unknown_parses_as_unknown_variant() {
        // Real data has 9 entries with `error: "unknown"`. That's the API's
        // own catch-all category — distinct from drift.
        let v: AssistantError = serde_json::from_str("\"unknown\"").unwrap();
        assert_eq!(v, AssistantError::Unknown);
        assert_eq!(v.as_str(), "unknown");
    }

    #[test]
    fn assistant_error_drift_lands_in_other() {
        // A future error string the parser hasn't seen.
        let v: AssistantError = serde_json::from_str("\"future_error_xyz\"").unwrap();
        assert_eq!(v, AssistantError::Other);
        assert_eq!(v.as_str(), "other");
    }
}
