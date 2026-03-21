# cc-token-usage

[![Release](https://github.com/LokiQ0713/cc-token-usage/actions/workflows/release.yml/badge.svg)](https://github.com/LokiQ0713/cc-token-usage/actions/workflows/release.yml)
[![npm](https://img.shields.io/npm/v/cc-token-usage)](https://www.npmjs.com/package/cc-token-usage)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Ever wonder how many tokens Claude has been munching through?** This tool digs into your local Claude Code session data and tells you exactly where every token went — no API calls, no cloud, just your local `.jsonl` files.

[中文文档](README_ZH.md)

![Overview Dashboard](assets/preview1.png)

## What You Get

- **The big picture** — sessions, turns, tokens read/written, cache savings, API-equivalent cost
- **Project drill-down** — which project is burning the most tokens? Click to see sessions, click again to see every single turn
- **Monthly trends** — daily cost chart, month-over-month comparison
- **Cache analysis** — 90% of your "reads" are free thanks to caching. We show you exactly how much that saved
- **Message preview** — see what you asked and what Claude replied, turn by turn

### Monthly Trends
![Monthly](assets/preview2.png)

### Project Drill-Down
![Projects](assets/preview3.png)

## Install

Via npx (zero install):

```bash
npx cc-token-usage
```

Via cargo:

```bash
cargo install cc-token-usage
```

From source:

```bash
git clone https://github.com/LokiQ0713/cc-token-usage.git
cd cc-token-usage
cargo install --path .
```

## Usage

Just run it — prints summary to terminal, generates HTML dashboard, opens in browser:

```bash
cc-token-usage
```

HTML dashboard only:

```bash
cc-token-usage --format html
```

All projects ranked by cost:

```bash
cc-token-usage project --top 0
```

Latest session details:

```bash
cc-token-usage session --latest
```

Monthly breakdown:

```bash
cc-token-usage trend --group-by month
```

Daily trend (last 30 days):

```bash
cc-token-usage trend --days 30
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

## How It Works

Reads `~/.claude/projects/` directly. Parses every JSONL session file, including subagent files (both old flat-style and new nested-style). Validates data, deduplicates, attributes orphan agents, detects context compactions.

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

## Tech Stack

Rust (serde, clap, chrono, comfy-table) + Chart.js

## License

MIT
