//! Audit: enumerate every JSONL file that `cc_session_jsonl::load_all_sessions`
//! discovers and compare against `find ~/.claude -name *.jsonl`. Anything `find`
//! finds but the loader skips must be classified as: (a) legitimate exclusion
//! (journal, history, backup), or (b) miss (real session file the loader
//! forgot).
//!
//! The loader's file-layout contract is the public surface; the underlying
//! scanner module is private, so we re-derive each discovered file path from
//! `Session.main_entries` (via its file location) + `Session.agents[*].path`.
//! Main session paths are reconstructed from the project + session id since
//! `Session` carries them implicitly through its `project` + `id` fields.
use cc_session_jsonl::load_all_sessions;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

fn main() -> std::io::Result<()> {
    let home = PathBuf::from(std::env::var("HOME").unwrap()).join(".claude");

    let sessions = load_all_sessions(&home)?;

    // Collect every JSONL path the loader produced: main session file (derived
    // from project+sid) + every agent's `.path`.
    let mut scanned: BTreeSet<String> = BTreeSet::new();
    for s in &sessions {
        if let Some(project) = &s.project {
            let main_path = home
                .join("projects")
                .join(project)
                .join(format!("{}.jsonl", s.id));
            if main_path.is_file() {
                scanned.insert(main_path.display().to_string());
            }
        }
        for a in &s.agents {
            scanned.insert(a.path.display().to_string());
        }
    }

    let out = Command::new("find")
        .arg(&home)
        .args(["-name", "*.jsonl", "-type", "f"])
        .output()?;
    let all: BTreeSet<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();

    let missed: Vec<&String> = all.difference(&scanned).collect();
    let extra: Vec<&String> = scanned.difference(&all).collect();

    println!("loader found:   {}", scanned.len());
    println!("find found:     {}", all.len());
    println!("missed by loader (find ∖ loader): {}", missed.len());
    println!("loader extra (loader ∖ find):    {}", extra.len());

    let mut buckets: std::collections::BTreeMap<&str, Vec<&String>> = Default::default();
    for p in &missed {
        let k: &str = if p.contains("projects-title-backup-") {
            "backup-dir"
        } else if p.contains("/subagents/workflows/") && p.ends_with("/journal.jsonl") {
            "workflow-journal"
        } else if p.ends_with("/history.jsonl") {
            "history"
        } else if p.contains("/metrics/") {
            "metrics"
        } else if p.contains("/todos/") {
            "todos"
        } else if p.contains("/statsig/") {
            "statsig"
        } else if p.contains("/ide/") {
            "ide-state"
        } else if p.contains("/shell-snapshots/") {
            "shell-snapshots"
        } else if p.contains("/state/") {
            "state"
        } else if p.contains("/preferences") {
            "preferences"
        } else if p.contains("/homunculus/") {
            "homunculus-observations"
        } else {
            "OTHER (suspicious)"
        };
        buckets.entry(k).or_default().push(p);
    }

    println!("\n── missed-by-loader breakdown ──");
    for (k, v) in &buckets {
        println!("  {:<22} {:>4}", k, v.len());
        if *k == "OTHER (suspicious)" {
            for p in v {
                println!("    {}", p);
            }
        } else if v.len() <= 3 {
            for p in v {
                println!("    e.g. {}", p);
            }
        } else {
            for p in v.iter().take(2) {
                println!("    e.g. {}", p);
            }
        }
    }

    let suspicious_count = buckets
        .get("OTHER (suspicious)")
        .map(|v| v.len())
        .unwrap_or(0);
    println!();
    if suspicious_count == 0 {
        println!("✓ 全部 missed 文件都属于已知 legitimate exclusion；loader 覆盖完整。");
    } else {
        println!("✗ 有 {} 个未分类 missed 文件，需要检查 loader 是否漏了。", suspicious_count);
    }

    Ok(())
}
