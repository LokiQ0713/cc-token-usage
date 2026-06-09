//! Reconstruct Claude Code session conversation DAG from raw JSONL.
//!
//! This crate consumes `cc_session_jsonl::Entry` and rebuilds the conversation
//! structure — trunk chain, rewind branches, tool_use ↔ tool_result pairing,
//! sidechain spawn edges — preserving the original `uuid → parentUuid`
//! deterministic chain semantics. Zero analysis logic.
