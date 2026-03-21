# cc-token-usage

**Ever wonder how many tokens Claude has been munching through?** This tool digs into your local Claude Code session data and tells you exactly where every token went — no API calls, no cloud, just your local `.jsonl` files.

**想知道 Claude 到底吃掉了你多少 token？** 这个工具直接分析本地 Claude Code 的 session 数据，告诉你每一个 token 的去向 —— 不调 API，不联网，纯本地分析。

![Overview Dashboard](assets/preview1.png)

## What You Get / 你能看到什么

- **The big picture** — sessions, turns, tokens read/written, cache savings, API-equivalent cost
  **全局概览** — 会话数、轮次、读写 token、缓存节省、API 等效费用
- **Project drill-down** — which project is burning the most tokens? Click to see sessions, click again to see every single turn
  **项目钻取** — 哪个项目最烧 token？点开看会话，再点开看每一轮对话
- **Monthly trends** — daily cost chart, month-over-month comparison
  **月度趋势** — 每日费用柱状图、按月汇总对比
- **Cache analysis** — 90% of your "reads" are free thanks to caching. We'll show you exactly how much that saved
  **缓存分析** — 90% 的"读取"因为缓存而免费。我们会告诉你这省了多少钱
- **Message preview** — see what you asked and what Claude replied, turn by turn
  **消息预览** — 逐轮查看你问了什么、Claude 回了什么

### Monthly Trends / 月度趋势
![Monthly](assets/preview2.png)

### Project Drill-Down / 项目钻取
![Projects](assets/preview3.png)

## Install / 安装

```bash
# One-liner, no Rust needed / 一行搞定，不需要 Rust
npx cc-token-usage

# Or install the Rust binary / 或者安装 Rust 二进制
cargo install --path .
```

## Usage / 使用

```bash
# Just run it. Summary + HTML dashboard, opens in browser
# 直接跑。终端汇总 + HTML 仪表盘，自动打开浏览器
cc-token-usage

# HTML dashboard only / 只生成 HTML 仪表盘
cc-token-usage --format html

# All projects ranked by cost / 按费用排名所有项目
cc-token-usage project --top 0

# What did the latest session look like? / 最近一次会话长什么样？
cc-token-usage session --latest

# Monthly breakdown / 按月汇总
cc-token-usage trend --group-by month
```

### Example Output / 示例输出

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

## How It Works / 工作原理

Reads `~/.claude/projects/` directly. Parses every JSONL session file, including subagent files (both old flat-style and new nested-style). Validates data, deduplicates, attributes orphan agents, detects context compactions.

直接读取 `~/.claude/projects/`。解析所有 JSONL session 文件，包括子 agent 文件（新旧两种格式）。校验数据、去重、归属孤立 agent、检测上下文压缩。

**Pricing:** Uses official Anthropic rates from [platform.claude.com](https://platform.claude.com/docs/en/about-claude/pricing). Distinguishes 5-minute vs 1-hour cache TTL for accurate cost calculation.

**定价：** 使用 [Anthropic 官方费率](https://platform.claude.com/docs/en/about-claude/pricing)。区分 5 分钟和 1 小时缓存 TTL，精确计算费用。

## Configuration / 配置

Optional. Create `~/.config/cc-token-usage/config.toml`:

可选。创建 `~/.config/cc-token-usage/config.toml`：

```toml
[[subscription]]
start_date = "2026-01-01"
monthly_price_usd = 200.0
plan = "max_20x"
```

## Tech Stack

Rust (serde, clap, chrono, comfy-table) + Chart.js

## License

MIT
