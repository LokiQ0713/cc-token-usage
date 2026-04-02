# HTML Report Redesign — Final Integrated Specification

> 本文档在 `html-redesign-proposal.md` (设计方案) 基础上，用 `dashboard-data-guide.md` (数据指南) 的真实数据特征进行校正。所有修改以 `[REVISED]` 标记，原文未标记部分保持不变。

---

## 1. Information Architecture

### 1.1 Navigation Structure

Top-level: **fixed sidebar** (desktop) / **bottom tab bar** (mobile), 6 pages:

```
[Logo/Title]
 |-- Overview      (default landing page)
 |-- Trends
 |-- Projects
 |-- Sessions      (full session list with search/filter)
 |-- Wrapped       (annual summary cards)
 |-- Heatmap       (contribution-style activity map)

[Footer area]
 |-- Theme toggle (sun/moon)
 |-- Language toggle (EN/ZH)
 |-- Data source switcher (when dual-source mode)
```

### 1.2 Page Hierarchy

```
Overview ........... High-level KPIs + at-a-glance charts
Trends ............. Time-series analysis (daily/monthly)
Projects ........... Per-project drill-down
Sessions ........... Sortable/filterable session table -> click for detail
Wrapped ............ Annual highlight cards (Spotify Wrapped style)
Heatmap ............ GitHub-style contribution calendar
```

### 1.3 Dual-Source Support

When `render_dual_report_html` is called, a **source toggle pill** appears at the top of the sidebar (below the title). Each source maintains independent state.

---

## 2. Page-by-Page Component Design

### 2.1 Overview Page

**Layout**: Three rows of KPI cards, followed by chart panels in a 2-column grid, followed by full-width panels.

#### Row 1 -- Primary KPIs (6 cards, auto-fit grid)

| Card | Value | Source Field |
|------|-------|-------------|
| Sessions | `total_sessions` | `OverviewResult.total_sessions` |
| Turns | `total_turns` (with agent sub-label) | `OverviewResult.total_turns`, `total_agent_turns` |
| Claude Wrote | `total_output_tokens` (compact: "2.1M") | `OverviewResult.total_output_tokens` |
| Claude Read | `total_context_tokens` (compact) | `OverviewResult.total_context_tokens` |
| Cache Hit Rate | `avg_cache_hit_rate` (with progress ring) | `OverviewResult.avg_cache_hit_rate` |
| API Cost | `total_cost` | `OverviewResult.total_cost` |

Each card: centered value in large font, label below in small caps. On hover, show tooltip with exact number (no rounding).

**[REVISED] Turns 卡片增加 Agent 占比信息**

> 依据：数据指南 A1 显示 `total_agent_turns` 占 `total_turns` 的 30%~60%（样本数据中为 51%）。这是一个核心效率指标，不应仅作为 sub-label。
>
> 修改：Turns 卡片在主数值下方增加一个**迷你环形图**（类似 Apple Watch 活动圆环），内环为 agent turns，外环为 total turns。副标签改为 `"51% agent-driven"`。这样无需点击即可感知 agent 活跃度。

#### Row 2 -- Secondary KPIs (4 cards)

| Card | Value | Source |
|------|-------|--------|
| Daily Avg | `total_cost / days` | Computed from `quality.time_range` |
| Peak Context | max of `session_summaries[].max_context` | Computed |
| Compactions | sum of `session_summaries[].compaction_count` | Computed |
| Avg Session Duration | mean of `session_summaries[].duration_minutes` | Computed |

**[REVISED] 增加 "Output Ratio" 卡片至 Row 2，调整为 5 卡**

> 依据：数据指南 A1 列出 `output_ratio` 为独立指标（典型范围 0.3%~3%），设计方案将其放在了下方 Efficiency Metrics 面板中。但 output_ratio 是衡量"Claude 写 vs 读"效率的关键数据，提升到 KPI Row 2 可提高可见性。
>
> 修改：Row 2 变为 5 卡：Daily Avg | Peak Context | Compactions | Avg Duration | Output Ratio。在移动端 5 卡自动折行为 3+2。

#### Panel: Cache Savings Badge

A standalone accent-colored banner:
```
Cache saved you $8,503 (90% of reads were free)
Subscription: $200/mo -> 7.8x value multiplier
```

Source: `OverviewResult.cache_savings`, `subscription_value`.

**[REVISED] Cache Savings 金额格式化与条件渲染**

> 依据：数据指南 C1 真实数据 `cache_savings.total_saved = $8503`，远大于实际支出 $1595。数据指南 A1 指出 `subscription_value` 可能为 null。
>
> 修改：
> 1. `total_saved` 使用千分位格式 `$8,503.10`
> 2. 增加对比文案：`"Saved $8,503 — that's 5.3x your actual spend of $1,595"`
> 3. 当 `subscription_value` 为 null 时，隐藏整个 Subscription 行，不留空白

#### Panel: Model Distribution (2-column, left)

**Chart type**: Horizontal bar chart (Chart.js `bar`, `indexAxis: 'y'`)

- Y-axis: model short name (e.g., "opus-4", "sonnet-4")
- X-axis: cost ($)
- Bar color: palette by index
- Tooltip: turns, output tokens, cost

**[REVISED] Model Distribution 必须使用对数刻度或截断设计**

> 依据：数据指南 C1 揭示模型费用分布极度不均——Opus-4-6 占 91.5%（$1459），而 Haiku 仅 $18，Sonnet-4-5 仅 $2.34。线性刻度下 Haiku/Sonnet-4-5 的条形几乎不可见。
>
> 修改：
> 1. **默认对数刻度**（Chart.js `type: 'logarithmic'`），让所有模型可见
> 2. 提供一个 **"Linear / Log" 切换按钮**在面板右上角
> 3. 每个条形右侧附加**百分比标签**（如 `91.5%`），让用户即使在 log 模式下也能感知绝对占比
> 4. Tooltip 增加 `turns` 和 `output_tokens` 字段（已有），再加 **cost 占比百分比**

#### Panel: Cost Composition (2-column, right)

**Chart type**: Doughnut chart

- 4 segments: Output / Cache Write / Input / Cache Read
- Center label: total cost
- Hover: exact dollar + percentage

**[REVISED] Cost Composition Donut 合并 input_cost 为 "Other"，突出 cache_read**

> 依据：数据指南 C1 中 `input_cost = $6.06`（占 0.38%），在 donut 上几乎不可见。数据指南 D2 明确建议"合并 input_cost 到 Other 或用标注线指出"。同时 `cache_read_cost` 占 59% 是最大块。
>
> 修改：
> 1. 将 4 段改为 **3 段 + 标注**：Cache Read (59%) / Cache Write (28%) / Output (13%)。`input_cost` 合并入 Output 段，或作为单独的极细段并用**标注线**（leader line）指出
> 2. **排序**：donut 从大到小排列（cache_read → cache_write → output → input），避免小段被大段夹住
> 3. Cache Read 段使用**高饱和绿色**，因为它代表"缓存红利"——节省的部分
> 4. Center label 除了 total cost，增加一行小字：`"59% from cache reads"`

#### Panel: Efficiency Metrics (full-width card, 3-column stat row)

Three inline stats with micro-sparklines:

| Metric | Value | Description |
|--------|-------|-------------|
| Output Ratio | `output_ratio` % | "How much Claude writes vs reads" |
| Cost per Turn | `cost_per_turn` | "Average cost of each response" |
| Tokens per Turn | `tokens_per_output_turn` | "Average output length" |

> 注：Output Ratio 已在 Row 2 KPI 中展示（见上方 REVISED），此处保留但可考虑替换为其他效率指标。

#### Panel: Top Tools (2-column, left)

**Chart type**: Horizontal bar chart (top 10)

**[REVISED] 工具排行需要处理头部极端值**

> 依据：数据指南 C1 中 Bash(6274) 远超 ToolSearch(238)，差距 26 倍。数据指南 D4 建议"考虑用 log scale 或截断显示"。
>
> 修改：
> 1. 使用**百分比条形**而非绝对值条形——每个工具的条形长度 = `count / max_count * 100%`，但在条形内/右侧显示绝对次数
> 2. 或使用 **log scale** 与 Model Distribution 面板保持一致
> 3. 每个工具名旁用**图标**（Bash=终端、Read=眼睛、Edit=铅笔）增强识别度

#### Panel: Top Projects (2-column, right)

**Chart type**: Treemap (via `chartjs-chart-treemap` plugin, inline)

**[REVISED] Top Projects 使用 Horizontal Bar + Pareto Line 替代 Treemap**

> 依据：数据指南 C5 中项目费用分布呈长尾——Top 1 占 50%，Top 5 占 86%。数据指南 D4 明确建议"适合 Pareto 图"。Treemap 在只有 5~15 个项目时视觉效果一般（矩形太少），且面积感知不如条形直观。
>
> 修改：
> 1. **主图**：Horizontal Bar Chart（与 Model Distribution 风格统一）
> 2. **叠加**：一条 **Pareto 累积百分比线**（右 Y 轴 0~100%），在 Top 1 处标注 "50%"，Top 3 处标注 "74%"
> 3. 如果项目数 > 10，只展示 Top 10 + 一个 "Others" 汇总条
> 4. Treemap 降级为 "Nice to have"——如果 chartjs-chart-treemap 已引入（用于其他地方），可作为可切换的备选视图

#### Panel: Session Efficiency Bubble Chart (full-width)

**Chart type**: Bubble chart (existing, refined)

- X = turn count, Y = cost, Bubble size = output tokens
- Color: gradient by `cost_per_turn`
- Tooltip: session ID (short), project, turns, cost, cost/turn, output tokens

**[REVISED] Bubble Chart 必须处理极端值域跨度**

> 依据：数据指南 C2 显示 session 值域极大——turn_count: 0~2700, cost: $0~$180。大部分 session 集中在 turns < 100, cost < $5 区间，少数极端值会将主体数据挤压到左下角。
>
> 修改：
> 1. **双对数刻度**（X 轴 log, Y 轴 log），让密集区和离群值都可见
> 2. 提供 **"Linear / Log" 切换**
> 3. 添加**刷选缩放**（Chart.js zoom plugin 的 drag-to-zoom），允许用户框选密集区域放大查看
> 4. Tooltip 增加 **project name** 和 **duration**
> 5. 气泡颜色改为按 **project** 分色（使用 chart palette），而非 cost_per_turn 渐变——因为项目归属更有可操作性

#### Panel: Most Expensive Sessions Top 5 (full-width)

Compact table:

| Session | Project | Turns | Duration | Cost |

**[REVISED] 增加 Agent 列和 Model 列**

> 依据：数据指南 A2 中 session 包含 `agent_turns`、`model` 字段。最贵 session 通常是 agent-heavy + Opus 驱动的，这两个属性对理解"为什么贵"至关重要。
>
> 修改：表格增加 `Model` 和 `Agent%` 列：
>
> | Session | Project | Model | Turns | Agent% | Duration | Cost |

---

### 2.2 Trends Page

**Layout**: KPI row for current period, then chart panels, then data table.

#### Row 1 -- Current Period KPIs

Auto-detect latest month from `TrendResult.entries`. Display:

| Sessions | Turns | Input Tokens | Output Tokens | Cost |

#### Panel: Daily Cost + Cost/Turn Combo (full-width)

**Chart type**: Bar + Line combo (existing, refined)

- Bar: daily cost (left Y-axis)
- Line: cost/turn (right Y-axis, amber color)
- Tooltip: includes turn count

**[REVISED] Daily Cost Trend 图需要处理 2500x 波动和缺失日期**

> 依据：数据指南 C4 揭示日费用波动 $0.13 ~ $328.85（差距 2500x）。同时存在缺失日期（某些天没有数据）。数据指南 D3 建议"缺失日期需要显示为空白（不要连线跨越缺失日期）"。
>
> 修改：
> 1. **Y 轴**：默认使用**线性刻度**但增加一个 **"Log Scale" 切换按钮**，让高峰和低谷都能看清
> 2. **缺失日期**：X 轴使用 Chart.js `time` scale（`type: 'time'`），不传入缺失日期的数据点。Bar chart 会自然留空，Line chart 需设置 `spanGaps: false`
> 3. **极端值标注**：当某天费用超过 P90（约 $100）时，在对应 bar 上方显示**红色标注** `$329`
> 4. **区间统计**：在图表下方增加一行文字：`"Range: $0.13 – $328.85 | Median: $XX | Std Dev: $XX"`

#### Panel: Daily Turns + Cost Combo (full-width)

**Chart type**: Bar + Line combo

- Bar: daily turns (left Y-axis, blue)
- Line: daily cost (right Y-axis, green)

**[REVISED] 增加说明文案**

> 依据：数据指南 B1/C4 指出 session_count 和 cost 不总是正比（1 个长 session 可能比 20 个短 session 更贵）。用户可能疑惑为什么某天 turns 少但 cost 高。
>
> 修改：在图表标题下方增加 `subtitle`：`"Note: cost depends on model choice and context size, not just turn count"`

#### Panel: Daily Model Distribution (full-width, conditional)

**Chart type**: Stacked bar chart

Only shown when more than one model is used.

Source: `TrendEntry.models`.

#### Panel: Monthly Summary Table (conditional, shown when data spans 2+ months)

| Month | Sessions | Turns | Input Tokens | Output Tokens | Cost |

**[REVISED] Monthly Summary 增加 MoM 变化列**

> 依据：数据指南 D1 建议 KPI Card 可显示 "vs last 30 days" 变化百分比。月表是最适合展示环比变化的位置。
>
> 修改：增加 `Cost MoM` 列，显示相较前月的百分比变化，正值绿色/负值红色箭头。

#### Panel: Daily Breakdown Table (full-width)

| Date | Sessions | Turns | Input Tokens | Output Tokens | Cost | Models |

---

### 2.3 Projects Page

**Layout**: Bar chart at top, then drill-down table.

#### Panel: Project Cost Top 10

**Chart type**: Horizontal bar chart (existing, refined)

**[REVISED] 增加 Pareto 累积线和 session_count 二级信息**

> 依据：数据指南 C5 项目费用呈长尾——Top 1 = 50%, Top 3 = 74%。数据指南 D4 建议 Pareto 图。同时 session_count 与 cost 不成比例（project-c 只有 4 session 但 $160）。
>
> 修改：
> 1. 叠加 **Pareto 累积线**（右 Y 轴 0~100%）
> 2. 每个 bar 内部用**双色段**：左段 = agent cost，右段 = non-agent cost
> 3. Bar 右侧标注 `(N sessions)` 让用户直接看到 session 数

#### Panel: Project Drill-Down Table (3-level)

**Level 1 -- Project row** (click to expand):

| | Project | Sessions | Turns (Agent) | Output | CacheHit | Cost |

**[REVISED] 增加 Primary Model 列和 Agent Turns 列**

> 依据：数据指南 A5 包含 `primary_model` 和 `agent_turns` 字段。不同项目可能使用不同模型（影响费用），agent_turns 占比体现自动化程度。
>
> 修改：增加 `Model` 列（显示 primary_model 短名）和独立的 `Agent` 列（显示 agent_turns 数 + 占比百分比）。

**Level 2 -- Session row** (shown when project expanded, click to expand):

| | Session ID (date, duration) | | Turns (Agent) | Output | CacheHit | Tools | Cost |

**Level 3 -- Turn detail** (shown when session expanded): Full turn table.

---

### 2.4 Sessions Page (new dedicated page)

#### Filter Bar

- Text search (session ID, project name)
- Model filter (dropdown)
- Date range filter
- Cost range filter (slider)
- Sort by: Cost / Turns / Duration / Date

**[REVISED] 增加 Agent 模式筛选器**

> 依据：数据指南 E3 建议增加 "Agent 模式" 筛选：All / Agent-heavy / Manual-heavy。agent_turns/total_turns 的比例是区分使用模式的关键维度。
>
> 修改：Filter Bar 增加 **Agent Mode** toggle pill：`All | Agent-heavy (>50%) | Manual-heavy (<20%)`

#### Session Table

| Date | Session | Project | Model | Turns | Agent | Duration | Output | CacheHit | Cost |

**[REVISED] 增加 Title 列**

> 依据：数据指南 A2 中 `title` 字段提供会话标题（从元数据提取）。标题是用户识别 session 内容的最直接方式。
>
> 修改：在 Project 列和 Model 列之间增加 `Title` 列（截断显示，hover tooltip 完整文本）。

#### Session Detail Panel (expanded inline or as modal overlay)

**Section 1 -- KPI Row** (6 cards):

| Duration | Model | Max Context | Cache Hit % | Compactions | Total Cost |

**[REVISED] 增加 Agent Cost 和 Output Tokens 卡片至 KPI Row**

> 依据：数据指南 A2 中 session 详情包含 `agent_cost`（$0~$25）和 `output_tokens`（100~600K），是衡量 session 产出的核心指标。样本数据 C3 中 agent_cost = $24.88 占 total_cost $62.39 的 40%。
>
> 修改：KPI Row 扩展为 8 卡：Duration | Model | Max Context | Cache Hit % | Compactions | Output Tokens | Agent Cost | Total Cost

**Section 2 -- Metadata Card** (conditional, only shown if data exists):

- Title, Tags (as chips), Mode, Branch, PR Links (as clickable links)

**Section 3 -- Performance Card** (conditional):

| Metric | Value |
|--------|-------|
| Autonomy Ratio | `1:X.X (Y turns / Z user prompts)` |
| API Errors | count |
| Tool Errors | count |
| Truncated | count (stop_reason == "max_tokens") |
| Speculation | `saved X.Xs across Y accepts` |
| Service Tiers | tier breakdown with percentages |
| Speed | speed breakdown |
| Inference Geo | geo breakdown |

**Section 4 -- Charts (2-column grid)**:

- Left: **Context Growth** line chart (X=turn, Y=context_size)
- Right: **Cache Hit Rate** line chart (X=turn, Y=cache_hit_rate, 0-100%)

**[REVISED] Context Growth 图增加 Compaction 标记和 Agent 颜色编码**

> 依据：数据指南 D3 指出 context_size 呈"锯齿形增长"——正常 turn 增长后 compaction 导致断崖式下降。数据指南建议"用垂直虚线标记 is_compaction = true 的 turn"和"用颜色区分 is_agent turn"。样本数据 C3 中 compaction_count = 132。
>
> 修改：
> 1. `is_compaction = true` 的 turn 在 X 轴位置画**红色垂直虚线**
> 2. 数据点颜色：main turn = 蓝色，agent turn = 紫色
> 3. Compaction 节点使用**红色三角形标记**（pointStyle: 'triangle'）
> 4. Cache Hit Rate 图的第一个点（始终为 0%）用**灰色虚线**从 0% 连到第二个点，标注 "Cold start"

Point radius adapts: 3px for <50 turns, 0px for >=50 turns.

**[REVISED] 增加 Section 4b -- Token Composition Stacked Area**

> 依据：数据指南 D6 建议用 stacked area chart 展示 session 内 token 演变（input + cache_write + cache_read + output）。数据指南 A3 提供了逐 turn 粒度的 token 分拆数据。这是理解"钱花在哪"的关键图表，当前设计方案缺失。
>
> 修改：在 Section 4 charts 下方新增一个 full-width 图表：
> - **Token Composition Stacked Area**：X 轴 = turn number，Y 轴 = token count
> - 4 层：cache_read (绿) / cache_write (蓝) / input (灰) / output (橙)
> - 注：cache_read 通常占 90%+，因此可提供 "Exclude Cache Read" toggle 来放大查看其余三层

**Section 5 -- Agent Breakdown** (conditional, only if agents exist):

Table: | Type | Description | Turns | Output Tokens | Cost |

**Section 6 -- Stop Reason Distribution**:

**Chart type**: Doughnut chart (existing).

**Section 7 -- Context Collapse** (conditional):

Card showing collapse count, avg risk, max risk (with warning icon if max > 0.5), list of collapse summaries.

**Section 8 -- Code Attribution** (conditional):

Card showing files touched, Claude wrote (chars), prompts count, permission prompts count.

**Section 9 -- Turn Detail Table** (full-width):

| Turn | Time | Model | User | Assistant | Tools | Output | Context | Hit% | Cost | Stop | Flags |

**[REVISED] Turn Detail Table 增加 is_agent 和 is_compaction 可视化**

> 依据：数据指南 A3 中 `is_agent` 和 `is_compaction` 是布尔标识。数据指南 D7 建议"节点颜色 → is_agent(蓝) / main(绿) / compaction(红)"。
>
> 修改：
> 1. Agent 行左侧加 **蓝色竖条**（border-left: 3px solid var(--accent-blue)）
> 2. Compaction 行 **整行背景红色半透明**（已在原方案中提到，此处确认保留）
> 3. Flags 列使用图标：⚡ compaction，🤖 agent（原方案已有，确认保留）
> 4. 增加 **筛选按钮组**在表格上方：`All | Main only | Agent only | Compactions`

---

### 2.5 Wrapped Page

**Design**: Spotify Wrapped-style vertical scroll of full-width cards with bold typography, dark gradients, and animated counters.

#### Card 1 -- Hero Card

Large, centered. Archetype label in 2.5rem bold. Description in subtle italic.

**[REVISED] 增加 6 种 Archetype 视觉样式定义**

> 依据：数据指南 G 列出了 6 种 Archetype（Architect, Sprinter, NightOwl, Delegator, Explorer, Marathoner），各有不同触发条件和推荐视觉风格。当前设计方案只提到"archetype label"但未定义每种的视觉差异。
>
> 修改：为每种 Archetype 定义**渐变背景色 + 图标 + 关键词**：
>
> | Archetype | 背景渐变 | 图标 | 一句话描述 |
> |-----------|---------|------|-----------|
> | Architect | 深蓝→靛蓝 | 蓝图/网格 | "You design systems that think for themselves" |
> | Sprinter | 橙→红 | 闪电 | "Fast iterations, rapid results" |
> | Night Owl | 深紫→黑 | 月亮/星空 | "The city sleeps, your code doesn't" |
> | Delegator | 青→蓝 | 指挥棒 | "You orchestrate agents like a symphony" |
> | Explorer | 绿→青 | 指南针 | "Every project is a new frontier" |
> | Marathoner | 琥珀→深橙 | 跑道 | "Endurance is your superpower" |

#### Card 2 -- Activity Stats

2x2 grid: Active Days, Longest Streak, Ghost Days, Total Days.

#### Card 3 -- Volume Stats

Sessions, Turns, Agent Turns, Total Cost.

#### Card 4 -- Token Volume

Claude wrote (output tokens), Claude read (context tokens), Output ratio.

#### Card 5 -- Peak Patterns

**[REVISED] Peak Patterns 卡片的小时分布图需特殊设计**

> 依据：数据指南 C6 中 `hourly_distribution` 呈现明显夜猫子模式——峰值 23:00 (3489 turns)，凌晨 05:00-09:00 几乎为零。标准 0-23 排列会让峰值出现在图表最右侧，视觉上不够直观。
>
> 修改：
> 1. 小时分布 bar chart 的 X 轴从 **06:00 开始环绕到 05:00**（而非 00:00-23:00），让 "一天" 的视觉起点是清晨，夜间活跃的连续性不被截断
> 2. 夜间时段（22:00-05:00）bar 使用**深紫色**，日间使用**蓝色**，突出夜猫子模式
> 3. 在 `peak_hour` 对应的 bar 上方放置一个 **星标/箭头**标注
> 4. weekday 分布同样需要处理：数据显示 Sat=5383, Sun=5037 > 工作日，周末 bar 使用不同颜色

#### Card 6 -- Model Distribution

**Chart type**: Doughnut chart, compact.

**[REVISED] Model Distribution Doughnut 需要处理极度不均**

> 依据：数据指南 C6 中 model_distribution Opus-4-6 = 14263 turns (73%)，其余 4 个模型合计 27%。在 doughnut 上只会看到一大块+几小条。
>
> 修改：
> 1. **改为 Horizontal Bar Chart**（与 Overview Model Distribution 一致），保持图表类型统一
> 2. 或者如果坚持 doughnut，使用**拉出效果**（offset）将最小的 1-2 段拉出 5px，让小段更可见
> 3. 在图例中同时显示 `turns` 和 **cost**（因为 cost 的不均匀程度更高：Opus 91.5% cost vs 73% turns）

#### Card 7 -- Top Projects (top 5)

Ranked list with cost bars.

#### Card 8 -- Top Tools (top 5)

Same format as Top Projects but with count.

#### Card 9 -- Highlight Sessions

Most expensive session and longest session.

**[REVISED] Highlight Sessions 增加极端值上下文**

> 依据：数据指南 C6 中 `most_expensive_session` = $180.29, `longest_session` = 2353 min ≈ 39 小时。这些极端值需要上下文才有意义。
>
> 修改：
> 1. Most expensive 增加副文字：`"That's XX% of your total spend"`
> 2. Longest session 增加副文字：`"That's XX hours — longer than a full work day"`
> 3. 两者均增加项目名称（已有）和 **turn count** 信息

#### Card 10 -- Bonus Stats (if data exists)

- PRs linked: `total_pr_count`
- Speculation time saved: `total_speculation_time_saved_ms`
- Context collapses: `total_collapse_count`

#### Efficiency Stats

- Autonomy ratio: `autonomy_ratio`
- Avg session duration: `avg_session_duration_min`
- Avg cost per session: `avg_cost_per_session`

**[REVISED] 将 Efficiency Stats 合并入 Card 10 或独立为 Card 11**

> 依据：数据指南 A6 中 `autonomy_ratio`, `avg_session_duration_min`, `avg_cost_per_session` 与 Bonus Stats 的 PR/speculation/collapse 处于同一层级但意义不同。原方案中 Efficiency Stats 没有明确的卡片编号，位置不确定。
>
> 修改：将 Efficiency Stats 独立为 **Card 10 -- Efficiency Profile**，原 Bonus Stats 变为 **Card 11 -- Bonus Stats**。Card 10 使用 3 列大数字布局。

---

### 2.6 Heatmap Page (new)

**Design**: Full-width GitHub-style contribution calendar.

#### Controls

- **Metric switcher**: Turns / Cost / Sessions (pill buttons)
- Year selector (if multi-year data)

#### Heatmap Component

**Rendering**: Canvas-based (calendar layout).

**Calendar layout**:
- Columns = weeks (52-53 columns)
- Rows = weekdays (Mon-Sun, 7 rows)
- Cell size: 14x14px with 2px gap
- Month labels above
- Weekday labels on left (Mon, Wed, Fri)

**[REVISED] Heatmap 色阶需要处理极端值分布**

> 依据：数据指南 C4 日费用波动 2500x（$0.13 ~ $328.85）。如果用线性色阶，大部分天会是浅色，只有几个极端天是深色。数据指南 D5 建议色阶方案但未提及量化分位策略。
>
> 修改：
> 1. **色阶分级使用分位数**而非等距：Level 0 = 无数据, L1 = 0~P25, L2 = P25~P50, L3 = P50~P75, L4 = P75+
> 2. 在色阶图例旁显示每级的**实际阈值**（如 "0-5 turns | 5-20 | 20-80 | 80+"）
> 3. 当选择 "Cost" 指标时，使用**暖色调**（浅黄→深红），与 "Turns" 的冷色调（绿色系）区分

**Color scale**:
- Dark theme (Turns): transparent -> `#0e4429` -> `#006d32` -> `#26a641` -> `#39d353`
- Dark theme (Cost): transparent -> `#5f1e1e` -> `#a63226` -> `#e64141` -> `#ff6b6b`
- Light theme (Turns): `#ebedf0` -> `#9be9a8` -> `#40c463` -> `#216e39` (GitHub green)
- Light theme (Cost): `#ebedf0` -> `#fde68a` -> `#f59e0b` -> `#dc2626`

**Interactions**:
- Hover: tooltip with date, turn count, cost, session count
- Click: filters Sessions page to that date

#### Data Source

Daily data from `TrendResult.entries`.

#### Weekday x Hour Heatmap (retained)

**[REVISED] 7x24 Heatmap 色阶同样需要分位数处理**

> 依据：数据指南 A7 中 weekday_hour_matrix 单元格值 0~500，但分布极不均匀（夜间高峰 vs 凌晨几乎为零）。
>
> 修改：与 Calendar Heatmap 统一使用分位数色阶。同时在 heatmap 右侧增加**行汇总**（每天总 turns）和下方增加**列汇总**（每小时总 turns），形成完整的 marginal bar 布局。

---

## 3. Data Flow Design

### 3.1 ~ 3.5

保持原方案不变（`HtmlReportPayload` Rust struct → JSON blob → Vue 3 reactive store）。

**[REVISED] HtmlReportPayload 需要增加字段**

> 依据：数据指南 A7 指出 `weekday_hour_matrix` 目前仅在 HTML 输出中使用，JSON API 未暴露。数据指南建议"需要新增 JSON 字段"。此外，Calendar Heatmap 需要 `daily_activity` 数据。
>
> 修改：确认 `HtmlReportPayload` 必须包含以下字段（原方案的 `daily_activity` 已有，此处补充）：
> ```rust
> pub struct HtmlReportPayload {
>     // ... existing fields ...
>     pub daily_activity: BTreeMap<String, DailyActivity>,
>     pub weekday_hour_matrix: Option<[[u32; 24]; 7]>,  // NEW: 7x24 heatmap data
> }
> ```

---

## 4. Frontend Directory Structure

保持原方案不变。

**[REVISED] 补充缺失的组件**

> 修改：在 `components/charts/` 下增加：
> ```
> ├── TreemapChart.vue        # Optional treemap (if plugin included)
> └── ParetoChart.vue         # Bar + cumulative line combo
> ```
> 在 `components/common/` 下增加：
> ```
> ├── MiniRing.vue            # Agent ratio mini ring for KPI card
> └── FilterPills.vue         # Pill button group for filter/toggle
> ```

---

## 5. Build Flow

保持原方案不变。

---

## 6. Color / Typography / Design Token Specification

### 6.1 Color Palette

保持原方案不变。

**[REVISED] 增加 Cost Heatmap 色阶变量**

> 依据：原方案只定义了 green 系 heatmap 色阶（用于 turns/activity），Cost 维度需要独立的暖色调。
>
> 修改：在 CSS 变量中增加：
> ```css
> :root {
>   /* Heatmap - Cost (dark, warm) */
>   --heatmap-cost-empty: #161b22;
>   --heatmap-cost-l1:    #5f1e1e;
>   --heatmap-cost-l2:    #a63226;
>   --heatmap-cost-l3:    #e64141;
>   --heatmap-cost-l4:    #ff6b6b;
> }
> [data-theme="light"] {
>   --heatmap-cost-empty: #ebedf0;
>   --heatmap-cost-l1:    #fde68a;
>   --heatmap-cost-l2:    #f59e0b;
>   --heatmap-cost-l3:    #ea580c;
>   --heatmap-cost-l4:    #dc2626;
> }
> ```

### 6.2 ~ 6.6

保持原方案不变。

---

## 7. i18n Strategy

保持原方案不变。

---

## 8. Interaction Design

### 8.1 ~ 8.5

保持原方案不变。

**[REVISED] 增加 8.6 Cross-highlight 联动**

> 依据：数据指南 E4 详细描述了 3 个联动场景，但设计方案的交互设计章节完全没有提到跨图表联动。
>
> 修改：增加 Section 8.6：
>
> **8.6 Cross-highlight (联动)**
>
> 在同一页面内的多个图表之间建立联动：
>
> - **Overview 页面**：点击 Model Bar 中的某个模型 → Top Projects bar 高亮使用该模型的项目 → Most Expensive Sessions 表格过滤
> - **Session Detail 页面**：选中 Context Growth 图表的某段区间 → Turn Detail Table 自动滚动到对应区域 → Cost bar 同步高亮
> - **Trends 页面**：brush 选中一段时间 → 下方 Daily Breakdown Table 自动过滤 → KPI cards 更新为选中区间的统计
>
> 实现方式：使用 Vue `provide/inject` 传递一个 `highlightState` reactive object，各组件监听并响应。
>
> **优先级**：Phase 4 实现，Phase 2-3 预留 `onHighlight` 回调接口。

---

## 9. Responsive Design

保持原方案不变。

**[REVISED] 增加移动端内容优先级说明**

> 依据：数据指南 E5 给出了明确的移动端内容裁剪优先级。
>
> 修改：在 Responsive Design 章节增加：
>
> **移动端内容优先级**（< 768px）：
> 1. **必须保留**：total_cost, total_sessions, cache_savings, model distribution
> 2. **优先保留**：trend chart (简化为 sparkline), session list (精简列: Date/Project/Cost)
> 3. **可折叠**：cost_by_category, tool ranking, heatmap
> 4. **可隐藏**：turn-level timeline, detailed tables, bubble chart

---

## 10. Migration Strategy

保持原方案不变。

---

## 11. Performance Considerations

保持原方案不变。

---

## 12. 数值格式化规范

**[REVISED] 新增章节——从数据指南 F 整合**

> 依据：数据指南 F 提供了完整的格式化参考，但设计方案中只在零散位置提到"compact: 2.1M"等，缺乏统一规范。
>
> 修改：新增以下规范表，`useFormatters()` composable 必须实现所有格式：

| 类型 | 格式规则 | 示例 | 实现函数 |
|------|---------|------|---------|
| 大额费用 (>=$1) | `$X,XXX.XX` | $1,595.06 | `formatCost()` |
| 小额费用 (<$1) | `$X.XXXX` | $0.0824 | `formatCost()` |
| Token 数 | 带 K/M/B 后缀 | 8.87M, 2.07B | `formatCompact()` |
| 百分比 | `XX.X%` | 96.9% | `formatPct()` |
| 时长 | 智能单位 | 3.7 hr, 22 min, 39 hr | `formatDuration()` |
| 日期 | ISO 短格式 | Mar 15, 2026 | `formatDate()` |
| 时间戳 | 转为本地时间 | 14:19 | `formatTime()` |
| Session ID | 前 8 字符 | 69bb9f9f | `formatSessionId()` |
| 倍率 | `X.Xx` | 7.8x | `formatMultiplier()` |

---

## 修订记录

| # | 位置 | 修订内容 | 依据（数据指南） |
|---|------|---------|----------------|
| 1 | 2.1 Row 1 Turns 卡片 | 增加 Agent 占比迷你环形图 | A1: agent_turns 占 30%~60% |
| 2 | 2.1 Row 2 | 增加 Output Ratio 卡片（4卡→5卡） | A1: output_ratio 是独立效率指标 |
| 3 | 2.1 Cache Savings | 格式化改进、null 处理、对比文案 | C1: saved > spent, subscription_value 可 null |
| 4 | 2.1 Model Distribution | 增加对数刻度和 Linear/Log 切换 | C1: Opus 91.5% vs Haiku 1.1%, 差距 80x |
| 5 | 2.1 Cost Composition | 合并 input_cost，突出 cache_read 59% | C1: input_cost < 1%, D2 明确建议合并 |
| 6 | 2.1 Top Tools | 百分比条形/log scale 处理头部极值 | C1: Bash 26x ToolSearch, D4 建议 log |
| 7 | 2.1 Top Projects | 用 Horizontal Bar + Pareto 替代 Treemap | C5: 长尾分布, D4 建议 Pareto |
| 8 | 2.1 Bubble Chart | 双对数刻度 + 刷选缩放 + 按项目分色 | C2: turns 0~2700, cost $0~$180, 极端跨度 |
| 9 | 2.1 Top 5 Sessions 表格 | 增加 Model 和 Agent% 列 | A2: model/agent_turns 是理解"为什么贵"的关键 |
| 10 | 2.2 Daily Cost Trend | Log toggle, 缺失日期处理, 极端值标注 | C4: 波动 2500x, 有 gap 日期 |
| 11 | 2.2 Daily Turns + Cost | 增加说明文案（cost 不只取决于 turns） | B1/C4: session_count 与 cost 不成正比 |
| 12 | 2.2 Monthly Summary | 增加 MoM 变化列 | D1: 建议对比变化百分比 |
| 13 | 2.3 Project Chart | 增加 Pareto 线和 session_count 标注 | C5: Top 1=50%, session 数不成比例 |
| 14 | 2.3 Project Table | 增加 Primary Model 和 Agent Turns 列 | A5: primary_model, agent_turns 字段 |
| 15 | 2.4 Filter Bar | 增加 Agent Mode 筛选 | E3: 建议 Agent mode toggle |
| 16 | 2.4 Session Table | 增加 Title 列 | A2: title 是用户识别 session 的关键 |
| 17 | 2.4 Session KPI Row | 扩展为 8 卡（+Agent Cost, +Output Tokens） | A2: agent_cost/output_tokens 是核心指标 |
| 18 | 2.4 Context Growth 图 | 增加 Compaction 红线标记 + Agent 颜色编码 | D3: 锯齿形, compaction 标记, agent 颜色 |
| 19 | 2.4 新增 Token Composition | 增加 Stacked Area 图表 | D6: turn-level token 组成可视化 |
| 20 | 2.4 Turn Detail Table | 增加筛选按钮组 (All/Main/Agent/Compaction) | D7: agent/compaction 颜色区分 |
| 21 | 2.5 Card 1 Hero | 定义 6 种 Archetype 视觉样式 | G: 6 种 archetype 触发条件和视觉建议 |
| 22 | 2.5 Card 5 Peak Patterns | 小时分布 X 轴从 06:00 开始，夜间深紫色 | C6: peak_hour=23, 夜猫子模式 |
| 23 | 2.5 Card 6 Model Dist | 建议改 Horizontal Bar 或拉出小段 | C6: Opus 73% turns, doughnut 不均匀 |
| 24 | 2.5 Card 9 Highlights | 增加极端值上下文描述 | C6: $180 session, 39 小时 session |
| 25 | 2.5 Efficiency Stats | 独立为 Card 10 + Bonus Stats 为 Card 11 | 结构清晰化 |
| 26 | 2.6 Heatmap 色阶 | 分位数色阶 + Cost 暖色调独立色系 | C4: 2500x 波动, D5: 色阶建议 |
| 27 | 2.6 7x24 Heatmap | 增加行/列 marginal bar 汇总 | A7: weekday_hour_matrix 分布不均 |
| 28 | 3 Data Flow | HtmlReportPayload 增加 weekday_hour_matrix | A7: 目前 JSON API 未暴露 |
| 29 | 4 Directory | 增加 ParetoChart, MiniRing, FilterPills 组件 | 配合新增图表和交互 |
| 30 | 6.1 Colors | 增加 Cost Heatmap 暖色调 CSS 变量 | D5: turns 和 cost 需要不同色系 |
| 31 | 8 Interaction | 新增 8.6 Cross-highlight 联动 | E4: 3 个联动场景 |
| 32 | 9 Responsive | 增加移动端内容优先级 | E5: 4 级优先级 |
| 33 | 12 格式化规范 | 新增完整数值格式化规范表 | F: 9 种格式类型 |

---

## 指标覆盖清单

下表对照数据指南 A1~A7 全部 70+ 指标，检查设计方案覆盖情况。

**图例**：✓ 已覆盖 | ✗ 缺失但应展示（附放置建议） | ○ 可省略（附原因）

### A1. Overview (全局总览)

| 指标 | 状态 | 设计方案位置 | 备注 |
|------|------|-------------|------|
| total_sessions | ✓ | Overview Row 1 KPI | |
| total_turns | ✓ | Overview Row 1 KPI | |
| total_agent_turns | ✓ | Overview Row 1 KPI (sub-label + mini ring) | [REVISED] 增加 mini ring |
| total_output_tokens | ✓ | Overview Row 1 KPI "Claude Wrote" | |
| total_context_tokens | ✓ | Overview Row 1 KPI "Claude Read" | |
| total_cost | ✓ | Overview Row 1 KPI "API Cost" | |
| avg_cache_hit_rate | ✓ | Overview Row 1 KPI (progress ring) | |
| output_ratio | ✓ | Overview Row 2 KPI + Efficiency Metrics | [REVISED] 提升到 Row 2 |
| cost_per_turn | ✓ | Overview Efficiency Metrics | |
| tokens_per_output_turn | ✓ | Overview Efficiency Metrics | |
| cache_savings.total_saved | ✓ | Overview Cache Savings Badge | |
| cache_savings.savings_pct | ✓ | Overview Cache Savings Badge | |
| subscription_value.monthly_price | ✓ | Overview Cache Savings Badge | |
| subscription_value.api_equivalent | ✓ | Overview Cache Savings Badge | |
| subscription_value.value_multiplier | ✓ | Overview Cache Savings Badge | |
| cost_by_category.input_cost | ✓ | Overview Cost Composition Donut | [REVISED] 合并为 "Other" |
| cost_by_category.output_cost | ✓ | Overview Cost Composition Donut | |
| cost_by_category.cache_write_cost | ✓ | Overview Cost Composition Donut | |
| cost_by_category.cache_read_cost | ✓ | Overview Cost Composition Donut | |
| models[] | ✓ | Overview Model Distribution Chart | |
| top_tools[] | ✓ | Overview Top Tools Chart | |
| sessions[] | ✓ | Sessions Page Table | |

### A2. Session Detail (会话详情)

| 指标 | 状态 | 设计方案位置 | 备注 |
|------|------|-------------|------|
| session_id | ✓ | Session Table + Detail Header | |
| project | ✓ | Session Table | |
| model | ✓ | Session Detail KPI Row | |
| duration_minutes | ✓ | Session Detail KPI Row | |
| total_cost | ✓ | Session Detail KPI Row | |
| max_context | ✓ | Session Detail KPI Row | |
| compaction_count | ✓ | Session Detail KPI Row | |
| output_tokens | ✓ | Session Detail KPI Row | [REVISED] 新增到 KPI Row |
| context_tokens | ✗ → ✓ | Session Detail KPI Row tooltip | 主 KPI 显示 max_context，context_tokens 放在 tooltip 或二级面板 |
| cache_hit_rate | ✓ | Session Detail KPI Row + Chart | |
| agent_turns | ✓ | Session Detail Agent Breakdown | |
| agent_output_tokens | ✓ | Session Detail Agent Breakdown | |
| agent_cost | ✓ | Session Detail KPI Row | [REVISED] 新增到 KPI Row |
| title | ✓ | Session Detail Metadata Card | [REVISED] 也加入 Session Table |
| tags[] | ✓ | Session Detail Metadata Card (chips) | |
| turns[] | ✓ | Session Detail Turn Table | |

### A3. Turn Detail (单回合粒度)

| 指标 | 状态 | 设计方案位置 | 备注 |
|------|------|-------------|------|
| turn_number | ✓ | Turn Table "Turn" 列 | |
| timestamp | ✓ | Turn Table "Time" 列 | |
| model | ✓ | Turn Table "Model" 列 | |
| input_tokens | ✓ | Turn Table (mapped to "User" or dedicated) | |
| output_tokens | ✓ | Turn Table "Output" 列 | |
| cache_read_tokens | ○ | 不单独列出 | 通过 cache_hit_rate 间接体现；直接展示会增加表格宽度，对用户价值有限 |
| context_size | ✓ | Turn Table "Context" 列 + Context Growth Chart | |
| cache_hit_rate | ✓ | Turn Table "Hit%" 列 + Chart | |
| cost | ✓ | Turn Table "Cost" 列 | |
| stop_reason | ✓ | Turn Table "Stop" 列 | |
| is_agent | ✓ | Turn Table "Flags" 列 + 左侧蓝色竖条 | |
| is_compaction | ✓ | Turn Table "Flags" 列 + 红色背景 | |
| tool_names[] | ✓ | Turn Table "Tools" 列 (tag chips) | |

### A4. Trend (趋势)

| 指标 | 状态 | 设计方案位置 | 备注 |
|------|------|-------------|------|
| group_label | ✓ | Trends Page subtitle | |
| entries[].label | ✓ | X 轴标签 | |
| entries[].session_count | ✓ | Daily Breakdown Table + tooltip | |
| entries[].turn_count | ✓ | Daily Turns + Cost Combo Chart | |
| entries[].output_tokens | ✓ | Daily Breakdown Table | |
| entries[].context_tokens | ✓ | Daily Breakdown Table | |
| entries[].cost | ✓ | Daily Cost + Cost/Turn Combo Chart | |
| entries[].cost_per_turn | ✓ | Daily Cost Combo (line overlay) | |

### A5. Project (项目)

| 指标 | 状态 | 设计方案位置 | 备注 |
|------|------|-------------|------|
| projects[].name | ✓ | Project Table (internal key) | |
| projects[].display_name | ✓ | Project Table "Project" 列 | |
| projects[].session_count | ✓ | Project Table "Sessions" 列 | |
| projects[].total_turns | ✓ | Project Table "Turns" 列 | |
| projects[].agent_turns | ✓ | Project Table "Agent" 列 | [REVISED] 新增独立列 |
| projects[].output_tokens | ✓ | Project Table "Output" 列 | |
| projects[].context_tokens | ○ | 不单独列出 | 与 output_tokens 高度相关但量级差 3 个数量级，同时展示会分散注意力 |
| projects[].cost | ✓ | Project Chart + Table "Cost" 列 | |
| projects[].primary_model | ✓ | Project Table "Model" 列 | [REVISED] 新增 |

### A6. Wrapped (年度总结)

| 指标 | 状态 | 设计方案位置 | 备注 |
|------|------|-------------|------|
| year | ✓ | Wrapped Hero Card title | |
| active_days | ✓ | Card 2 Activity Stats | |
| total_days | ✓ | Card 2 Activity Stats | |
| longest_streak | ✓ | Card 2 Activity Stats | |
| ghost_days | ✓ | Card 2 Activity Stats | |
| autonomy_ratio | ✓ | Efficiency Stats (Card 10) | |
| avg_session_duration_min | ✓ | Efficiency Stats (Card 10) | |
| avg_cost_per_session | ✓ | Efficiency Stats (Card 10) | |
| output_ratio | ✓ | Card 4 Token Volume | |
| peak_hour | ✓ | Card 5 Peak Patterns | |
| peak_weekday | ✓ | Card 5 Peak Patterns | |
| hourly_distribution | ✓ | Card 5 bar chart | [REVISED] X 轴从 06:00 开始 |
| weekday_distribution | ✓ | Card 5 bar chart | |
| top_projects | ✓ | Card 7 | |
| top_tools | ✓ | Card 8 | |
| most_expensive_session | ✓ | Card 9 Highlights | [REVISED] 增加上下文 |
| longest_session | ✓ | Card 9 Highlights | [REVISED] 增加上下文 |
| model_distribution | ✓ | Card 6 | [REVISED] 建议改 bar chart |
| archetype | ✓ | Card 1 Hero | [REVISED] 增加 6 种视觉样式 |
| total_pr_count | ✓ | Card 11 Bonus Stats | |
| total_speculation_time_saved_ms | ✓ | Card 11 Bonus Stats | |
| total_collapse_count | ✓ | Card 11 Bonus Stats | |
| total_sessions | ✓ | Card 3 Volume Stats | |
| total_turns | ✓ | Card 3 Volume Stats | |
| total_agent_turns | ✓ | Card 3 Volume Stats | |
| total_output_tokens | ✓ | Card 4 Token Volume | |
| total_input_tokens | ✓ | Card 4 Token Volume | |
| total_cost | ✓ | Card 3 Volume Stats | |

### A7. Heatmap (矩阵)

| 指标 | 状态 | 设计方案位置 | 备注 |
|------|------|-------------|------|
| weekday_hour_matrix | ✓ | Heatmap Page secondary panel | [REVISED] 需新增到 JSON API |

### 覆盖率统计

| 分类 | 总指标数 | ✓ 已覆盖 | ✗ 缺失已修正 | ○ 可省略 |
|------|---------|----------|-------------|---------|
| A1. Overview | 22 | 22 | 0 | 0 |
| A2. Session Detail | 16 | 16 | 0 | 0 |
| A3. Turn Detail | 13 | 12 | 0 | 1 (cache_read_tokens) |
| A4. Trend | 8 | 8 | 0 | 0 |
| A5. Project | 9 | 8 | 0 | 1 (context_tokens) |
| A6. Wrapped | 27 | 27 | 0 | 0 |
| A7. Heatmap | 1 | 1 | 0 | 0 |
| **合计** | **96** | **94** | **0** | **2** |

> 最终覆盖率：98%（96 个指标中 94 个已覆盖，2 个合理省略）。原设计方案的基础覆盖度很高，本次修订主要聚焦在**展示方式**的数据适配（scale、色阶、布局）和**缺失细节**的补全（archetype 样式、格式化规范、联动交互）。
