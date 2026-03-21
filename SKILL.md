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

Pre-built binaries for macOS (arm64/x64) and Linux (x64/arm64). No Rust required.
```

If npm is unavailable, fall back to `cargo install cc-token-usage` (requires Rust toolchain).

If the user is on an unsupported platform (e.g., Windows), they can compile from source:

```bash
git clone https://github.com/LokiQ0713/cc-token-usage.git
cd cc-token-usage && cargo build --release
```

The CI/CD pipeline supports cross-compilation — contributors can add new platform targets to `.github/workflows/release.yml` by adding a matrix entry.

Do not proceed until the tool is available.

## Step 2: Detect Claude Data Directory

The tool defaults to `~/.claude/` but supports a `--claude-home` flag for any directory:

```bash
cc-token-usage --claude-home /path/to/.claude
```

**If the default path doesn't work** (no data found, non-standard install, or unfamiliar OS), probe the environment to find the right path:

```bash
# Check default location
ls ~/.claude/projects/ 2>/dev/null | head -3

# If empty, try common alternatives
ls "$HOME/.claude/projects/" 2>/dev/null | head -3
ls "$USERPROFILE/.claude/projects/" 2>/dev/null | head -3  # Windows-style
```

The tool expects a directory containing a `projects/` subdirectory with JSONL session files. Once you find it, pass it via `--claude-home`. The tool handles the rest — path parsing, project naming, and data loading are all platform-agnostic.

## Step 3: Match Query to Command

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

## Step 4: Interpret Results

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
- Try `--claude-home` with detected path if default doesn't work

**Stale pricing warning:**
- Built-in pricing is from 2026-03-21. If >90 days old, tool warns automatically
- User can override prices via `~/.config/cc-token-usage/config.toml`

**Custom data location:**
- Non-standard Claude install: `cc-token-usage --claude-home /path/to/.claude`
- The tool only needs the `.claude` directory — it will find `projects/` inside it automatically
