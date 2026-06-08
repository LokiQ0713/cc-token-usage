use std::collections::HashMap;
use std::path::Path;

use cc_session_jsonl::types::{
    ApiMessage, AssistantEntry, ContentBlock, Entry, Usage, UserEntry,
};
use cc_session_jsonl::SessionReader;

use super::{DagEdge, DagGraph, DagNode, EdgeLabel};

/// Internal entry info for the unified DAG index.
#[derive(Clone)]
struct EntryInfo {
    uuid: String,
    parent_uuid: Option<String>,
    timestamp: chrono::DateTime<chrono::Utc>,
    kind: String,
    label: String,
    full_label: String,
    agent_id: Option<String>,
    is_sidechain: bool,
    tokens: Option<u64>,
}

pub fn build_dag_graph(path: &Path, session_id: String) -> DagGraph {
    let reader = match SessionReader::open(path) {
        Ok(r) => r,
        Err(_) => {
            return DagGraph {
                session_id,
                nodes: vec![],
                edges: vec![],
            }
        }
    };

    let mut entries: Vec<EntryInfo> = Vec::new();
    let mut index: HashMap<String, EntryInfo> = HashMap::new();
    let fallback_ts = chrono::Utc::now();

    for result in reader {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let info = entry_to_info(&entry, fallback_ts);
        let Some(info) = info else { continue };

        index.insert(info.uuid.clone(), info.clone());
        entries.push(info);
    }

    // Sort by timestamp
    entries.sort_by_key(|e| e.timestamp);

    // Build children map
    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    for e in &entries {
        if let Some(ref pid) = e.parent_uuid {
            if index.contains_key(pid) {
                children_of.entry(pid.clone()).or_default().push(e.uuid.clone());
            }
        }
    }

    // Assign levels (BFS from roots)
    let levels = assign_levels(&index, &children_of);

    // Build edges
    let mut edges: Vec<DagEdge> = Vec::new();
    for e in &entries {
        if let Some(ref pid) = e.parent_uuid {
            if index.contains_key(pid) {
                let sibling_count = children_of.get(pid).map_or(1, |c| c.len());
                let label = if sibling_count > 1 {
                    EdgeLabel::Fork
                } else {
                    EdgeLabel::Chain
                };
                edges.push(DagEdge {
                    from: pid.clone(),
                    to: e.uuid.clone(),
                    label,
                    dashes: matches!(label, EdgeLabel::Fork),
                });
            }
        }
    }

    // Build nodes
    let nodes: Vec<DagNode> = entries
        .iter()
        .map(|e| DagNode {
            id: e.uuid.clone(),
            label: e.label.clone(),
            full_label: e.full_label.clone(),
            level: levels.get(&e.uuid).copied().unwrap_or(0),
            entry_type: e.kind.clone(),
            agent_id: e.agent_id.clone(),
            is_sidechain: e.is_sidechain,
            tokens: e.tokens,
            cost: None,
            timestamp: e.timestamp.to_rfc3339(),
        })
        .collect();

    DagGraph {
        session_id,
        nodes,
        edges,
    }
}

// ── Entry → EntryInfo conversion (zero filtering, zero dedup) ──────────

fn entry_to_info(entry: &Entry, fallback_ts: chrono::DateTime<chrono::Utc>) -> Option<EntryInfo> {
    match entry {
        Entry::User(u) => user_info(u, fallback_ts),
        Entry::Assistant(a) => assistant_info(a, fallback_ts),
        Entry::Attachment(att) => {
            let (uuid, parent_uuid, timestamp, is_sidechain, agent_id) = common_fields(
                att.uuid.as_deref(),
                att.parent_uuid.as_deref(),
                att.timestamp.as_deref(),
                att.is_sidechain,
                att.agent_id.as_deref(),
                fallback_ts,
            );
            let uid = uuid?;
            Some(EntryInfo {
                uuid: uid.clone(),
                parent_uuid,
                timestamp,
                kind: "attachment".into(),
                label: "attachment".into(),
                full_label: format!("uuid: {}\ntype: attachment", uid),
                agent_id,
                is_sidechain,
                tokens: None,
            })
        }
        Entry::System(sys) => {
            let (uuid, parent_uuid, timestamp, is_sidechain, agent_id) = common_fields(
                sys.uuid.as_deref(),
                sys.parent_uuid.as_deref(),
                sys.timestamp.as_deref(),
                sys.is_sidechain,
                sys.agent_id.as_deref(),
                fallback_ts,
            );
            let uid = uuid?;
            let subtype = sys.subtype.as_deref().unwrap_or("system");
            Some(EntryInfo {
                uuid: uid.clone(),
                parent_uuid,
                timestamp,
                kind: "system".into(),
                label: subtype.into(),
                full_label: format!("uuid: {}\ntype: system ({})", uid, subtype),
                agent_id,
                is_sidechain,
                tokens: None,
            })
        }
        Entry::Passthrough(p) => {
            let ts = p
                .timestamp
                .as_deref()
                .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
                .unwrap_or(fallback_ts);
            Some(EntryInfo {
                uuid: p.uuid.clone(),
                parent_uuid: p.parent_uuid.clone(),
                timestamp: ts,
                kind: p.entry_type.clone(),
                label: p.entry_type.clone(),
                full_label: format!("uuid: {}\ntype: {} (passthrough)", p.uuid, p.entry_type),
                agent_id: p.agent_id.clone(),
                is_sidechain: p.is_sidechain.unwrap_or(false),
                tokens: None,
            })
        }
        // Metadata-only entries without DAG relevance — skip
        Entry::CustomTitle(_)
        | Entry::AiTitle(_)
        | Entry::Tag(_)
        | Entry::Mode(_)
        | Entry::LastPrompt(_)
        | Entry::PrLink(_)
        | Entry::AgentName(_)
        | Entry::AgentColor(_)
        | Entry::AgentSetting(_)
        | Entry::PermissionMode(_)
        | Entry::Summary(_)
        | Entry::TaskSummary(_)
        | Entry::Ignored => None,
        // Other entries with common transcript fields — include with basic info
        _ => {
            // These variants have common fields via transcript_entry! macro
            None
        }
    }
}

// ── User entry ─────────────────────────────────────────────────────────

fn user_info(u: &UserEntry, fallback_ts: chrono::DateTime<chrono::Utc>) -> Option<EntryInfo> {
    let (uuid, parent_uuid, timestamp, is_sidechain, agent_id) = common_fields(
        u.uuid.as_deref(),
        u.parent_uuid.as_deref(),
        u.timestamp.as_deref(),
        u.is_sidechain,
        u.agent_id.as_deref(),
        fallback_ts,
    );
    let uuid = uuid?;

    let is_tool_result = u.tool_use_result.is_some()
        || u.source_tool_use_id.is_some()
        || u.source_tool_assistant_uuid.is_some();

    let kind = if is_tool_result {
        "tool_result"
    } else {
        "user"
    };

    let text = user_text(u);
    let label = if is_tool_result {
        "📥 result".to_string()
    } else {
        text.as_deref().unwrap_or("?").chars().take(40).collect()
    };

    let mut fl = format!("uuid: {}\nparent: {}", uuid, parent_uuid.as_deref().unwrap_or("?"));
    fl.push_str(&format!(
        "\ntype: user{}",
        if is_tool_result { " (tool_result)" } else { "" }
    ));
    if let Some(ref txt) = text {
        fl.push_str(&format!("\ncontent: {}", txt));
    }

    Some(EntryInfo {
        uuid,
        parent_uuid,
        timestamp,
        kind: kind.into(),
        label,
        full_label: fl,
        agent_id,
        is_sidechain,
        tokens: None,
    })
}

fn user_text(u: &UserEntry) -> Option<String> {
    let content_val = u.message.as_ref()?.content.as_ref()?;

    let text = if let Some(s) = content_val.as_str() {
        s.to_string()
    } else if let Some(arr) = content_val.as_array() {
        arr.iter()
            .filter_map(|b| {
                if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                    b.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        return None;
    };

    if text.is_empty() {
        return None;
    }
    Some(if text.len() > 500 {
        format!("{}...", &text[..text.floor_char_boundary(500)])
    } else {
        text
    })
}

// ── Assistant entry ────────────────────────────────────────────────────

fn assistant_info(
    a: &AssistantEntry,
    fallback_ts: chrono::DateTime<chrono::Utc>,
) -> Option<EntryInfo> {
    let (uuid, parent_uuid, timestamp, is_sidechain, agent_id) = common_fields(
        a.uuid.as_deref(),
        a.parent_uuid.as_deref(),
        a.timestamp.as_deref(),
        a.is_sidechain,
        a.agent_id.as_deref(),
        fallback_ts,
    );
    let uuid = uuid?;
    let api: &ApiMessage = a.message.as_ref()?;
    let usage: Option<&Usage> = api.usage.as_ref();
    let content: &[ContentBlock] = api.content.as_deref().unwrap_or(&[]);

    let total_tokens = usage.map(|u| {
        u.input_tokens.unwrap_or(0)
            + u.output_tokens.unwrap_or(0)
            + u.cache_creation_input_tokens.unwrap_or(0)
            + u.cache_read_input_tokens.unwrap_or(0)
    });

    let (content_types, text_parts, tool_names) = extract_content_info(content);

    let kind = classify_assistant(&tool_names, &text_parts, &content_types);
    let label = assistant_label(total_tokens, &tool_names);

    let mut fl = format!(
        "uuid: {}\nparent: {}\ntype: assistant ({})",
        uuid,
        parent_uuid.as_deref().unwrap_or("?"),
        kind,
    );
    if let Some(ref txt) = a.request_id {
        fl.push_str(&format!("\nrequestId: {}", txt));
    }
    if !text_parts.is_empty() {
        let combined = text_parts.join("\n");
        let truncated: String = combined.chars().take(500).collect();
        fl.push_str(&format!("\nresponse: {}", truncated));
    }
    if let Some(tok) = total_tokens {
        fl.push_str(&format!("\ntokens: {}", tok));
    }

    Some(EntryInfo {
        uuid,
        parent_uuid,
        timestamp,
        kind: kind.into(),
        label,
        full_label: fl,
        agent_id,
        is_sidechain,
        tokens: total_tokens,
    })
}

fn extract_content_info(content: &[ContentBlock]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut content_types = Vec::new();
    let mut text_parts = Vec::new();
    let mut tool_names = Vec::new();

    for b in content {
        match b {
            ContentBlock::Text { text } => {
                content_types.push("text".to_string());
                if let Some(t) = text {
                    text_parts.push(t.clone());
                }
            }
            ContentBlock::ToolUse { name, .. } => {
                content_types.push("tool_use".to_string());
                if let Some(n) = name {
                    tool_names.push(n.clone());
                }
            }
            ContentBlock::Thinking { .. } => {
                content_types.push("thinking".to_string());
            }
            ContentBlock::ToolResult { .. } => {
                content_types.push("tool_result".to_string());
            }
            _ => {
                content_types.push("other".to_string());
            }
        }
    }

    (content_types, text_parts, tool_names)
}

fn classify_assistant(tool_names: &[String], text_parts: &[String], content_types: &[String]) -> &'static str {
    if tool_names.contains(&"Task".to_string()) || tool_names.contains(&"Agent".to_string()) {
        return "spawn";
    }
    if !tool_names.is_empty() {
        return "tool_use";
    }
    if !text_parts.is_empty() {
        return "assistant";
    }
    if content_types.contains(&"thinking".to_string()) {
        return "thinking";
    }
    "assistant"
}

fn assistant_label(total_tokens: Option<u64>, tool_names: &[String]) -> String {
    let mut parts = Vec::new();
    if let Some(tok) = total_tokens {
        parts.push(format!("{}tok", tok));
    }
    if !tool_names.is_empty() {
        parts.push(format!("\u{b7}{}", tool_names.join(",")));
    }
    parts.join("")
}

// ── Helpers ────────────────────────────────────────────────────────────

fn common_fields(
    uuid: Option<&str>,
    parent_uuid: Option<&str>,
    timestamp: Option<&str>,
    is_sidechain: Option<bool>,
    agent_id: Option<&str>,
    fallback_ts: chrono::DateTime<chrono::Utc>,
) -> (
    Option<String>,
    Option<String>,
    chrono::DateTime<chrono::Utc>,
    bool,
    Option<String>,
) {
    let uid = uuid.and_then(|s| if s.is_empty() { None } else { Some(s.to_string()) });
    let puid = parent_uuid.and_then(|s| if s.is_empty() { None } else { Some(s.to_string()) });
    let ts = timestamp
        .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
        .unwrap_or(fallback_ts);
    let side = is_sidechain.unwrap_or(false);
    let aid = agent_id.and_then(|s| if s.is_empty() { None } else { Some(s.to_string()) });
    (uid, puid, ts, side, aid)
}

fn assign_levels(
    index: &HashMap<String, EntryInfo>,
    children_of: &HashMap<String, Vec<String>>,
) -> HashMap<String, u32> {
    let mut levels: HashMap<String, u32> = HashMap::new();
    for (uuid, e) in index.iter() {
        let is_root = match &e.parent_uuid {
            Some(pid) => !index.contains_key(pid),
            None => true,
        };
        if is_root {
            levels.insert(uuid.clone(), 0);
        }
    }
    let mut queue: Vec<String> = levels.keys().cloned().collect();
    while let Some(pid) = queue.pop() {
        let cur = *levels.get(&pid).unwrap_or(&0);
        if let Some(kids) = children_of.get(&pid) {
            for kid in kids {
                if !levels.contains_key(kid) {
                    levels.insert(kid.clone(), cur + 1);
                    queue.push(kid.clone());
                }
            }
        }
    }
    for uuid in index.keys() {
        levels.entry(uuid.clone()).or_insert(0);
    }
    levels
}
