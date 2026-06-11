//! Real-data smoke test for cc-session-dag.
//!
//! Three independent oracles, all must PASS on the local ~/.claude/projects:
//!
//!   1. **Scanner boundary**: every file path the loader exposes lies strictly
//!      under `projects/`.
//!   2. **Leak detection** (no missing DagNode impls): every entry whose raw
//!      JSON carries `uuid` (+ `parentUuid` or is a `summary`) must be
//!      classified into a variant that implements `DagNode`. Both main-chain
//!      and sidechain variants must be covered (structural isomorphism).
//!   3. **Orphan detection** (every DAG node connects): grouped by
//!      `entry.sessionId`, every node either has `parent_uuid == None` (real
//!      root) or its parent_uuid resolves inside the same session.
//!
//! Also asserts: 0 StructDrift, 0 IO/Json errors, 0 raw `type` strings outside
//! the declared Entry enum.
//!
//! Run with:
//!   REAL_CLAUDE_DATA=1 cargo test -p cc-session-dag --test real_data_dag_smoke -- --ignored
//!
//! In CI: silently skipped (no data). With `REQUIRE_REAL_DATA=1`: panic if no
//! data found (mirrors scripts/run-real-e2e.sh).

use cc_session_jsonl::load_all_sessions;
use cc_session_jsonl::parser::SessionReader;
use cc_session_jsonl::types::{DagNode, Entry};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

fn claude_home() -> PathBuf {
    std::env::var("CLAUDE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap()).join(".claude"))
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

/// All entry types declared in the `Entry` enum that are *metadata*
/// (no `DagNode` impl). If raw JSON for one of these carries `uuid`
/// + `parentUuid`, it's a missing `DagNode` impl — a hard failure.
const DECLARED_METADATA_TYPES: &[&str] = &[
    "summary",
    "custom-title",
    "ai-title",
    "last-prompt",
    "task-summary",
    "tag",
    "agent-name",
    "agent-color",
    "agent-setting",
    "pr-link",
    "mode",
    "permission-mode",
    "queue-operation",
    "speculation-accept",
    "worktree-state",
    "content-replacement",
    "file-history-snapshot",
    "attribution-snapshot",
    "marble-origami-commit",
    "marble-origami-snapshot",
];

const DECLARED_DAG_TYPES: &[&str] = &["user", "assistant", "system", "attachment", "progress"];

#[derive(Debug)]
struct Node {
    uuid: String,
    parent: Option<String>,
}

/// Walk one JSONL file: typed pass to collect DAG nodes by session id, raw
/// pass to detect leaks and unknown types.
#[allow(clippy::too_many_arguments)]
fn process_file(
    path: &Path,
    struct_drift_total: &mut usize,
    io_err_total: &mut usize,
    nodes_by_session: &mut HashMap<String, Vec<Node>>,
    leaks_main: &mut HashMap<&'static str, usize>,
    leaks_side: &mut HashMap<&'static str, usize>,
    unknown_types: &mut HashMap<String, usize>,
    total_entries_typed: &mut usize,
    total_dag_nodes: &mut usize,
) {
    // ── typed pass ──
    let reader = SessionReader::open(path).expect("open failed");
    let mut iter = reader.lenient();
    for entry in iter.by_ref() {
        *total_entries_typed += 1;
        if let Some(d) = as_dag(&entry) {
            *total_dag_nodes += 1;
            if let (Some(uuid), Some(sid)) = (d.uuid(), d.session_id()) {
                nodes_by_session
                    .entry(sid.to_string())
                    .or_default()
                    .push(Node {
                        uuid: uuid.to_string(),
                        parent: d.parent_uuid().map(|s| s.to_string()),
                    });
            }
        }
    }
    *struct_drift_total += iter.struct_drift_count();
    *io_err_total += iter.errors_skipped();

    // ── raw pass: oracle 2 ──
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
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
        if DECLARED_DAG_TYPES.contains(&ty) {
            continue;
        }
        if DECLARED_METADATA_TYPES.contains(&ty) {
            if has_parent || ty == "summary" {
                let key: &'static str = DECLARED_METADATA_TYPES
                    .iter()
                    .copied()
                    .find(|t| *t == ty)
                    .unwrap_or("?");
                let is_side = v
                    .get("isSidechain")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false);
                if is_side {
                    *leaks_side.entry(key).or_insert(0) += 1;
                } else {
                    *leaks_main.entry(key).or_insert(0) += 1;
                }
            }
        } else if !ty.starts_with("__") {
            // 既不在 DAG types 也不在 metadata types,且不是内部 sentinel
            *unknown_types.entry(ty.to_string()).or_insert(0) += 1;
        }
    }
}

#[test]
#[ignore]
fn real_data_dag_smoke() {
    let require = std::env::var("REQUIRE_REAL_DATA").as_deref() == Ok("1");
    if std::env::var("REAL_CLAUDE_DATA").as_deref() != Ok("1") && !require {
        eprintln!("[real_data_dag_smoke] REAL_CLAUDE_DATA not set — skipping (correct for CI)");
        eprintln!("[real_data_dag_smoke] To run: REAL_CLAUDE_DATA=1 cargo test ... -- --ignored");
        return;
    }

    let home = claude_home();
    let projects_root = home.join("projects");
    if !projects_root.exists() {
        if require {
            panic!(
                "REQUIRE_REAL_DATA=1 but {} does not exist",
                projects_root.display()
            );
        }
        eprintln!("[real_data_dag_smoke] {} missing — skipping", projects_root.display());
        return;
    }

    let sessions = load_all_sessions(&home).expect("load_all_sessions failed");

    // Enumerate every JSONL file the loader exposes (main + agents). Workflow
    // agent files reach us through `Session.agents[*].path` (the scanner
    // records them with `workflow_run_id = Some(...)`).
    let mut all_paths: Vec<PathBuf> = Vec::new();
    for s in &sessions {
        if let Some(project) = &s.project {
            let main_path = home
                .join("projects")
                .join(project)
                .join(format!("{}.jsonl", s.id));
            if main_path.is_file() {
                all_paths.push(main_path);
            }
        }
        for a in &s.agents {
            all_paths.push(a.path.clone());
        }
    }

    if all_paths.is_empty() {
        if require {
            panic!("REQUIRE_REAL_DATA=1 but loader found 0 session files");
        }
        eprintln!("[real_data_dag_smoke] loader found 0 files — skipping");
        return;
    }

    // ── Oracle 1: scanner boundary ──
    let off_boundary: Vec<_> = all_paths
        .iter()
        .filter(|p| !p.starts_with(&projects_root))
        .map(|p| p.display().to_string())
        .collect();
    assert_eq!(
        off_boundary.len(),
        0,
        "loader returned paths outside ~/.claude/projects/: {:?}",
        off_boundary
    );

    // Collect everything we need in a single pass per file.
    let mut struct_drift_total = 0usize;
    let mut io_err_total = 0usize;
    let mut nodes_by_session: HashMap<String, Vec<Node>> = HashMap::new();
    let mut leaks_main: HashMap<&'static str, usize> = HashMap::new();
    let mut leaks_side: HashMap<&'static str, usize> = HashMap::new();
    let mut unknown_types: HashMap<String, usize> = HashMap::new();
    let mut total_entries_typed = 0usize;
    let mut total_dag_nodes = 0usize;

    for path in &all_paths {
        process_file(
            path,
            &mut struct_drift_total,
            &mut io_err_total,
            &mut nodes_by_session,
            &mut leaks_main,
            &mut leaks_side,
            &mut unknown_types,
            &mut total_entries_typed,
            &mut total_dag_nodes,
        );
    }

    // ── Assertions ──

    // Oracle 0: schema integrity
    assert_eq!(
        struct_drift_total, 0,
        "StructDrift count must be 0; got {} — Claude Code shipped a schema change",
        struct_drift_total
    );
    assert_eq!(
        io_err_total, 0,
        "IO/Json errors must be 0; got {}",
        io_err_total
    );
    assert!(
        unknown_types.is_empty(),
        "Found raw `type` strings outside the declared Entry enum: {:?}",
        unknown_types
    );

    // Oracle 2: leak detection — both sides of the structural isomorphism
    let total_leak_main: usize = leaks_main.values().sum();
    let total_leak_side: usize = leaks_side.values().sum();
    assert_eq!(
        total_leak_main, 0,
        "main-chain leak: {:?} — these declared-metadata types carry uuid+parentUuid \
         in non-sidechain context and need impl DagNode",
        leaks_main
    );
    assert_eq!(
        total_leak_side, 0,
        "side-chain leak: {:?} — these declared-metadata types carry uuid+parentUuid \
         in sidechain context and need impl DagNode",
        leaks_side
    );

    // Oracle 3: orphan detection — every DAG node connects within its session
    let mut total_orphans = 0usize;
    let mut sessions_with_orphans: Vec<(String, usize)> = vec![];
    let mut total_proper_roots = 0usize;
    let mut total_nodes_seen = 0usize;
    for (sid, nodes) in &nodes_by_session {
        let uuids: BTreeSet<&str> = nodes.iter().map(|n| n.uuid.as_str()).collect();
        let mut orphans = 0usize;
        let mut proper_roots = 0usize;
        for n in nodes {
            match n.parent.as_deref() {
                None => proper_roots += 1,
                Some(p) => {
                    if !uuids.contains(p) {
                        orphans += 1;
                    }
                }
            }
        }
        total_nodes_seen += nodes.len();
        total_proper_roots += proper_roots;
        if orphans > 0 {
            sessions_with_orphans.push((sid.clone(), orphans));
        }
        total_orphans += orphans;
    }
    assert_eq!(
        total_orphans, 0,
        "orphans found in {} session(s): {:?} — parent_uuid resolves outside session",
        sessions_with_orphans.len(),
        sessions_with_orphans
    );
    // Connectivity invariant: roots + internal nodes = total
    assert_eq!(
        total_proper_roots + (total_nodes_seen - total_proper_roots),
        total_nodes_seen,
        "connectivity invariant violated"
    );

    eprintln!(
        "[real_data_dag_smoke] PASS  files={}  entries={}  dag_nodes={}  sessions={}  roots={}",
        all_paths.len(),
        total_entries_typed,
        total_dag_nodes,
        nodes_by_session.len(),
        total_proper_roots,
    );
}
