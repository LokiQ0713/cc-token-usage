use std::fmt::Write as _;

use chrono::Local;
use crate::analysis::{OverviewResult, ProjectResult, SessionResult, TrendResult};
use crate::pricing::calculator::{PricingCalculator, PRICING_FETCH_DATE, PRICING_SOURCE};

// ─── Chart Colors ────────────────────────────────────────────────────────────

const COLORS: &[&str] = &[
    "#58a6ff", "#ff6b6b", "#ffd93d", "#6bcb77", "#4d96ff", "#9b59b6",
    "#e17055", "#00cec9", "#fd79a8", "#fdcb6e",
];

// ─── ReportData ──────────────────────────────────────────────────────────────

/// Bundled analysis results for one data source.
pub struct ReportData {
    pub overview: OverviewResult,
    pub projects: ProjectResult,
    pub trend: TrendResult,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Escape HTML special characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Format a number with thousands separators for display.
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

/// Format large numbers with M/B/K suffixes for compact display.
fn format_compact(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format_number(n)
    }
}

/// Format a cost value: 1234.5 -> "$1,234.50"
fn format_cost(c: f64) -> String {
    let abs = c.abs();
    let whole = abs as u64;
    let cents = ((abs - whole as f64) * 100.0).round() as u64;
    let sign = if c < 0.0 { "-" } else { "" };
    format!("{}${}.{:02}", sign, format_number(whole), cents)
}

/// Pick a color from the palette by index.
fn color(i: usize) -> &'static str {
    COLORS[i % COLORS.len()]
}

/// Shorten model name: claude-haiku-4-5-20251001 → haiku-4-5
fn short_model(name: &str) -> String {
    let s = name.strip_prefix("claude-").unwrap_or(name);
    // Remove date suffix like -20251001 or -20250929
    if s.len() > 9 {
        let last_dash = s.rfind('-').unwrap_or(s.len());
        let suffix = &s[last_dash + 1..];
        if suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_digit()) {
            return s[..last_dash].to_string();
        }
    }
    s.to_string()
}

/// Format duration in minutes to a human-readable string.
fn format_duration(minutes: f64) -> String {
    if minutes < 1.0 {
        format!("{:.0}s", minutes * 60.0)
    } else if minutes < 60.0 {
        format!("{:.0}m", minutes)
    } else {
        let h = (minutes / 60.0).floor();
        let m = (minutes % 60.0).round();
        format!("{:.0}h{:.0}m", h, m)
    }
}

// ─── CSS ─────────────────────────────────────────────────────────────────────

fn css() -> &'static str {
    r#"
* { box-sizing: border-box; margin: 0; padding: 0; }
body {
  font-family: -apple-system, BlinkMacSystemFont, 'SF Pro', sans-serif;
  background: #0d1117; color: #c9d1d9;
  max-width: 1400px; margin: 0 auto; padding: 20px;
}
.card { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 16px; }
.card > h2:first-child { margin-top: 0; }
.kpi-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 12px; margin: 16px 0; }
.kpi-value { font-size: 1.6em; font-weight: 700; color: #58a6ff; line-height: 1; }
.kpi-label { font-size: 0.85em; color: #8b949e; margin-top: 4px; }
nav { display: flex; gap: 8px; margin-bottom: 20px; flex-wrap: wrap; }
nav button {
  padding: 8px 20px; border: 1px solid #30363d; border-radius: 6px;
  background: #161b22; color: #c9d1d9; cursor: pointer; font-size: 14px;
  transition: background 0.15s, border-color 0.15s;
}
nav button:hover { border-color: #58a6ff; }
nav button.active { background: #1f6feb; border-color: #1f6feb; color: #fff; }
.tab-content { display: none; }
.tab-content.active { display: block; }
h1 { color: #58a6ff; font-size: 1.5em; margin-bottom: 16px; }
h2 { color: #c9d1d9; font-size: 1.2em; margin: 16px 0 12px; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th {
  padding: 8px 10px; text-align: right; border-bottom: 2px solid #30363d;
  color: #8b949e; cursor: pointer; user-select: none; white-space: nowrap;
  position: sticky; top: 0; background: #161b22; z-index: 2;
}
th.text-left { text-align: left; }
th:hover { color: #58a6ff; }
td { padding: 6px 10px; text-align: right; border-bottom: 1px solid #21262d; }
td.text-left { text-align: left; }
tr:hover { background: #1c2128; }
.sort-asc::after { content: ' \25b2'; color: #58a6ff; }
.sort-desc::after { content: ' \25bc'; color: #58a6ff; }
.expandable { cursor: pointer; }
.session-detail { background: #0d1117; }
.session-detail td { padding: 0; }
.session-detail:hover { background: #0d1117; }
.detail-content { padding: 16px; overflow-x: auto; }
.detail-content table { font-size: 12px; }
.compact-row { background: #2d1b1b !important; }
.chart-container { position: relative; height: 350px; margin: 16px 0; }
.grid-2x2 { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
.grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
.grid-4 { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; margin: 16px 0; }
.footer { color: #484f58; font-size: 12px; margin-top: 30px; padding-top: 16px; border-top: 1px solid #21262d; }
.header-row { display: flex; align-items: baseline; gap: 16px; margin-bottom: 16px; flex-wrap: wrap; }
.subtitle { color: #8b949e; font-size: 0.85em; }
.expand-btn { background: none; border: none; color: #8b949e; cursor: pointer; font-size: 14px; padding: 2px 6px; }
.expand-btn:hover { color: #58a6ff; }
.project-session-row { background: #111822; }
.project-session-row:hover { background: #1c2128; }
.project-row { background: #161b22; font-weight: 600; }
.progress-bar { display: inline-block; width: 80px; height: 14px; background: #21262d; border-radius: 7px; overflow: hidden; vertical-align: middle; }
.progress-fill { height: 100%; border-radius: 7px; transition: width 0.3s; }
.progress-text { display: inline-block; width: 45px; text-align: right; margin-left: 4px; font-size: 12px; }
.stale-warning { color: #ff6b6b; margin-bottom: 8px; }
.top-nav { display: flex; gap: 8px; margin-bottom: 12px; }
.top-nav button { padding: 10px 24px; border: 2px solid #30363d; border-radius: 8px; background: #161b22; color: #c9d1d9; cursor: pointer; font-size: 15px; font-weight: 600; transition: background 0.15s, border-color 0.15s; }
.top-nav button:hover { border-color: #58a6ff; }
.top-nav button.active { background: #1f6feb; border-color: #1f6feb; color: #fff; }
.sub-nav { display: flex; gap: 8px; margin-bottom: 16px; }
.sub-nav button { padding: 6px 16px; border: 1px solid #30363d; border-radius: 6px; background: #161b22; color: #c9d1d9; cursor: pointer; font-size: 13px; transition: background 0.15s, border-color 0.15s; }
.sub-nav button:hover { border-color: #238636; }
.sub-nav button.active { background: #238636; border-color: #238636; color: #fff; }
.source-content { display: none; }
.source-content.active { display: block; }
.sub-tab-content { display: none; }
.sub-tab-content.active { display: block; }
.tool-tag { display: inline-block; padding: 1px 6px; margin: 1px 2px; border-radius: 4px; background: #21262d; color: #8b949e; font-size: 11px; white-space: nowrap; }
.tool-tag .tool-count { color: #58a6ff; font-weight: 600; margin-left: 2px; }
.session-tools-cell { max-width: 260px; line-height: 1.8; }
.agent-badge { display: inline-block; padding: 1px 5px; border-radius: 3px; background: #1f3a5f; color: #58a6ff; font-size: 11px; font-weight: 600; margin-left: 4px; }
.turn-count-cell { white-space: nowrap; }
.grid-1-2 { display: grid; grid-template-columns: 1fr 2fr; gap: 16px; }
.chart-container-sm { position: relative; height: 250px; margin: 12px 0; }
.model-legend { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 8px; }
.model-legend-item { display: flex; align-items: center; gap: 4px; font-size: 12px; color: #8b949e; }
.model-legend-dot { width: 10px; height: 10px; border-radius: 50%; display: inline-block; }
.data-table th { cursor: default; }
.data-table th:hover { color: #8b949e; }
.table-wrap { overflow-x: auto; -webkit-overflow-scrolling: touch; }
.glossary { color: #8b949e; font-size: 12px; margin-bottom: 16px; line-height: 1.7; padding: 12px 16px; background: #161b22; border: 1px solid #30363d; border-radius: 8px; }
.heatmap-wrap { overflow-x: auto; -webkit-overflow-scrolling: touch; }
.heatmap-wrap canvas { display: block; }
@media (max-width: 1100px) {
  .grid-4 { grid-template-columns: repeat(2, 1fr); }
}
@media (max-width: 900px) {
  .grid-2x2 { grid-template-columns: 1fr; }
  .grid-2 { grid-template-columns: 1fr; }
  .grid-1-2 { grid-template-columns: 1fr; }
  .grid-4 { grid-template-columns: repeat(2, 1fr); }
  .kpi-grid { grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); }
}
@media (max-width: 600px) {
  body { padding: 12px; }
  .grid-4 { grid-template-columns: 1fr; }
  .kpi-grid { grid-template-columns: 1fr 1fr; }
  .kpi-value { font-size: 1.2em; }
  .header-row { flex-direction: column; gap: 8px; }
  .header-row button { margin-left: 0 !important; }
}
"#
}

// ─── JavaScript ──────────────────────────────────────────────────────────────

fn js_common() -> &'static str {
    r#"
function showTab(name) {
  document.querySelectorAll('.tab-content').forEach(el => el.classList.remove('active'));
  document.querySelectorAll('nav button').forEach(el => el.classList.remove('active'));
  document.getElementById('tab-' + name).classList.add('active');
  document.querySelector('nav button[data-tab="' + name + '"]').classList.add('active');
}

function sortTable(th, tableId) {
  const table = document.getElementById(tableId);
  const tbody = table.querySelector('tbody');
  const rows = Array.from(tbody.querySelectorAll('tr:not(.session-detail)'));
  const colIndex = th.cellIndex;
  const isAsc = th.classList.contains('sort-asc');

  table.querySelectorAll('th').forEach(h => {
    h.classList.remove('sort-asc', 'sort-desc');
  });

  th.classList.add(isAsc ? 'sort-desc' : 'sort-asc');

  rows.sort((a, b) => {
    let va = a.cells[colIndex].getAttribute('data-value') || a.cells[colIndex].textContent;
    let vb = b.cells[colIndex].getAttribute('data-value') || b.cells[colIndex].textContent;
    const na = parseFloat(va.replace(/[\$,%]/g, ''));
    const nb = parseFloat(vb.replace(/[\$,%]/g, ''));
    if (!isNaN(na) && !isNaN(nb)) {
      return isAsc ? nb - na : na - nb;
    }
    return isAsc ? vb.localeCompare(va) : va.localeCompare(vb);
  });

  rows.forEach(row => {
    const detail = row.nextElementSibling;
    tbody.appendChild(row);
    if (detail && detail.classList.contains('session-detail')) {
      tbody.appendChild(detail);
    }
  });
}

function toggleSession(btn) {
  const row = btn.closest('tr');
  const detail = row.nextElementSibling;
  if (detail && detail.classList.contains('session-detail')) {
    const isHidden = detail.style.display === 'none';
    detail.style.display = isHidden ? 'table-row' : 'none';
    btn.textContent = isHidden ? '\u25bc' : '\u25b6';
  }
}

function toggleProject(btn, projectId) {
  const sessionRows = document.querySelectorAll('.project-session-row.project-sessions-' + projectId);
  const detailRows = document.querySelectorAll('.session-detail.project-sessions-' + projectId);
  const isHidden = sessionRows.length > 0 && sessionRows[0].style.display === 'none';

  if (isHidden) {
    // Expand: show session rows only (not turn details)
    sessionRows.forEach(r => r.style.display = 'table-row');
  } else {
    // Collapse: hide session rows AND any open turn details
    sessionRows.forEach(r => {
      r.style.display = 'none';
      const sbtn = r.querySelector('.expand-btn');
      if (sbtn) sbtn.textContent = '\u25b6';
    });
    detailRows.forEach(r => r.style.display = 'none');
  }
  btn.textContent = isHidden ? '\u25bc' : '\u25b6';
}

// Heatmap data is already in local timezone (converted in Rust).
// No JS-side timezone shift needed.

function drawHeatmap(canvasId, data) {
  const canvas = document.getElementById(canvasId);
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const localData = data; // already local timezone from Rust
  const zhDays = ['周一','周二','周三','周四','周五','周六','周日'];
  const enDays = ['Mon','Tue','Wed','Thu','Fri','Sat','Sun'];
  const days = (currentLang === 'zh') ? zhDays : enDays;
  const cellW = 28, cellH = 28, padL = 44, padT = 30;
  canvas.width = padL + 24 * cellW + 10;
  canvas.height = padT + 7 * cellH + 10;

  const max = Math.max(...localData.flat(), 1);

  for (let d = 0; d < 7; d++) {
    for (let h = 0; h < 24; h++) {
      const val = localData[d][h];
      const intensity = val / max;
      const r = Math.round(13 + intensity * 75);
      const g = Math.round(17 + intensity * 130);
      const b = Math.round(34 + intensity * 221);
      ctx.fillStyle = 'rgb(' + r + ',' + g + ',' + b + ')';
      ctx.fillRect(padL + h * cellW, padT + d * cellH, cellW - 2, cellH - 2);

      if (val > 0) {
        ctx.fillStyle = intensity > 0.6 ? '#fff' : '#8b949e';
        ctx.font = '10px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText(val, padL + h * cellW + cellW/2 - 1, padT + d * cellH + cellH/2 + 3);
      }
    }
    ctx.fillStyle = '#8b949e';
    ctx.font = '11px sans-serif';
    ctx.textAlign = 'right';
    ctx.fillText(days[d], padL - 5, padT + d * cellH + cellH/2 + 3);
  }
  ctx.textAlign = 'center';
  for (let h = 0; h < 24; h += 2) {
    ctx.fillText(h.toString().padStart(2, '0'), padL + h * cellW + cellW/2, padT - 8);
  }
}

function sortTableSimple(th, tableId) {
  const table = document.getElementById(tableId);
  const tbody = table.querySelector('tbody');
  const rows = Array.from(tbody.querySelectorAll('tr'));
  const colIndex = th.cellIndex;
  const isAsc = th.classList.contains('sort-asc');

  table.querySelectorAll('th').forEach(h => {
    h.classList.remove('sort-asc', 'sort-desc');
  });
  th.classList.add(isAsc ? 'sort-desc' : 'sort-asc');

  rows.sort((a, b) => {
    let va = a.cells[colIndex].getAttribute('data-value') || a.cells[colIndex].textContent;
    let vb = b.cells[colIndex].getAttribute('data-value') || b.cells[colIndex].textContent;
    const na = parseFloat(va.replace(/[\$,%]/g, ''));
    const nb = parseFloat(vb.replace(/[\$,%]/g, ''));
    if (!isNaN(na) && !isNaN(nb)) {
      return isAsc ? nb - na : na - nb;
    }
    return isAsc ? vb.localeCompare(va) : va.localeCompare(vb);
  });
  rows.forEach(row => tbody.appendChild(row));
}

function switchSource(sourceId) {
  document.querySelectorAll('.source-content').forEach(el => el.style.display = 'none');
  document.querySelectorAll('.top-nav button').forEach(el => el.classList.remove('active'));
  document.getElementById('source-' + sourceId).style.display = 'block';
  event.target.classList.add('active');
  // Redraw heatmap for this source
  if (window['_heatmapData_' + sourceId]) {
    drawHeatmap('heatmap-' + sourceId, window['_heatmapData_' + sourceId]);
  }
}

function showSubTab(sourceId, tabName) {
  const container = document.getElementById('source-' + sourceId);
  container.querySelectorAll('.sub-tab-content').forEach(el => el.classList.remove('active'));
  container.querySelectorAll('.sub-nav button').forEach(el => el.classList.remove('active'));
  document.getElementById(sourceId + '-tab-' + tabName).classList.add('active');
  event.target.classList.add('active');
  // Redraw heatmap when overview tab becomes visible
  if (tabName === 'overview' && window['_heatmapData_' + sourceId]) {
    drawHeatmap('heatmap-' + sourceId, window['_heatmapData_' + sourceId]);
  }
}

let currentLang = localStorage.getItem('cc-lang') || 'en';
function toggleLang() {
  currentLang = currentLang === 'en' ? 'zh' : 'en';
  localStorage.setItem('cc-lang', currentLang);
  applyLang();
}
function applyLang() {
  document.querySelectorAll('[data-en]').forEach(el => {
    el.textContent = el.getAttribute('data-' + currentLang) || el.getAttribute('data-en');
  });
  const btn = document.getElementById('lang-btn');
  if (btn) btn.textContent = currentLang === 'en' ? '中文' : 'EN';
  // Redraw heatmaps with localized day names
  for (const key of Object.keys(window)) {
    if (key.startsWith('_heatmapData_')) {
      const pfx = key.replace('_heatmapData_', '');
      drawHeatmap('heatmap-' + pfx, window[key]);
    }
  }
}
// Convert UTC timestamps to local timezone
function convertTimestamps() {
  document.querySelectorAll('[data-utc]').forEach(el => {
    const utc = el.getAttribute('data-utc');
    const d = new Date(utc);
    if (!isNaN(d)) {
      const pad = n => String(n).padStart(2, '0');
      el.textContent = pad(d.getHours()) + ':' + pad(d.getMinutes()) + ':' + pad(d.getSeconds());
      el.title = d.toLocaleString();
    }
  });
  document.querySelectorAll('[data-utc-datetime]').forEach(el => {
    const utc = el.getAttribute('data-utc-datetime');
    const d = new Date(utc);
    if (!isNaN(d)) {
      const pad = n => String(n).padStart(2, '0');
      el.textContent = pad(d.getMonth()+1) + '-' + pad(d.getDate()) + ' ' + pad(d.getHours()) + ':' + pad(d.getMinutes());
      el.title = d.toLocaleString();
    }
  });
}
document.addEventListener('DOMContentLoaded', function() { applyLang(); convertTimestamps(); });
"#
}

// ─── Source Tabs Renderer ────────────────────────────────────────────────────

/// Render sub-nav + 3 tab contents (overview, monthly, projects) for one data source.
/// All element IDs are prefixed with `pfx` to avoid conflicts in dual-source mode.
fn render_source_tabs(
    out: &mut String,
    pfx: &str,
    overview: &OverviewResult,
    projects: &ProjectResult,
    trend: &TrendResult,
    calc: &PricingCalculator,
) {
    // Sub-navigation
    writeln!(out, r#"<nav class="sub-nav">"#).unwrap();
    writeln!(out, r#"<button class="active" onclick="showSubTab('{pfx}','overview')" data-en="Overview" data-zh="概览">Overview</button>"#,
        pfx = pfx).unwrap();
    writeln!(out, r#"<button onclick="showSubTab('{pfx}','monthly')" data-en="Monthly" data-zh="月度">Monthly</button>"#,
        pfx = pfx).unwrap();
    writeln!(out, r#"<button onclick="showSubTab('{pfx}','projects')" data-en="Projects" data-zh="项目">Projects</button>"#,
        pfx = pfx).unwrap();
    writeln!(out, "</nav>").unwrap();

    // Tab 1: Overview
    writeln!(out, r#"<div id="{pfx}-tab-overview" class="sub-tab-content active">"#, pfx = pfx).unwrap();
    render_overview_tab(out, overview, pfx);
    writeln!(out, "</div>").unwrap();

    // Tab 2: Monthly
    writeln!(out, r#"<div id="{pfx}-tab-monthly" class="sub-tab-content">"#, pfx = pfx).unwrap();
    render_monthly_tab(out, overview, trend, pfx);
    writeln!(out, "</div>").unwrap();

    // Tab 3: Projects
    writeln!(out, r#"<div id="{pfx}-tab-projects" class="sub-tab-content">"#, pfx = pfx).unwrap();
    render_projects_tab(out, projects, &overview.session_summaries, pfx);
    writeln!(out, "</div>").unwrap();

    // Pricing source note
    writeln!(out, r#"<p style="color:#484f58;font-size:11px;margin-top:12px;">Price data: {} ({})</p>"#,
        PRICING_SOURCE, PRICING_FETCH_DATE).unwrap();

    let _ = calc;
}

// ─── 1. Full Report (single source) ─────────────────────────────────────────

/// Generate a comprehensive HTML dashboard with 3 tabs, charts, and sortable tables.
/// Single data source — no top-level source switcher.
pub fn render_full_report_html(
    overview: &OverviewResult,
    projects: &ProjectResult,
    trend: &TrendResult,
    calc: &PricingCalculator,
) -> String {
    let mut out = String::with_capacity(256 * 1024);

    // ── HTML head ────────────────────────────────────────────────────────────
    write!(out, r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Claude Code Token Analyzer</title>
  <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
  <style>{css}</style>
</head>
<body>
"#, css = css()).unwrap();

    // ── Header ───────────────────────────────────────────────────────────────
    writeln!(out, r#"<div class="header-row">"#).unwrap();
    writeln!(out, r#"<h1>Claude Code Token Analyzer</h1>"#).unwrap();
    if let Some((start, end)) = &overview.quality.time_range {
        writeln!(out, r#"<span class="subtitle">{} ~ {}</span>"#,
            start.format("%Y-%m-%d"), end.format("%Y-%m-%d")).unwrap();
    }
    writeln!(out, r#"<button id="lang-btn" onclick="toggleLang()" style="margin-left:auto;padding:4px 12px;border:1px solid #30363d;border-radius:4px;background:#161b22;color:#c9d1d9;cursor:pointer;font-size:13px;">中文</button>"#).unwrap();
    writeln!(out, "</div>").unwrap();

    // ── Glossary ──────────────────────────────────────────────────────────────
    writeln!(out, r#"<div class="glossary" data-en="Glossary: Turn = one Claude response (each time you send a message or Claude calls a tool, it produces one turn). Session = one conversation from start to finish. Token = the unit Claude uses to process text (~4 chars = 1 token). Context = all tokens Claude sees per request (your message + history + cached content). Cache Hit = reusing previously processed context (saves cost)." data-zh="术语说明：Turn = 一次 Claude 响应（每次你发消息或 Claude 调用工具，都算一个 turn）。Session = 一次完整对话（从开始到结束）。Token = Claude 处理文本的单位（约 4 个英文字符 = 1 token）。Context = 每次请求 Claude 看到的全部内容（你的消息 + 历史记录 + 缓存内容）。Cache Hit = 复用之前处理过的上下文（节省费用）。">Glossary: Turn = one Claude response (each time you send a message or Claude calls a tool, it produces one turn). Session = one conversation from start to finish. Token = the unit Claude uses to process text (~4 chars = 1 token). Context = all tokens Claude sees per request (your message + history + cached content). Cache Hit = reusing previously processed context (saves cost).</div>"#).unwrap();

    // ── Single source: use sub-nav directly (no top-nav) ─────────────────────
    let pfx = "s1";
    writeln!(out, r#"<div id="source-{pfx}" class="source-content active">"#, pfx = pfx).unwrap();
    render_source_tabs(&mut out, pfx, overview, projects, trend, calc);
    writeln!(out, "</div>").unwrap();

    // ── JavaScript ───────────────────────────────────────────────────────────
    write!(out, "<script>{}</script>", js_common()).unwrap();

    // ── Footer ───────────────────────────────────────────────────────────────
    render_footer(&mut out, calc);

    writeln!(out, "</body>\n</html>").unwrap();
    out
}

// ─── 1b. Dual Report (two sources) ──────────────────────────────────────────

/// Generate a dual-source HTML dashboard with top-level source switcher.
/// Each source gets its own sub-nav with 3 tabs.
pub fn render_dual_report_html(
    source1_name: &str,
    source1: &ReportData,
    source2_name: &str,
    source2: &ReportData,
    calc: &PricingCalculator,
) -> String {
    let mut out = String::with_capacity(512 * 1024);

    // ── HTML head ────────────────────────────────────────────────────────────
    write!(out, r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Claude Code Token Analyzer</title>
  <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
  <style>{css}</style>
</head>
<body>
"#, css = css()).unwrap();

    // ── Header ───────────────────────────────────────────────────────────────
    writeln!(out, r#"<div class="header-row">"#).unwrap();
    writeln!(out, r#"<h1>Claude Code Token Analyzer</h1>"#).unwrap();
    // Show combined time range
    let time_range_str = {
        let mut global_min = None;
        let mut global_max = None;
        for q in [&source1.overview.quality, &source2.overview.quality] {
            if let Some((s, e)) = &q.time_range {
                global_min = Some(global_min.map_or(*s, |m: chrono::DateTime<chrono::Utc>| m.min(*s)));
                global_max = Some(global_max.map_or(*e, |m: chrono::DateTime<chrono::Utc>| m.max(*e)));
            }
        }
        match (global_min, global_max) {
            (Some(s), Some(e)) => format!("{} ~ {}", s.format("%Y-%m-%d"), e.format("%Y-%m-%d")),
            _ => String::new(),
        }
    };
    if !time_range_str.is_empty() {
        writeln!(out, r#"<span class="subtitle">{}</span>"#, time_range_str).unwrap();
    }
    writeln!(out, r#"<button id="lang-btn" onclick="toggleLang()" style="margin-left:auto;padding:4px 12px;border:1px solid #30363d;border-radius:4px;background:#161b22;color:#c9d1d9;cursor:pointer;font-size:13px;">中文</button>"#).unwrap();
    writeln!(out, "</div>").unwrap();

    // ── Glossary ──────────────────────────────────────────────────────────────
    writeln!(out, r#"<div class="glossary" data-en="Glossary: Turn = one Claude response (each time you send a message or Claude calls a tool, it produces one turn). Session = one conversation from start to finish. Token = the unit Claude uses to process text (~4 chars = 1 token). Context = all tokens Claude sees per request (your message + history + cached content). Cache Hit = reusing previously processed context (saves cost)." data-zh="术语说明：Turn = 一次 Claude 响应（每次你发消息或 Claude 调用工具，都算一个 turn）。Session = 一次完整对话（从开始到结束）。Token = Claude 处理文本的单位（约 4 个英文字符 = 1 token）。Context = 每次请求 Claude 看到的全部内容（你的消息 + 历史记录 + 缓存内容）。Cache Hit = 复用之前处理过的上下文（节省费用）。">Glossary: Turn = one Claude response (each time you send a message or Claude calls a tool, it produces one turn). Session = one conversation from start to finish. Token = the unit Claude uses to process text (~4 chars = 1 token). Context = all tokens Claude sees per request (your message + history + cached content). Cache Hit = reusing previously processed context (saves cost).</div>"#).unwrap();

    // ── Top-level source switcher ────────────────────────────────────────────
    let s1_sessions = source1.overview.total_sessions;
    let s2_sessions = source2.overview.total_sessions;
    writeln!(out, r#"<nav class="top-nav">"#).unwrap();
    writeln!(out, r#"<button class="active" onclick="switchSource('s1')">{} ({} sessions)</button>"#,
        escape_html(source1_name), s1_sessions).unwrap();
    writeln!(out, r#"<button onclick="switchSource('s2')">{} ({} sessions)</button>"#,
        escape_html(source2_name), s2_sessions).unwrap();
    writeln!(out, "</nav>").unwrap();

    // ── Source 1 ─────────────────────────────────────────────────────────────
    writeln!(out, r#"<div id="source-s1" class="source-content active">"#).unwrap();
    render_source_tabs(&mut out, "s1", &source1.overview, &source1.projects, &source1.trend, calc);
    writeln!(out, "</div>").unwrap();

    // ── Source 2 ─────────────────────────────────────────────────────────────
    writeln!(out, r#"<div id="source-s2" class="source-content">"#).unwrap();
    render_source_tabs(&mut out, "s2", &source2.overview, &source2.projects, &source2.trend, calc);
    writeln!(out, "</div>").unwrap();

    // ── JavaScript ───────────────────────────────────────────────────────────
    write!(out, "<script>{}</script>", js_common()).unwrap();

    // ── Footer ───────────────────────────────────────────────────────────────
    render_footer(&mut out, calc);

    writeln!(out, "</body>\n</html>").unwrap();
    out
}

// ─── Tab 1: Overview ─────────────────────────────────────────────────────────

fn render_overview_tab(out: &mut String, overview: &OverviewResult, pfx: &str) {
    // KPI cards
    writeln!(out, r#"<div class="kpi-grid">"#).unwrap();
    write_kpi_i18n(out, &format_number(overview.total_sessions as u64), "Sessions", "会话数");
    write_kpi_i18n(out, &format_number(overview.total_turns as u64), "Turns", "响应数");
    write_kpi_i18n(out, &format_compact(overview.total_output_tokens), "Claude Wrote", "Claude 写了");
    write_kpi_i18n(out, &format_compact(overview.total_context_tokens), "Claude Read", "Claude 读了");
    write_kpi_progress(out, overview.avg_cache_hit_rate, "Avg Cache Hit Rate", "平均缓存命中率");
    write_kpi_i18n(out, &format_cost(overview.total_cost), "Token Value (API Rate)", "Token 价值 (API 费率)");
    if overview.cache_savings.total_saved > 0.0 {
        write_kpi_i18n(out, &format_cost(overview.cache_savings.total_saved),
            &format!("Cache Savings ({:.0}%)", overview.cache_savings.savings_pct),
            &format!("缓存节省 ({:.0}%)", overview.cache_savings.savings_pct));
    }
    writeln!(out, "</div>").unwrap();

    // Row 1: Usage Insights KPI cards
    {
        let summaries = &overview.session_summaries;

        // Daily avg cost
        let daily_avg = overview.quality.time_range.map(|(s, e)| {
            let days = (e - s).num_days().max(1) as f64;
            (overview.total_cost / days, days as u64)
        });

        // Compaction stats
        let total_compactions: usize = summaries.iter().map(|s| s.compaction_count).sum();

        // Max context
        let max_ctx = summaries.iter().map(|s| s.max_context).max().unwrap_or(0);

        // Average session duration
        let durations: Vec<f64> = summaries.iter()
            .map(|s| s.duration_minutes).filter(|d| *d > 0.0).collect();
        let avg_dur = if !durations.is_empty() { durations.iter().sum::<f64>() / durations.len() as f64 } else { 0.0 };

        writeln!(out, r#"<div class="grid-4">"#).unwrap();
        if let Some((avg, days)) = daily_avg {
            write_kpi_i18n(out,
                &format!("{}/day", format_cost(avg)),
                &format!("Daily Avg ({} days)", days),
                &format!("日均费用（{} 天）", days));
        }
        write_kpi_i18n(out,
            &format_number(max_ctx),
            "Peak Context",
            "峰值上下文");
        write_kpi_i18n(out,
            &format_number(total_compactions as u64),
            "Compactions",
            "上下文压缩次数");
        write_kpi_i18n(out,
            &format_duration(avg_dur),
            "Avg Session",
            "平均会话时长");
        writeln!(out, "</div>").unwrap();

        // Top 3 most expensive sessions
        let mut by_cost: Vec<&crate::analysis::SessionSummary> = summaries.iter().collect();
        by_cost.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
        let top3 = &by_cost[..by_cost.len().min(3)];
        if !top3.is_empty() {
            writeln!(out, r#"<div class="card" style="margin-top:16px;">"#).unwrap();
            writeln!(out, r#"<h2 data-en="Most Expensive Sessions" data-zh="最贵会话 Top 3">Most Expensive Sessions</h2>"#).unwrap();
            writeln!(out, r#"<div class="table-wrap">"#).unwrap();
            writeln!(out, r#"<table class="data-table"><thead><tr>
                <th class="text-left" data-en="Session" data-zh="会话">Session</th>
                <th class="text-left" data-en="Project" data-zh="项目">Project</th>
                <th data-en="Turns" data-zh="响应数">Turns</th>
                <th data-en="Duration" data-zh="时长">Duration</th>
                <th data-en="Cost" data-zh="费用">Cost</th>
            </tr></thead><tbody>"#).unwrap();
            for s in top3 {
                writeln!(out, "<tr><td class=\"text-left\">{}</td><td class=\"text-left\">{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    escape_html(&s.session_id[..s.session_id.len().min(8)]),
                    escape_html(&s.project_display_name),
                    s.turn_count,
                    format_duration(s.duration_minutes),
                    format_cost(s.cost),
                ).unwrap();
            }
            writeln!(out, "</tbody></table></div></div>").unwrap();
        }
    }

    // Row 2: Heatmap
    writeln!(out, r#"<div class="grid-2" style="margin-top:16px;">"#).unwrap();

    // Chart 3: Heatmap (Weekday x Hour) - now with local timezone
    {
        let canvas_id = format!("heatmap-{}", pfx);
        writeln!(out, r#"<div class="card">"#).unwrap();
        writeln!(out, r#"<h2 data-en="Activity Heatmap (Local Time)" data-zh="活跃热力图（本地时间）">Activity Heatmap (Local Time)</h2>"#).unwrap();
        writeln!(out, r#"<p style="color:#8b949e;font-size:12px;margin-bottom:8px;" data-en="Each cell = number of turns in that hour slot (local timezone). Rows = weekdays, columns = hours (00-23). Darker = more active." data-zh="每个格子 = 该时段的 turn 数量（本地时区）。行 = 星期几，列 = 小时（00-23）。颜色越深 = 越活跃。">Each cell = number of turns in that hour slot (local timezone). Rows = weekdays, columns = hours (00-23). Darker = more active.</p>"#).unwrap();
        writeln!(out, r#"<div class="heatmap-wrap"><canvas id="{}"></canvas></div>"#, canvas_id).unwrap();

        let mut matrix_js = String::from("[");
        for d in 0..7 {
            if d > 0 { matrix_js.push(','); }
            matrix_js.push('[');
            for h in 0..24 {
                if h > 0 { matrix_js.push(','); }
                write!(matrix_js, "{}", overview.weekday_hour_matrix[d][h]).unwrap();
            }
            matrix_js.push(']');
        }
        matrix_js.push(']');

        writeln!(out, r#"<script>
window._heatmapData_{pfx} = {matrix};
document.addEventListener('DOMContentLoaded', function() {{
  drawHeatmap('{canvas_id}', window._heatmapData_{pfx});
}});
</script>"#, pfx = pfx, matrix = matrix_js, canvas_id = canvas_id).unwrap();
        writeln!(out, "</div>").unwrap();
    }

    // Chart 4: Efficiency Scatter (Bubble, turns vs cost)
    {
        let chart_id = format!("{}-scatterChart", pfx);
        writeln!(out, r#"<div class="card">"#).unwrap();
        writeln!(out, r#"<h2 data-en="Session Efficiency (Turns vs Cost)" data-zh="会话效率（Turns vs 费用）">Session Efficiency (Turns vs Cost)</h2>"#).unwrap();
        writeln!(out, r#"<p style="color:#8b949e;font-size:12px;margin-bottom:8px;" data-en="Each bubble = one session. X = turns, Y = cost. Bubble size = output tokens. Top-right = expensive long sessions." data-zh="每个气泡 = 一个会话。X = turn 数，Y = 费用。气泡大小 = 输出 token。右上角 = 昂贵的长会话。">Each bubble = one session. X = turns, Y = cost. Bubble size = output tokens. Top-right = expensive long sessions.</p>"#).unwrap();
        writeln!(out, r#"<div class="chart-container"><canvas id="{}"></canvas></div>"#, chart_id).unwrap();

        let max_output: u64 = overview.session_summaries.iter().map(|s| s.output_tokens).max().unwrap_or(1);
        let mut scatter_data = String::from("[");
        for (i, s) in overview.session_summaries.iter().enumerate() {
            if i > 0 { scatter_data.push(','); }
            let radius = if max_output > 0 {
                3.0 + (s.output_tokens as f64 / max_output as f64) * 20.0
            } else { 3.0 };
            let cpt = if s.turn_count > 0 { s.cost / s.turn_count as f64 } else { 0.0 };
            write!(scatter_data, "{{x:{},y:{:.4},r:{:.1},cpt:{:.4},out:{}}}", s.turn_count, s.cost, radius, cpt, s.output_tokens).unwrap();
        }
        scatter_data.push(']');

        writeln!(out, r#"<script>
new Chart(document.getElementById('{chart_id}'), {{
  type: 'bubble',
  data: {{
    datasets: [{{
      label: 'Sessions',
      data: {data},
      backgroundColor: 'rgba(88,166,255,0.4)',
      borderColor: '#58a6ff',
      borderWidth: 1
    }}]
  }},
  options: {{
    responsive: true, maintainAspectRatio: false,
    plugins: {{
      legend: {{ display: false }},
      tooltip: {{ callbacks: {{
        label: function(ctx) {{
          const d = ctx.raw;
          return ['Turns: ' + d.x + '  Cost: $' + d.y.toFixed(2), 'Cost/Turn: $' + d.cpt.toFixed(3) + '  Output: ' + d.out.toLocaleString()];
        }}
      }} }}
    }},
    scales: {{
      x: {{ title: {{ display: true, text: 'Turn Count', color: '#8b949e' }}, ticks: {{ color: '#8b949e' }}, grid: {{ color: '#21262d' }} }},
      y: {{ title: {{ display: true, text: 'Cost ($)', color: '#8b949e' }}, ticks: {{ color: '#8b949e', callback: function(v) {{ return '$' + v; }} }}, grid: {{ color: '#21262d' }} }}
    }}
  }}
}});
</script>"#, chart_id = chart_id, data = scatter_data).unwrap();
        writeln!(out, "</div>").unwrap();
    }

    writeln!(out, "</div>").unwrap(); // close grid-2
}

// ─── Tab 2: Monthly ──────────────────────────────────────────────────────────

fn render_monthly_tab(out: &mut String, _overview: &OverviewResult, trend: &TrendResult, pfx: &str) {
    if trend.entries.is_empty() {
        writeln!(out, r#"<div class="card"><p style="color:#8b949e;">No trend data available.</p></div>"#).unwrap();
        return;
    }

    // Determine the latest month from trend entries
    let latest_month = trend.entries.last().map(|e| &e.label[..7]).unwrap_or("");

    // Aggregate current month data
    let mut month_cost = 0.0f64;
    let mut month_turns = 0usize;
    let mut month_sessions = 0usize;
    let mut month_output = 0u64;

    let mut daily_entries: Vec<&crate::analysis::TrendEntry> = Vec::new();

    for entry in &trend.entries {
        if entry.label.starts_with(latest_month) {
            month_cost += entry.cost;
            month_turns += entry.turn_count;
            month_sessions += entry.session_count;
            month_output += entry.tokens.output_tokens;
            daily_entries.push(entry);
        }
    }

    let avg_cost_per_turn = if month_turns > 0 { month_cost / month_turns as f64 } else { 0.0 };

    // KPI cards for current month
    writeln!(out, r#"<h2 data-en="Current Period: {m}" data-zh="当前周期：{m}">Current Period: {m}</h2>"#, m = escape_html(latest_month)).unwrap();
    writeln!(out, r#"<div class="kpi-grid">"#).unwrap();
    write_kpi_i18n(out, &format_number(month_sessions as u64), "Sessions", "会话数");
    write_kpi_i18n(out, &format_number(month_turns as u64), "Turns", "响应数");
    write_kpi_i18n(out, &format_compact(month_output), "Output Tokens", "输出 Token");
    write_kpi_i18n(out, &format_cost(month_cost), "Cost", "费用");
    write_kpi_i18n(out, &format!("${:.4}", avg_cost_per_turn), "Avg Cost/Turn", "平均每 Turn 费用");
    writeln!(out, "</div>").unwrap();

    // Chart: Daily Cost + Cost/Turn combo chart
    if !daily_entries.is_empty() {
        let chart_id = format!("{}-dailyCostChart", pfx);
        writeln!(out, r#"<div class="card">"#).unwrap();
        writeln!(out, r#"<h2 data-en="Daily Cost &amp; Cost/Turn ({})" data-zh="每日费用 &amp; 每 Turn 费用 ({})">Daily Cost &amp; Cost/Turn ({})</h2>"#,
            escape_html(latest_month), escape_html(latest_month), escape_html(latest_month)).unwrap();
        writeln!(out, r#"<div class="chart-container"><canvas id="{}"></canvas></div>"#, chart_id).unwrap();

        let labels: Vec<String> = daily_entries.iter().map(|e| format!("\"{}\"", &e.label[5..])).collect();
        let cost_data: Vec<String> = daily_entries.iter().map(|e| format!("{:.2}", e.cost)).collect();
        let cpt_data: Vec<String> = daily_entries.iter().map(|e| {
            if e.turn_count > 0 { format!("{:.4}", e.cost / e.turn_count as f64) } else { "0".to_string() }
        }).collect();
        let turn_data: Vec<String> = daily_entries.iter().map(|e| e.turn_count.to_string()).collect();

        writeln!(out, r#"<script>
new Chart(document.getElementById('{chart_id}'), {{
  type: 'bar',
  data: {{
    labels: [{labels}],
    datasets: [
      {{
        label: 'Cost ($)',
        data: [{cost_data}],
        backgroundColor: 'rgba(88,166,255,0.6)',
        borderColor: '#58a6ff',
        borderWidth: 1,
        borderRadius: 4,
        yAxisID: 'y',
        order: 2
      }},
      {{
        label: 'Cost/Turn ($)',
        data: [{cpt_data}],
        type: 'line',
        borderColor: '#ffd93d',
        backgroundColor: 'rgba(255,217,61,0.1)',
        pointRadius: 3,
        tension: 0.3,
        yAxisID: 'y1',
        order: 1
      }}
    ]
  }},
  options: {{
    responsive: true, maintainAspectRatio: false,
    plugins: {{
      legend: {{ labels: {{ color: '#c9d1d9' }} }},
      tooltip: {{ callbacks: {{
        afterLabel: function(ctx) {{
          const turns = [{turn_data}];
          return 'Turns: ' + turns[ctx.dataIndex];
        }}
      }} }}
    }},
    scales: {{
      x: {{ ticks: {{ color: '#8b949e' }}, grid: {{ color: '#21262d' }} }},
      y: {{ position: 'left', ticks: {{ color: '#8b949e', callback: function(v) {{ return '$' + v; }} }}, grid: {{ color: '#21262d' }}, title: {{ display: true, text: 'Cost ($)', color: '#8b949e' }} }},
      y1: {{ position: 'right', ticks: {{ color: '#ffd93d', callback: function(v) {{ return '$' + v.toFixed(3); }} }}, grid: {{ drawOnChartArea: false }}, title: {{ display: true, text: 'Cost/Turn ($)', color: '#ffd93d' }} }}
    }}
  }}
}});
</script>"#, chart_id = chart_id,
            labels = labels.join(","),
            cost_data = cost_data.join(","),
            cpt_data = cpt_data.join(","),
            turn_data = turn_data.join(","),
        ).unwrap();
        writeln!(out, "</div>").unwrap();
    }

    // Chart: Model distribution per day (stacked bar)
    {
        // Collect all unique model names
        let mut all_models: Vec<String> = Vec::new();
        for entry in &daily_entries {
            for model_name in entry.models.keys() {
                let short = short_model(model_name);
                if !all_models.contains(&short) {
                    all_models.push(short);
                }
            }
        }
        all_models.sort();

        if all_models.len() > 1 || (!all_models.is_empty() && daily_entries.len() > 1) {
            let chart_id = format!("{}-modelDistChart", pfx);
            writeln!(out, r#"<div class="card" style="margin-top:16px;">"#).unwrap();
            writeln!(out, r#"<h2 data-en="Model Usage per Day (Output Tokens)" data-zh="每日模型使用（输出 Token）">Model Usage per Day (Output Tokens)</h2>"#).unwrap();
            writeln!(out, r#"<div class="chart-container"><canvas id="{}"></canvas></div>"#, chart_id).unwrap();

            let labels: Vec<String> = daily_entries.iter().map(|e| format!("\"{}\"", &e.label[5..])).collect();

            let mut datasets = String::new();
            for (mi, model_short) in all_models.iter().enumerate() {
                if mi > 0 { datasets.push(','); }
                let values: Vec<String> = daily_entries.iter().map(|e| {
                    // Sum output tokens for all variants of this short model name
                    let total: u64 = e.models.iter()
                        .filter(|(k, _)| short_model(k) == *model_short)
                        .map(|(_, v)| *v)
                        .sum();
                    total.to_string()
                }).collect();
                write!(datasets, "{{label:\"{}\",data:[{}],backgroundColor:\"{}\",borderWidth:0,borderRadius:2}}",
                    escape_html(model_short), values.join(","), color(mi)).unwrap();
            }

            writeln!(out, r#"<script>
new Chart(document.getElementById('{chart_id}'), {{
  type: 'bar',
  data: {{
    labels: [{labels}],
    datasets: [{datasets}]
  }},
  options: {{
    responsive: true, maintainAspectRatio: false,
    plugins: {{ legend: {{ labels: {{ color: '#c9d1d9' }} }} }},
    scales: {{
      x: {{ stacked: true, ticks: {{ color: '#8b949e' }}, grid: {{ color: '#21262d' }} }},
      y: {{ stacked: true, ticks: {{ color: '#8b949e' }}, grid: {{ color: '#21262d' }}, title: {{ display: true, text: 'Output Tokens', color: '#8b949e' }} }}
    }}
  }}
}});
</script>"#, chart_id = chart_id, labels = labels.join(","), datasets = datasets).unwrap();
            writeln!(out, "</div>").unwrap();
        }
    }

    // Table: Monthly summary (aggregate by month if multi-month data)
    {
        // Group trend entries by month
        let mut months: std::collections::BTreeMap<String, (usize, usize, u64, u64, u64, f64)> = std::collections::BTreeMap::new();
        for entry in &trend.entries {
            let month_key = entry.label[..7].to_string();
            let e = months.entry(month_key).or_insert((0, 0, 0, 0, 0, 0.0));
            e.0 += entry.session_count;
            e.1 += entry.turn_count;
            e.2 += entry.tokens.output_tokens;
            e.3 += entry.tokens.cache_creation_tokens;
            e.4 += entry.tokens.cache_read_tokens;
            e.5 += entry.cost;
        }

        if months.len() > 1 {
            let tbl_id = format!("{}-tbl-monthly", pfx);
            writeln!(out, r#"<div class="card" style="margin-top:16px;">"#).unwrap();
            writeln!(out, r#"<h2 data-en="Monthly Summary" data-zh="月度汇总">Monthly Summary</h2>"#).unwrap();
            writeln!(out, r#"<div class="table-wrap">"#).unwrap();
            writeln!(out, r#"<table id="{}">"#, tbl_id).unwrap();
            writeln!(out, "<thead><tr>\
                <th class=\"text-left\" onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Month\" data-zh=\"月份\">Month</th>\
                <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Sessions\" data-zh=\"会话\">Sessions</th>\
                <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Turns\" data-zh=\"响应数\">Turns</th>\
                <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Output Tokens\" data-zh=\"输出 Token\">Output Tokens</th>\
                <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Cost/Turn\" data-zh=\"每 Turn 费用\">Cost/Turn</th>\
                <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Cost\" data-zh=\"费用\">Cost</th>\
            </tr></thead>", id = tbl_id).unwrap();
            writeln!(out, "<tbody>").unwrap();

            for (month, (sessions, turns, output, _cache_write, _cache_read, cost)) in &months {
                let cpt = if *turns > 0 { cost / *turns as f64 } else { 0.0 };
                writeln!(out, "<tr>\
                    <td class=\"text-left\" data-value=\"{}\">{}</td>\
                    <td data-value=\"{}\">{}</td>\
                    <td data-value=\"{}\">{}</td>\
                    <td data-value=\"{}\">{}</td>\
                    <td data-value=\"{:.6}\">${:.4}</td>\
                    <td data-value=\"{:.4}\">{}</td>\
                </tr>",
                    escape_html(month), escape_html(month),
                    sessions, format_number(*sessions as u64),
                    turns, format_number(*turns as u64),
                    output, format_compact(*output),
                    cpt, cpt,
                    cost, format_cost(*cost),
                ).unwrap();
            }

            writeln!(out, "</tbody></table></div></div>").unwrap();
        }
    }

    // Table: Daily detail with cost/turn
    {
        let tbl_id = format!("{}-tbl-daily", pfx);
        let group_zh = match trend.group_label.as_str() {
            "Day" => "每日",
            "Week" => "每周",
            "Month" => "每月",
            other => other,
        };
        let group_col_zh = match trend.group_label.as_str() {
            "Day" => "日期",
            "Week" => "周",
            "Month" => "月份",
            other => other,
        };
        writeln!(out, r#"<div class="card" style="margin-top:16px;">"#).unwrap();
        writeln!(out, r#"<h2 data-en="{} Breakdown" data-zh="{}明细">{} Breakdown</h2>"#,
            escape_html(&trend.group_label), escape_html(group_zh), escape_html(&trend.group_label)).unwrap();
        writeln!(out, r#"<div class="table-wrap">"#).unwrap();
        writeln!(out, r#"<table id="{}">"#, tbl_id).unwrap();
        writeln!(out, "<thead><tr>\
            <th class=\"text-left\" onclick=\"sortTableSimple(this,'{id}')\" data-en=\"{en}\" data-zh=\"{zh}\">{en}</th>\
            <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Sessions\" data-zh=\"会话\">Sessions</th>\
            <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Turns\" data-zh=\"响应数\">Turns</th>\
            <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Input Tokens\" data-zh=\"输入 Token\">Input Tokens</th>\
            <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Output Tokens\" data-zh=\"输出 Token\">Output Tokens</th>\
            <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Cost\" data-zh=\"费用\">Cost</th>\
            <th class=\"text-left\" data-en=\"Models\" data-zh=\"模型\">Models</th>\
        </tr></thead>", en = escape_html(&trend.group_label), zh = escape_html(group_col_zh), id = tbl_id).unwrap();
        writeln!(out, "<tbody>").unwrap();

        for entry in &trend.entries {
            let input_tokens = entry.tokens.input_tokens + entry.tokens.cache_creation_tokens + entry.tokens.cache_read_tokens;
            // Model summary for this day
            let mut model_list: Vec<(&String, &u64)> = entry.models.iter().collect();
            model_list.sort_by(|a, b| b.1.cmp(a.1));
            let models_html: String = model_list.iter().take(3).map(|(m, tokens)| {
                format!("<span class=\"tool-tag\">{} <span class=\"tool-count\">{}</span></span>",
                    escape_html(&short_model(m)), format_compact(**tokens))
            }).collect::<Vec<_>>().join("");

            writeln!(out, "<tr>\
                <td class=\"text-left\" data-value=\"{}\">{}</td>\
                <td data-value=\"{}\">{}</td>\
                <td data-value=\"{}\">{}</td>\
                <td data-value=\"{}\">{}</td>\
                <td data-value=\"{}\">{}</td>\
                <td data-value=\"{:.4}\">{}</td>\
                <td class=\"text-left\">{}</td>\
            </tr>",
                escape_html(&entry.label), escape_html(&entry.label),
                entry.session_count, format_number(entry.session_count as u64),
                entry.turn_count, format_number(entry.turn_count as u64),
                input_tokens, format_compact(input_tokens),
                entry.tokens.output_tokens, format_compact(entry.tokens.output_tokens),
                entry.cost, format_cost(entry.cost),
                models_html,
            ).unwrap();
        }

        writeln!(out, "</tbody></table></div></div>").unwrap();
    }
}

// ─── Tab 3: Projects ─────────────────────────────────────────────────────────

fn render_projects_tab(out: &mut String, projects: &ProjectResult, sessions: &[crate::analysis::SessionSummary], pfx: &str) {
    // Chart: Project Cost Top 10
    {
        let top_n = projects.projects.iter().take(10).collect::<Vec<_>>();
        if !top_n.is_empty() {
            let chart_id = format!("{}-projectCostChart", pfx);
            writeln!(out, r#"<div class="card">"#).unwrap();
            writeln!(out, r#"<h2 data-en="Project Cost Top 10" data-zh="项目费用 Top 10">Project Cost Top 10</h2>"#).unwrap();
            writeln!(out, r#"<div class="chart-container"><canvas id="{}"></canvas></div>"#, chart_id).unwrap();

            let labels: Vec<String> = top_n.iter().map(|p| format!("\"{}\"", escape_html(&p.display_name))).collect();
            let data: Vec<String> = top_n.iter().map(|p| format!("{:.2}", p.cost)).collect();
            let colors_list: Vec<String> = (0..top_n.len()).map(|i| format!("\"{}\"", color(i))).collect();

            writeln!(out, r#"<script>
new Chart(document.getElementById('{chart_id}'), {{
  type: 'bar',
  data: {{
    labels: [{labels}],
    datasets: [{{ label: 'Cost ($)', data: [{data}], backgroundColor: [{colors}], borderWidth: 0, borderRadius: 4 }}]
  }},
  options: {{
    indexAxis: 'y', responsive: true, maintainAspectRatio: false,
    plugins: {{ legend: {{ display: false }}, tooltip: {{ callbacks: {{ label: function(ctx) {{ return '$' + ctx.raw.toFixed(2); }} }} }} }},
    scales: {{
      x: {{ ticks: {{ color: '#8b949e', callback: function(v) {{ return '$' + v; }} }}, grid: {{ color: '#21262d' }} }},
      y: {{ ticks: {{ color: '#c9d1d9' }}, grid: {{ color: '#21262d' }} }}
    }}
  }}
}});
</script>"#, chart_id = chart_id, labels = labels.join(","), data = data.join(","), colors = colors_list.join(",")).unwrap();
            writeln!(out, "</div>").unwrap();
        }
    }

    // Three-level drill-down table: Project → Session → Turn
    let tbl_id = format!("{}-tbl-projects-drill", pfx);
    writeln!(out, r#"<div class="card" style="margin-top:16px;">"#).unwrap();
    writeln!(out, r#"<h2 data-en="Project Drill-Down" data-zh="项目钻取">Project Drill-Down</h2>"#).unwrap();
    writeln!(out, r#"<div class="table-wrap">"#).unwrap();
    writeln!(out, r#"<table id="{}">"#, tbl_id).unwrap();
    writeln!(out, "<thead><tr>\
        <th class=\"text-left\"></th>\
        <th class=\"text-left\" data-en=\"Project / Session\" data-zh=\"项目 / 会话\">Project / Session</th>\
        <th data-en=\"Sessions\" data-zh=\"会话\">Sessions</th>\
        <th data-en=\"Turns (Agent)\" data-zh=\"响应数 (Agent)\">Turns (Agent)</th>\
        <th data-en=\"Output\" data-zh=\"输出\">Output</th>\
        <th data-en=\"CacheHit\" data-zh=\"缓存命中率\">CacheHit</th>\
        <th class=\"text-left\" data-en=\"Tools\" data-zh=\"工具\">Tools</th>\
        <th data-en=\"Cost\" data-zh=\"费用\">Cost</th>\
    </tr></thead>").unwrap();
    writeln!(out, "<tbody>").unwrap();

    // Group sessions by project_display_name
    let mut sessions_by_project: std::collections::HashMap<String, Vec<&crate::analysis::SessionSummary>> = std::collections::HashMap::new();
    for s in sessions {
        sessions_by_project.entry(s.project_display_name.clone()).or_default().push(s);
    }

    for (i, proj) in projects.projects.iter().enumerate() {
        let cache_hit = if proj.tokens.context_tokens() > 0 {
            proj.tokens.cache_read_tokens as f64 / proj.tokens.context_tokens() as f64 * 100.0
        } else { 0.0 };
        let pid = format!("{}-p{}", pfx, i);

        // Level 1: Project row (expandable)
        let hit_bar = html_progress(cache_hit);
        let turns_display = if proj.agent_turns > 0 {
            format!("{} <span class=\"agent-badge\">+{} agent</span>",
                format_number(proj.total_turns as u64), proj.agent_turns)
        } else {
            format_number(proj.total_turns as u64)
        };
        writeln!(out, r#"<tr class="project-row expandable">"#).unwrap();
        writeln!(out, r#"<td class="text-left"><button class="expand-btn" onclick="toggleProject(this,'{pid}')">{arrow}</button></td>"#,
            pid = pid, arrow = "\u{25b6}").unwrap();
        writeln!(out, "\
            <td class=\"text-left\"><strong>{name}</strong></td>\
            <td data-value=\"{sess}\">{sess_fmt}</td>\
            <td class=\"turn-count-cell\" data-value=\"{turns}\">{turns_display}</td>\
            <td data-value=\"{out}\">{out_fmt}</td>\
            <td data-value=\"{hit:.1}\">{hit_bar}</td>\
            <td class=\"text-left\"></td>\
            <td data-value=\"{cost:.4}\">{cost_fmt}</td>",
            name = escape_html(&proj.display_name),
            sess = proj.session_count, sess_fmt = format_number(proj.session_count as u64),
            turns = proj.total_turns, turns_display = turns_display,
            out = proj.tokens.output_tokens, out_fmt = format_compact(proj.tokens.output_tokens),
            hit = cache_hit, hit_bar = hit_bar,
            cost = proj.cost, cost_fmt = format_cost(proj.cost),
        ).unwrap();
        writeln!(out, "</tr>").unwrap();

        // Level 2: Session rows (hidden by default, belong to this project)
        if let Some(proj_sessions) = sessions_by_project.get(&proj.display_name) {
            let mut sorted = proj_sessions.clone();
            sorted.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));

            for s in sorted.iter().filter(|s| s.turn_count > 0) {
                let utc_iso = s.first_timestamp.map(|t| t.to_rfc3339()).unwrap_or_default();
                let date_fallback = s.first_timestamp.map(|t| t.format("%m-%d %H:%M").to_string()).unwrap_or_default();
                let s_hit = html_progress(s.cache_hit_rate);

                // Session summary row
                writeln!(out, r#"<tr class="project-session-row project-sessions-{pid} expandable" style="display:none">"#,
                    pid = pid).unwrap();

                let has_detail = s.turn_details.is_some();
                if has_detail {
                    writeln!(out, r#"<td class="text-left"><button class="expand-btn" onclick="toggleSession(this)">{}</button></td>"#, "\u{25b6}").unwrap();
                } else {
                    writeln!(out, r#"<td class="text-left"></td>"#).unwrap();
                }

                // Turns with agent badge
                let s_turns_display = if s.agent_turn_count > 0 {
                    format!("{} <span class=\"agent-badge\">+{} agent</span>",
                        format_number(s.turn_count as u64), s.agent_turn_count)
                } else {
                    format_number(s.turn_count as u64)
                };

                // Top tools as tags
                let tools_html: String = s.top_tools.iter().take(5).map(|(name, count)| {
                    format!("<span class=\"tool-tag\">{} <span class=\"tool-count\">{}</span></span>",
                        escape_html(name), count)
                }).collect::<Vec<_>>().join("");

                let duration_str = format_duration(s.duration_minutes);
                let short_sid = &s.session_id[..s.session_id.len().min(10)];

                writeln!(out, "\
                    <td class=\"text-left\" style=\"padding-left:30px;\">{sid} <span style=\"color:#8b949e;font-size:11px;\">(<span data-utc-datetime=\"{utc}\">{date}</span> &middot; {dur})</span></td>\
                    <td></td>\
                    <td class=\"turn-count-cell\" data-value=\"{turns}\">{turns_display}</td>\
                    <td data-value=\"{out}\">{out_fmt}</td>\
                    <td data-value=\"{hit:.1}\">{hit_bar}</td>\
                    <td class=\"text-left session-tools-cell\">{tools}</td>\
                    <td data-value=\"{cost:.4}\">{cost_fmt}</td>",
                    sid = escape_html(short_sid),
                    utc = utc_iso,
                    date = date_fallback,
                    dur = duration_str,
                    turns = s.turn_count, turns_display = s_turns_display,
                    out = s.output_tokens, out_fmt = format_compact(s.output_tokens),
                    hit = s.cache_hit_rate, hit_bar = s_hit,
                    tools = tools_html,
                    cost = s.cost, cost_fmt = format_cost(s.cost),
                ).unwrap();
                writeln!(out, "</tr>").unwrap();

                // Level 3: Turn detail (hidden, shown when session is expanded)
                if let Some(ref details) = s.turn_details {
                    writeln!(out, r#"<tr class="session-detail project-sessions-{pid}" style="display:none"><td colspan="8"><div class="detail-content">"#,
                        pid = pid).unwrap();
                    render_turn_detail_table(out, details, &format!("{}-detail-proj-{}", pfx, escape_html(&s.session_id)));
                    writeln!(out, "</div></td></tr>").unwrap();
                }
            }
        }
    }

    writeln!(out, "</tbody></table></div></div>").unwrap();
}

// ─── Turn Detail Sub-table ───────────────────────────────────────────────────

fn render_turn_detail_table(out: &mut String, turns: &[crate::analysis::TurnDetail], table_id: &str) {
    render_turn_table_impl(out, turns, table_id);
}

// ─── Footer ──────────────────────────────────────────────────────────────────

fn render_footer(out: &mut String, calc: &PricingCalculator) {
    let stale_warning = if PricingCalculator::is_pricing_stale() {
        format!(r#"<p class="stale-warning">Warning: Price data is {} days old, costs may be inaccurate!</p>"#,
            PricingCalculator::pricing_age_days())
    } else { String::new() };
    let _ = calc;

    let now_local = Local::now().format("%Y-%m-%d %H:%M");
    writeln!(out, r#"<div class="footer">
  {}
  <p>Price data: {} ({}) | Generated by cc-token-analyzer at {}</p>
</div>"#, stale_warning, PRICING_SOURCE, PRICING_FETCH_DATE, now_local).unwrap();
}

// ─── KPI Card Helper ─────────────────────────────────────────────────────────

fn write_kpi(out: &mut String, value: &str, label: &str) {
    writeln!(out, r#"<div class="card" style="text-align:center;"><div class="kpi-value">{}</div><div class="kpi-label">{}</div></div>"#,
        value, label).unwrap();
}

/// KPI card with bilingual label.
fn write_kpi_i18n(out: &mut String, value: &str, en: &str, zh: &str) {
    writeln!(out, r#"<div class="card" style="text-align:center;"><div class="kpi-value">{}</div><div class="kpi-label" data-en="{}" data-zh="{}">{}</div></div>"#,
        value, en, zh, en).unwrap();
}

/// KPI card with a progress bar for percentage values, bilingual label.
fn write_kpi_progress(out: &mut String, pct: f64, en: &str, zh: &str) {
    let bar_color = if pct >= 90.0 { "#6bcb77" } else if pct >= 70.0 { "#ffd93d" } else { "#ff6b6b" };
    writeln!(out, r#"<div class="card" style="text-align:center;">
        <div class="kpi-value">{:.1}%</div>
        <div style="margin:4px auto;width:120px;"><div class="progress-bar" style="width:120px;">
            <div class="progress-fill" style="width:{:.1}%;background:{};"></div>
        </div></div>
        <div class="kpi-label" data-en="{}" data-zh="{}">{}</div>
    </div>"#, pct, pct, bar_color, en, zh, en).unwrap();
}

/// Render a progress bar inline for table cells.
fn html_progress(pct: f64) -> String {
    let bar_color = if pct >= 90.0 { "#6bcb77" } else if pct >= 70.0 { "#ffd93d" } else { "#ff6b6b" };
    format!(r#"<div class="progress-bar"><div class="progress-fill" style="width:{:.1}%;background:{};"></div></div><span class="progress-text">{:.1}%</span>"#,
        pct, bar_color, pct)
}

// ─── 2. Session Report ───────────────────────────────────────────────────────

/// Generate a detailed HTML report for a single session.
pub fn render_session_html(result: &SessionResult) -> String {
    let mut out = String::with_capacity(64 * 1024);

    let short_id = &result.session_id[..result.session_id.len().min(12)];

    // ── HTML head ────────────────────────────────────────────────────────────
    write!(out, r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Session {short_id} - Claude Code Token Analyzer</title>
  <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
  <style>{css}</style>
</head>
<body>
"#, short_id = escape_html(short_id), css = css()).unwrap();

    // Header
    writeln!(out, r#"<div class="header-row">"#).unwrap();
    writeln!(out, "<h1>Session Analysis</h1>").unwrap();
    writeln!(out, r#"<span class="subtitle">{} &middot; {}</span>"#,
        escape_html(&result.session_id), escape_html(&result.project)).unwrap();
    writeln!(out, "</div>").unwrap();

    // ── KPI cards ────────────────────────────────────────────────────────────
    let cache_hit_rate = {
        let total_ctx = result.total_tokens.context_tokens();
        if total_ctx > 0 {
            result.total_tokens.cache_read_tokens as f64 / total_ctx as f64 * 100.0
        } else { 0.0 }
    };

    writeln!(out, r#"<div class="kpi-grid">"#).unwrap();
    write_kpi(&mut out, &format_duration(result.duration_minutes), "Duration");
    write_kpi(&mut out, &short_model(&result.model), "Model");
    write_kpi(&mut out, &format_number(result.max_context), "Max Context");
    write_kpi(&mut out, &format!("{:.1}%", cache_hit_rate), "Cache Hit Rate");
    write_kpi(&mut out, &format_number(result.compaction_count as u64), "Compactions");
    write_kpi(&mut out, &format_cost(result.total_cost), "Total Cost");
    writeln!(out, "</div>").unwrap();

    // ── Charts (Context Growth + Cache Hit Rate) ─────────────────────────────
    if !result.turn_details.is_empty() {
        writeln!(out, r#"<div class="grid-2">"#).unwrap();

        // Context Growth Line Chart
        {
            writeln!(out, r#"<div class="card">"#).unwrap();
            writeln!(out, "<h2>Context Growth</h2>").unwrap();
            writeln!(out, r#"<div class="chart-container"><canvas id="contextChart"></canvas></div>"#).unwrap();

            let turn_nums: Vec<String> = result.turn_details.iter().map(|t| t.turn_number.to_string()).collect();
            let ctx_sizes: Vec<String> = result.turn_details.iter().map(|t| t.context_size.to_string()).collect();
            let pr = if result.turn_details.len() > 50 { 0 } else { 3 };

            writeln!(out, r#"<script>
new Chart(document.getElementById('contextChart'), {{
  type: 'line',
  data: {{
    labels: [{turns}],
    datasets: [{{
      label: 'Context Size',
      data: [{sizes}],
      borderColor: '#58a6ff',
      backgroundColor: 'rgba(88,166,255,0.1)',
      fill: true, tension: 0.3, pointRadius: {pr}
    }}]
  }},
  options: {{
    responsive: true, maintainAspectRatio: false,
    plugins: {{ legend: {{ labels: {{ color: '#c9d1d9' }} }} }},
    scales: {{
      x: {{ title: {{ display: true, text: 'Turn', color: '#8b949e' }}, ticks: {{ color: '#8b949e' }}, grid: {{ color: '#21262d' }} }},
      y: {{ title: {{ display: true, text: 'Context Tokens', color: '#8b949e' }}, ticks: {{ color: '#8b949e' }}, grid: {{ color: '#21262d' }} }}
    }}
  }}
}});
</script>"#,
                turns = turn_nums.join(","),
                sizes = ctx_sizes.join(","),
                pr = pr,
            ).unwrap();
            writeln!(out, "</div>").unwrap();
        }

        // Cache Hit Rate Line Chart
        {
            writeln!(out, r#"<div class="card">"#).unwrap();
            writeln!(out, "<h2>Cache Hit Rate</h2>").unwrap();
            writeln!(out, r#"<div class="chart-container"><canvas id="cacheChart"></canvas></div>"#).unwrap();

            let turn_nums: Vec<String> = result.turn_details.iter().map(|t| t.turn_number.to_string()).collect();
            let cache_rates: Vec<String> = result.turn_details.iter().map(|t| format!("{:.2}", t.cache_hit_rate)).collect();
            let pr = if result.turn_details.len() > 50 { 0 } else { 3 };

            writeln!(out, r#"<script>
new Chart(document.getElementById('cacheChart'), {{
  type: 'line',
  data: {{
    labels: [{turns}],
    datasets: [{{
      label: 'Cache Hit Rate (%)',
      data: [{rates}],
      borderColor: '#ffd93d',
      backgroundColor: 'rgba(255,217,61,0.1)',
      fill: true, tension: 0.3, pointRadius: {pr}
    }}]
  }},
  options: {{
    responsive: true, maintainAspectRatio: false,
    plugins: {{ legend: {{ labels: {{ color: '#c9d1d9' }} }} }},
    scales: {{
      x: {{ title: {{ display: true, text: 'Turn', color: '#8b949e' }}, ticks: {{ color: '#8b949e' }}, grid: {{ color: '#21262d' }} }},
      y: {{ title: {{ display: true, text: 'Hit Rate (%)', color: '#8b949e' }}, ticks: {{ color: '#8b949e' }}, grid: {{ color: '#21262d' }}, min: 0, max: 100 }}
    }}
  }}
}});
</script>"#,
                turns = turn_nums.join(","),
                rates = cache_rates.join(","),
                pr = pr,
            ).unwrap();
            writeln!(out, "</div>").unwrap();
        }

        writeln!(out, "</div>").unwrap(); // close grid-2
    }

    // ── Stop Reason Doughnut ─────────────────────────────────────────────────
    if !result.stop_reason_counts.is_empty() {
        writeln!(out, r#"<div class="card" style="margin-top:16px;">"#).unwrap();
        writeln!(out, "<h2>Stop Reason Distribution</h2>").unwrap();
        writeln!(out, r#"<div class="chart-container" style="max-width:400px;margin:0 auto;"><canvas id="stopReasonChart"></canvas></div>"#).unwrap();

        let mut reasons: Vec<(&String, &usize)> = result.stop_reason_counts.iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(a.1));

        let labels: Vec<String> = reasons.iter().map(|(r, _)| format!("\"{}\"", escape_html(r))).collect();
        let data: Vec<String> = reasons.iter().map(|(_, c)| c.to_string()).collect();
        let colors_list: Vec<String> = (0..reasons.len()).map(|i| format!("\"{}\"", color(i))).collect();

        writeln!(out, r#"<script>
new Chart(document.getElementById('stopReasonChart'), {{
  type: 'doughnut',
  data: {{
    labels: [{labels}],
    datasets: [{{ data: [{data}], backgroundColor: [{colors}], borderWidth: 0 }}]
  }},
  options: {{
    responsive: true, maintainAspectRatio: false,
    plugins: {{ legend: {{ position: 'bottom', labels: {{ color: '#c9d1d9' }} }} }}
  }}
}});
</script>"#,
            labels = labels.join(","), data = data.join(","), colors = colors_list.join(",")).unwrap();
        writeln!(out, "</div>").unwrap();
    }

    // ── Turn Detail Table ────────────────────────────────────────────────────
    writeln!(out, r#"<div class="card" style="margin-top:16px;">"#).unwrap();
    writeln!(out, "<h2>Turn Details</h2>").unwrap();
    writeln!(out, r#"<div class="table-wrap">"#).unwrap();
    render_turn_table_impl(&mut out, &result.turn_details, "tbl-session-turns");
    writeln!(out, "</div></div>").unwrap();

    // ── JavaScript ───────────────────────────────────────────────────────────
    write!(out, "<script>{}</script>", js_common()).unwrap();

    // ── Footer ───────────────────────────────────────────────────────────────
    let now = Local::now().format("%Y-%m-%d %H:%M");
    writeln!(out, r#"<div class="footer">
  <p>Session: {} | Generated by cc-token-analyzer at {}</p>
</div>"#, escape_html(&result.session_id), now).unwrap();

    writeln!(out, "</body>\n</html>").unwrap();
    out
}

/// Shared turn detail table -- used by both expandable session detail and single session report.
fn render_turn_table_impl(out: &mut String, turns: &[crate::analysis::TurnDetail], table_id: &str) {
    writeln!(out, r#"<table id="{}" style="font-size:12px;">"#, table_id).unwrap();
    writeln!(out, "<thead><tr>\
        <th onclick=\"sortTableSimple(this,'{id}')\">Turn</th>\
        <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Time\" data-zh=\"时间\">Time</th>\
        <th class=\"text-left\" data-en=\"Model\" data-zh=\"模型\">Model</th>\
        <th class=\"text-left\" data-en=\"User\" data-zh=\"用户\">User</th>\
        <th class=\"text-left\" data-en=\"Assistant\" data-zh=\"助手\">Assistant</th>\
        <th class=\"text-left\" data-en=\"Tools\" data-zh=\"工具\">Tools</th>\
        <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Output\" data-zh=\"输出\">Output</th>\
        <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Context\" data-zh=\"上下文\">Context</th>\
        <th onclick=\"sortTableSimple(this,'{id}')\">Hit%</th>\
        <th onclick=\"sortTableSimple(this,'{id}')\" data-en=\"Cost\" data-zh=\"费用\">Cost</th>\
        <th class=\"text-left\">Stop</th>\
        <th class=\"text-left\">\u{26a1}</th>\
    </tr></thead>", id = table_id).unwrap();
    writeln!(out, "<tbody>").unwrap();

    for t in turns {
        let row_class = if t.is_compaction {
            " class=\"compact-row\""
        } else if t.is_agent {
            " style=\"border-left:2px solid #58a6ff;\""
        } else {
            ""
        };
        let stop = t.stop_reason.as_deref().unwrap_or("-");
        let compact_mark = if t.is_compaction { "\u{26a1}" } else if t.is_agent { "\u{1f916}" } else { "" };

        let user_text = t.user_text.as_deref().unwrap_or("");
        let user_preview = if user_text.len() > 80 {
            format!("{}...", &user_text[..user_text.floor_char_boundary(80)])
        } else {
            user_text.to_string()
        };
        let asst_text = t.assistant_text.as_deref().unwrap_or("");
        let asst_preview = if asst_text.len() > 80 {
            format!("{}...", &asst_text[..asst_text.floor_char_boundary(80)])
        } else {
            asst_text.to_string()
        };

        // Tools as tags instead of plain text
        let tools_html: String = if t.tool_names.is_empty() {
            String::new()
        } else {
            t.tool_names.iter().map(|name| {
                format!("<span class=\"tool-tag\">{}</span>", escape_html(name))
            }).collect::<Vec<_>>().join("")
        };
        let hit_bar = html_progress(t.cache_hit_rate);

        let model_short = short_model(&t.model);
        let utc_iso = t.timestamp.to_rfc3339();
        let time_fallback = t.timestamp.format("%H:%M:%S").to_string();

        writeln!(out, "<tr{cls}>\
            <td data-value=\"{turn}\">{turn}</td>\
            <td><span data-utc=\"{utc}\">{time}</span></td>\
            <td class=\"text-left\">{model}</td>\
            <td class=\"text-left\" style=\"max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;\" title=\"{user_full}\">{user}</td>\
            <td class=\"text-left\" style=\"max-width:250px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;\" title=\"{asst_full}\">{asst}</td>\
            <td class=\"text-left\" style=\"max-width:160px;line-height:1.6;\">{tools}</td>\
            <td data-value=\"{out_val}\">{out_fmt}</td>\
            <td data-value=\"{ctx_val}\">{ctx_fmt}</td>\
            <td data-value=\"{hit:.1}\">{hit_bar}</td>\
            <td data-value=\"{cost:.6}\">{cost_fmt}</td>\
            <td class=\"text-left\">{stop}</td>\
            <td class=\"text-left\">{compact}</td>\
        </tr>",
            cls = row_class,
            turn = t.turn_number,
            utc = utc_iso,
            time = time_fallback,
            model = model_short,
            user_full = escape_html(user_text),
            user = escape_html(&user_preview),
            asst_full = escape_html(asst_text),
            asst = escape_html(&asst_preview),
            tools = tools_html,
            out_val = t.output_tokens, out_fmt = format_compact(t.output_tokens),
            ctx_val = t.context_size, ctx_fmt = format_compact(t.context_size),
            hit = t.cache_hit_rate, hit_bar = hit_bar,
            cost = t.cost, cost_fmt = format_cost(t.cost),
            stop = escape_html(stop),
            compact = compact_mark,
        ).unwrap();
    }

    writeln!(out, "</tbody></table>").unwrap();
}
