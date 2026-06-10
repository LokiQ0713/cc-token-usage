//! Probe: build the conversation DAG using ONLY the v2 `DagNode` trait,
//! grouped by `entry.sessionId` (not by file path / file layout).
//!
//! Insight: subagent and workflow-agent JSONL files carry the **parent**
//! `sessionId`, so a "logical session" = every entry across the main file
//! and all its agent files that share the same `sessionId`. We merge them
//! and ask: how many independent chains exist, and what's left over?
//!
//! Oracle: every entry that has raw `uuid` + `parentUuid` must be reachable
//! through `DagNode`. Any leftover (i.e. classified as metadata but actually
//! carries DAG keys) means a `DagNode` impl is missing.

use cc_session_jsonl::parser::SessionReader;
use cc_session_jsonl::scanner;
use cc_session_jsonl::types::{DagNode, Entry};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

fn entry_type(e: &Entry) -> &'static str {
    match e {
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
        Entry::Passthrough(_) => "__passthrough",
        Entry::Ignored => "__ignored",
    }
}

fn as_dag(e: &Entry) -> Option<&dyn DagNode> {
    match e {
        Entry::User(u) => Some(u),
        Entry::Assistant(a) => Some(a),
        Entry::System(s) => Some(s),
        Entry::Attachment(a) => Some(a),
        Entry::Progress(p) => Some(p),
        Entry::Passthrough(p) => Some(p),
        _ => None,
    }
}

#[derive(Debug, Default, Clone)]
struct LeakSample {
    count: usize,
    sidechain_count: usize,
    main_chain_count: usize,
    example_uuid: String,
    example_parent: Option<String>,
    example_session: String,
    example_file: String,
}

#[derive(Debug, Clone)]
struct Node {
    uuid: String,
    parent: Option<String>,
    sidechain: bool,
    ts: String,
}

#[derive(Debug, Default, Clone)]
struct SessionStats {
    session_id: String,
    files: BTreeSet<String>,
    total_nodes: usize,
    sidechain_nodes: usize,
    roots_proper: usize, // parent == None
    orphans: usize,      // parent != None but not in session
    longest_chain: usize,
    branch_points: usize,
}

/// returns (roots_proper, orphans, longest, branches)
fn build_session(nodes: &[Node]) -> (usize, usize, usize, usize) {
    let uuids: BTreeSet<&str> = nodes.iter().map(|n| n.uuid.as_str()).collect();
    let mut children: HashMap<&str, Vec<&Node>> = HashMap::new();
    let mut roots_proper = 0usize;
    let mut orphans = 0usize;
    for n in nodes {
        match n.parent.as_deref() {
            None => roots_proper += 1,
            Some(p) => {
                if uuids.contains(p) {
                    children.entry(p).or_default().push(n);
                } else {
                    orphans += 1;
                }
            }
        }
    }
    for v in children.values_mut() {
        v.sort_by(|a, b| a.ts.cmp(&b.ts));
    }
    let branch_points = children.values().filter(|v| v.len() > 1).count();

    // 走链：起点 = 真根 ∪ orphan（父找不到的节点视作新链头）
    let mut longest = 0usize;
    for r in nodes.iter().filter(|n| {
        n.parent.is_none()
            || n.parent
                .as_deref()
                .is_some_and(|p| !uuids.contains(p))
    }) {
        let mut len = 1usize;
        let mut cursor: &Node = r;
        loop {
            let Some(kids) = children.get(cursor.uuid.as_str()) else {
                break;
            };
            let Some(next) = kids.last() else { break };
            len += 1;
            cursor = next;
        }
        if len > longest {
            longest = len;
        }
    }

    (roots_proper, orphans, longest, branch_points)
}

fn dist(label: &str, mut lens: Vec<usize>) {
    lens.sort();
    let n = lens.len();
    let sum: usize = lens.iter().sum();
    let pct = |p: usize| if n == 0 { 0 } else { lens[(n * p) / 100] };
    let median = if n == 0 { 0 } else { lens[n / 2] };
    let min = lens.first().copied().unwrap_or(0);
    let max = lens.last().copied().unwrap_or(0);
    let mean = if n > 0 { sum as f64 / n as f64 } else { 0.0 };
    println!("\n  ── {label}  ({n} 项) ──");
    println!("    total={sum}  min={min}  median={median}  mean={mean:.1}  p90={}  p99={}  max={max}", pct(90), pct(99));
}

fn main() -> std::io::Result<()> {
    let claude_home = std::env::var("CLAUDE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap()).join(".claude"));

    let projects_root = claude_home.join("projects");
    eprintln!("Scanning {} (strictly under projects/) ...", claude_home.display());
    let files = scanner::scan_sessions(&claude_home)?;
    let off_boundary: Vec<_> = files
        .iter()
        .filter(|sf| !sf.path.starts_with(&projects_root))
        .map(|sf| sf.path.display().to_string())
        .collect();
    if !off_boundary.is_empty() {
        eprintln!("✗ scanner returned {} path(s) outside ~/.claude/projects/:", off_boundary.len());
        for p in &off_boundary {
            eprintln!("    {p}");
        }
        std::process::exit(1);
    }
    eprintln!("Found {} session files under projects/", files.len());

    let mut type_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut dag_nodes_total = 0usize;
    let mut sidechain_total = 0usize;
    let mut struct_drift_total = 0usize;
    let mut skipped_io_total = 0usize;
    let mut leaks: HashMap<&'static str, LeakSample> = HashMap::new();
    let mut unknown_types: BTreeMap<String, usize> = BTreeMap::new();

    // 按 entry.sessionId 聚合所有 DAG node（跨文件）
    let mut by_session: HashMap<String, SessionStats> = HashMap::new();
    let mut nodes_by_session: HashMap<String, Vec<Node>> = HashMap::new();

    for sf in &files {
        // ── typed pass：DAG nodes ──
        let Ok(reader) = SessionReader::open(&sf.path) else {
            continue;
        };
        let mut typed_iter = reader.lenient();
        for entry in typed_iter.by_ref() {
            let ty = entry_type(&entry);
            *type_counts.entry(ty).or_insert(0) += 1;
            if let Some(d) = as_dag(&entry) {
                dag_nodes_total += 1;
                let is_side = d.is_sidechain() == Some(true);
                if is_side {
                    sidechain_total += 1;
                }
                if let (Some(uuid), Some(sid)) = (d.uuid(), d.session_id()) {
                    let stats = by_session.entry(sid.to_string()).or_insert_with(|| {
                        let mut s = SessionStats::default();
                        s.session_id = sid.to_string();
                        s
                    });
                    stats.files.insert(sf.path.display().to_string());
                    nodes_by_session.entry(sid.to_string()).or_default().push(Node {
                        uuid: uuid.to_string(),
                        parent: d.parent_uuid().map(|s| s.to_string()),
                        sidechain: is_side,
                        ts: d.timestamp().unwrap_or("").to_string(),
                    });
                }
            }
        }
        struct_drift_total += typed_iter.struct_drift_count();
        skipped_io_total += typed_iter.errors_skipped();

        // ── raw pass：oracle 漏网检测 ──
        let Ok(text) = std::fs::read_to_string(&sf.path) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let ty_str = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
            let has_uuid = v
                .get("uuid")
                .and_then(|x| x.as_str())
                .is_some_and(|s| !s.is_empty());
            let has_parent = v
                .get("parentUuid")
                .and_then(|x| x.as_str())
                .is_some_and(|s| !s.is_empty());
            if !has_uuid {
                continue;
            }
            // 已被 DagNode 覆盖
            let already_dag = matches!(
                ty_str,
                "user" | "assistant" | "system" | "attachment" | "progress"
            );
            if already_dag {
                continue;
            }
            let declared_in_enum = matches!(
                ty_str,
                "summary"
                    | "custom-title"
                    | "ai-title"
                    | "last-prompt"
                    | "task-summary"
                    | "tag"
                    | "agent-name"
                    | "agent-color"
                    | "agent-setting"
                    | "pr-link"
                    | "mode"
                    | "permission-mode"
                    | "queue-operation"
                    | "speculation-accept"
                    | "worktree-state"
                    | "content-replacement"
                    | "file-history-snapshot"
                    | "attribution-snapshot"
                    | "marble-origami-commit"
                    | "marble-origami-snapshot"
            );
            if declared_in_enum {
                if has_parent || ty_str == "summary" {
                    let raw_sidechain = v
                        .get("isSidechain")
                        .and_then(|x| x.as_bool())
                        .unwrap_or(false);
                    let key: &'static str = match ty_str {
                        "summary" => "summary",
                        "custom-title" => "custom-title",
                        "ai-title" => "ai-title",
                        "last-prompt" => "last-prompt",
                        "task-summary" => "task-summary",
                        "tag" => "tag",
                        "agent-name" => "agent-name",
                        "agent-color" => "agent-color",
                        "agent-setting" => "agent-setting",
                        "pr-link" => "pr-link",
                        "mode" => "mode",
                        "permission-mode" => "permission-mode",
                        "queue-operation" => "queue-operation",
                        "speculation-accept" => "speculation-accept",
                        "worktree-state" => "worktree-state",
                        "content-replacement" => "content-replacement",
                        "file-history-snapshot" => "file-history-snapshot",
                        "attribution-snapshot" => "attribution-snapshot",
                        "marble-origami-commit" => "marble-origami-commit",
                        "marble-origami-snapshot" => "marble-origami-snapshot",
                        _ => "?",
                    };
                    let entry = leaks.entry(key).or_insert_with(|| LeakSample {
                        example_uuid: v
                            .get("uuid")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        example_parent: v
                            .get("parentUuid")
                            .and_then(|x| x.as_str())
                            .map(|s| s.to_string()),
                        example_session: v
                            .get("sessionId")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        example_file: sf.path.display().to_string(),
                        ..Default::default()
                    });
                    entry.count += 1;
                    if raw_sidechain {
                        entry.sidechain_count += 1;
                    } else {
                        entry.main_chain_count += 1;
                    }
                }
            } else {
                *unknown_types.entry(ty_str.to_string()).or_insert(0) += 1;
            }
        }
    }

    // ── 按 session 构图 ──
    let mut session_list: Vec<SessionStats> = by_session.into_values().collect();
    for s in &mut session_list {
        let Some(nodes) = nodes_by_session.get(&s.session_id) else { continue };
        s.total_nodes = nodes.len();
        s.sidechain_nodes = nodes.iter().filter(|n| n.sidechain).count();
        let (rp, orph, longest, branches) = build_session(nodes);
        s.roots_proper = rp;
        s.orphans = orph;
        s.longest_chain = longest;
        s.branch_points = branches;
    }

    // ─── 报告 ───
    println!("\n────────── Entry type 分布 ──────────");
    let total: usize = type_counts.values().sum();
    for (k, c) in &type_counts {
        println!("  {:<26} {:>8}  ({:.2}%)", k, c, 100.0 * *c as f64 / total.max(1) as f64);
    }
    println!("  {:<26} {:>8}", "TOTAL", total);

    println!("\n────────── DAG 节点统计 ──────────");
    println!("  DagNode-able entries:    {}", dag_nodes_total);
    println!("  其中 sidechain:          {}", sidechain_total);
    println!("  main chain 候选:         {}", dag_nodes_total - sidechain_total);
    println!("  StructDrift (skipped):   {}", struct_drift_total);
    println!("  IO/Json error (skipped): {}", skipped_io_total);

    println!("\n────────── 剩余 entry 里的 DAG 节点检测（oracle） ──────────");
    let total_leftover_main: usize = leaks.values().map(|s| s.main_chain_count).sum();
    let total_leftover_side: usize = leaks.values().map(|s| s.sidechain_count).sum();
    println!(
        "  剩余 main-chain 节点 = {}  剩余 side-chain 节点 = {}",
        total_leftover_main, total_leftover_side
    );
    if total_leftover_main == 0 && total_leftover_side == 0 {
        println!("  ✓ PASS: 0 剩余 DAG 节点（main + side 同构均通过）。");
    } else {
        println!("  ✗ FAIL: 下方 ✗ 标记的 type 需要 impl DagNode。");
    }
    if !leaks.is_empty() {
        let mut sorted: Vec<_> = leaks.iter().collect();
        sorted.sort_by(|a, b| {
            (b.1.main_chain_count + b.1.sidechain_count)
                .cmp(&(a.1.main_chain_count + a.1.sidechain_count))
        });
        for (ty, samp) in sorted {
            let total = samp.main_chain_count + samp.sidechain_count;
            let flag = if total > 0 { "✗" } else { "·" };
            println!(
                "  {} type={:<28} total={:>6}  main={:>6}  side={:>6}",
                flag, ty, samp.count, samp.main_chain_count, samp.sidechain_count
            );
            if total > 0 {
                println!(
                    "      example_uuid={}  parent={:?}",
                    samp.example_uuid, samp.example_parent
                );
                println!("      sessionId={}", samp.example_session);
                println!("      file={}", samp.example_file);
            }
        }
    }

    println!("\n────────── 完全未声明的 type ──────────");
    if unknown_types.is_empty() {
        println!("  无。");
    } else {
        for (k, c) in &unknown_types {
            println!("  {:<26} {:>6}", k, c);
        }
    }

    // ── 全连接性判定（每个 DAG 节点都要么是真根，要么父在 session 内） ──
    println!("\n────────── 全连接性判定（每个 DAG 节点是否都连得上？） ──────────");
    let total_orphans: usize = session_list.iter().map(|s| s.orphans).collect::<Vec<_>>().iter().sum();
    let total_proper_roots: usize = session_list.iter().map(|s| s.roots_proper).sum();
    let total_nodes_sum: usize = session_list.iter().map(|s| s.total_nodes).sum();
    println!("  总 DAG 节点:             {}", total_nodes_sum);
    println!("  真根（parent=None）:     {}", total_proper_roots);
    println!("  内部节点（parent 在内）: {}", total_nodes_sum - total_proper_roots - total_orphans);
    println!("  孤儿（parent 找不到）:   {}", total_orphans);
    let with_orphan = session_list.iter().filter(|s| s.orphans > 0).count();
    println!("  含孤儿的 session:        {}", with_orphan);
    if total_orphans == 0 {
        println!("  ✓ PASS: 全部 {} 个 DAG 节点都连得上（真根 + 内部节点 = 总数）。", total_nodes_sum);
    } else {
        println!("  ✗ FAIL: 有 {} 个节点的 parent_uuid 在 session 内找不到。", total_orphans);
    }

    println!("\n────────── 逻辑 session 统计 (group by entry.sessionId) ──────────");
    println!("  独立 session 数:         {}", session_list.len());
    let multi_file_sessions = session_list.iter().filter(|s| s.files.len() > 1).count();
    println!("  跨多文件 session 数:     {} (主+subagent+workflow agent)", multi_file_sessions);
    let multi_root = session_list.iter().filter(|s| s.roots_proper > 1).count();
    println!("  多真根 session 数:       {} (>1 个 parent=None 节点)", multi_root);

    let roots_per_session: Vec<usize> = session_list.iter().map(|s| s.roots_proper).collect();
    let lens: Vec<usize> = session_list.iter().map(|s| s.longest_chain).collect();
    let nodes_per_session: Vec<usize> = session_list.iter().map(|s| s.total_nodes).collect();
    let files_per_session: Vec<usize> = session_list.iter().map(|s| s.files.len()).collect();

    dist("每 session 文件数（main+所有 subagent）", files_per_session);
    dist("每 session DAG 节点数", nodes_per_session);
    dist("每 session 真根数 (parent=None 的节点)", roots_per_session);
    dist("每 session 最长链长度", lens.clone());

    println!("\n  最长链分布:");
    let n = lens.len();
    let buckets = [
        (0..=1, "      0..1"),
        (2..=10, "     2..10"),
        (11..=50, "    11..50"),
        (51..=200, "   51..200"),
        (201..=1000, "  201..1000"),
        (1001..=usize::MAX, "      >1000"),
    ];
    for (range, label) in &buckets {
        let count = lens.iter().filter(|l| range.contains(*l)).count();
        let bar_len = ((count as f64 / n.max(1) as f64) * 50.0) as usize;
        println!("    {:<12} {:>4} sessions {}", label, count, "█".repeat(bar_len));
    }

    println!("\n────────── 链数 (roots) 分布 ──────────");
    let mut root_hist: BTreeMap<usize, usize> = BTreeMap::new();
    for r in session_list.iter().map(|s| s.roots_proper) {
        *root_hist.entry(r).or_insert(0) += 1;
    }
    for (r, c) in &root_hist {
        let bar_len = ((*c as f64 / n.max(1) as f64) * 50.0) as usize;
        println!("    {:>3} 真根: {:>4} sessions {}", r, c, "█".repeat(bar_len));
    }

    println!("\n────────── Top-10（最长链） ──────────");
    let mut sorted = session_list.clone();
    sorted.sort_by(|a, b| b.longest_chain.cmp(&a.longest_chain));
    for (i, s) in sorted.iter().take(10).enumerate() {
        println!(
            "  #{:<2} len={:>5}  roots={:>3}  orphans={:>2}  branches={:>2}  files={:>3}  nodes={:>5}  sessionId={}",
            i + 1,
            s.longest_chain,
            s.roots_proper,
            s.orphans,
            s.branch_points,
            s.files.len(),
            s.total_nodes,
            s.session_id
        );
    }

    println!("\n────────── 跨文件 session 抽查（top-3 by file count） ──────────");
    let mut by_files = session_list.clone();
    by_files.sort_by(|a, b| b.files.len().cmp(&a.files.len()));
    for s in by_files.iter().take(3) {
        println!(
            "\n  sessionId={}  files={}  nodes={}  roots={}  orphans={}  longest={}  sidechain={}",
            s.session_id, s.files.len(), s.total_nodes, s.roots_proper, s.orphans, s.longest_chain, s.sidechain_nodes
        );
        for f in s.files.iter().take(5) {
            println!("    {}", f);
        }
        if s.files.len() > 5 {
            println!("    ... {} more", s.files.len() - 5);
        }
    }

    Ok(())
}
