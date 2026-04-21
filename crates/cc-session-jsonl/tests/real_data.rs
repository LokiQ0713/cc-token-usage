//! Real data validation test.
//!
//! This test scans the actual `~/.claude/projects/` directory, opens every JSONL
//! file with LenientReader, and verifies that the parse error rate is below 1%.
//!
//! Run with: `cargo test -p cc-session-jsonl --all-features -- --ignored real_data`

use std::collections::HashMap;
use std::path::PathBuf;

use cc_session_jsonl::types::Entry;
use cc_session_jsonl::SessionReader;

fn claude_projects_dir() -> Option<PathBuf> {
    let home = dirs_next().or_else(dirs_env)?;
    let projects = home.join(".claude").join("projects");
    if projects.is_dir() {
        Some(projects)
    } else {
        None
    }
}

fn dirs_next() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn dirs_env() -> Option<PathBuf> {
    std::env::var("USERPROFILE").ok().map(PathBuf::from)
}

fn find_jsonl_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip well-known non-session dirs
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "memory" || name == "tool-results" {
                    continue;
                }
                result.extend(find_jsonl_files(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                result.push(path);
            }
        }
    }
    result
}

fn entry_type_name(entry: &Entry) -> &'static str {
    match entry {
        Entry::User(_) => "user",
        Entry::Assistant(_) => "assistant",
        Entry::System(_) => "system",
        Entry::Attachment(_) => "attachment",
        Entry::Summary(_) => "summary",
        Entry::CustomTitle(_) => "custom-title",
        Entry::AiTitle(_) => "ai-title",
        Entry::LastPrompt(_) => "last-prompt",
        Entry::TaskSummary(_) => "task-summary",
        Entry::Tag(_) => "tag",
        Entry::AgentName(_) => "agent-name",
        Entry::AgentColor(_) => "agent-color",
        Entry::AgentSetting(_) => "agent-setting",
        Entry::PrLink(_) => "pr-link",
        Entry::Mode(_) => "mode",
        Entry::PermissionMode(_) => "permission-mode",
        Entry::Progress(_) => "progress",
        Entry::QueueOperation(_) => "queue-operation",
        Entry::SpeculationAccept(_) => "speculation-accept",
        Entry::WorktreeState(_) => "worktree-state",
        Entry::ContentReplacement(_) => "content-replacement",
        Entry::FileHistorySnapshot(_) => "file-history-snapshot",
        Entry::AttributionSnapshot(_) => "attribution-snapshot",
        Entry::ContextCollapseCommit(_) => "marble-origami-commit",
        Entry::ContextCollapseSnapshot(_) => "marble-origami-snapshot",
        Entry::Unknown => "unknown",
    }
}

#[test]
#[ignore]
fn real_data_parse_all_sessions() {
    let projects_dir = match claude_projects_dir() {
        Some(p) => p,
        None => {
            eprintln!("Skipping real_data test: ~/.claude/projects/ not found");
            return;
        }
    };

    let jsonl_files = find_jsonl_files(&projects_dir);
    if jsonl_files.is_empty() {
        eprintln!("Skipping real_data test: no JSONL files found");
        return;
    }

    let mut total_entries: usize = 0;
    let mut total_errors: usize = 0;
    let mut type_counts: HashMap<&'static str, usize> = HashMap::new();
    let mut files_processed: usize = 0;
    let mut files_failed_to_open: usize = 0;

    for path in &jsonl_files {
        let reader = match SessionReader::open(path) {
            Ok(r) => r,
            Err(_) => {
                files_failed_to_open += 1;
                continue;
            }
        };

        let mut lenient = reader.lenient();
        for entry in lenient.by_ref() {
            let type_name = entry_type_name(&entry);
            *type_counts.entry(type_name).or_insert(0) += 1;
            total_entries += 1;
        }

        total_errors += lenient.errors_skipped();
        files_processed += 1;
    }

    let total = total_entries + total_errors;
    let error_rate = if total > 0 {
        total_errors as f64 / total as f64
    } else {
        0.0
    };

    // Print summary
    eprintln!("\n=== Real Data Parse Summary ===");
    eprintln!("Files found:          {}", jsonl_files.len());
    eprintln!("Files processed:      {files_processed}");
    eprintln!("Files failed to open: {files_failed_to_open}");
    eprintln!("Total entries parsed: {total_entries}");
    eprintln!("Total errors skipped: {total_errors}");
    eprintln!("Error rate:           {:.4}%", error_rate * 100.0);
    eprintln!();
    eprintln!("Entry type distribution:");

    let mut sorted_types: Vec<_> = type_counts.iter().collect();
    sorted_types.sort_by(|a, b| b.1.cmp(a.1));
    for (type_name, count) in &sorted_types {
        let pct = if total_entries > 0 {
            **count as f64 / total_entries as f64 * 100.0
        } else {
            0.0
        };
        eprintln!("  {type_name:<30} {count:>8} ({pct:.2}%)");
    }
    eprintln!("================================\n");

    // Assert error rate < 1%
    assert!(
        error_rate < 0.01,
        "Parse error rate {:.4}% exceeds 1% threshold ({total_errors} errors out of {total} total lines)",
        error_rate * 100.0,
    );

    // Sanity: we should have found at least some entries
    assert!(
        total_entries > 0,
        "Expected to find at least some entries in real data"
    );
}
