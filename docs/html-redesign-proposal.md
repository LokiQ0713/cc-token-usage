# HTML Report Redesign Proposal

## Vue 3 + Vite + Chart.js + TailwindCSS

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

**Rationale**: The current tab-based sub-nav (`Overview / Monthly / Projects`) forces 3 different views into a flat structure. A sidebar provides persistent context about which page you are on, and each page gets its own scroll context. On narrow screens (<768px), the sidebar collapses to a bottom tab bar with icons + labels.

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

When `render_dual_report_html` is called, a **source toggle pill** appears at the top of the sidebar (below the title). Each source maintains independent state. Switching sources re-renders all page content for that source.

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

#### Row 2 -- Secondary KPIs (4 cards)

| Card | Value | Source |
|------|-------|--------|
| Daily Avg | `total_cost / days` | Computed from `quality.time_range` |
| Peak Context | max of `session_summaries[].max_context` | Computed |
| Compactions | sum of `session_summaries[].compaction_count` | Computed |
| Avg Session Duration | mean of `session_summaries[].duration_minutes` | Computed |

#### Panel: Cache Savings Badge

A standalone accent-colored banner:
```
Cache saved you $432 (78% of reads were free)
Subscription: $200/mo -> 3.2x value multiplier
```

Source: `OverviewResult.cache_savings`, `subscription_value`.

#### Panel: Model Distribution (2-column, left)

**Chart type**: Horizontal bar chart (Chart.js `bar`, `indexAxis: 'y'`)

- Y-axis: model short name (e.g., "opus-4", "sonnet-4")
- X-axis: cost ($)
- Bar color: palette by index
- Tooltip: turns, output tokens, cost

Source: `tokens_by_model`, `cost_by_model`.

#### Panel: Cost Composition (2-column, right)

**Chart type**: Doughnut chart

- 4 segments: Output / Cache Write / Input / Cache Read
- Center label: total cost
- Hover: exact dollar + percentage

Source: `cost_by_category` (`input_cost`, `output_cost`, `cache_write_5m_cost + cache_write_1h_cost`, `cache_read_cost`).

#### Panel: Efficiency Metrics (full-width card, 3-column stat row)

Three inline stats with micro-sparklines:

| Metric | Value | Description |
|--------|-------|-------------|
| Output Ratio | `output_ratio` % | "How much Claude writes vs reads" |
| Cost per Turn | `cost_per_turn` | "Average cost of each response" |
| Tokens per Turn | `tokens_per_output_turn` | "Average output length" |

No chart needed -- just large numbers with subtle description text.

#### Panel: Top Tools (2-column, left)

**Chart type**: Horizontal bar chart (top 10)

- Bar length proportional to count
- Tool name as label
- Tooltip: exact count

Source: `tool_counts`.

#### Panel: Top Projects (2-column, right)

**Chart type**: Treemap (via `chartjs-chart-treemap` plugin, inline)

- Each rectangle = one project
- Size = cost
- Color intensity = cost/session
- Label: project name + cost

Fallback if treemap plugin is too heavy: horizontal bar chart like Model Distribution.

Source: Computed from `session_summaries` aggregated by `project_display_name`.

#### Panel: Session Efficiency Bubble Chart (full-width)

**Chart type**: Bubble chart (existing, refined)

- X = turn count, Y = cost, Bubble size = output tokens
- Color: gradient by `cost_per_turn`
- Tooltip: session ID (short), project, turns, cost, cost/turn, output tokens

Source: `session_summaries`.

#### Panel: Most Expensive Sessions Top 5 (full-width)

**Component**: Compact table (no chart)

| Session | Project | Turns | Duration | Cost |
|---------|---------|-------|----------|------|

Source: `session_summaries` sorted by cost desc, take 5.

---

### 2.2 Trends Page

**Layout**: KPI row for current period, then chart panels, then data table.

#### Row 1 -- Current Period KPIs

Auto-detect latest month from `TrendResult.entries`. Display:

| Sessions | Turns | Input Tokens | Output Tokens | Cost |
|----------|-------|--------------|---------------|------|

#### Panel: Daily Cost + Cost/Turn Combo (full-width)

**Chart type**: Bar + Line combo (existing, refined)

- Bar: daily cost (left Y-axis)
- Line: cost/turn (right Y-axis, amber color)
- Tooltip: includes turn count

Source: `TrendResult.entries` filtered to current month.

#### Panel: Daily Turns + Cost Combo (full-width)

**Chart type**: Bar + Line combo

- Bar: daily turns (left Y-axis, blue)
- Line: daily cost (right Y-axis, green)

Source: same as above.

#### Panel: Daily Model Distribution (full-width, conditional)

**Chart type**: Stacked bar chart

- X-axis: dates
- Each stack segment = one model
- Y-axis: output tokens
- Legend below

Only shown when more than one model is used.

Source: `TrendEntry.models`.

#### Panel: Monthly Summary Table (conditional, shown when data spans 2+ months)

**Component**: Sortable table

| Month | Sessions | Turns | Input Tokens | Output Tokens | Cost |

Source: `TrendResult.entries` aggregated by month.

#### Panel: Daily Breakdown Table (full-width)

**Component**: Sortable table

| Date | Sessions | Turns | Input Tokens | Output Tokens | Cost | Models |

"Models" column: rendered as small tags with compact token count.

Source: `TrendResult.entries`.

---

### 2.3 Projects Page

**Layout**: Bar chart at top, then drill-down table.

#### Panel: Project Cost Top 10

**Chart type**: Horizontal bar chart (existing, refined)

Source: `ProjectResult.projects`, take 10.

#### Panel: Project Drill-Down Table (3-level)

**Component**: Nested expandable table

**Level 1 -- Project row** (click to expand):

| | Project | Sessions | Turns (Agent) | Output | CacheHit | Cost |

- Bold text, expand arrow on left
- CacheHit rendered as progress bar with color (green/amber/red)
- Agent turns shown as badge: `42 +8 agent`

**Level 2 -- Session row** (shown when project expanded, click to expand):

| | Session ID (date, duration) | | Turns (Agent) | Output | CacheHit | Tools | Cost |

- Session ID is first 10 chars
- Date/duration as subtle sub-text
- Tools: tag chips showing top 5 tools with counts
- Has expand arrow if turn details available

**Level 3 -- Turn detail** (shown when session expanded):

Full turn table (see Session Detail below).

Source: `ProjectResult.projects`, `OverviewResult.session_summaries`, `SessionSummary.turn_details`.

---

### 2.4 Sessions Page (new dedicated page)

Currently sessions are only accessible through the Projects drill-down. This new page provides a **top-level searchable, filterable, sortable session list**.

#### Filter Bar

- Text search (session ID, project name)
- Model filter (dropdown)
- Date range filter
- Cost range filter (slider)
- Sort by: Cost / Turns / Duration / Date

#### Session Table

| Date | Session | Project | Model | Turns | Agent | Duration | Output | CacheHit | Cost |

Click any row to expand inline detail, or click a "detail" icon to navigate to an anchor showing the full Session Detail panel.

#### Session Detail Panel (expanded inline or as modal overlay)

**Section 1 -- KPI Row** (6 cards):

| Duration | Model | Max Context | Cache Hit % | Compactions | Total Cost |

**Section 2 -- Metadata Card** (conditional, only shown if data exists):

- Title, Tags (as chips), Mode, Branch, PR Links (as clickable links)

Source: `SessionResult.title`, `tags`, `mode`, `git_branches`, `pr_links`.

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

Source: `SessionResult` fields.

**Section 4 -- Charts (2-column grid)**:

- Left: **Context Growth** line chart (X=turn, Y=context_size)
- Right: **Cache Hit Rate** line chart (X=turn, Y=cache_hit_rate, 0-100%)

Point radius adapts: 3px for <50 turns, 0px for >=50 turns.

**Section 5 -- Agent Breakdown** (conditional, only if agents exist):

Table:

| Type | Description | Turns | Output Tokens | Cost |

First row is always "main (this conversation)".

Source: `SessionResult.agent_summary.agents`.

**Section 6 -- Stop Reason Distribution**:

**Chart type**: Doughnut chart (existing).

Source: `SessionResult.stop_reason_counts`.

**Section 7 -- Context Collapse** (conditional):

Card showing:
- Collapse count, avg risk, max risk (with warning icon if max > 0.5)
- List of collapse summaries (numbered, truncated)

Source: `SessionResult.collapse_count`, `collapse_summaries`, `collapse_avg_risk`, `collapse_max_risk`.

**Section 8 -- Code Attribution** (conditional):

Card showing:
- Files touched
- Claude wrote (chars)
- Prompts count (+ escaped count)
- Permission prompts count

Source: `SessionResult.attribution`.

**Section 9 -- Turn Detail Table** (full-width):

| Turn | Time | Model | User | Assistant | Tools | Output | Context | Hit% | Cost | Stop | Flags |

- Time: converted from UTC to local timezone via JS
- User/Assistant: truncated preview with full text in tooltip
- Tools: tag chips
- CacheHit: progress bar
- Compaction rows: highlighted background (red-tinted)
- Agent rows: left blue border
- Flags column: lightning for compaction, robot for agent

---

### 2.5 Wrapped Page

**Design**: Spotify Wrapped-style vertical scroll of full-width cards with bold typography, dark gradients, and animated counters.

#### Card 1 -- Hero Card

```
+------------------------------------------+
|     Your 2026 Claude Code Wrapped        |
|                                          |
|          The Architect                   |
|   (archetype description in italic)     |
+------------------------------------------+
```

Large, centered. Archetype label in 2.5rem bold. Description in subtle italic.

Source: `WrappedResult.archetype`.

#### Card 2 -- Activity Stats

```
+------------------------------------------+
|  142 Active Days   |  23-day Streak      |
|  223 Ghost Days    |  365 Total Days     |
+------------------------------------------+
```

2x2 grid of large numbers.

Source: `active_days`, `longest_streak`, `ghost_days`, `total_days`.

#### Card 3 -- Volume Stats

```
+------------------------------------------+
|  1,247 Sessions    |  18,432 Turns       |
|  2,891 Agent Turns |  $1,247 Total Cost  |
+------------------------------------------+
```

Source: `total_sessions`, `total_turns`, `total_agent_turns`, `total_cost`.

#### Card 4 -- Token Volume

```
+------------------------------------------+
|  Claude wrote 42.7M tokens for you       |
|  Claude read 891.2M tokens total         |
|  Output ratio: 4.8%                      |
+------------------------------------------+
```

Source: `total_output_tokens`, `total_input_tokens`, `output_ratio`.

#### Card 5 -- Peak Patterns

```
+------------------------------------------+
|  Your peak hour: 22:00                   |
|  Your peak day: Wednesday                |
|                                          |
|  [hourly bar chart, 24 bars]             |
|  [weekday bar chart, 7 bars]             |
+------------------------------------------+
```

**Chart type**: Two small bar charts (Chart.js, minimal styling).

Source: `peak_hour`, `peak_weekday`, `hourly_distribution`, `weekday_distribution`.

#### Card 6 -- Model Distribution

**Chart type**: Doughnut chart, compact.

Source: `model_distribution`.

#### Card 7 -- Top Projects (top 5)

Ranked list with cost bars:

```
1. cc-token-analyzer    $432  ████████████
2. my-webapp            $281  ████████
3. api-server           $124  ████
```

Source: `top_projects`.

#### Card 8 -- Top Tools (top 5)

Same format as Top Projects but with count.

Source: `top_tools`.

#### Card 9 -- Highlight Sessions

```
+------------------------------------------+
|  Most expensive: abc12345 ($42.50)       |
|    Project: cc-token-analyzer            |
|                                          |
|  Longest session: def67890 (4h32m)       |
|    Project: my-webapp                    |
+------------------------------------------+
```

Source: `most_expensive_session`, `longest_session`.

#### Card 10 -- Bonus Stats (if data exists)

- PRs linked: `total_pr_count`
- Speculation time saved: `total_speculation_time_saved_ms`
- Context collapses: `total_collapse_count`

#### Efficiency Stats

- Autonomy ratio: `autonomy_ratio`
- Avg session duration: `avg_session_duration_min`
- Avg cost per session: `avg_cost_per_session`

---

### 2.6 Heatmap Page (new)

**Design**: Full-width GitHub-style contribution calendar.

#### Controls

- **Metric switcher**: Turns / Cost / Sessions (pill buttons)
- Year selector (if multi-year data)

#### Heatmap Component

**Rendering**: Canvas-based (same technique as current `drawHeatmap`, but calendar layout instead of weekday x hour matrix).

**Calendar layout**:
- Columns = weeks (52-53 columns)
- Rows = weekdays (Mon-Sun, 7 rows)
- Cell size: 14x14px with 2px gap
- Month labels above
- Weekday labels on left (Mon, Wed, Fri)

**Color scale**:
- Dark theme: transparent -> `#1e3a5f` -> `#3b82f6` -> `#60a5fa` (4 intensity levels)
- Light theme: `#ebedf0` -> `#9be9a8` -> `#40c463` -> `#216e39` (GitHub green)

**Interactions**:
- Hover: tooltip with date, turn count, cost, session count
- Click: filters Sessions page to that date (if implemented as SPA)

#### Data Source

The current `weekday_hour_matrix` provides weekday x hour data. For the calendar heatmap, we need **daily data** which is available from `TrendResult.entries` (each entry has a `date: NaiveDate`). The Rust side will serialize a `daily_activity` map:

```json
{
  "daily_activity": {
    "2026-01-15": { "turns": 42, "cost": 3.21, "sessions": 2 },
    "2026-01-16": { "turns": 87, "cost": 7.42, "sessions": 4 },
    ...
  }
}
```

#### Weekday x Hour Heatmap (retained)

Keep the existing 7x24 heatmap as a secondary panel below the calendar, with the same metric switcher affecting it.

---

## 3. Data Flow Design

### 3.1 Overview

```
┌─────────────┐     serialize      ┌──────────────┐     hydrate      ┌──────────────┐
│  Rust CLI    │ ──────────────>   │  JSON blob    │ ──────────────> │  Vue 3 App   │
│  (analysis)  │                   │  (in <script>)│                  │  (reactive)  │
└─────────────┘                    └──────────────┘                  └──────────────┘
```

### 3.2 Rust Side

Create a unified `HtmlReportPayload` struct that serializes everything the frontend needs:

```rust
#[derive(Serialize)]
pub struct HtmlReportPayload {
    // Source identification
    pub source_name: String,
    pub generated_at: String,

    // Overview data
    pub overview: OverviewPayload,

    // Trend data
    pub trend: TrendPayload,

    // Projects data
    pub projects: ProjectsPayload,

    // Session detail data (for top N sessions with turn details)
    pub session_details: Vec<SessionDetailPayload>,

    // Wrapped data (optional, only when year data available)
    pub wrapped: Option<WrappedPayload>,

    // Heatmap calendar data
    pub daily_activity: BTreeMap<String, DailyActivity>,
}
```

This single JSON blob replaces all the scattered inline `<script>` data assignments.

### 3.3 Embedding in HTML

The Rust HTML renderer produces:

```html
<script>
  window.__CC_DATA__ = { /* entire HtmlReportPayload as JSON */ };
</script>
```

The Vue app reads `window.__CC_DATA__` at mount time.

### 3.4 Dual-Source Support

For dual-source mode:

```html
<script>
  window.__CC_DATA__ = {
    sources: [
      { name: "Local", data: { /* HtmlReportPayload */ } },
      { name: "Team", data: { /* HtmlReportPayload */ } }
    ]
  };
</script>
```

### 3.5 Vue Data Flow

```
window.__CC_DATA__
    |
    v
App.vue (provide/inject global state)
    |
    ├── useReportStore()  -- Pinia store or reactive() for current source data
    |     ├── currentSource (ref)
    |     ├── overview (computed)
    |     ├── trend (computed)
    |     ├── projects (computed)
    |     ├── sessions (computed)
    |     ├── wrapped (computed)
    |     └── heatmapData (computed)
    |
    ├── useTheme()        -- composable: dark/light toggle + localStorage
    ├── useI18n()         -- composable: EN/ZH toggle + localStorage
    └── useFormatters()   -- composable: formatCost, formatNumber, formatCompact, formatDuration
```

Each page component receives data through the store:

```vue
<!-- OverviewPage.vue -->
<script setup>
import { useReportStore } from '@/stores/report'
const store = useReportStore()
const overview = computed(() => store.overview)
</script>
```

---

## 4. Frontend Directory Structure

```
frontend/
├── index.html              # Vite entry point (dev only)
├── package.json
├── vite.config.ts          # Vite config with inlining plugin
├── tailwind.config.ts      # TailwindCSS config
├── tsconfig.json
│
├── src/
│   ├── main.ts             # App bootstrap, read window.__CC_DATA__
│   ├── App.vue             # Root: sidebar + router-view
│   │
│   ├── types/
│   │   ├── index.ts        # All TypeScript interfaces matching Rust payloads
│   │   └── formatters.ts   # Formatting utility types
│   │
│   ├── stores/
│   │   └── report.ts       # Pinia store: source switching, computed data
│   │
│   ├── composables/
│   │   ├── useTheme.ts     # Dark/light theme
│   │   ├── useI18n.ts      # EN/ZH i18n
│   │   ├── useFormatters.ts# Number, cost, duration formatting
│   │   └── useChartTheme.ts# Chart.js theme-aware defaults
│   │
│   ├── components/
│   │   ├── layout/
│   │   │   ├── Sidebar.vue        # Navigation sidebar (desktop)
│   │   │   ├── BottomBar.vue      # Navigation bottom bar (mobile)
│   │   │   ├── SourceSwitcher.vue # Dual-source toggle
│   │   │   └── HeaderControls.vue # Theme + language toggles
│   │   │
│   │   ├── common/
│   │   │   ├── KpiCard.vue        # Reusable KPI card
│   │   │   ├── ProgressBar.vue    # Cache hit rate bar
│   │   │   ├── ToolTag.vue        # Tool name chip
│   │   │   ├── AgentBadge.vue     # "+N agent" badge
│   │   │   ├── SortableTable.vue  # Generic sortable table
│   │   │   ├── ExpandableRow.vue  # Expandable table row
│   │   │   └── ChartCard.vue      # Card wrapper for Chart.js canvas
│   │   │
│   │   ├── charts/
│   │   │   ├── BarChart.vue       # Wrapper for bar/horizontal bar
│   │   │   ├── DoughnutChart.vue  # Wrapper for doughnut
│   │   │   ├── LineChart.vue      # Wrapper for line
│   │   │   ├── BubbleChart.vue    # Wrapper for bubble scatter
│   │   │   ├── ComboChart.vue     # Bar + Line combo
│   │   │   ├── StackedBarChart.vue
│   │   │   └── CalendarHeatmap.vue# Canvas-based GitHub heatmap
│   │   │
│   │   └── sections/
│   │       ├── CacheSavingsBanner.vue
│   │       ├── SessionMetadata.vue
│   │       ├── PerformanceCard.vue
│   │       ├── AgentBreakdown.vue
│   │       ├── ContextCollapse.vue
│   │       ├── CodeAttribution.vue
│   │       ├── TurnDetailTable.vue
│   │       └── WrappedCard.vue    # Single Wrapped-style card
│   │
│   ├── pages/
│   │   ├── OverviewPage.vue
│   │   ├── TrendsPage.vue
│   │   ├── ProjectsPage.vue
│   │   ├── SessionsPage.vue
│   │   ├── WrappedPage.vue
│   │   └── HeatmapPage.vue
│   │
│   └── styles/
│       ├── variables.css    # CSS custom properties (colors, spacing)
│       └── base.css         # Global resets, font imports
│
├── public/                  # Static assets (none needed for production)
│
└── scripts/
    └── build-inline.ts      # Post-build script: inline all assets into single HTML
```

---

## 5. Build Flow

### 5.1 Development

```bash
cd frontend
npm install
npm run dev        # Vite dev server with HMR
```

During development, `index.html` contains a mock `window.__CC_DATA__` with sample data (generated by `cargo run -- --format json` and saved as a fixture).

### 5.2 Production Build

```bash
npm run build      # Outputs: dist/index.html (self-contained)
```

#### Vite Configuration for Single-File Output

```typescript
// vite.config.ts
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { viteSingleFile } from 'vite-plugin-singlefile'

export default defineConfig({
  plugins: [
    vue(),
    viteSingleFile(),  // Inlines all JS/CSS/assets into index.html
  ],
  build: {
    target: 'es2020',
    cssCodeSplit: false,
    assetsInlineLimit: Infinity,  // Inline everything
  },
})
```

The `vite-plugin-singlefile` handles:
- Inlining all JS chunks into `<script>` tags
- Inlining all CSS into `<style>` tags
- Inlining Chart.js (bundled via npm, not CDN)
- Removing all external resource references

#### Output

`dist/index.html` -- a single self-contained HTML file with:
- Vue 3 runtime (~40KB gzip)
- Chart.js (~70KB gzip)
- TailwindCSS (purged, ~10KB gzip)
- Application code (~20KB gzip)
- Total: ~140KB gzip (acceptable for a report file)

### 5.3 Rust Integration

The built `dist/index.html` is treated as a **template**. The Rust side:

1. Reads the template at compile time via `include_str!("../../../frontend/dist/index.html")`
2. Replaces the `window.__CC_DATA__ = {};` placeholder with actual serialized data
3. Outputs the final HTML

```rust
pub fn render_html_report(payload: &HtmlReportPayload) -> String {
    let template = include_str!("../../../frontend/dist/index.html");
    let json = serde_json::to_string(payload).unwrap();
    template.replace(
        "window.__CC_DATA__={};",
        &format!("window.__CC_DATA__={};", json)
    )
}
```

### 5.4 CI Integration

Add to the build workflow:

```yaml
- name: Build frontend
  working-directory: frontend
  run: |
    npm ci
    npm run build

- name: Verify frontend template
  run: |
    test -f frontend/dist/index.html
    # Verify placeholder exists
    grep -q '__CC_DATA__' frontend/dist/index.html
```

The built template (`frontend/dist/index.html`) is committed to the repo so that `cargo build` works without requiring Node.js. Only updated when frontend changes are made.

---

## 6. Color / Typography / Design Token Specification

### 6.1 Color Palette

Extending the current CSS variables to a full design token system:

```css
/* ── Base Palette ─────────────────────────────────────────── */

:root {
  /* Dark theme (default) */
  --bg-primary:      #0a0a0b;
  --bg-secondary:    #111113;
  --bg-tertiary:     #18181b;
  --bg-deep:         #27272a;
  --bg-elevated:     #1c1c1f;    /* NEW: cards with subtle lift */

  --border-color:    #27272a;
  --border-subtle:   #1f1f23;    /* NEW: subtle dividers */

  --text-primary:    #fafafa;
  --text-secondary:  #a1a1aa;
  --text-tertiary:   #71717a;
  --text-muted:      #52525b;    /* NEW: very low emphasis */

  --accent-blue:     #3b82f6;
  --accent-purple:   #8b5cf6;
  --accent-cyan:     #06b6d4;
  --accent-green:    #22c55e;
  --accent-amber:    #f59e0b;
  --accent-red:      #ef4444;
  --accent-pink:     #ec4899;

  /* Semantic colors */
  --success:         #22c55e;
  --warning:         #f59e0b;
  --error:           #ef4444;
  --info:            #3b82f6;

  /* Chart-specific */
  --chart-grid:      #27272a;
  --chart-text:      #a1a1aa;

  /* Component-specific */
  --kpi-bg:          #111113;
  --card-shadow:     none;
  --sidebar-bg:      #0d0d0e;
  --sidebar-active:  #18181b;

  /* Heatmap scale (dark) */
  --heatmap-empty:   #161b22;
  --heatmap-l1:      #0e4429;
  --heatmap-l2:      #006d32;
  --heatmap-l3:      #26a641;
  --heatmap-l4:      #39d353;
}

[data-theme="light"] {
  --bg-primary:      #ffffff;
  --bg-secondary:    #fafafa;
  --bg-tertiary:     #f4f4f5;
  --bg-deep:         #e4e4e7;
  --bg-elevated:     #ffffff;

  --border-color:    #e4e4e7;
  --border-subtle:   #f0f0f2;

  --text-primary:    #09090b;
  --text-secondary:  #52525b;
  --text-tertiary:   #a1a1aa;
  --text-muted:      #d4d4d8;

  --accent-blue:     #2563eb;
  --accent-purple:   #7c3aed;
  --accent-cyan:     #0891b2;
  --accent-green:    #16a34a;
  --accent-amber:    #d97706;
  --accent-red:      #dc2626;
  --accent-pink:     #db2777;

  --success:         #16a34a;
  --warning:         #d97706;
  --error:           #dc2626;
  --info:            #2563eb;

  --chart-grid:      #e4e4e7;
  --chart-text:      #52525b;

  --kpi-bg:          #ffffff;
  --card-shadow:     0 1px 3px rgba(0,0,0,0.04), 0 1px 2px rgba(0,0,0,0.06);
  --sidebar-bg:      #fafafa;
  --sidebar-active:  #f4f4f5;

  /* Heatmap scale (light -- GitHub green) */
  --heatmap-empty:   #ebedf0;
  --heatmap-l1:      #9be9a8;
  --heatmap-l2:      #40c463;
  --heatmap-l3:      #30a14e;
  --heatmap-l4:      #216e39;
}
```

### 6.2 Chart Color Palette (10 colors)

Used for multi-series charts, doughnut segments, bar charts:

```
#3b82f6  blue
#8b5cf6  purple
#06b6d4  cyan
#22c55e  green
#f59e0b  amber
#ef4444  red
#ec4899  pink
#a78bfa  light purple
#2dd4bf  teal
#fb923c  orange
```

Same palette as current. Applied consistently via a `useChartTheme` composable that provides a `getColor(index)` function.

### 6.3 Typography

```css
/* Font stack */
font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;

/* Scale */
--text-xs:    0.75rem;    /* 12px -- table cells, captions */
--text-sm:    0.8125rem;  /* 13px -- secondary content */
--text-base:  0.875rem;   /* 14px -- body text */
--text-lg:    1rem;        /* 16px -- card labels */
--text-xl:    1.25rem;     /* 20px -- section headers */
--text-2xl:   1.5rem;      /* 24px -- page title */
--text-3xl:   2rem;        /* 32px -- KPI values */
--text-4xl:   2.5rem;      /* 40px -- Wrapped hero numbers */

/* Weights */
--font-normal:  400;
--font-medium:  500;
--font-semibold: 600;
--font-bold:    700;

/* Line heights */
--leading-tight:  1.1;    /* KPI values */
--leading-snug:   1.3;    /* Headers */
--leading-normal: 1.5;    /* Body */
--leading-relaxed: 1.7;   /* Descriptions */
```

### 6.4 Spacing System

Based on 4px grid:

```
--space-1:  4px
--space-2:  8px
--space-3:  12px
--space-4:  16px
--space-5:  20px
--space-6:  24px
--space-8:  32px
--space-10: 40px
--space-12: 48px
```

TailwindCSS default spacing scale applies, augmented by these CSS variables for non-Tailwind contexts.

### 6.5 Border Radius

```
--radius-sm:  6px     /* buttons, tags */
--radius-md:  8px     /* inputs, small cards */
--radius-lg:  12px    /* cards */
--radius-xl:  16px    /* modal, wrapped cards */
--radius-full: 9999px /* pills, avatars */
```

### 6.6 Component Patterns

**KPI Card**:
- Background: `var(--bg-secondary)`
- Border: `1px solid var(--border-color)`
- Border radius: `var(--radius-lg)`
- Padding: `14px 16px`
- Value: `var(--text-3xl)`, `var(--font-semibold)`, `var(--text-primary)`
- Label: `var(--text-xs)`, `var(--font-medium)`, `var(--text-tertiary)`, uppercase, letter-spacing 0.05em

**Panel Card**:
- Same as KPI Card but larger padding: `20px 24px`
- Title: `var(--text-xl)`, `var(--font-semibold)`, margin-bottom `12px`

**Table**:
- Header: sticky, `var(--bg-secondary)` background, uppercase small text
- Rows: `border-bottom: 1px solid var(--border-color)`
- Hover: `var(--bg-tertiary)`
- Number columns: right-aligned, tabular-nums
- Text columns: left-aligned

**Tag/Chip**:
- Background: `var(--bg-deep)`
- Text: `var(--text-secondary)`
- Font: `var(--text-xs)`
- Padding: `1px 6px`
- Border radius: `var(--radius-sm)`
- Count sub-text: `var(--accent-blue)`, `var(--font-semibold)`

**Progress Bar**:
- Track: `var(--bg-deep)`, height 14px, full radius
- Fill: green (>=90%), amber (>=70%), red (<70%)
- Text: inline right of bar, `var(--text-xs)`

---

## 7. i18n Strategy

### 7.1 Approach

Use a `useI18n()` composable with a simple key-value dictionary:

```typescript
// i18n/messages.ts
export const messages = {
  en: {
    'nav.overview': 'Overview',
    'nav.trends': 'Trends',
    'nav.projects': 'Projects',
    'nav.sessions': 'Sessions',
    'nav.wrapped': 'Wrapped',
    'nav.heatmap': 'Heatmap',
    'kpi.sessions': 'Sessions',
    'kpi.turns': 'Turns',
    'kpi.claude_wrote': 'Claude Wrote',
    'kpi.claude_read': 'Claude Read',
    'kpi.cache_hit': 'Avg Cache Hit Rate',
    'kpi.api_cost': 'Token Value (API Rate)',
    'kpi.cache_savings': 'Cache Savings ({pct}%)',
    // ... 100+ keys
  },
  zh: {
    'nav.overview': '概览',
    'nav.trends': '趋势',
    'nav.projects': '项目',
    'nav.sessions': '会话',
    'nav.wrapped': '年度总结',
    'nav.heatmap': '热力图',
    'kpi.sessions': '会话数',
    'kpi.turns': '响应数',
    'kpi.claude_wrote': 'Claude 写了',
    'kpi.claude_read': 'Claude 读了',
    'kpi.cache_hit': '平均缓存命中率',
    'kpi.api_cost': 'Token 价值 (API 费率)',
    'kpi.cache_savings': '缓存节省 ({pct}%)',
    // ...
  },
}
```

### 7.2 Usage in Templates

```vue
<span>{{ t('kpi.sessions') }}</span>
```

No more `data-en` / `data-zh` attributes. The Vue reactivity system handles re-renders when language changes.

### 7.3 Persistence

Language preference saved to `localStorage` under key `cc-lang`. Default: detected from `navigator.language` (if starts with "zh", default zh; else en).

---

## 8. Interaction Design

### 8.1 Sorting

All data tables support click-to-sort on column headers:
- First click: descending (for numeric columns) / ascending (for text)
- Second click: toggle direction
- Visual indicator: up/down arrow in active column header
- Implementation: Vue `ref` for sort column + direction, `computed` for sorted array

### 8.2 Filtering (Sessions Page)

- Debounced text search (300ms)
- Model filter: multi-select dropdown
- Date range: two date inputs (start, end)
- Instant filtering via computed properties (no server round-trip)

### 8.3 Expand/Collapse

- Project rows: click expand arrow to show sessions
- Session rows: click expand arrow to show turn details
- State managed per-row via `Set<string>` of expanded IDs
- Smooth height transition: `max-height` with CSS transition

### 8.4 Tooltips

- Chart tooltips: Chart.js built-in, customized via `tooltip.callbacks`
- Table cell tooltips: native `title` attribute for truncated text
- Heatmap tooltips: custom floating div positioned on hover

### 8.5 Theme Toggle

- Instant toggle via CSS custom properties
- Chart.js instances updated via global `updateChartColors()` function
- Canvas heatmaps redrawn on toggle
- Persisted in `localStorage`

---

## 9. Responsive Design

### Breakpoints

```
>= 1280px  Desktop (full sidebar + 2-column charts)
>= 768px   Tablet (collapsed sidebar icon-only + 2-column charts)
< 768px    Mobile (bottom tab bar + single column)
```

### Specific Adaptations

| Component | Desktop | Tablet | Mobile |
|-----------|---------|--------|--------|
| Navigation | Sidebar with text | Icon-only sidebar | Bottom tab bar |
| KPI grid | 6 columns | 3 columns | 2 columns |
| Chart grid | 2 columns | 2 columns | 1 column |
| Tables | Full columns | Scroll horizontal | Scroll horizontal |
| Wrapped cards | Max-width centered | Full width | Full width |
| Heatmap calendar | Full year visible | Scrollable | Scrollable |

---

## 10. Migration Strategy

### Phase 1: Foundation (Week 1)
- Set up `frontend/` with Vite + Vue 3 + TailwindCSS
- Create `HtmlReportPayload` Rust struct + serialization
- Build layout shell: sidebar, page routing, theme/i18n composables
- KpiCard, ChartCard, SortableTable base components
- Verify single-file build pipeline works end-to-end

### Phase 2: Core Pages (Week 2)
- Overview page (KPIs, model chart, cost composition, efficiency, tools, bubble chart)
- Trends page (daily/monthly charts, tables)
- Projects page (chart, drill-down table)

### Phase 3: Detail + New Pages (Week 3)
- Sessions page (searchable list, inline detail expansion)
- Session detail (all sections: metadata, performance, agents, charts, turn table)
- Calendar heatmap page

### Phase 4: Wrapped + Polish (Week 4)
- Wrapped page (all cards, animations)
- Cross-browser testing
- Performance optimization (virtual scroll for large turn tables)
- Accessibility pass (ARIA labels, keyboard navigation)
- Final dual-source integration testing

### Backward Compatibility

During migration, the old `html.rs` renderer remains functional. The new frontend is behind a `--html-v2` flag until feature-complete, then becomes the default.

---

## 11. Performance Considerations

### Virtual Scrolling

For sessions with 500+ turns, the turn detail table uses virtual scrolling (only render visible rows). Library: a lightweight inline implementation or `@tanstack/vue-virtual`.

### Chart Lazy Initialization

Charts are only initialized when their containing page/section becomes visible (via `IntersectionObserver`). This avoids 6+ Chart.js instances competing for CPU on load.

### Data Size

Typical report data: 50-200 sessions, ~5000 turns total. JSON payload size: ~500KB-2MB. This is acceptable for a local report file. For very large datasets (1000+ sessions), consider:
- Truncating turn details to top N sessions only (already done: `turn_details: Option<Vec<TurnDetail>>`)
- Lazy-loading turn details on expand

### Bundle Size Budget

| Component | Size (gzip) |
|-----------|-------------|
| Vue 3 runtime | ~40KB |
| Chart.js | ~70KB |
| TailwindCSS (purged) | ~10KB |
| Application code | ~25KB |
| **Total** | **~145KB** |

The final HTML file (template + data) should stay under 5MB for typical usage.
