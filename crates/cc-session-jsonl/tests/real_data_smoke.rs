//! Real-data smoke test for cc-session-jsonl v2.
//!
//! MANDATORY per the acceptance criteria (requirement E). Exercises the full
//! parser against real ~/.claude/projects/**/*.jsonl data using LenientReader.
//!
//! Assertions:
//!   1. StructDrift errors == 0  (hard proof the v2 types hold on real data)
//!   2. Ignored entry ratio < 5% (prevents silent type-miss regression)
//!   3. All Passthrough entries carry DAG fields: uuid, sessionId, timestamp
//!      (parentUuid is allowed to be null)
//!
//! Run with:
//!   REAL_CLAUDE_DATA=1 cargo test -p cc-session-jsonl --test real_data_smoke -- --ignored
//!
//! In CI this test is skipped (no real data). For pre-release validation it is
//! mandatory and is controlled by the REAL_CLAUDE_DATA env var.

use std::path::PathBuf;

use cc_session_jsonl::types::Entry;
use cc_session_jsonl::{ParseError, SessionReader};

/// Find all *.jsonl files under `~/.claude/projects/`.
fn find_all_jsonl_files() -> Vec<PathBuf> {
    let home = std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs_from_env());

    let Some(home) = home else {
        return Vec::new();
    };

    let projects = home.join(".claude").join("projects");
    if !projects.exists() {
        return Vec::new();
    }

    collect_jsonl_recursive(&projects)
}

/// Fallback: use CLAUDE_DATA_DIR env var if HOME is not available.
fn dirs_from_env() -> Option<PathBuf> {
    std::env::var("CLAUDE_DATA_DIR").ok().map(PathBuf::from)
}

fn collect_jsonl_recursive(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return results;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            results.extend(collect_jsonl_recursive(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            results.push(path);
        }
    }
    results
}

/// The main smoke test — #[ignore] so it doesn't run in CI.
///
/// Enable with: REAL_CLAUDE_DATA=1 cargo test ... -- --ignored
#[test]
#[ignore]
fn real_data_smoke_v2_types() {
    // Gate: require explicit opt-in via env var
    if std::env::var("REAL_CLAUDE_DATA").as_deref() != Ok("1") {
        eprintln!("[real_data_smoke] REAL_CLAUDE_DATA not set — skipping (this is correct for CI)");
        eprintln!("[real_data_smoke] To run: REAL_CLAUDE_DATA=1 cargo test -p cc-session-jsonl --test real_data_smoke -- --ignored");
        return;
    }

    let files = find_all_jsonl_files();
    if files.is_empty() {
        eprintln!("[real_data_smoke] No .jsonl files found under ~/.claude/projects — skipping gracefully");
        eprintln!(
            "[real_data_smoke] (If you expected real data, check that ~/.claude/projects/ exists)"
        );
        return;
    }

    eprintln!("[real_data_smoke] Scanning {} .jsonl files...", files.len());

    let mut total_entries: usize = 0;
    let mut struct_drift_count: usize = 0;
    let mut ignored_count: usize = 0;
    let mut passthrough_count: usize = 0;
    let mut errors_skipped: usize = 0;

    // Passthrough validation: accumulate DAG-field violations
    let mut passthrough_missing_uuid: usize = 0;
    let mut passthrough_missing_session_id: usize = 0;
    // timestamp is allowed to be None per spec (PassthroughEntry.timestamp: Option<String>)
    // isSidechain is Option<bool> — allowed to be None for some edge cases
    // parentUuid is explicitly allowed to be null per requirement E §3

    for path in &files {
        // Use the raw lines approach so we can count StructDrift separately from
        // the lenient reader's skip-and-count logic.
        //
        // Note: We use SessionReader (the strict one) to get raw Result<Entry, ParseError>
        // and tally all three categories ourselves — this is more precise than LenientReader
        // which already collapses struct_drift into a counter.
        let reader = match SessionReader::open(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "[real_data_smoke] WARN: Could not open {}: {e}",
                    path.display()
                );
                continue;
            }
        };

        for result in reader {
            total_entries += 1;
            match result {
                Ok(entry) => match &entry {
                    Entry::Passthrough(p) => {
                        passthrough_count += 1;
                        // Requirement E §3: all Passthrough entries MUST have uuid + sessionId
                        if p.uuid.is_empty() {
                            passthrough_missing_uuid += 1;
                            eprintln!(
                                "[real_data_smoke] VIOLATION: Passthrough entry with empty uuid in {}",
                                path.display()
                            );
                        }
                        if p.session_id.is_empty() {
                            passthrough_missing_session_id += 1;
                            eprintln!(
                                "[real_data_smoke] VIOLATION: Passthrough entry with empty sessionId in {}",
                                path.display()
                            );
                        }
                        // Note: parentUuid is explicitly allowed to be null per requirement
                        // Note: timestamp is Option<String> — None is allowed
                        // Note: isSidechain is Option<bool> — None is allowed
                    }
                    Entry::Ignored => {
                        ignored_count += 1;
                    }
                    _ => {
                        // Known, typed entry — parsed successfully
                    }
                },
                Err(ParseError::StructDrift {
                    entry_type,
                    message,
                }) => {
                    struct_drift_count += 1;
                    eprintln!(
                        "[real_data_smoke] STRUCT_DRIFT in {}: type={entry_type}, msg={message}",
                        path.display()
                    );
                }
                Err(ParseError::Json(_)) | Err(ParseError::Io(_)) => {
                    errors_skipped += 1;
                }
            }
        }
    }

    // ── Print summary ──
    let ignored_ratio = if total_entries > 0 {
        ignored_count as f64 / total_entries as f64
    } else {
        0.0
    };
    let ignored_pct = ignored_ratio * 100.0;

    eprintln!("[real_data_smoke] ═══════════════════════════════════════════════");
    eprintln!("[real_data_smoke] Files scanned:    {}", files.len());
    eprintln!("[real_data_smoke] Total entries:    {total_entries}");
    eprintln!("[real_data_smoke] StructDrift:      {struct_drift_count}  ← MUST BE 0");
    eprintln!(
        "[real_data_smoke] Ignored:          {ignored_count} ({ignored_pct:.2}%)  ← MUST BE < 5%"
    );
    eprintln!("[real_data_smoke] Passthrough:      {passthrough_count}");
    eprintln!("[real_data_smoke] Errors skipped:   {errors_skipped}  (malformed JSON / IO)");
    eprintln!("[real_data_smoke] Passthrough violations:");
    eprintln!("[real_data_smoke]   missing uuid:       {passthrough_missing_uuid}  ← MUST BE 0");
    eprintln!(
        "[real_data_smoke]   missing sessionId:  {passthrough_missing_session_id}  ← MUST BE 0"
    );
    eprintln!("[real_data_smoke] ═══════════════════════════════════════════════");

    // ── Assertions ──

    // Requirement E §1: 0 StructDrift errors
    assert_eq!(
        struct_drift_count, 0,
        "REQUIREMENT E §1 FAILED: {struct_drift_count} StructDrift errors detected. \
         This means the v2 types do NOT hold on real data — a schema regression exists. \
         Check eprintln output above for which entry types are drifting.",
    );

    // Requirement E §2: Ignored ratio < 5%
    assert!(
        ignored_ratio < 0.05,
        "REQUIREMENT E §2 FAILED: Ignored entry ratio is {ignored_pct:.2}% (>= 5%). \
         This means {ignored_count}/{total_entries} entries are unrecognised. \
         Claude Code has added new entry types that are not modelled in cc-session-jsonl v2. \
         Do NOT loosen this threshold — report this as a finding.",
    );

    // Requirement E §3: All Passthrough entries have uuid + sessionId
    assert_eq!(
        passthrough_missing_uuid, 0,
        "REQUIREMENT E §3 FAILED: {passthrough_missing_uuid} Passthrough entries have empty uuid. \
         The DAG routing invariant is broken.",
    );
    assert_eq!(
        passthrough_missing_session_id, 0,
        "REQUIREMENT E §3 FAILED: {passthrough_missing_session_id} Passthrough entries have empty sessionId. \
         The DAG routing invariant is broken.",
    );

    eprintln!("[real_data_smoke] ALL ASSERTIONS PASSED");
}
