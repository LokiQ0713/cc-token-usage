//! Layer B: Field fill-rate parity tests.
//!
//! These tests compare the number of lines in a real JSONL fixture that contain
//! a given JSON key (grep count) against the number of parsed entries where the
//! corresponding Rust field is `Some(...)` (parser count).
//!
//! The critical risk this guards against: if a `#[serde(rename = "sourceToolUseID")]`
//! annotation misspells the key (e.g. "sourceToolUseId" with lowercase 'd'), every
//! unit test that constructs its own inline JSON will still pass because the test JSON
//! uses the same wrong key. Only comparing against real JSONL data exposes the bug.
//!
//! Fixtures:
//!   - `fixtures/real_v2_1_140.jsonl` — 665 lines from a v2.1.140 session (all 11 new
//!     fields present).
//!   - `fixtures/real_v2_0_legacy.jsonl` — 176 lines from a v2.1.91 session (pre-
//!     attributionPlugin/attributionSkill, which only appear from v2.1.138+).

use cc_session_jsonl::types::Entry;

// ── helpers to extract field counts per entry type ──

fn count_user_field<F>(fixture: &str, grep_key: &str, field_predicate: F) -> (usize, usize)
where
    F: Fn(&cc_session_jsonl::types::UserEntry) -> bool,
{
    // Grep for lines where this is a top-level field AND the entry type is "user".
    // We use serde_json::Value to check both conditions without going through the typed
    // parser — this keeps the count independent of the parser under test.
    let top_level_grep = fixture
        .lines()
        .filter(|l| !l.trim().is_empty() && l.contains(grep_key))
        .filter(|l| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(obj) = v.as_object() {
                    obj.get("type").and_then(|t| t.as_str()) == Some("user")
                        && obj.contains_key(grep_key.trim_matches('"'))
                } else {
                    false
                }
            } else {
                false
            }
        })
        .count();

    let parsed_count = fixture
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
        .filter(|e| matches!(e, Entry::User(u) if field_predicate(u)))
        .count();

    (parsed_count, top_level_grep)
}

fn count_assistant_field<F>(fixture: &str, grep_key: &str, field_predicate: F) -> (usize, usize)
where
    F: Fn(&cc_session_jsonl::types::AssistantEntry) -> bool,
{
    let top_level_grep = fixture
        .lines()
        .filter(|l| !l.trim().is_empty() && l.contains(grep_key))
        .filter(|l| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(obj) = v.as_object() {
                    obj.get("type").and_then(|t| t.as_str()) == Some("assistant")
                        && obj.contains_key(grep_key.trim_matches('"'))
                } else {
                    false
                }
            } else {
                false
            }
        })
        .count();

    let parsed_count = fixture
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
        .filter(|e| matches!(e, Entry::Assistant(a) if field_predicate(a)))
        .count();

    (parsed_count, top_level_grep)
}

fn count_system_field<F>(fixture: &str, grep_key: &str, field_predicate: F) -> (usize, usize)
where
    F: Fn(&cc_session_jsonl::types::SystemEntry) -> bool,
{
    let top_level_grep = fixture
        .lines()
        .filter(|l| !l.trim().is_empty() && l.contains(grep_key))
        .filter(|l| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(obj) = v.as_object() {
                    obj.get("type").and_then(|t| t.as_str()) == Some("system")
                        && obj.contains_key(grep_key.trim_matches('"'))
                } else {
                    false
                }
            } else {
                false
            }
        })
        .count();

    let parsed_count = fixture
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
        .filter(|e| matches!(e, Entry::System(s) if field_predicate(s)))
        .count();

    (parsed_count, top_level_grep)
}

fn count_attachment_field<F>(fixture: &str, grep_key: &str, field_predicate: F) -> (usize, usize)
where
    F: Fn(&cc_session_jsonl::types::AttachmentEntry) -> bool,
{
    let top_level_grep = fixture
        .lines()
        .filter(|l| !l.trim().is_empty() && l.contains(grep_key))
        .filter(|l| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(obj) = v.as_object() {
                    obj.get("type").and_then(|t| t.as_str()) == Some("attachment")
                        && obj.contains_key(grep_key.trim_matches('"'))
                } else {
                    false
                }
            } else {
                false
            }
        })
        .count();

    let parsed_count = fixture
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
        .filter(|e| matches!(e, Entry::Attachment(a) if field_predicate(a)))
        .count();

    (parsed_count, top_level_grep)
}

// ── Layer B: v2.1.140 new-field parity tests ──

const FIXTURE_NEW: &str = include_str!("fixtures/real_v2_1_140.jsonl");
const FIXTURE_LEGACY: &str = include_str!("fixtures/real_v2_0_legacy.jsonl");

// ── UserEntry new fields ──

#[test]
fn fill_rate_is_meta() {
    let (parsed, top_level) = count_user_field(FIXTURE_NEW, "isMeta", |u| u.is_meta.is_some());
    assert_eq!(
        top_level, parsed,
        "isMeta fill-rate mismatch: JSONL has {} top-level occurrences in user entries, \
         parser produced {} Some values",
        top_level, parsed
    );
    // Sanity: fixture must have at least one occurrence
    assert!(
        parsed > 0,
        "fixture must contain at least one isMeta in a user entry"
    );
}

#[test]
fn fill_rate_permission_mode_user() {
    let (parsed, top_level) = count_user_field(FIXTURE_NEW, "permissionMode", |u| {
        u.permission_mode.is_some()
    });
    assert_eq!(
        top_level, parsed,
        "permissionMode fill-rate mismatch (user): JSONL has {} top-level occurrences, \
         parser produced {} Some values",
        top_level, parsed
    );
    assert!(
        parsed > 0,
        "fixture must contain at least one permissionMode in a user entry"
    );
}

#[test]
fn fill_rate_tool_use_result() {
    let (parsed, top_level) = count_user_field(FIXTURE_NEW, "toolUseResult", |u| {
        u.tool_use_result.is_some()
    });
    assert_eq!(
        top_level, parsed,
        "toolUseResult fill-rate mismatch: JSONL has {} top-level occurrences in user entries, \
         parser produced {} Some values",
        top_level, parsed
    );
    assert!(
        parsed > 0,
        "fixture must contain at least one toolUseResult in a user entry"
    );
}

#[test]
fn fill_rate_source_tool_use_id() {
    // Critical test: sourceToolUseID uses capital ID. If rename annotation is wrong
    // (e.g. "sourceToolUseId" lowercase), this will be 0 ≠ top_level and fail.
    let (parsed, top_level) = count_user_field(FIXTURE_NEW, "sourceToolUseID", |u| {
        u.source_tool_use_id.is_some()
    });
    assert_eq!(
        top_level, parsed,
        "sourceToolUseID fill-rate mismatch: JSONL has {} top-level occurrences in user entries, \
         parser produced {} Some values. \
         Check #[serde(rename = \"sourceToolUseID\")] — capital ID required.",
        top_level, parsed
    );
    assert!(
        parsed >= 1,
        "fixture must contain at least one sourceToolUseID sample to meaningfully test the \
         camelCase-ID rename; parity 0=0 is trivially true and would mask a typo in \
         #[serde(rename = \"sourceToolUseID\")] or related",
    );
}

#[test]
fn fill_rate_source_tool_assistant_uuid() {
    // Critical test: sourceToolAssistantUUID uses capital UUID.
    let (parsed, top_level) = count_user_field(FIXTURE_NEW, "sourceToolAssistantUUID", |u| {
        u.source_tool_assistant_uuid.is_some()
    });
    assert_eq!(
        top_level, parsed,
        "sourceToolAssistantUUID fill-rate mismatch: JSONL has {} top-level occurrences in user \
         entries, parser produced {} Some values. \
         Check #[serde(rename = \"sourceToolAssistantUUID\")] — capital UUID required.",
        top_level, parsed
    );
    assert!(
        parsed >= 1,
        "fixture must contain at least one sourceToolAssistantUUID sample to meaningfully test \
         the camelCase-ID rename; parity 0=0 is trivially true and would mask a typo in \
         #[serde(rename = \"sourceToolAssistantUUID\")] or related",
    );
}

// ── AssistantEntry new fields ──

#[test]
fn fill_rate_attribution_plugin() {
    // This is the highest-value test: if attributionPlugin fails to parse, all
    // plugin attribution analysis in Phase 2 will silently produce empty results.
    let (parsed, top_level) = count_assistant_field(FIXTURE_NEW, "attributionPlugin", |a| {
        a.attribution_plugin.is_some()
    });
    assert_eq!(
        top_level, parsed,
        "attributionPlugin fill-rate mismatch: JSONL has {} top-level occurrences in assistant \
         entries, parser produced {} Some values",
        top_level, parsed
    );
    assert!(
        parsed > 0,
        "fixture must contain at least one attributionPlugin in an assistant entry"
    );
}

#[test]
fn fill_rate_attribution_skill() {
    let (parsed, top_level) = count_assistant_field(FIXTURE_NEW, "attributionSkill", |a| {
        a.attribution_skill.is_some()
    });
    assert_eq!(
        top_level, parsed,
        "attributionSkill fill-rate mismatch: JSONL has {} top-level occurrences in assistant \
         entries, parser produced {} Some values",
        top_level, parsed
    );
    // Note: attributionSkill may appear fewer times than attributionPlugin (some turns
    // have plugin but no specific skill). Assert parity, not minimum count.
}

// ── SystemEntry new fields ──

#[test]
fn fill_rate_hook_count() {
    let (parsed, top_level) =
        count_system_field(FIXTURE_NEW, "hookCount", |s| s.hook_count().is_some());
    assert_eq!(
        top_level, parsed,
        "hookCount fill-rate mismatch: JSONL has {} top-level occurrences in system entries, \
         parser produced {} Some values",
        top_level, parsed
    );
    assert!(
        parsed > 0,
        "fixture must contain at least one hookCount in a system entry"
    );
}

#[test]
fn fill_rate_hook_infos() {
    let (parsed, top_level) =
        count_system_field(FIXTURE_NEW, "hookInfos", |s| s.hook_infos().is_some());
    assert_eq!(
        top_level, parsed,
        "hookInfos fill-rate mismatch: JSONL has {} top-level occurrences in system entries, \
         parser produced {} Some values",
        top_level, parsed
    );
}

#[test]
fn fill_rate_prevented_continuation() {
    let (parsed, top_level) = count_system_field(FIXTURE_NEW, "preventedContinuation", |s| {
        s.prevented_continuation().is_some()
    });
    assert_eq!(
        top_level, parsed,
        "preventedContinuation fill-rate mismatch: JSONL has {} top-level occurrences in system \
         entries, parser produced {} Some values",
        top_level, parsed
    );
}

#[test]
fn fill_rate_tool_use_id_system() {
    // Critical test: toolUseID uses capital ID. The field also appears as toolUseId
    // (camelCase, lowercase d) in some other contexts, so we must use top-level JSON
    // key matching to count only the exact string "toolUseID".
    let (parsed, top_level) =
        count_system_field(FIXTURE_NEW, "toolUseID", |s| s.tool_use_id().is_some());
    assert_eq!(
        top_level, parsed,
        "toolUseID fill-rate mismatch: JSONL has {} top-level occurrences in system entries, \
         parser produced {} Some values. \
         Check #[serde(rename = \"toolUseID\")] — capital ID required.",
        top_level, parsed
    );
    assert!(
        parsed >= 1,
        "fixture must contain at least one toolUseID sample to meaningfully test the \
         camelCase-ID rename; parity 0=0 is trivially true and would mask a typo in \
         #[serde(rename = \"toolUseID\")] or related",
    );
}

// ── Optional system fields (present if fixture contains them) ──

#[test]
fn fill_rate_hook_errors_if_present() {
    // hookErrors is optional in this test — if the fixture has it, assert parity;
    // if not, the test passes trivially (field simply wasn't generated in that session).
    let top_level_grep = FIXTURE_NEW
        .lines()
        .filter(|l| !l.trim().is_empty() && l.contains("hookErrors"))
        .filter(|l| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(obj) = v.as_object() {
                    obj.get("type").and_then(|t| t.as_str()) == Some("system")
                        && obj.contains_key("hookErrors")
                } else {
                    false
                }
            } else {
                false
            }
        })
        .count();

    if top_level_grep > 0 {
        let parsed = FIXTURE_NEW
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
            .filter(|e| matches!(e, Entry::System(s) if s.hook_errors().is_some()))
            .count();
        assert_eq!(
            top_level_grep, parsed,
            "hookErrors fill-rate mismatch: JSONL has {} top-level occurrences in system entries, \
             parser produced {} Some values",
            top_level_grep, parsed
        );
    }
}

// ── v2.1.159 SystemEntry / AttachmentEntry fields ──

#[test]
fn fill_rate_message_count() {
    // turn_duration system entries carry messageCount. Renamed field
    // (message_count → messageCount); parity guards the rename spelling.
    let (parsed, top_level) =
        count_system_field(FIXTURE_NEW, "messageCount", |s| s.message_count().is_some());
    assert_eq!(
        top_level, parsed,
        "messageCount fill-rate mismatch: JSONL has {} top-level occurrences in system entries, \
         parser produced {} Some values",
        top_level, parsed
    );
    assert!(
        parsed > 0,
        "fixture must contain at least one messageCount in a system entry"
    );
}

#[test]
fn fill_rate_attachment_top_level_field() {
    // Critical: in v2.1.159+ every attachment entry carries its content under the
    // top-level `attachment` object (NOT `message`). If the field is named wrong,
    // all attachment content is silently dropped. Parity must hold and be > 0.
    let (parsed, top_level) =
        count_attachment_field(FIXTURE_NEW, "attachment", |a| a.attachment.is_some());
    assert_eq!(
        top_level, parsed,
        "attachment fill-rate mismatch: JSONL has {} top-level occurrences in attachment \
         entries, parser produced {} Some values. \
         Check the AttachmentEntry `attachment` field name.",
        top_level, parsed
    );
    assert!(
        parsed > 0,
        "fixture must contain at least one attachment entry with a top-level `attachment` object"
    );
}

#[test]
fn fill_rate_attachment_subtype_resolves() {
    // V2 design: AttachmentBody models the top-6 subtypes by volume and
    // collapses the long tail into `AttachmentBody::Unknown`. So the helper
    // resolves to a typed variant for the common cases and lands on
    // `AttachmentBody::Unknown` for everything else. Parity is then:
    //
    //   {attachment entries that have a typed attachment body}
    //     ==
    //   {attachment entries where the JSONL attachment object exists}
    //
    // i.e. every attachment with a nested object lands somewhere in the enum
    // (Some-variant including Unknown).
    let with_any_body = FIXTURE_NEW
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
        .filter(|e| matches!(e, Entry::Attachment(a) if a.attachment.is_some()))
        .count();

    let grep_attachment_obj = FIXTURE_NEW
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| {
            v.get("type").and_then(|t| t.as_str()) == Some("attachment")
                && v.get("attachment").is_some_and(|a| a.is_object())
        })
        .count();

    assert_eq!(
        grep_attachment_obj, with_any_body,
        "every JSONL attachment-with-object must land in some AttachmentBody variant: \
         {} object-bearing entries vs {} parsed bodies",
        grep_attachment_obj, with_any_body
    );
    assert!(
        with_any_body > 0,
        "fixture must contain at least one attachment with a nested object"
    );
}

// ── Layer B: Legacy fixture — new fields must all be absent ──

#[test]
fn fill_rate_attribution_plugin_legacy_zero() {
    // The v2.1.91 fixture predates attribution fields (2.1.138+).
    // Both grep count and parser count must be 0.
    let grep_count = FIXTURE_LEGACY
        .lines()
        .filter(|l| l.contains("\"attributionPlugin\""))
        .count();
    assert_eq!(
        grep_count, 0,
        "legacy fixture must not contain attributionPlugin (found {} occurrences)",
        grep_count
    );

    let parsed = FIXTURE_LEGACY
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
        .filter(|e| matches!(e, Entry::Assistant(a) if a.attribution_plugin.is_some()))
        .count();
    assert_eq!(
        parsed, 0,
        "parser must not produce any Some(attribution_plugin) for the legacy fixture (got {})",
        parsed
    );
}

#[test]
fn fill_rate_attribution_skill_legacy_zero() {
    let grep_count = FIXTURE_LEGACY
        .lines()
        .filter(|l| l.contains("\"attributionSkill\""))
        .count();
    assert_eq!(
        grep_count, 0,
        "legacy fixture must not contain attributionSkill (found {} occurrences)",
        grep_count
    );

    let parsed = FIXTURE_LEGACY
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
        .filter(|e| matches!(e, Entry::Assistant(a) if a.attribution_skill.is_some()))
        .count();
    assert_eq!(
        parsed, 0,
        "parser must not produce any Some(attribution_skill) for the legacy fixture (got {})",
        parsed
    );
}

#[test]
fn fill_rate_source_tool_use_id_legacy_zero() {
    // v2.1.91 predates sourceToolUseID (introduced ~2.1.71). Must be 0.
    let (parsed, top_level) = count_user_field(FIXTURE_LEGACY, "sourceToolUseID", |u| {
        u.source_tool_use_id.is_some()
    });
    assert_eq!(
        top_level, 0,
        "legacy fixture must not have top-level sourceToolUseID in user entries (found {})",
        top_level
    );
    assert_eq!(
        parsed, 0,
        "parser must not produce Some(source_tool_use_id) from legacy fixture (got {})",
        parsed
    );
}

/// Regression guard: the legacy fixture must parse without panics and yield only
/// well-formed entries (no unexpected parse errors from the new fields being absent).
#[test]
fn legacy_fixture_parses_without_errors() {
    let mut parsed_ok = 0usize;
    let mut parse_errors = 0usize;

    for line in FIXTURE_LEGACY.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Entry>(line) {
            Ok(_) => parsed_ok += 1,
            Err(_) => parse_errors += 1,
        }
    }

    // All 176 lines in the legacy fixture are valid JSONL — no errors expected
    assert_eq!(
        parse_errors, 0,
        "legacy fixture produced {} parse errors (expected 0); adding new Option fields \
         must not break backward compatibility",
        parse_errors
    );
    assert!(
        parsed_ok > 0,
        "at least one entry must parse from legacy fixture"
    );
}

/// Regression guard: the new fixture must parse without panics and yield only
/// well-formed entries.
#[test]
fn new_fixture_parses_without_errors() {
    let mut parsed_ok = 0usize;
    let mut parse_errors = 0usize;

    for line in FIXTURE_NEW.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Entry>(line) {
            Ok(_) => parsed_ok += 1,
            Err(_) => parse_errors += 1,
        }
    }

    assert_eq!(
        parse_errors, 0,
        "v2.1.140 fixture produced {} parse errors (expected 0)",
        parse_errors
    );
    assert!(
        parsed_ok > 0,
        "at least one entry must parse from v2.1.140 fixture"
    );
}
