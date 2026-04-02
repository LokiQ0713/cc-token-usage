<script setup lang="ts" generic="T extends Record<string, any>">
import { ref, computed } from 'vue'

// ─── Props ────────────────────────────────────────────────────────────────

export interface Column<T> {
  key: string
  label: string
  sortable?: boolean
  align?: 'left' | 'right' | 'center'
  format?: (row: T) => string | number
  hideOnNarrow?: boolean
}

const props = withDefaults(
  defineProps<{
    columns: Column<T>[]
    rows: T[]
    rowKey: string | ((row: T) => string)
    expandable?: boolean
    defaultSortKey?: string
    defaultSortDir?: 'asc' | 'desc'
    showRank?: boolean
  }>(),
  {
    expandable: false,
    defaultSortDir: 'desc',
    showRank: false,
  }
)

defineSlots<{
  expand(props: { row: T; index: number }): any
}>()

// ─── Sorting ──────────────────────────────────────────────────────────────

const sortKey = ref<string | null>(props.defaultSortKey ?? null)
const sortDir = ref<'asc' | 'desc' | null>(props.defaultSortDir)

function getRowKey(row: T): string {
  if (typeof props.rowKey === 'function') return props.rowKey(row)
  return String(row[props.rowKey])
}

function handleSort(col: Column<T>) {
  if (!col.sortable) return
  if (sortKey.value === col.key) {
    if (sortDir.value === 'desc') sortDir.value = 'asc'
    else if (sortDir.value === 'asc') {
      sortKey.value = null
      sortDir.value = null
    }
  } else {
    sortKey.value = col.key
    sortDir.value = 'desc'
  }
}

function sortIndicator(col: Column<T>): string {
  if (!col.sortable) return ''
  if (sortKey.value !== col.key) return '\u2195'
  return sortDir.value === 'asc' ? '\u2191' : '\u2193'
}

const sortedRows = computed(() => {
  if (!sortKey.value || !sortDir.value) return [...props.rows]
  const key = sortKey.value
  const dir = sortDir.value === 'asc' ? 1 : -1
  return [...props.rows].sort((a, b) => {
    const va = a[key]
    const vb = b[key]
    if (typeof va === 'number' && typeof vb === 'number') return (va - vb) * dir
    return String(va).localeCompare(String(vb)) * dir
  })
})

// ─── Expand ───────────────────────────────────────────────────────────────

const expandedKeys = ref<Set<string>>(new Set())

function toggleExpand(row: T) {
  if (!props.expandable) return
  const key = getRowKey(row)
  if (expandedKeys.value.has(key)) {
    expandedKeys.value.delete(key)
  } else {
    expandedKeys.value.add(key)
  }
  // trigger reactivity
  expandedKeys.value = new Set(expandedKeys.value)
}

function isExpanded(row: T): boolean {
  return expandedKeys.value.has(getRowKey(row))
}

// ─── Cell value ───────────────────────────────────────────────────────────

function cellValue(row: T, col: Column<T>): string | number {
  if (col.format) return col.format(row)
  return row[col.key] ?? ''
}
</script>

<template>
  <div class="data-table-wrapper">
    <table class="data-table">
      <thead>
        <tr>
          <th v-if="showRank" class="col-rank">#</th>
          <th v-if="expandable" class="col-expand"></th>
          <th
            v-for="col in columns"
            :key="col.key"
            :class="[
              'col-header',
              col.align ? `align-${col.align}` : 'align-left',
              { sortable: col.sortable, 'hide-narrow': col.hideOnNarrow },
            ]"
            @click="handleSort(col)"
          >
            <span class="header-text">{{ col.label }}</span>
            <span v-if="col.sortable" class="sort-indicator">{{ sortIndicator(col) }}</span>
          </th>
        </tr>
      </thead>
      <tbody>
        <template v-for="(row, idx) in sortedRows" :key="getRowKey(row)">
          <tr
            :class="['data-row', { expandable: expandable, expanded: isExpanded(row) }]"
            @click="toggleExpand(row)"
          >
            <td v-if="showRank" class="col-rank cell-rank">{{ idx + 1 }}</td>
            <td v-if="expandable" class="col-expand cell-chevron">
              <span class="chevron" :class="{ open: isExpanded(row) }">&#9656;</span>
            </td>
            <td
              v-for="col in columns"
              :key="col.key"
              :class="[
                col.align ? `align-${col.align}` : 'align-left',
                { 'hide-narrow': col.hideOnNarrow },
              ]"
            >
              {{ cellValue(row, col) }}
            </td>
          </tr>
          <tr v-if="expandable && isExpanded(row)" class="expand-row">
            <td :colspan="columns.length + (showRank ? 1 : 0) + 1" class="expand-cell">
              <div class="expand-content">
                <slot name="expand" :row="row" :index="idx" />
              </div>
            </td>
          </tr>
        </template>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.data-table-wrapper {
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
}

.data-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.85rem;
}

/* ─── Header ──────────────────────────────────────────────────────────────── */

.data-table thead th {
  padding: 10px 12px;
  font-weight: 600;
  font-size: 0.75rem;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  border-bottom: 1px solid var(--border-color);
  white-space: nowrap;
  user-select: none;
}

.col-header.sortable {
  cursor: pointer;
  transition: color 0.15s ease;
}

.col-header.sortable:hover {
  color: var(--text-primary);
}

.sort-indicator {
  margin-left: 4px;
  font-size: 0.7rem;
  opacity: 0.6;
}

.col-rank {
  width: 40px;
  text-align: center;
}

.col-expand {
  width: 28px;
  text-align: center;
}

/* ─── Body Rows ───────────────────────────────────────────────────────────── */

.data-table tbody td {
  padding: 10px 12px;
  color: var(--text-primary);
  border-bottom: 1px solid var(--border-color);
  white-space: nowrap;
}

.data-row.expandable {
  cursor: pointer;
  transition: background 0.12s ease;
}

.data-row.expandable:hover {
  background: var(--bg-tertiary);
}

.data-row.expanded {
  background: var(--bg-tertiary);
}

.cell-rank {
  color: var(--text-tertiary);
  font-weight: 600;
  text-align: center;
  font-size: 0.8rem;
}

.cell-chevron {
  text-align: center;
}

.chevron {
  display: inline-block;
  font-size: 0.7rem;
  color: var(--text-tertiary);
  transition: transform 0.2s ease;
}

.chevron.open {
  transform: rotate(90deg);
}

/* ─── Expand Row ──────────────────────────────────────────────────────────── */

.expand-row td {
  padding: 0;
  border-bottom: 1px solid var(--border-color);
}

.expand-cell {
  padding: 0 !important;
}

.expand-content {
  overflow: hidden;
  animation: slideDown 0.2s ease;
  padding: 12px 16px 16px;
  background: var(--bg-primary);
  border-top: 1px solid var(--border-color);
}

@keyframes slideDown {
  from {
    max-height: 0;
    opacity: 0;
  }
  to {
    max-height: 600px;
    opacity: 1;
  }
}

/* ─── Alignment ───────────────────────────────────────────────────────────── */

.align-left {
  text-align: left;
}

.align-right {
  text-align: right;
}

.align-center {
  text-align: center;
}

/* ─── Responsive ──────────────────────────────────────────────────────────── */

@media (max-width: 768px) {
  .hide-narrow {
    display: none;
  }
}
</style>
