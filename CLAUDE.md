# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**cc-token-usage** — CLI tool that analyzes Claude Code session token usage, costs, and efficiency. Reads JSONL session files from `~/.claude/projects/`, calculates API-equivalent costs, and outputs terminal tables or interactive HTML dashboards.

- **GitHub:** https://github.com/LokiQ0713/cc-token-usage
- **npm:** https://www.npmjs.com/package/cc-token-usage

## Commands

```bash
# Build & run
cargo build
cargo run
cargo run -- --format html
cargo run -- session --latest
cargo run -- validate --failures-only

# Test (all tests, or a single test)
cargo test
cargo test parse_valid_assistant_turn

# Lint & format
cargo clippy -- -D warnings
cargo fmt
```

## Architecture

### Data Pipeline (src/data/)

The core data flow is a 5-stage pipeline in `parser.rs`:

```
JSONL line → JSON parse → type filter → validation → content extraction → request_id dedup
```

Key design decisions:
- **Sidechain filtering**: Main session files skip `isSidechain=true` entries (abandoned generations), but agent files keep them (agents always have `isSidechain=true`)
- **Streaming dedup**: Same `requestId` appearing multiple times (streaming retries) keeps the last entry
- **Cross-file dedup** (in `loader.rs`): Agent turns that already appear in the parent session (by `requestId`) are dropped to prevent double-counting

Session files are discovered by `scanner.rs` which handles three file layouts:
1. `<project>/<uuid>.jsonl` — main sessions
2. `<project>/agent-<id>.jsonl` — legacy agents (parent resolved from first line's `sessionId`)
3. `<project>/<uuid>/subagents/agent-<id>.jsonl` — new-style agents (parent from directory name)

### Pricing (src/pricing/)

`PricingCalculator` resolves model prices with a 4-step fallback: exact override → prefix override → exact builtin → prefix builtin. This handles versioned model IDs like `claude-opus-4-5-20251101` matching `claude-opus-4-5`. Cache write costs distinguish two TTL tiers (5-minute and 1-hour). Built-in prices have a staleness check (>90 days).

### Analysis (src/analysis/)

Each subcommand has its own analysis module (`overview.rs`, `project.rs`, `session.rs`, `trend.rs`). All consume `Vec<SessionData>` + `PricingCalculator` and return typed result structs defined in `mod.rs`.

`validate.rs` implements **dual-path cross-validation**: an independent raw JSON counter (using `serde_json::Value`, no shared types with the main pipeline) re-counts tokens from JSONL files and compares against the pipeline's output.

### Output (src/output/)

- `text.rs` — terminal tables via `comfy-table`
- `html.rs` — self-contained HTML with embedded Chart.js, supports light/dark theme and i18n (en/zh)

## Release Workflow

1. Bump version in both `Cargo.toml` and `npm-package/package.json`
2. `npm version patch/minor/major` (creates git tag)
3. `git push && git push --tags`
4. GitHub Actions `release.yml` builds cross-platform binaries and publishes to npm

**Important**: Always commit `Cargo.lock` in release commits — CI needs it for reproducible builds. Never delete and re-create release tags; always bump the version instead.

## CI/CD

- **CI** (`ci.yml`): `cargo check` + `cargo test` + `cargo clippy` on push/PR to master
- **Release** (`release.yml`): Triggered by `v*` tags, cross-compiles for Linux/macOS/Windows, publishes npm package
