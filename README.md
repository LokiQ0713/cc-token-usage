# cc-token-usage

Analyze Claude Code session token usage, costs, and efficiency from local JSONL data.

![Overview Dashboard](assets/preview1.png)

## Features

- **Multi-dimension analysis** — overview, by project, by session, by day/month
- **Three-level drill-down** — Project → Session → Turn (with user/assistant message preview)
- **Token metrics** — output tokens, context size, cache hit rate, cache savings
- **Cost calculation** — official Anthropic API rates, 5m/1h cache TTL split
- **HTML dashboard** — Chart.js visualizations, sortable tables, activity heatmap
- **Chinese/English toggle** — built into the HTML report
- **Compaction detection** — identifies context window resets
- **Orphan agent attribution** — handles both old-style and new-style subagent files

## Screenshots

### Monthly Trends
![Monthly](assets/preview2.png)

### Project Drill-Down
![Projects](assets/preview3.png)

## Install

### Via cargo (Rust)

```bash
cargo install --path .
```

### Via npx (Node.js)

```bash
npx cc-token-usage
```

## Usage

```bash
# Default: print summary + generate HTML report
cc-token-usage

# Generate HTML dashboard and open in browser
cc-token-usage --format html

# By project
cc-token-usage project --top 0

# Latest session details
cc-token-usage session --latest

# Monthly trend
cc-token-usage trend --group-by month

# Daily trend (last 30 days)
cc-token-usage trend --days 30
```

### Example output

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

## How it works

Reads `~/.claude/projects/` JSONL session files directly. No API calls, no network access — pure local data analysis.

**Data sources:**
- Main sessions: `<project>/<uuid>.jsonl`
- Old-style agents: `<project>/agent-<id>.jsonl` (linked via `sessionId`)
- New-style agents: `<project>/<uuid>/subagents/agent-<id>.jsonl`

**Pricing data:** Anthropic official rates from [platform.claude.com/docs/en/about-claude/pricing](https://platform.claude.com/docs/en/about-claude/pricing)

## Configuration

Optional config file at `~/.config/cc-token-usage/config.toml`:

```toml
# Subscription price history
[[subscription]]
start_date = "2026-01-01"
monthly_price_usd = 200.0
plan = "max_20x"

# Override model pricing (optional)
# [pricing_override.claude-opus-4-6]
# base_input = 5.0
# cache_write_5m = 6.25
# cache_write_1h = 10.0
# cache_read = 0.50
# output = 25.0
```

## Tech Stack

Rust + serde + clap + chrono + comfy-table + Chart.js

## License

MIT
