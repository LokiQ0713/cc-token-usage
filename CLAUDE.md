# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**cc-token-usage** — CLI tool that analyzes Claude Code session token usage, costs, and efficiency. Reads JSONL session files from `~/.claude/projects/`, calculates API-equivalent costs, and outputs terminal tables, interactive HTML dashboards, or JSON.

- **GitHub:** https://github.com/LokiQ0713/cc-token-usage
- **npm:** https://www.npmjs.com/package/cc-token-usage

## Workspace Structure

This is a cargo workspace with two crates:

- **`crates/cc-session-jsonl/`** — Pure JSONL parsing library. Types for all 25 Claude Code entry types, file scanner, session aggregation. Zero analysis logic.
- **`crates/cc-token-usage/`** — Analysis CLI. Consumes cc-session-jsonl for parsing, adds validation, deduplication, cost calculation, and output rendering.

## Commands

```bash
# Build & run
cargo build
cargo run -p cc-token-usage
cargo run -p cc-token-usage -- --format html        # Vue dashboard (real data)
cargo run -p cc-token-usage -- --format json
cargo run -p cc-token-usage -- session --latest
cargo run -p cc-token-usage -- validate --failures-only
cargo run -p cc-token-usage -- heatmap              # terminal heatmap ░▒▓█
cargo run -p cc-token-usage -- heatmap --days 0     # all history
cargo run -p cc-token-usage -- wrapped              # annual summary
cargo run -p cc-token-usage -- wrapped --year 2025  # specific year
cargo run -p cc-token-usage -- project --name foo   # filter by project name

# Test
cargo test --workspace --all-features               # all tests (324+ pass, 16 #[ignore]'d real-data)
cargo test -p cc-session-jsonl --all-features        # parsing library
cargo test -p cc-token-usage                         # analysis tool

# Pre-release: run real-data e2e against your local ~/.claude.
# REQUIRE_REAL_DATA=1 turns silent-skip into panic, so a missing reference
# session (e.g. ae289b37) becomes a test failure instead of a silent pass.
scripts/run-real-e2e.sh                              # MANDATORY before any version bump

# Lint & format
cargo clippy --workspace --all-features -- -D warnings
cargo fmt

# Snapshot tests (text.rs renderer)
cargo test -p cc-token-usage --test text_snapshots   # overview / projects / trend lock
# When intentional rendering changes:
#   cargo install cargo-insta    # one-time
#   cargo insta review           # accept/reject each .snap diff
# Commit the updated .snap file alongside the renderer change.

# Frontend (Vue dashboard)
cd frontend && npm install && npm run dev            # dev server with HMR
cd frontend && npm run build                         # build → dist/index.html (single-file, ~370KB)
# After build: copy into the crate, then it is embedded via include_str! in html_new.rs
cp frontend/dist/index.html crates/cc-token-usage/src/output/template.html
```

## Architecture

### cc-session-jsonl (parsing layer)

Pure types + parsing for Claude Code JSONL files. No analysis logic.

- **`types/`** — 25 entry types via `#[serde(tag = "type")]` enum. Uses `transcript_entry!` macro for shared fields across user/assistant/system/attachment entries. All fields `Option<T>` for version compatibility, `#[serde(other)] Unknown` for forward compatibility. `AttachmentEntry.attachment` (subtype in nested `attachment.type`) and `SystemEntry` 2.1.159+ fields (`content`/`is_meta`/`message_count`/`pending_workflow_count`, subtypes `turn_duration`/`local_command`/`away_summary`) are the newest additions.
- **`types/workflow.rs`** — workflow snapshot/journal types (Claude Code 2.1.159+): `WorkflowRunSnapshot` (`workflows/wf_<runId>.json`), `WorkflowPhase`, `WorkflowProgress`, `WorkflowJournalEntry` (`journal.jsonl`). Agent transcripts themselves reuse the regular `Entry`/`SessionReader` path.
- **`parser.rs`** — `parse_entry()`, `SessionReader` (strict), `LenientReader` (skip errors + count)
- **`scanner.rs`** — File discovery for 4 layouts (main session, legacy agent, new-style subagent, and Type4 workflow agents under `<uuid>/subagents/workflows/wf_*/agent-*.jsonl`). Workflow agent `SessionFile`s carry `is_agent`, `parent_session_id`, and `workflow_run_id`. Adds `scan_session_workflows`/`scan_workflows`/`load_workflow_agent_meta` and the `WorkflowRun`/`WorkflowAgentFile` types. Feature-gated behind `scanner`.
- **`session.rs`** — `RawSession` aggregation with title/tag/mode extraction from metadata entries; `RawSession.workflow_runs` holds discovered workflow runs (workflow agent files are aggregated into `agent_files` so their tokens count uniformly).

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
- **Workflow agents**: Workflow agent files reuse the subagent aggregation channel — their tokens auto-enter total cost via `all_responses()`. `Subagent.workflow_run_id` distinguishes workflow agents from ordinary ones; non-workflow and workflow `.meta.json` maps are merged (non-workflow wins on collision).
- **Metadata collection**: Titles, tags, mode, PR links, speculation accepts, errors collected during parsing into `SessionMetadata`
- **TokenUsage conversion**: `From<cc_session_jsonl::Usage>` converts between parsing and analysis types

#### Pricing (src/pricing/)

4-step model price resolution: exact override → prefix override → exact builtin → prefix builtin (then a latest-builtin fallback). Distinguishes 5m/1h cache write TTL tiers. `get_price` first strips a trailing context-window suffix (`claude-opus-4-8[1m]` / `[200k]` → `claude-opus-4-8`). A `claude-opus-4-8` builtin (opus-4-6 tier, $5/$25) was added — previously the name prefix-matched `claude-opus-4` ($15/$75), a ~3x overcharge now fixed.

#### Analysis (src/analysis/)

Each subcommand has its own module. All consume `Vec<SessionData>` + `PricingCalculator`.

- `overview.rs` — aggregate stats, model breakdown, per-agent costs, usage insights
- `project.rs` — project ranking by cost, `--name` filtering, session drill-down
- `session.rs` — single session detail with metadata, context collapse detection, attribution; `build_workflow_summaries` produces one `WorkflowSummary` per discovered run (declared snapshot figures + measured `parsed_*` totals re-aggregated from subagents matching `workflow_run_id`), carried on `SessionResult.workflows`
- `trend.rs` — daily/monthly trends with month-over-month comparison
- `heatmap.rs` — GitHub-style activity heatmap for terminal (░▒▓█ blocks), `--days` control
- `wrapped.rs` — Spotify-style annual summary with developer archetype classification (e.g., Night Owl, Weekend Warrior)
- `validate.rs` — **dual-path cross-validation**: independent raw JSON counter (serde_json::Value) re-counts tokens separately from the pipeline. Must stay independent of cc-session-jsonl to preserve verification integrity. Also reconciles each workflow run's parsed totals against its snapshot (supplementary checks; the independent re-count already covers workflow agent files since the scanner records them as ordinary `is_agent` files).

#### Output (src/output/)

- `text.rs` — terminal tables (comfy-table) + terminal heatmap (░▒▓█) + per-run `── Workflows ──` block in session detail
- `html.rs` — legacy HTML renderer (Chart.js, preserved as fallback)
- `html_new.rs` — Vue dashboard renderer: `include_str!` embeds `crates/cc-token-usage/src/output/template.html` (a committed copy of `frontend/dist/index.html`), injects JSON via `__DATA_PLACEHOLDER__` replacement
- `json.rs` — JSON export + `HtmlReportPayload` unified type for dashboard. Session entries always emit `workflows: WorkflowSummary[]` (camelCase: `runId`/`workflowName`/`status`/`snapshot*`/`phases`/`parsedAgentCount`/`parsedTurns`/`parsedOutputTokens`/`parsedCost`), possibly empty.

### Frontend (frontend/)

Vue 3 + Vite + TailwindCSS + Chart.js dashboard. Builds to self-contained single-file HTML (370KB, zero network dependencies).

- **Pages**: Overview, Trends, Projects, Sessions (with a per-run workflow drill-down block), Heatmap, Wrapped
- **Build**: `npm run build` → `dist/index.html`, then copied to `crates/cc-token-usage/src/output/template.html` for `include_str!`
- **Data flow**: Rust serializes `HtmlReportPayload` JSON → replaces `__DATA_PLACEHOLDER__` in template → self-contained HTML
- **Themes**: Dark/light via CSS variables
- **i18n**: EN/ZH via `useI18n()` composable

**Template update workflow**: When modifying the frontend, run `cd frontend && npm run build`, then `cp frontend/dist/index.html crates/cc-token-usage/src/output/template.html`. `include_str!` in `html_new.rs` embeds `template.html` (not `frontend/dist/index.html` directly) on the next `cargo build`. The copied `template.html` must be committed to git.

## Release Workflow

1. Run `scripts/run-real-e2e.sh` — must exit 0. This is the only validation that exercises the loader/scanner against real ~/.claude history; CI cannot run it.
2. Bump version in `crates/cc-token-usage/Cargo.toml` and `npm-package/package.json`
3. `npm version patch/minor/major` (creates git tag)
4. `git push && git push --tags`
5. GitHub Actions builds cross-platform binaries and publishes to npm

**Important**: Always commit `Cargo.lock` in release commits. Never delete and re-create release tags; always bump the version.

## CI/CD

- **CI** (`ci.yml`): `cargo check` + `cargo test` + `cargo clippy` (full workspace) on push/PR to master
- **Release** (`release.yml`): Triggered by `v*` tags, cross-compiles for Linux/macOS/Windows, publishes cc-session-jsonl then cc-token-usage to crates.io, publishes npm
