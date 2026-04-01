# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**cc-token-usage** — CLI tool that analyzes Claude Code session token usage, costs, and efficiency. Reads JSONL session files from `~/.claude/projects/`, calculates API-equivalent costs, and outputs terminal tables, interactive HTML dashboards, or JSON.

- **GitHub:** https://github.com/LokiQ0713/cc-token-usage
- **npm:** https://www.npmjs.com/package/cc-token-usage

## Workspace Structure

This is a cargo workspace with two crates:

- **`crates/cc-session-jsonl/`** — Pure JSONL parsing library. Types for all 23 Claude Code entry types, file scanner, session aggregation. Zero analysis logic.
- **`crates/cc-token-usage/`** — Analysis CLI. Consumes cc-session-jsonl for parsing, adds validation, deduplication, cost calculation, and output rendering.

## Commands

```bash
# Build & run
cargo build
cargo run -p cc-token-usage
cargo run -p cc-token-usage -- --format html
cargo run -p cc-token-usage -- --format json
cargo run -p cc-token-usage -- session --latest
cargo run -p cc-token-usage -- validate --failures-only

# Test
cargo test                                    # all workspace tests
cargo test -p cc-session-jsonl --all-features  # parsing library (104 tests)
cargo test -p cc-token-usage                   # analysis tool (44+ tests)

# Lint & format
cargo clippy --workspace --all-features -- -D warnings
cargo fmt
```

## Architecture

### cc-session-jsonl (parsing layer)

Pure types + parsing for Claude Code JSONL files. No analysis logic.

- **`types/`** — 23 entry types via `#[serde(tag = "type")]` enum. Uses `transcript_entry!` macro for shared fields across user/assistant/system/attachment entries. All fields `Option<T>` for version compatibility, `#[serde(other)] Unknown` for forward compatibility.
- **`parser.rs`** — `parse_entry()`, `SessionReader` (strict), `LenientReader` (skip errors + count)
- **`scanner.rs`** — File discovery for 3 layouts (main session, legacy agent, new-style subagent). Feature-gated behind `scanner`.
- **`session.rs`** — `RawSession` aggregation with title/tag/mode extraction from metadata entries.

### cc-token-usage (analysis layer)

Consumes cc-session-jsonl types, adds validation + dedup + pricing + output.

#### Data Pipeline (src/data/)

```
cc_session_jsonl::Entry → type filter → validation → content extraction → request_id dedup → ValidatedTurn
```

Key design decisions:
- **Sidechain filtering**: Main files skip `isSidechain=true`, agent files keep them
- **Streaming dedup**: Same `requestId` keeps the last entry
- **Cross-file dedup**: Agent turns already in parent session (by `requestId`) are dropped
- **Metadata collection**: Titles, tags, mode, PR links, speculation accepts, errors collected during parsing into `SessionMetadata`
- **TokenUsage conversion**: `From<cc_session_jsonl::Usage>` converts between parsing and analysis types

#### Pricing (src/pricing/)

4-step model price resolution: exact override → prefix override → exact builtin → prefix builtin. Distinguishes 5m/1h cache write TTL tiers.

#### Analysis (src/analysis/)

Each subcommand has its own module. All consume `Vec<SessionData>` + `PricingCalculator`.

`validate.rs` — **dual-path cross-validation**: independent raw JSON counter (serde_json::Value) re-counts tokens separately from the pipeline. Must stay independent of cc-session-jsonl to preserve verification integrity.

#### Output (src/output/)

- `text.rs` — terminal tables (comfy-table)
- `html.rs` — self-contained HTML with Chart.js, light/dark theme, i18n
- `json.rs` — JSON export for tool integration

## Release Workflow

1. Bump version in `crates/cc-token-usage/Cargo.toml` and `npm-package/package.json`
2. `npm version patch/minor/major` (creates git tag)
3. `git push && git push --tags`
4. GitHub Actions builds cross-platform binaries and publishes to npm

**Important**: Always commit `Cargo.lock` in release commits. Never delete and re-create release tags; always bump the version.

## CI/CD

- **CI** (`ci.yml`): `cargo check` + `cargo test` + `cargo clippy` on push/PR to master
- **Release** (`release.yml`): Triggered by `v*` tags, cross-compiles for Linux/macOS/Windows, publishes npm
