pub mod parser;
pub mod types;

// Loader is the only public entry point for session aggregation.
// Scanner is an implementation detail — held private so the loader owns the
// file-layout contract.
mod loader;
pub(crate) mod scanner;

// Re-exports — parser & types unchanged.
pub use parser::{parse_entry, LenientReader, ParseError, SessionReader};
pub use types::Entry;
pub use types::{WorkflowJournalEntry, WorkflowPhase, WorkflowProgress, WorkflowRunSnapshot};

// Loader public surface.
pub use loader::{
    load_agent_metadata, load_all_sessions, load_session, load_workflows_for_session, Agent,
    AgentMetadata, Session, Workflow,
};
