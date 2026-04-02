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
cargo test --workspace --all-features               # all tests (164+: 98 parsing + 61 analysis + 5 integration)
cargo test -p cc-session-jsonl --all-features        # parsing library (98 tests)
cargo test -p cc-token-usage                         # analysis tool (61+ tests)

# Lint & format
cargo clippy --workspace --all-features -- -D warnings
cargo fmt

# Frontend (Vue dashboard)
cd frontend && npm install && npm run dev            # dev server with HMR
cd frontend && npm run build                         # build → dist/index.html (single-file, ~370KB)
# After build: dist/index.html is committed to git, embedded via include_str! in html_new.rs
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

- `overview.rs` — aggregate stats, model breakdown, per-agent costs, usage insights
- `project.rs` — project ranking by cost, `--name` filtering, session drill-down
- `session.rs` — single session detail with metadata, context collapse detection, attribution
- `trend.rs` — daily/monthly trends with month-over-month comparison
- `heatmap.rs` — GitHub-style activity heatmap for terminal (░▒▓█ blocks), `--days` control
- `wrapped.rs` — Spotify-style annual summary with developer archetype classification (e.g., Night Owl, Weekend Warrior)
- `validate.rs` — **dual-path cross-validation**: independent raw JSON counter (serde_json::Value) re-counts tokens separately from the pipeline. Must stay independent of cc-session-jsonl to preserve verification integrity.

#### Output (src/output/)

- `text.rs` — terminal tables (comfy-table) + terminal heatmap (░▒▓█)
- `html.rs` — legacy HTML renderer (Chart.js, preserved as fallback)
- `html_new.rs` — Vue dashboard renderer: `include_str!` embeds `frontend/dist/index.html`, injects JSON via `__DATA_PLACEHOLDER__` replacement
- `json.rs` — JSON export + `HtmlReportPayload` unified type for dashboard

### Frontend (frontend/)

Vue 3 + Vite + TailwindCSS + Chart.js dashboard. Builds to self-contained single-file HTML (370KB, zero network dependencies).

- **Pages**: Overview, Trends, Projects, Sessions, Heatmap, Wrapped
- **Build**: `npm run build` → `dist/index.html` (committed to git for `include_str!`)
- **Data flow**: Rust serializes `HtmlReportPayload` JSON → replaces `__DATA_PLACEHOLDER__` in template → self-contained HTML
- **Themes**: Dark/light via CSS variables
- **i18n**: EN/ZH via `useI18n()` composable

**Template update workflow**: When modifying the frontend, run `cd frontend && npm run build`, then the updated `dist/index.html` is automatically picked up by `include_str!` in `html_new.rs` on next `cargo build`. The built file must be committed to git.

## Release Workflow

1. Bump version in `crates/cc-token-usage/Cargo.toml` and `npm-package/package.json`
2. `npm version patch/minor/major` (creates git tag)
3. `git push && git push --tags`
4. GitHub Actions builds cross-platform binaries and publishes to npm

**Important**: Always commit `Cargo.lock` in release commits. Never delete and re-create release tags; always bump the version.

## CI/CD

- **CI** (`ci.yml`): `cargo check` + `cargo test` + `cargo clippy` (full workspace) on push/PR to master
- **Release** (`release.yml`): Triggered by `v*` tags, cross-compiles for Linux/macOS/Windows, publishes cc-session-jsonl then cc-token-usage to crates.io, publishes npm
