//! DAG (directed acyclic graph) representation of a Claude Code session.
//!
//! Built from `ValidatedTurn` data, the graph captures `parentUuid → uuid`
//! relationships including main-chain links, agent-spawn edges, and fork
//! points.

mod builder;

pub use builder::build_dag_graph;

use serde::Serialize;

/// The complete DAG for one session, ready for serialization.
#[derive(Debug, Serialize)]
pub struct DagGraph {
    /// Session identifier (trunk session UUID).
    pub session_id: String,
    /// Every entry that participates in the DAG (has a `uuid`).
    pub nodes: Vec<DagNode>,
    /// Directed edges: `parent_uuid → uuid`.
    pub edges: Vec<DagEdge>,
}

/// One node in the DAG — corresponds to one `ValidatedTurn`.
#[derive(Debug, Serialize)]
pub struct DagNode {
    /// Unique node id (the entry's `uuid`).
    pub id: String,
    /// Short human-readable label.
    pub label: String,
    /// Full text for tooltip.
    #[serde(rename = "fullLabel")]
    pub full_label: String,
    /// DAG depth level (0 = root).
    pub level: u32,
    /// Entry type: `user`, `assistant`, `system`, `agent`, `passthrough`.
    #[serde(rename = "entryType")]
    pub entry_type: String,
    /// Which agent this node belongs to, if any.
    #[serde(rename = "agentId")]
    pub agent_id: Option<String>,
    /// Whether this is a sidechain node (belongs to an agent).
    #[serde(rename = "isSidechain")]
    pub is_sidechain: bool,
    /// Token usage for this turn.
    pub tokens: Option<u64>,
    /// Estimated cost for this turn.
    pub cost: Option<f64>,
    /// ISO-8601 timestamp.
    pub timestamp: String,
}

/// A directed edge between two DAG nodes.
#[derive(Debug, Serialize)]
pub struct DagEdge {
    /// Source node id (parent_uuid).
    pub from: String,
    /// Target node id (uuid).
    pub to: String,
    /// Edge kind for styling.
    pub label: EdgeLabel,
    /// Whether to render as a dashed line.
    pub dashes: bool,
}

/// Semantic classification of a DAG edge.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EdgeLabel {
    /// Ordinary parent→child chain link.
    Chain,
    /// Agent spawn: trunk tool_use → agent root entry.
    Spawn,
    /// Fork point: same parent has multiple children (rewind / branch).
    Fork,
}
