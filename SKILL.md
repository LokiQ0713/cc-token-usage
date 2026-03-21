---
name: cc-token-usage
description: |
  Analyze Claude Code token usage, costs, and cache efficiency from local session data.
  Use when user asks about token spending, usage stats, how much they've used, cost analysis,
  cache performance, subscription value, or wants a usage dashboard.
  Trigger: "token usage", "how much have I spent", "cost analysis", "cache hit rate",
  "usage report", "token stats", "show my usage", "how many tokens", "spending",
  "subscription worth it", "cache savings", "which project costs most", "/cc-token-usage"
user-invocable: true
allowed-tools:
  - Bash
  - Read
---

# Claude Code Token Usage Analyzer

Analyze Claude Code session token usage, costs, cache efficiency from local JSONL session files. Generates terminal summaries and interactive HTML dashboards.

## Step 1: Check Installation

```bash
which cc-token-usage 2>/dev/null && cc-token-usage --version || echo "NOT_INSTALLED"
```

If `NOT_INSTALLED`, guide the user to install:

```
cc-token-usage is not installed yet. Install with:

  npm install -g cc-token-usage

Pre-built binaries are included for macOS (arm64/x64) and Linux (x64/arm64).
No Rust toolchain required.
```

If npm is unavailable or the user is on an unsupported platform, fall back to:

```
cargo install cc-token-usage    # requires Rust toolchain
```

Do not proceed until the tool is available.

## Step 2: Match Query to Command

| User Intent | Command |
|---|---|
| General usage / "how much have I used?" | `cc-token-usage` |
| Cost breakdown / "how much am I spending?" | `cc-token-usage` |
| Visual dashboard / "show me a dashboard" | `cc-token-usage --format html` |
| Project comparison / "which project uses most?" | `cc-token-usage project --top 10` |
| Monthly trend / "show monthly breakdown" | `cc-token-usage trend --group-by month` |
| Daily trend / "last 30 days" | `cc-token-usage trend --days 30` |
| Latest session / "what happened last session?" | `cc-token-usage session --latest` |
| Specific session | `cc-token-usage session <id>` |
| All history trend | `cc-token-usage trend --days 0` |
| Cache performance / "cache hit rate?" | `cc-token-usage` (cache savings in overview) |

When the user asks a vague question like "show my stats", default to `cc-token-usage` (overview mode).

When the user wants comprehensive analysis, suggest `cc-token-usage --format html` which opens an interactive dashboard in the browser.

## Step 3: Interpret Results

After running the command, help the user understand the output:

**Key metrics to highlight:**
- **Total token value**: API-equivalent cost at official Anthropic rates (not what user actually paid)
- **Cache savings**: How much prompt caching saved vs. no-cache baseline. Higher is better — 90%+ cache read rate means excellent efficiency
- **Cost by model**: Opus is 5-8x more expensive than Sonnet/Haiku per token
- **Output vs context ratio**: High output ratio = Claude is writing a lot; high context ratio = large conversations or many files read
- **Compaction events**: Context window resets — indicates conversations hitting the limit

**Cache TTL breakdown:**
- **5-minute cache**: Short-lived, cheaper writes (1.25x base input)
- **1-hour cache**: Longer-lived, more expensive writes (2x base input)
- **Cache reads**: Very cheap (0.1x base input) — this is where savings come from

**Activity patterns:**
- Heatmap shows when the user is most active (weekday x hour)
- Can reveal work patterns, late-night coding sessions, etc.

## HTML Dashboard

The `--format html` output generates a self-contained HTML file with:

- **Overview tab**: Total stats, model breakdown, cost pie chart, activity heatmap, cache savings
- **Monthly tab**: Trend charts with month-over-month comparison
- **Projects tab**: Three-level drill-down (Project → Session → Turn with user/assistant messages)
- Sortable columns, cache hit rate progress bars, Chinese/English toggle

## Error Handling

**No session data found:**
- Check if `~/.claude/projects/` exists and contains JSONL files
- The user may need to run Claude Code at least once first

**Stale pricing warning:**
- Built-in pricing is from 2026-03-21. If >90 days old, tool warns automatically
- User can override prices via `~/.config/cc-token-usage/config.toml`

**Custom Claude home:**
- If Claude data is in a non-standard location: `cc-token-usage --claude-home /path/to/claude`
