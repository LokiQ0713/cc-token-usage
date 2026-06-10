//! Audit: enumerate every file `scanner::scan_sessions` returns and compare
//! against `find ~/.claude -name *.jsonl`. Anything `find` finds but the
//! scanner skips must be classified as: (a) legitimate exclusion (journal,
//! history, backup), or (b) miss (real session file the scanner forgot).
use cc_session_jsonl::scanner;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

fn main() -> std::io::Result<()> {
    let home = PathBuf::from(std::env::var("HOME").unwrap()).join(".claude");

    let scanned: BTreeSet<String> = scanner::scan_sessions(&home)?
        .into_iter()
        .map(|sf| sf.path.display().to_string())
        .collect();

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

    println!("scanner found:  {}", scanned.len());
    println!("find found:     {}", all.len());
    println!("missed by scanner (find ∖ scanner): {}", missed.len());
    println!("scanner extra (scanner ∖ find):    {}", extra.len());

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

    println!("\n── missed-by-scanner breakdown ──");
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
        println!("✓ 全部 missed 文件都属于已知 legitimate exclusion；scanner 覆盖完整。");
    } else {
        println!("✗ 有 {} 个未分类 missed 文件，需要检查 scanner 是否漏了。", suspicious_count);
    }

    Ok(())
}
