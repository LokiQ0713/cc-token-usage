pub mod parser;
pub mod types;

#[cfg(feature = "scanner")]
pub mod scanner;

#[cfg(feature = "scanner")]
pub mod session;

// Re-exports
pub use parser::{parse_entry, LenientReader, ParseError, SessionReader};
pub use types::Entry;
pub use types::{WorkflowJournalEntry, WorkflowPhase, WorkflowProgress, WorkflowRunSnapshot};

#[cfg(feature = "scanner")]
pub use scanner::{
    load_workflow_agent_meta, scan_session_workflows, scan_sessions, scan_workflows, AgentMeta,
    SessionFile, WorkflowAgentFile, WorkflowRun,
};

#[cfg(feature = "scanner")]
pub use session::{load_all_sessions, load_session, AgentFile, RawSession};
