# Workflow Support

Claude Code 2.1.159+ introduced **workflows**: script-orchestrated runs that
spawn one or more `agent()` invocations (e.g. `session-review-self`). Each
`agent()` call produces a full session transcript — including `usage` tokens —
that lives **entirely outside the main session JSONL**. Before this support
landed, those tokens were never counted; a single workflow run can be very large
(observed: ~3.18M tokens in one session).

This document describes the on-disk data model, the parsing-layer types, how
workflow tokens flow into cost totals, the related pricing fix, the new entry
subtypes, the output surface, the `WorkflowSummary` payload contract, and the
known limitations.

## On-disk data model

Workflow data is stored in a directory named after the **session UUID**, beside
the main session JSONL:

```text
<claude_home>/projects/<project>/<session-id>/
├── workflows/wf_<runId>.json                          # run state snapshot (WorkflowRunSnapshot)
├── workflows/scripts/<name>-wf_<runId>.js             # workflow script source
├── subagents/workflows/wf_<runId>/agent-<id>.jsonl    # full agent() transcript (incl. usage tokens)
├── subagents/workflows/wf_<runId>/agent-<id>.meta.json# {"agentType": "..."}
├── subagents/workflows/wf_<runId>/journal.jsonl       # WorkflowJournalEntry per line (started/result)
└── tool-results/*.txt                                 # large tool_result spillover (hard-link de-duped)
```

The per-agent `agent-<id>.jsonl` transcripts are **plain session JSONL files**
and are parsed through the regular `Entry` / `SessionReader` path. The
workflow-specific snapshot and journal files have their own types.

## Parsing layer (`cc-session-jsonl`)

### Types — `src/types/workflow.rs`

All fields are `Option<T>` for version compatibility; shape-varying fields
(`args`, `result`, `logs`) are `serde_json::Value`.

- **`WorkflowRunSnapshot`** — parsed from `workflows/wf_<runId>.json`. Fields:
  `run_id`, `task_id`, `workflow_name`, `timestamp`, `status`, `script`,
  `script_path`, `args`, `default_model`, `start_time`, `duration_ms`,
  `agent_count`, `total_tokens`, `total_tool_calls`, `phases`,
  `workflow_progress`, `logs`, `result`, `summary`.
- **`WorkflowPhase`** — a declared phase (`title`, `detail`) from `phases[]`.
- **`WorkflowProgress`** — one record from `workflowProgress[]`. Two real
  shapes: phase markers (`type: "workflow_phase"`) and per-agent progress
  (`type: "workflow_agent"`, with `agent_id`, `agent_type`, `model`, `state`,
  `tokens`, `tool_calls`, `duration_ms`). `agent_id` carries no `agent-` prefix.
  Extra fields are dropped (no `deny_unknown_fields`).
- **`WorkflowJournalEntry`** — one line of `journal.jsonl`. Each agent
  invocation emits a `started` and (on completion) a `result` record sharing the
  same content-addressed `key` (e.g. `v2:<sha256>`). `result` shape varies →
  `Value`.

### Scanner — `src/scanner.rs`

- **`SessionFile.workflow_run_id: Option<String>`** — set on agent files
  discovered under `<uuid>/subagents/workflows/wf_<runId>/`; `None` for ordinary
  session and agent files.
- **Type 4 discovery in `scan_sessions`** — after finding new-style subagents
  under `<uuid>/subagents/`, the scanner also walks `subagents/workflows/wf_*/`
  (`collect_workflow_agent_files`) and records every `agent-*.jsonl` as a
  `SessionFile` with `is_agent = true`, `parent_session_id = Some(<uuid>)`, and
  `workflow_run_id = Some("wf_<runId>")`. This reuses the existing agent
  aggregation channel so workflow tokens are counted, while the `workflow_run_id`
  keeps them distinguishable.
- **`WorkflowAgentFile`** — one agent transcript (`agent_id` with `agent-`
  prefix stripped, `path`, optional `meta_path`).
- **`WorkflowRun`** — a discovered run: `run_id`, owning `session_id`,
  `project`, parsed `snapshot`, `snapshot_path`, `script_paths`, `agent_files`,
  `journal_path`.
- **`scan_session_workflows(session_id, claude_home)`** — all runs for one
  session, resolving snapshot, scripts, agent transcripts, and journal beneath
  each project's `<session_id>/` directory.
- **`scan_workflows(claude_home)`** — all runs across every session.
- **`load_workflow_agent_meta(session_id, claude_home)`** — reads the
  `subagents/workflows/wf_*/agent-*.meta.json` sidecars into a map keyed by agent
  id (prefix stripped). Complements `load_agent_meta`, which only reads the
  non-workflow `subagents/agent-*.meta.json` sidecars.

### Session aggregation — `src/session.rs`

- **`RawSession.workflow_runs: Vec<WorkflowRun>`** — workflow runs for the
  session, symmetric to `agent_files`. Workflow agent files are aggregated into
  `agent_files` alongside ordinary subagents (they carry a `workflow_run_id`),
  so their tokens are counted uniformly.
- `load_all_sessions` merges ordinary subagent meta with workflow agent meta and
  calls `scan_session_workflows` per session.

## Cost aggregation (`cc-token-usage`)

Workflow agents flow through the **existing subagent aggregation channel**, so
their tokens automatically enter the total cost via `all_responses()`:

- `data/loader.rs` carries `workflow_run_id` from the `SessionFile` onto each
  `Subagent` (`data/models.rs`: `Subagent.workflow_run_id: Option<String>`).
  Workflow agent turns survive the sidechain filter (they are `is_agent = true`)
  and the cross-file `requestId` dedup, then contribute to the parent session's
  totals.
- The non-workflow and workflow `.meta.json` maps are merged (first-level /
  non-workflow entries win on key collision via `or_insert`) so workflow agents
  surface their `agentType`.
- `analysis/session.rs::build_workflow_summaries` produces one
  `WorkflowSummary` per discovered run. The **declared** figures come from the
  snapshot; the **measured** figures (`parsed_*`) are re-aggregated from the
  session's own subagents whose `workflow_run_id == run_id`, so they reflect
  exactly what flows into the session/overview cost totals. Runs are sorted by
  `run_id`.
- `SessionResult.workflows: Vec<WorkflowSummary>` (`analysis/mod.rs`) carries the
  result into output.

### Cross-validation — `analysis/validate.rs`

For each discovered run the validator reconciles parsed totals against the
snapshot. These are **supplementary reconciliation** checks against Claude
Code's own self-reported snapshot, not an independent re-count; the genuine
no-loss guarantee comes from the pre-existing per-file `agent_output` (±5%) and
cross-file-dedup checks, which now automatically cover workflow agent files
because the scanner records them as ordinary `is_agent` files that the
independent raw re-count (`count_raw_tokens`) picks up.

- **Hard check 1**: parsed tokens > 0 (the run's agents fed into totals).
- **Hard check 2**: parsed agent count == snapshot `agentCount`.
- **Informational**: the snapshot's `totalTokens` is surfaced alongside the
  parsed sum but never asserted equal — empirically it tracks
  cache-write/`cache_creation` tokens, not `input + output + cache_read`.

## Pricing fix — `claude-opus-4-8`

`pricing/calculator.rs` adds a `claude-opus-4-8` built-in entry (`$5` base
input / `$25` output — the opus-4-6 tier), and `get_price` strips a trailing
context-window suffix in square brackets before lookup:

```text
claude-opus-4-8[1m]   → claude-opus-4-8
claude-opus-4-8[200k] → claude-opus-4-8
```

The suffix is a routing affix Claude Code appends to mark the active context
window; it is not part of the priced model identity. Stripping it makes the name
resolve via the exact built-in entry (`PriceSource::Builtin`).

**The fix**: before the `claude-opus-4-8` built-in existed, the name resolved by
longest-prefix match to `claude-opus-4` (`$15` / `$75`), a ~3x overcharge. It is
now priced correctly.

## New entry subtypes (`cc-session-jsonl`)

- **`AttachmentEntry.attachment: Option<serde_json::Value>`** — in real 2.1.159+
  data the attachment content lives in a top-level `attachment` object, and its
  subtype is the nested `attachment.type` (e.g. `hook_success`, `skill_listing`,
  `file`, `task_reminder`, `queued_command`). The helper
  `AttachmentEntry::attachment_subtype()` reads that nested type. The legacy
  top-level `message` field — which never actually held this content — is kept
  for compatibility.
- **`SystemEntry`** gained 2.1.159+ fields: `content`, `is_meta`, and (only on
  `subtype = turn_duration`) `message_count` and `pending_workflow_count`
  (alongside the existing `duration_ms`). Observed subtypes include
  `turn_duration`, `local_command`, and `away_summary`. All fields are
  `Option<T>`; legacy entries parse with them `None`.

## Output surface

- **`output/json.rs`** — `SessionResult.workflows` is always emitted (possibly
  empty) on the session JSON, and on the `HtmlReportPayload` session entries. The
  payload builder reuses `build_workflow_summaries`.
- **`output/text.rs`** — a `── Workflows ──` block renders per run for
  `session` detail: name `[status]`, then `agents / turns / output / cost`
  (measured), the snapshot's reported tokens + duration (if present), and the
  declared phase titles.
- **Frontend** — the Sessions page renders a workflow drill-down block per run
  (`frontend/src/pages/Sessions.vue`), driven by the `WorkflowSummary[]` payload.

## `WorkflowSummary` payload contract

Serializes to **camelCase** (frontend data contract; see
`analysis::WorkflowSummary` and `frontend/src/types.ts`):

| Field                 | Type                | Source / meaning |
|-----------------------|---------------------|------------------|
| `runId`               | string              | workflow run id, e.g. `wf_7c0e6255-566` |
| `workflowName`        | string \| null      | snapshot `workflowName` |
| `status`              | string \| null      | snapshot `status` (`completed` / `running` / `failed`) |
| `snapshotDurationMs`  | number \| null      | snapshot `durationMs` (declared) |
| `snapshotAgentCount`  | number \| null      | snapshot `agentCount` (declared) |
| `snapshotTotalTokens` | number \| null      | snapshot `totalTokens` (declared; ≈ cache-write tokens, **not** a clean in+out total — never headline it) |
| `phases`              | `WorkflowPhase[]`   | declared phases (`title`, `detail`) |
| `parsedAgentCount`    | number              | **measured**: agent transcripts actually parsed for this run |
| `parsedTurns`         | number              | **measured**: parsed assistant turns across the run's agents |
| `parsedOutputTokens`  | number              | **measured**: output tokens summed across the run's agents |
| `parsedCost`          | number              | **measured**: USD cost charged for the run's parsed turns |

The `parsed*` fields are authoritative (they are what the tool charges into
session/overview totals); the `snapshot*` fields are best-effort declarations
from Claude Code's own metadata and may be absent.

## Known limitations

1. **Per-session workflow re-scan is O(sessions × projects).**
   `scan_session_workflows` (called per session by `build_workflow_summaries`
   and the validator) walks every project directory looking for the session's
   `<session-id>/` folder. On large installations this repeated scan can be
   optimized (e.g. a single pre-indexed pass via `scan_workflows`).
2. **Workflow agent meta is flattened across runs.**
   `load_workflow_agent_meta` collapses all `wf_*/agent-*.meta.json` sidecars for
   a session into a single map keyed by agent id (prefix stripped). If the same
   agent id ever appeared in two different runs of the same session, the map
   would collide. This affects only the `agentType` label, never token or cost
   accounting (tokens are aggregated per file via `workflow_run_id`).
