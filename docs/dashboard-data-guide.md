# Dashboard Data Guide for UI/UX Designers

> 本文档基于 cc-token-usage v1.4.0 的真实数据和代码分析，为 HTML 仪表盘重设计提供数据层面的完整指导。

---

## A. 指标目录

### A1. Overview (全局总览)

| 指标名 | JSON 路径 | 类型 | 含义 | 典型范围 | 推荐图表 |
|--------|-----------|------|------|----------|----------|
| total_sessions | `overview.total_sessions` | int | 所有 session 数量 | 50~500 | KPI Card |
| total_turns | `overview.total_turns` | int | 所有 API 交互回合数 | 5K~50K | KPI Card |
| total_agent_turns | `overview.total_agent_turns` | int | 由 subagent 执行的回合数 | 占 total_turns 的 30%~60% | KPI Card + 占比环形图 |
| total_output_tokens | `overview.total_output_tokens` | int | 模型总输出 token 数 | 1M~20M | KPI Card |
| total_context_tokens | `overview.total_context_tokens` | int | 总上下文 token 数（input + cache_write + cache_read） | 500M~5B | KPI Card |
| total_cost | `overview.total_cost` | float | API 等价总费用（美元） | $100~$5000 | KPI Card (大字号) |
| avg_cache_hit_rate | `overview.avg_cache_hit_rate` | float | 全局平均缓存命中率（百分比） | 90%~99% | Gauge/进度条 |
| output_ratio | `overview.output_ratio` | float | 输出 token 占总 context 的百分比 | 0.3%~3% | KPI Card |
| cost_per_turn | `overview.cost_per_turn` | float | 每回合平均成本（美元） | $0.03~$0.20 | KPI Card |
| tokens_per_output_turn | `overview.tokens_per_output_turn` | int | 每回合平均输出 token 数 | 200~800 | KPI Card |
| cache_savings.total_saved | `overview.cache_savings.total_saved` | float | 缓存节省的金额（美元） | $500~$20000 | KPI Card (醒目) |
| cache_savings.savings_pct | `overview.cache_savings.savings_pct` | float | 缓存节省百分比 | 85%~95% | Gauge/进度条 |
| subscription_value.monthly_price | `overview.subscription_value.monthly_price` | float\|null | 订阅月费（如 $200） | 100~200 | KPI Card |
| subscription_value.api_equivalent | `overview.subscription_value.api_equivalent` | float\|null | API 等价月费用 | $200~$3000 | KPI Card |
| subscription_value.value_multiplier | `overview.subscription_value.value_multiplier` | float\|null | 订阅价值倍率 | 2x~15x | KPI Card (大字号) |
| cost_by_category.input_cost | `overview.cost_by_category.input_cost` | float | 原始输入费用 | 占总费用 <1% | Donut Chart |
| cost_by_category.output_cost | `overview.cost_by_category.output_cost` | float | 输出费用 | 占总费用 10%~15% | Donut Chart |
| cost_by_category.cache_write_cost | `overview.cost_by_category.cache_write_cost` | float | 缓存写入费用 | 占总费用 25%~30% | Donut Chart |
| cost_by_category.cache_read_cost | `overview.cost_by_category.cache_read_cost` | float | 缓存读取费用 | 占总费用 55%~65% | Donut Chart |
| models[] | `overview.models` | array | 按模型分组的 token/turn/cost | 通常 2~5 个模型 | Horizontal Bar |
| top_tools[] | `overview.top_tools` | array | 工具使用次数排行（前 20） | Bash > Read > Edit > Grep | Horizontal Bar |
| sessions[] | `overview.sessions` | array | 所有 session 的汇总信息 | 每项含 14 个字段 | Table + Sparkline |

### A2. Session Detail (会话详情)

| 指标名 | JSON 路径 | 类型 | 含义 | 典型范围 | 推荐图表 |
|--------|-----------|------|------|----------|----------|
| session_id | `session.session_id` | string | UUID 格式的会话 ID | - | 标题文本 |
| project | `session.project` | string | 项目目录名 | - | 标签 |
| model | `session.model` | string | 主力模型名 | claude-opus-4-6 等 | 标签 |
| duration_minutes | `session.duration_minutes` | float | 会话时长（分钟） | 0~2400 (最长 39 小时) | KPI Card |
| total_cost | `session.total_cost` | float | 会话总费用 | $0.01~$180 | KPI Card |
| max_context | `session.max_context` | int | 最大上下文窗口（token） | 10K~400K | KPI Card |
| compaction_count | `session.compaction_count` | int | 上下文压缩次数 | 0~150 | KPI Card |
| output_tokens | `session.output_tokens` | int | 输出 token 总量 | 100~600K | KPI Card |
| context_tokens | `session.context_tokens` | int | 上下文 token 总量 | 10K~100M | KPI Card |
| cache_hit_rate | `session.cache_hit_rate` | float | 缓存命中率（百分比） | 0%~99% | Gauge |
| agent_turns | `session.agent_turns` | int | subagent 回合数 | 0~600 | KPI Card |
| agent_output_tokens | `session.agent_output_tokens` | int | subagent 输出 token | 0~250K | KPI Card |
| agent_cost | `session.agent_cost` | float | subagent 费用 | $0~$25 | KPI Card |
| title | `session.title` | string\|null | 会话标题（从元数据提取） | - | 标题文本 |
| tags[] | `session.tags` | string[] | 会话标签 | - | 标签列表 |
| turns[] | `session.turns` | array | 逐 turn 详情 | 每项含 13 个字段 | Timeline/Table |

### A3. Turn Detail (单回合粒度)

| 指标名 | JSON 路径 | 类型 | 含义 | 典型范围 | 推荐图表 |
|--------|-----------|------|------|----------|----------|
| turn_number | `turns[].turn_number` | int | 回合序号（从 1 开始） | 1~1000+ | X 轴 |
| timestamp | `turns[].timestamp` | ISO 8601 | 回合时间戳 | - | X 轴 |
| model | `turns[].model` | string | 使用的模型 | - | 颜色编码 |
| input_tokens | `turns[].input_tokens` | int | 原始输入 token（非缓存） | 0~5000 | Stacked Area |
| output_tokens | `turns[].output_tokens` | int | 输出 token 数 | 50~2000 | Line Chart |
| cache_read_tokens | `turns[].cache_read_tokens` | int | 缓存读取 token | 0~400K | Stacked Area |
| context_size | `turns[].context_size` | int | 当前上下文大小 | 10K~400K | Line Chart (关键) |
| cache_hit_rate | `turns[].cache_hit_rate` | float | 本回合缓存命中率(%) | 0%~99.9% | Line Chart |
| cost | `turns[].cost` | float | 本回合费用 | $0.01~$0.50 | Bar Chart |
| stop_reason | `turns[].stop_reason` | string\|null | 停止原因 | "end_turn"/"tool_use"/"max_tokens" | 颜色编码 |
| is_agent | `turns[].is_agent` | bool | 是否为 subagent 回合 | - | 颜色编码 |
| is_compaction | `turns[].is_compaction` | bool | 是否为上下文压缩事件 | - | 标记点 |
| tool_names[] | `turns[].tool_names` | string[] | 本回合使用的工具 | 0~5 个工具 | Tooltip |

### A4. Trend (趋势)

| 指标名 | JSON 路径 | 类型 | 含义 | 典型范围 | 推荐图表 |
|--------|-----------|------|------|----------|----------|
| group_label | `trend.group_label` | string | "Day" 或 "Month" | - | 副标题 |
| entries[].label | `trend.entries[].label` | string | 日期标签 "2026-03-15" 或 "2026-03" | - | X 轴 |
| entries[].session_count | `trend.entries[].session_count` | int | 当日/月 session 数 | 1~60 | Bar Chart |
| entries[].turn_count | `trend.entries[].turn_count` | int | 当日/月 turn 数 | 1~3000 | Line Chart |
| entries[].output_tokens | `trend.entries[].output_tokens` | int | 当日/月输出 token | 500~1.3M | Stacked Bar |
| entries[].context_tokens | `trend.entries[].context_tokens` | int | 当日/月上下文 token | 10K~470M | Line Chart |
| entries[].cost | `trend.entries[].cost` | float | 当日/月费用 | $0.10~$330 | Bar Chart (关键) |
| entries[].cost_per_turn | `trend.entries[].cost_per_turn` | float | 当日/月每 turn 成本 | $0.02~$0.18 | Line Chart |

### A5. Project (项目)

| 指标名 | JSON 路径 | 类型 | 含义 | 典型范围 | 推荐图表 |
|--------|-----------|------|------|----------|----------|
| projects[].name | `projects[].name` | string | 项目内部名 | - | 表格列 |
| projects[].display_name | `projects[].display_name` | string | 项目显示名 | "~/cc" 等 | 标签 |
| projects[].session_count | `projects[].session_count` | int | 项目 session 数 | 1~154 | Bar |
| projects[].total_turns | `projects[].total_turns` | int | 项目 turn 数 | 1~8800 | Bar |
| projects[].agent_turns | `projects[].agent_turns` | int | 项目 agent turn 数 | 0~4200 | 堆叠 Bar |
| projects[].output_tokens | `projects[].output_tokens` | int | 项目输出 token | 500~3.8M | Bar |
| projects[].context_tokens | `projects[].context_tokens` | int | 项目上下文 token | 10K~1.1B | Bar |
| projects[].cost | `projects[].cost` | float | 项目总费用 | $0.10~$800 | Horizontal Bar (关键) |
| projects[].primary_model | `projects[].primary_model` | string | 项目主力模型 | - | 标签 |

### A6. Wrapped (年度总结)

| 指标名 | JSON 路径 | 类型 | 含义 | 典型范围 | 推荐图表 |
|--------|-----------|------|------|----------|----------|
| year | `wrapped.year` | int | 统计年份 | 2025/2026 | 标题 |
| active_days | `wrapped.active_days` | int | 活跃天数 | 10~200 | KPI Card |
| total_days | `wrapped.total_days` | int | 年度总天数 | 1~366 | KPI Card (分母) |
| longest_streak | `wrapped.longest_streak` | int | 最长连续活跃天数 | 1~60 | KPI Card |
| ghost_days | `wrapped.ghost_days` | int | 不活跃天数 | 0~300 | KPI Card |
| autonomy_ratio | `wrapped.autonomy_ratio` | float | 自主比 (total_turns / user_prompts) | 1.0~3.0 | KPI Card |
| avg_session_duration_min | `wrapped.avg_session_duration_min` | float | 平均 session 时长（分钟） | 20~120 | KPI Card |
| avg_cost_per_session | `wrapped.avg_cost_per_session` | float | 平均 session 费用 | $1~$10 | KPI Card |
| output_ratio | `wrapped.output_ratio` | float | 输出占输入百分比 | 0.3%~3% | KPI Card |
| peak_hour | `wrapped.peak_hour` | int | 最活跃的小时（0~23） | 0~23 | 标注 |
| peak_weekday | `wrapped.peak_weekday` | string | 最活跃的星期几 | "Saturday" 等 | 标注 |
| hourly_distribution | `wrapped.hourly_distribution` | int[24] | 每小时 turn 分布 | 0~3500 | Bar Chart |
| weekday_distribution | `wrapped.weekday_distribution` | int[7] | 每周日 turn 分布 (Mon=0) | 300~5400 | Bar Chart |
| top_projects | `wrapped.top_projects` | [string, float][] | 项目费用 Top5 | - | Horizontal Bar |
| top_tools | `wrapped.top_tools` | [string, int][] | 工具使用 Top5 | - | Horizontal Bar |
| most_expensive_session | `wrapped.most_expensive_session` | [id, cost, project]\|null | 最贵 session | $50~$200 | Highlight Card |
| longest_session | `wrapped.longest_session` | [id, duration_min, project]\|null | 最长 session | 500~2400 min | Highlight Card |
| model_distribution | `wrapped.model_distribution` | [string, int][] | 模型使用分布 | 2~5 个 | Donut/Pie |
| archetype | `wrapped.archetype` | enum | 开发者原型 | 6 种之一 | Hero Card |
| total_pr_count | `wrapped.total_pr_count` | int | PR 数量 | 0~50 | KPI Card |
| total_speculation_time_saved_ms | `wrapped.total_speculation_time_saved_ms` | float | 推测执行节省时间（毫秒） | 0~100000 | KPI Card |
| total_collapse_count | `wrapped.total_collapse_count` | int | context collapse 次数 | 0~20 | KPI Card |

### A7. Heatmap 矩阵 (仅 Overview 内部/HTML)

| 指标名 | 类型 | 含义 | 典型范围 | 推荐图表 |
|--------|------|------|----------|----------|
| weekday_hour_matrix | int[7][24] | 星期 x 小时 → turn 计数 | 0~500 per cell | Calendar Heatmap |

> 注意：此数据目前仅在 HTML 输出中使用，JSON API 未暴露。设计师如需在新仪表盘中使用，需要新增 JSON 字段。

---

## B. 数据关联图

### B1. 因果关系（强关联）

```
turns ──→ cost          越多回合 → 费用越高（正比）
turns ──→ output_tokens 越多回合 → 输出越多（正比）
context_size ──→ cost   上下文越大 → 单 turn 成本越高（因为 cache_write/read 更多）
compaction_count ←── context_size  上下文达到上限触发压缩（因果反向）
cache_hit_rate ──→ cost_savings    缓存命中率越高 → 节省越多
duration_minutes ──→ turns         时长越长 → 回合越多（但非线性，有 idle 时间）
agent_turns ──→ agent_cost         agent 回合越多 → agent 费用越高
```

### B2. 相关关系（弱到中等）

```
session_count ←→ total_cost      更多 session 通常意味着更高总费用
agent_turns / total_turns ←→ autonomy_ratio    同一概念的不同表达
output_ratio ←→ cost_per_turn    输出比越高 → 每 turn 产出效率越高
cache_write_cost ←→ cache_read_cost    写入后才能读取（时序因果）
peak_hour ←→ weekday_distribution      工作习惯的不同维度
cost_per_turn ←→ model           不同模型价格差异显著（Opus >> Sonnet >> Haiku）
```

### B3. 同一概念的多维度表达

```
┌─ agent_turns            ─┐
│  agent_output_tokens      │ → "Agent 活跃度" 的不同切面
│  agent_cost               │
│  autonomy_ratio           │
└───────────────────────────┘

┌─ cache_hit_rate           ─┐
│  cache_savings.total_saved  │ → "缓存效率" 的不同切面
│  cache_savings.savings_pct  │
│  cache_read_cost            │
└─────────────────────────────┘

┌─ context_size             ─┐
│  max_context               │ → "上下文使用" 的不同切面
│  compaction_count          │
│  context_tokens            │
└────────────────────────────┘
```

### B4. 层级关系

```
Overview
  ├── sessions[]  →  Session Detail
  │     └── turns[]  →  Turn Detail
  ├── models[]
  ├── top_tools[]
  └── cost_by_category{}

Project
  └── projects[]  →  可联查 sessions[]

Trend
  └── entries[]  →  时间切片，每项与 Overview 子集同构

Wrapped
  └── 整年聚合，与 Overview 同构但增加了 archetype/streak/ghost_days
```

---

## C. 真实数据样本

> 以下数据来自真实用户的一个月使用，已脱敏（session_id 截断，项目路径泛化）。设计师可直接用作 mock data。

### C1. Overview (全局)

```json
{
  "total_sessions": 345,
  "total_turns": 19531,
  "total_agent_turns": 9982,
  "total_output_tokens": 8865099,
  "total_context_tokens": 2067647548,
  "total_cost": 1595.06,
  "avg_cache_hit_rate": 96.85,
  "output_ratio": 0.43,
  "cost_per_turn": 0.082,
  "tokens_per_output_turn": 453,
  "cache_savings": {
    "total_saved": 8503.10,
    "savings_pct": 90.0
  },
  "subscription_value": null,
  "cost_by_category": {
    "input_cost": 6.06,
    "output_cost": 199.96,
    "cache_write_cost": 444.26,
    "cache_read_cost": 944.79
  },
  "models": [
    { "name": "claude-opus-4-6",         "output_tokens": 7226374, "turns": 14263, "cost": 1459.30 },
    { "name": "claude-sonnet-4-6",       "output_tokens": 1062521, "turns": 2796,  "cost": 101.53 },
    { "name": "claude-haiku-4-5",        "output_tokens": 537839,  "turns": 2059,  "cost": 18.08 },
    { "name": "claude-opus-4-5",         "output_tokens": 9345,    "turns": 320,   "cost": 13.82 },
    { "name": "claude-sonnet-4-5",       "output_tokens": 29020,   "turns": 93,    "cost": 2.34 }
  ],
  "top_tools": [
    { "name": "Bash",       "count": 6274 },
    { "name": "Read",       "count": 4851 },
    { "name": "Edit",       "count": 1882 },
    { "name": "Grep",       "count": 893 },
    { "name": "Write",      "count": 555 },
    { "name": "WebSearch",  "count": 476 },
    { "name": "WebFetch",   "count": 454 },
    { "name": "Agent",      "count": 311 },
    { "name": "Glob",       "count": 238 },
    { "name": "ToolSearch", "count": 238 }
  ]
}
```

**设计要点：**
- `total_cost` 是最醒目的 KPI，$1595 这个量级需要大字号显示
- `cache_savings.total_saved` = $8503 远大于实际支出，是 "省了多少" 的核心卖点
- `cost_by_category` 中 cache_read_cost 占 59%、cache_write_cost 占 28%、output_cost 占 13%、input_cost < 1% —— 饼图会非常不均匀，考虑合并 input_cost 到 "其他"
- 模型分布极度不均：Opus-4-6 贡献 91.5% 的费用

### C2. Session Summary (会话列表中的一项)

```json
{
  "session_id": "3772bfb6",
  "project": "~/my-project",
  "first_timestamp": "2026-03-16T14:12:31.687+00:00",
  "duration_minutes": 22.28,
  "model": "claude-opus-4-6",
  "turn_count": 100,
  "agent_turn_count": 78,
  "output_tokens": 30825,
  "context_tokens": 4570926,
  "max_context": 90593,
  "cache_hit_rate": 93.38,
  "cost": 5.09,
  "output_ratio": 0.67,
  "cost_per_turn": 0.051
}
```

**值域参考（来自 345 个 session 的实际分布）：**

| 字段 | 最小值 | P50（中位数估计） | P90 | 最大值 |
|------|--------|-------------------|-----|--------|
| turn_count | 0 | 15~30 | 400 | 2700+ |
| duration_minutes | 0 | 5~20 | 200 | 2353 |
| cost | $0.00 | $0.50~$2.00 | $30 | $180 |
| cache_hit_rate | 0% | 85%~90% | 97% | 99.9% |
| max_context | 0 | 30K~60K | 150K | 380K |
| output_ratio | 0% | 0.3%~1.0% | 3% | 6.7% |

### C3. Session Detail (单 session 详情)

```json
{
  "session_id": "69bb9f9f-...",
  "project": "~/my-project",
  "model": "claude-opus-4-6",
  "duration_minutes": 223.88,
  "total_cost": 62.39,
  "max_context": 380809,
  "compaction_count": 132,
  "output_tokens": 388258,
  "context_tokens": 90339486,
  "cache_hit_rate": 97.86,
  "agent_turns": 599,
  "agent_output_tokens": 237739,
  "agent_cost": 24.88,
  "title": "Dashboard data guide for UI designers",
  "tags": [],
  "turns": [
    {
      "turn_number": 1,
      "timestamp": "2026-04-01T14:19:27.349+00:00",
      "model": "claude-opus-4-6",
      "input_tokens": 3,
      "output_tokens": 316,
      "cache_read_tokens": 0,
      "context_size": 31625,
      "cache_hit_rate": 0.0,
      "cost": 0.324,
      "stop_reason": "tool_use",
      "is_agent": false,
      "is_compaction": false,
      "tool_names": ["Read"]
    },
    {
      "turn_number": 2,
      "timestamp": "2026-04-01T14:19:34.688+00:00",
      "model": "claude-opus-4-6",
      "input_tokens": 74,
      "output_tokens": 295,
      "cache_read_tokens": 31622,
      "context_size": 36150,
      "cache_hit_rate": 87.47,
      "cost": 0.068,
      "stop_reason": "tool_use",
      "is_agent": false,
      "is_compaction": false,
      "tool_names": ["Read"]
    }
  ]
}
```

**关键观察：**
- 第一个 turn 的 `cache_hit_rate` 始终为 0%（冷启动），后续迅速攀升到 85%+
- `context_size` 呈锯齿形增长：正常 turn 增长 → compaction 事件大幅下降
- `is_compaction = true` 的 turn 标记上下文压缩事件，设计师应在时间线上用明显标记
- agent turn 和 main turn 交替出现，建议用颜色区分

### C4. Trend (30 天趋势)

```json
{
  "group_label": "Day",
  "entries": [
    { "label": "2026-03-08", "session_count": 22, "turn_count": 799, "output_tokens": 341774, "context_tokens": 55037543, "cost": 58.08, "cost_per_turn": 0.073 },
    { "label": "2026-03-15", "session_count": 15, "turn_count": 813, "output_tokens": 530675, "context_tokens": 73438539, "cost": 72.66, "cost_per_turn": 0.089 },
    { "label": "2026-03-21", "session_count": 24, "turn_count": 3019, "output_tokens": 1306004, "context_tokens": 472548611, "cost": 328.85, "cost_per_turn": 0.109 },
    { "label": "2026-03-22", "session_count": 16, "turn_count": 2319, "output_tokens": 798898, "context_tokens": 163612093, "cost": 109.33, "cost_per_turn": 0.047 },
    { "label": "2026-04-01", "session_count": 14, "turn_count": 2704, "output_tokens": 1157526, "context_tokens": 244162190, "cost": 189.95, "cost_per_turn": 0.070 }
  ]
}
```

**分布特征：**
- 日费用波动极大：$0.13（低谷） ~ $328.85（高峰），差距 2500x
- 有些天完全没有数据（gap），图表需要处理缺失日期
- session_count 和 cost 不总是正比（1 个长 session 可能比 20 个短 session 更贵）
- cost_per_turn 在 $0.03~$0.18 范围波动，反映模型混合使用情况

### C5. Project (项目)

```json
{
  "projects": [
    { "display_name": "~/project-a",     "session_count": 154, "total_turns": 8780, "agent_turns": 4212, "output_tokens": 3783885, "context_tokens": 1090665903, "cost": 791.05, "primary_model": "claude-opus-4-6" },
    { "display_name": "~/project-b",     "session_count": 12,  "total_turns": 2817, "agent_turns": 1520, "output_tokens": 1024109, "context_tokens": 336938149,  "cost": 222.61, "primary_model": "claude-opus-4-6" },
    { "display_name": "~/project-c",     "session_count": 4,   "total_turns": 1957, "agent_turns": 879,  "output_tokens": 811168,  "context_tokens": 213345565,  "cost": 160.77, "primary_model": "claude-opus-4-6" },
    { "display_name": "~/project-d",     "session_count": 9,   "total_turns": 1398, "agent_turns": 983,  "output_tokens": 870467,  "context_tokens": 126452270,  "cost": 120.47, "primary_model": "claude-opus-4-6" },
    { "display_name": "~/home",          "session_count": 57,  "total_turns": 1177, "agent_turns": 297,  "output_tokens": 568210,  "context_tokens": 73731192,   "cost": 84.77,  "primary_model": "claude-opus-4-6" }
  ]
}
```

**分布特征：**
- 头部项目集中度极高：Top 1 项目占总费用的 ~50%
- session_count 与 cost 不成比例（project-c 只有 4 个 session 但费用 $160）
- 建议使用 Treemap 或水平条形图展示项目费用分布

### C6. Wrapped (年度总结)

```json
{
  "year": 2026,
  "active_days": 36,
  "total_days": 92,
  "longest_streak": 20,
  "ghost_days": 56,
  "total_sessions": 322,
  "total_turns": 19531,
  "total_agent_turns": 9982,
  "total_output_tokens": 8865099,
  "total_input_tokens": 2067647548,
  "total_cost": 1595.06,
  "autonomy_ratio": 1.54,
  "avg_session_duration_min": 88.19,
  "avg_cost_per_session": 4.95,
  "output_ratio": 0.43,
  "peak_hour": 23,
  "peak_weekday": "Saturday",
  "hourly_distribution": [2305,1085,764,300,5,0,0,0,0,95,391,50,185,299,335,520,348,109,291,973,1816,3030,3141,3489],
  "weekday_distribution": [1715,2036,3808,1164,388,5383,5037],
  "top_projects": [
    ["~/project-a", 791.05],
    ["~/project-b", 222.61],
    ["~/project-c", 160.77],
    ["~/project-d", 120.47],
    ["~/home",       84.77]
  ],
  "top_tools": [
    ["Bash", 6274], ["Read", 4851], ["Edit", 1882], ["Grep", 893], ["Write", 555]
  ],
  "most_expensive_session": ["ec6c7fe1", 180.29, "~/project-a"],
  "longest_session": ["9bec94b1", 2353.30, "~/project-a"],
  "model_distribution": [
    ["claude-opus-4-6", 14263],
    ["claude-sonnet-4-6", 2796],
    ["claude-haiku-4-5", 2059],
    ["claude-opus-4-5", 320],
    ["claude-sonnet-4-5", 93]
  ],
  "archetype": "Delegator",
  "total_pr_count": 0,
  "total_speculation_time_saved_ms": 0.0,
  "total_collapse_count": 0
}
```

**关键洞察（帮助设计）：**
- `archetype` 有 6 种：Architect, Sprinter, NightOwl, Delegator, Explorer, Marathoner —— 需要 6 套视觉风格
- `hourly_distribution` 呈现明显的夜猫子模式：峰值在 23:00，05:00-09:00 几乎为零
- `weekday_distribution` 周末(Sat=5383, Sun=5037) > 工作日，非典型模式
- `autonomy_ratio` = 1.54 意味着平均每个用户 prompt 触发 1.54 个 API turn
- `longest_session` = 2353 min ≈ 39 小时，是极端值

---

## D. 可视化建议

### D1. KPI Card（单值指标）

适用指标：total_cost, total_sessions, total_turns, cache_savings, value_multiplier, active_days, longest_streak

**设计建议：**
- 主数值大字号 + 副标签小字号
- `total_cost` 用美元符号格式化，保留 2 位小数
- `cache_savings` 建议用绿色 "You saved $8,503" 样式
- `value_multiplier` 如有值，用 "7.8x" 格式突出显示
- 对比值：可以显示 "vs last 30 days" 的变化百分比（需 trend 数据计算）

### D2. Donut/Pie Chart（占比）

适用指标：cost_by_category, model_distribution, agent vs non-agent turns

**设计建议：**
- `cost_by_category`：4 段 donut —— cache_read(绿) / cache_write(蓝) / output(橙) / input(灰)
- input_cost 通常 < 1%，在饼图上几乎不可见，可合并为 "Other" 或用标注线指出
- `model_distribution`：按模型着色，Opus 深色，Sonnet 中色，Haiku 浅色
- 中心可放置总值 (如 "$1,595.06")

### D3. Line/Area Chart（时间序列）

适用指标：trend.cost, trend.turn_count, session.turns[].context_size, session.turns[].cache_hit_rate

**设计建议：**
- **Trend 每日费用**：bar chart 更佳（离散天数），双轴可叠加 cost_per_turn 的 line
- **Session context_size 时间线**：这是最有信息量的 session 图表
  - 锯齿形增长模式：每次 compaction 导致断崖式下降
  - 用垂直虚线标记 `is_compaction = true` 的 turn
  - 用颜色区分 `is_agent` turn
- **cache_hit_rate 时间线**：第一个 turn 始终为 0%，然后快速上升，适合 area chart
- 缺失日期需要显示为空白（不要连线跨越缺失日期）

### D4. Horizontal Bar Chart（排行）

适用指标：top_tools, top_projects, models by cost

**设计建议：**
- 工具排行：Bash(6274) 远超其他，考虑用 log scale 或截断显示
- 项目排行：费用分布呈长尾（头部 50% 集中在 1 个项目），适合 Pareto 图
- 模型排行：费用差距极大（Opus $1459 vs Haiku $18），建议同时显示 turn 数

### D5. Calendar Heatmap（热力图）

适用指标：weekday_hour_matrix (7x24), trend entries

**设计建议：**
- **星期 x 小时热力图**：7 行 24 列，颜色深度表示 turn 密度
  - 典型模式：特定时段（如 21:00-02:00）有明显热区
  - 色阶建议：白/浅蓝 → 深蓝/紫色，0 值用灰色
- **日历视图**：类 GitHub contribution graph，每天一个方格
  - 颜色表示当日费用或 turn 数
  - 空白日期（ghost_days）用浅灰标记

### D6. Stacked Area/Bar（组成分析）

适用指标：turn-level token 组成（input + cache_write + cache_read + output）

**设计建议：**
- 每个 turn 的 token 组成：cache_read 通常占 90%+，input 极小
- 建议用 stacked area chart 展示 session 内 token 演变
- 色彩：input(红) / cache_write(蓝) / cache_read(绿) / output(橙)

### D7. Timeline/Waterfall（会话时间线）

适用指标：session.turns[]

**设计建议：**
- 垂直时间线，每个 turn 一个节点
- 节点大小 → 费用或 output_tokens
- 节点颜色 → is_agent(蓝) / main(绿) / compaction(红)
- Hover 显示完整 turn 信息
- 压缩事件用特殊图标标记（如折叠图标）

### D8. Wrapped 专属布局

**设计建议：**
- 采用类 Spotify Wrapped 的全屏卡片滑动模式
- 第 1 屏：archetype hero card（大字 "The Delegator" + 描述 + 代表性动画）
- 第 2 屏：活跃天数日历（active_days / total_days + longest_streak 高亮）
- 第 3 屏：费用与 token 大数字（total_cost + cache_savings 对比）
- 第 4 屏：时间模式（hourly + weekday 分布，peak_hour 突出）
- 第 5 屏：Top 排行（项目/工具/模型）
- 第 6 屏：极端值（最贵 session + 最长 session）

---

## E. 交互建议

### E1. Drill-down（点击展开详情）

| 入口 | 展开目标 | 说明 |
|------|----------|------|
| Session 列表中的一行 | Session Detail 页面 | 查看逐 turn 时间线、context 曲线、费用分布 |
| Model 列表中的一行 | 该模型的 session 列表筛选 | 过滤出使用该模型的 sessions |
| Project 列表中的一行 | 该项目的 session 列表 | 按项目过滤 sessions |
| Trend 图表中的一天/月 | 当日/月的 session 列表 | 时间切片 drill-down |
| Tool 排行中的一项 | 使用该工具的 turn 列表 | 需要从 turn_details 反查（高级） |
| cost_by_category 饼图的一段 | 高亮该类别费用在 trend 中的占比 | 联动高亮 |

### E2. Tooltip（Hover 查看详情）

| 悬停目标 | Tooltip 内容 | 说明 |
|----------|-------------|------|
| KPI Card (total_cost) | 费用分拆：input / output / cache_write / cache_read | 快速了解组成 |
| Trend 图表的某一天 | session_count, turn_count, cost, cost_per_turn | 当日完整数据 |
| Session 列表某行 | title, model, duration, agent 占比 | 快速预览 |
| Turn 时间线的节点 | model, tokens, cost, tools, stop_reason | 完整 turn 信息 |
| Heatmap 单元格 | "Wednesday 23:00 — 245 turns" | 精确值 |
| Model 饼图的一段 | turns, output_tokens, cost, 占比百分比 | 模型详情 |
| Tool 条形图的一项 | 使用次数, 占比百分比 | 工具详情 |

### E3. 筛选器（Filter）

| 筛选维度 | 适用页面 | 实现建议 |
|----------|----------|----------|
| 时间范围 | 全部页面 | Date range picker，预设：7d / 30d / 90d / All |
| 项目 | Overview, Trend, Session 列表 | Multi-select dropdown，从 projects[] 取值 |
| 模型 | Overview, Trend, Session 列表 | Checkbox 组，从 models[] 取值 |
| 费用阈值 | Session 列表 | Slider：$0 ~ max_cost |
| Agent 模式 | Session 列表 | Toggle：All / Agent-heavy / Manual-heavy |
| 排序 | Session 列表, Project 列表 | Sort by: cost / turns / duration / date |

### E4. 联动（Cross-highlight）

建议在同一页面内的多个图表之间建立联动：

- **Overview 页面**：点击 model 饼图中的 "Opus" → session 列表自动过滤为 Opus sessions → trend 图表高亮 Opus 部分
- **Session Detail 页面**：选中 context_size 图表的某段区间 → turn 列表自动滚动到对应区域 → cost 图表同步高亮
- **Trend 页面**：brush 选中一段时间 → 下方 session 列表自动过滤 → KPI cards 更新为选中区间的统计

### E5. 响应式优先级

在移动端或窄屏下，建议按以下优先级裁剪内容：

1. **必须保留**：total_cost, total_sessions, cache_savings, model distribution
2. **优先保留**：trend chart (简化为 sparkline), session list (精简列)
3. **可折叠**：cost_by_category, tool ranking, heatmap
4. **可隐藏**：turn-level 时间线, 详细表格

---

## F. 数值格式化参考

| 类型 | 格式 | 示例 |
|------|------|------|
| 费用 | `$X,XXX.XX` | $1,595.06 |
| 小额费用 | `$X.XXXX` | $0.0824 |
| Token 数 | 带 K/M 后缀 | 8.87M, 2.07B |
| 百分比 | `XX.X%` | 96.9% |
| 时长 | 智能单位 | 3.7 hr, 22 min, 39 hr |
| 日期 | ISO 短格式 | Mar 15, 2026 |
| 时间戳 | 本地时间 | 14:19 |
| Session ID | 前 8 字符 | 69bb9f9f |
| 倍率 | `X.Xx` | 7.8x |

---

## G. 附录：Developer Archetype 一览

| Archetype | 触发条件 | 视觉风格建议 |
|-----------|----------|-------------|
| **The Architect** | agent_ratio > 0.4 && avg_session > 60min | 蓝图/建筑线条感 |
| **The Sprinter** | avg_session < 30min && turns_per_session > 10 | 闪电/速度感 |
| **The Night Owl** | night_ratio > 0.5 (22:00-06:00) | 深色/月亮/星空 |
| **The Delegator** | agent_ratio > 0.5 | 指挥棒/团队协作 |
| **The Explorer** | unique_projects > 10 | 指南针/地图 |
| **The Marathoner** | avg_session > 120min | 跑道/耐力感 |

> 判定顺序：Delegator → NightOwl → Marathoner → Architect → Sprinter → Explorer → 默认 Architect
