---
name: cc-token-usage
description: Analyze Claude Code token usage, costs, and cache efficiency. Use when user asks about token spending, usage stats, how much they've used, cost analysis, cache performance, or wants a usage dashboard. Triggers on keywords like "token usage", "how much have I spent", "cost analysis", "cache hit rate", "usage report", "token stats".
---

# Claude Code Token Usage Analyzer

Analyze your Claude Code session token usage, costs, cache efficiency, and generate interactive HTML dashboards.

## Prerequisites

The tool must be installed first. Check if it's available:

```bash
which cc-token-usage || echo "not installed"
```

If not installed, install via one of:

```bash
# Via npm (recommended, includes auto-open browser)
npm install -g cc-token-usage

# Via cargo
cargo install cc-token-usage
```

## Commands

```bash
# Overview summary (default) - shows total tokens, costs, cache savings, model breakdown
cc-token-usage

# Interactive HTML dashboard (opens in browser)
cc-token-usage --format html

# Per-project breakdown
cc-token-usage project --top 10

# Latest session details
cc-token-usage session --latest

# Specific session by ID
cc-token-usage session <session-id>

# Monthly trend
cc-token-usage trend --group-by month

# Daily trend (last 30 days)
cc-token-usage trend --days 30

# All history trend
cc-token-usage trend --days 0
```

## When to Use

- **General usage questions**: "how much have I used?" / "show my token stats" → run `cc-token-usage`
- **Cost analysis**: "how much am I spending?" / "what's the API equivalent cost?" → run `cc-token-usage`
- **Visual dashboard**: "show me a dashboard" / "visualize my usage" → run `cc-token-usage --format html`
- **Project comparison**: "which project uses the most tokens?" → run `cc-token-usage project`
- **Trend analysis**: "show usage over time" / "monthly breakdown" → run `cc-token-usage trend --group-by month`
- **Cache performance**: "how's my cache hit rate?" → run `cc-token-usage` (cache savings shown in overview)
- **Session deep-dive**: "what happened in my last session?" → run `cc-token-usage session --latest`

## Output

The tool reads local JSONL session files from `~/.claude/projects/` and calculates:

- **Token consumption** by model (input, output, cache read, cache write 5m/1h)
- **API-equivalent costs** based on official Anthropic pricing
- **Cache savings** (how much you saved via prompt caching)
- **Activity heatmap** (weekday x hour usage patterns)
- **Per-project and per-session breakdowns** with three-level drill-down
- **Compaction detection** (context window resets)
- **Monthly/daily trends**

## HTML Dashboard Features

The `--format html` output generates a self-contained HTML file with:

- Interactive Chart.js visualizations
- Three tabs: Overview, Monthly Trends, Projects
- Three-level drill-down: Project → Session → Turn (with user/assistant messages)
- Sortable columns, cache hit rate progress bars
- Chinese/English language toggle
- Activity heatmap (canvas-rendered)
