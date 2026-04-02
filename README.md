# cc-token-usage

[![Release](https://github.com/LokiQ0713/cc-token-usage/actions/workflows/release.yml/badge.svg)](https://github.com/LokiQ0713/cc-token-usage/actions/workflows/release.yml)
[![npm](https://img.shields.io/npm/v/cc-token-usage)](https://www.npmjs.com/package/cc-token-usage)
[![crates.io](https://img.shields.io/crates/v/cc-token-usage)](https://crates.io/crates/cc-token-usage)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

CLI tool that analyzes your Claude Code session data locally -- token usage, costs, efficiency, and coding patterns. No API calls, no cloud, just your local `.jsonl` files.

[中文文档](README_ZH.md)

<!-- screenshot -->
![Overview Dashboard](assets/preview1.png)

## Features

- **Interactive HTML Dashboard** -- Vue 3 single-file app with 6 pages (Overview, Trends, Projects, Sessions, Heatmap, Wrapped), dark/light theme, EN/ZH i18n
- **Terminal Reports** -- rich tables via comfy-table, works anywhere
- **Heatmap** -- GitHub-style terminal activity heatmap (`░▒▓█`)
- **Wrapped** -- Spotify-style annual summary with developer archetypes
- **Project Drill-Down** -- rank projects by cost, filter by name, drill into sessions and turns
- **Trend Analysis** -- daily/monthly cost trends, month-over-month comparison
- **Context Collapse Detection** -- identify sessions where context was compacted and assess risk
- **Code Attribution** -- track Claude's code contributions per session
- **Efficiency Metrics** -- output ratio, cost per turn, cache savings analysis
- **Session Metadata Mining** -- titles, tags, mode, branch, errors, speculation accepts
- **Dual-Path Validation** -- independent raw JSON counter cross-checks the pipeline
- **JSON Export** -- all subcommands support `--format json` for tool integration
- **Parallel Processing** -- rayon-powered parallel parsing (3.9x speedup on large datasets)
- **164+ Tests** -- comprehensive coverage across both crates

### Monthly Trends
![Monthly](assets/preview2.png)

### Project Drill-Down
![Projects](assets/preview3.png)

## Install

Quick install (macOS / Linux):

```bash
curl -fsSL https://raw.githubusercontent.com/LokiQ0713/cc-token-usage/master/install.sh | sh
```

Via npm:

```bash
npx cc-token-usage            # zero-install, always latest
npm install -g cc-token-usage  # global install
```

Via cargo:

```bash
cargo install cc-token-usage
```

### Update

```bash
cc-token-usage update          # download and replace
cc-token-usage update --check  # check only, don't download
```

### As a Claude Code Skill

Install as a skill so Claude can run it for you when you ask "how much have I spent?":

```bash
npx skills add LokiQ0713/cc-token-usage -g -y
```

## Usage

Default: prints terminal summary + opens HTML dashboard in browser:

```bash
cc-token-usage
```

### Subcommands

| Command | Description |
|---------|-------------|
| `overview` | Overall usage summary across all projects |
| `project` | Project-level breakdown, ranked by cost |
| `session` | Single session detail with turn-by-turn analysis |
| `trend` | Daily/monthly usage trends |
| `heatmap` | GitHub-style terminal activity heatmap |
| `wrapped` | Annual "Wrapped" summary with developer archetype |
| `validate` | Cross-validate token counts against raw JSONL |
| `update` | Self-update the binary |

### Examples

```bash
# HTML dashboard only
cc-token-usage --format html

# JSON export
cc-token-usage --format json

# All projects ranked by cost
cc-token-usage project --top 0

# Filter a specific project
cc-token-usage project --name "my-project"

# Latest session details
cc-token-usage session --latest

# Monthly breakdown
cc-token-usage trend --group-by month

# Daily trend (last 30 days)
cc-token-usage trend --days 30

# Terminal heatmap (last year)
cc-token-usage heatmap

# Heatmap for all history
cc-token-usage heatmap --days 0

# Annual wrapped summary
cc-token-usage wrapped

# Wrapped for a specific year
cc-token-usage wrapped --year 2025

# Validate token counting accuracy
cc-token-usage validate --failures-only
```

### Example Output

```
Claude Code Token Report
2026-01-11 ~ 2026-03-21

  238 conversations, 19,720 rounds of back-and-forth

  Claude read  1,913,274,753 tokens
  Claude wrote 4,776,580 tokens

  Cache saved you $7,884.80 (90% of reads were free)
  All that would cost $1,554.50 at API rates

  Model                      Wrote        Rounds     Cost
  ---------------------------------------------------------
  opus-4-6                   4,005,415    15,219 $1,435.47
  sonnet-4-6                   479,336     1,533    $73.22
  haiku-4-5                    254,469     2,322    $19.26

  Top Projects                              Sessions   Turns    Cost
  -------------------------------------------------------------------
  ~/cc                                        80    6134   $606.15
  ~/Desktop/claude/statusline/config           2    5603   $439.16
```

## HTML Dashboard

The `--format html` flag generates a self-contained single-file HTML dashboard (370KB, zero network dependencies). It includes:

- **Overview** -- aggregate stats, model breakdown, per-agent costs, usage insights
- **Trends** -- daily cost chart with month-over-month comparison
- **Projects** -- all projects ranked by cost, click to drill into sessions
- **Sessions** -- session list with metadata (title, tags, mode, branch)
- **Heatmap** -- interactive activity heatmap
- **Wrapped** -- annual summary with developer archetype classification

Built with Vue 3 + TailwindCSS + Chart.js. Supports dark/light theme toggle and EN/ZH language switching.

## How It Works

Reads `~/.claude/projects/` directly. Parses every JSONL session file, including subagent files (both legacy flat-style and new nested-style). Validates data, deduplicates by `requestId`, attributes orphan agents, detects context compactions.

**Parsing library:** The `cc-session-jsonl` crate handles all JSONL parsing independently -- 23 entry types, 3 file layout formats, forward-compatible with `Unknown` variant. Available as a standalone crate on [crates.io](https://crates.io/crates/cc-session-jsonl).

**Pricing:** Uses official Anthropic rates from [platform.claude.com](https://platform.claude.com/docs/en/about-claude/pricing). Distinguishes 5-minute vs 1-hour cache TTL for accurate cost calculation.

## Configuration

Optional. Create `~/.config/cc-token-usage/config.toml` to override model pricing:

```toml
[pricing_override.claude-opus-4-6]
base_input = 5.0
cache_write_5m = 6.25
cache_write_1h = 10.0
cache_read = 0.50
output = 25.0
```

## Development

### Workspace Structure

```
crates/
  cc-session-jsonl/   # Pure JSONL parsing library (98 tests)
  cc-token-usage/     # Analysis CLI (61+ tests)
frontend/             # Vue 3 + Vite + TailwindCSS dashboard
npm-package/          # npm binary wrapper
```

### Build & Test

```bash
cargo build
cargo test --workspace --all-features    # 164+ tests
cargo clippy --workspace --all-features -- -D warnings
cargo fmt
```

### Frontend Development

```bash
cd frontend
npm install
npm run dev      # dev server with HMR
npm run build    # build single-file HTML → dist/index.html
```

After building, the Rust binary embeds `frontend/dist/index.html` via `include_str!` -- no separate file needed at runtime.

## Tech Stack

Rust (serde, clap, chrono, comfy-table, rayon) + Vue 3 + TailwindCSS + Chart.js

## License

MIT
