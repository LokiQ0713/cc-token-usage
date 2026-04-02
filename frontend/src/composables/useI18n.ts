import { ref, computed } from 'vue'

export type Locale = 'en' | 'zh'

const STORAGE_KEY = 'cc-dashboard-locale'

const messages: Record<string, Record<Locale, string>> = {
  // Navigation
  'nav.overview': { en: 'Overview', zh: '概览' },
  'nav.trends': { en: 'Trends', zh: '趋势' },
  'nav.projects': { en: 'Projects', zh: '项目' },
  'nav.sessions': { en: 'Sessions', zh: '会话' },
  'nav.heatmap': { en: 'Heatmap', zh: '热力图' },
  'nav.wrapped': { en: 'Wrapped', zh: '年度总结' },

  // Overview KPIs
  'kpi.sessions': { en: 'Sessions', zh: '会话数' },
  'kpi.turns': { en: 'Turns', zh: '对话轮次' },
  'kpi.claude_wrote': { en: 'Claude Wrote', zh: 'Claude 输出' },
  'kpi.claude_read': { en: 'Claude Read', zh: 'Claude 读取' },
  'kpi.cache_hit': { en: 'Cache Hit Rate', zh: '缓存命中率' },
  'kpi.api_cost': { en: 'API Cost', zh: 'API 费用' },
  'kpi.daily_avg': { en: 'Daily Avg', zh: '日均费用' },
  'kpi.peak_context': { en: 'Peak Context', zh: '峰值上下文' },
  'kpi.compactions': { en: 'Compactions', zh: '压缩次数' },
  'kpi.avg_duration': { en: 'Avg Duration', zh: '平均时长' },
  'kpi.output_ratio': { en: 'Output Ratio', zh: '输出比率' },

  // Overview sections
  'overview.model_distribution': { en: 'Model Distribution', zh: '模型分布' },
  'overview.cost_composition': { en: 'Cost Composition', zh: '费用构成' },
  'overview.top_tools': { en: 'Top Tools', zh: '工具排行' },
  'overview.top_projects': { en: 'Top Projects', zh: '项目排行' },
  'overview.efficiency_metrics': { en: 'Efficiency Metrics', zh: '效率指标' },
  'overview.summary_stats': { en: 'Summary Stats', zh: '汇总统计' },
  'overview.cache_saved': { en: 'Cache saved you', zh: '缓存为你节省了' },
  'overview.reads_free': { en: 'of reads were free', zh: '的读取免费' },
  'overview.subscription': { en: 'Subscription', zh: '订阅' },
  'overview.value_multiplier': { en: 'value multiplier', zh: '倍价值' },
  'overview.agent_driven': { en: 'agent-driven', zh: '由 Agent 驱动' },
  'overview.total_cost_center': { en: 'Total Cost', zh: '总费用' },
  'overview.output_input_ratio': { en: 'output / input tokens', zh: '输出 / 输入 token' },
  'overview.dollar_per_turn': { en: '$/turn', zh: '$/轮' },

  // Overview KPI labels
  'kpi.input_tokens': { en: 'Input Tokens', zh: '输入 Token' },
  'kpi.total_cost': { en: 'Total Cost', zh: '总费用' },
  'kpi.cache_savings': { en: 'Cache Savings', zh: '缓存节省' },
  'kpi.output_tokens': { en: 'Output Tokens', zh: '输出 Token' },
  'kpi.cost_per_turn': { en: 'Cost per Turn', zh: '每轮费用' },
  'kpi.avg_output_turn': { en: 'Avg Output/Turn', zh: '平均输出/轮' },
  'kpi.tokens': { en: 'tokens', zh: 'tokens' },

  // Cost category labels
  'cost.cache_read': { en: 'Cache Read', zh: '缓存读取' },
  'cost.cache_write': { en: 'Cache Write', zh: '缓存写入' },
  'cost.output': { en: 'Output', zh: '输出' },
  'cost.input': { en: 'Input', zh: '输入' },

  // Summary stats
  'summary.daily_avg_cost': { en: 'Daily Avg Cost', zh: '日均费用' },
  'summary.compactions': { en: 'Total Compactions', zh: '总压缩次数' },
  'summary.peak_context': { en: 'Peak Context', zh: '峰值上下文' },
  'summary.avg_duration': { en: 'Avg Session Duration', zh: '平均会话时长' },
  'summary.most_expensive': { en: 'Most Expensive Session', zh: '最贵会话' },

  // Heatmap
  'heatmap.title': { en: 'Activity Heatmap', zh: '活跃热力图' },
  'heatmap.metric_turns': { en: 'Turns', zh: '轮次' },
  'heatmap.metric_cost': { en: 'Cost', zh: '费用' },
  'heatmap.metric_sessions': { en: 'Sessions', zh: '会话' },
  'heatmap.legend_less': { en: 'Less', zh: '少' },
  'heatmap.legend_more': { en: 'More', zh: '多' },
  'heatmap.tooltip_date': { en: 'Date', zh: '日期' },
  'heatmap.tooltip_turns': { en: 'Turns', zh: '轮次' },
  'heatmap.tooltip_cost': { en: 'Cost', zh: '费用' },
  'heatmap.tooltip_sessions': { en: 'Sessions', zh: '会话' },
  'heatmap.active_days': { en: 'Active Days', zh: '活跃天数' },
  'heatmap.current_streak': { en: 'Current Streak', zh: '当前连续' },
  'heatmap.longest_streak': { en: 'Longest Streak', zh: '最长连续' },
  'heatmap.busiest_day': { en: 'Busiest Day', zh: '最忙碌日' },
  'heatmap.days': { en: 'days', zh: '天' },
  'heatmap.no_activity': { en: 'No activity', zh: '无活动' },
  'heatmap.hour_distribution': { en: 'Weekday x Hour Distribution', zh: '星期 x 小时分布' },
  'heatmap.weekday_mon': { en: 'Mon', zh: '一' },
  'heatmap.weekday_tue': { en: 'Tue', zh: '二' },
  'heatmap.weekday_wed': { en: 'Wed', zh: '三' },
  'heatmap.weekday_thu': { en: 'Thu', zh: '四' },
  'heatmap.weekday_fri': { en: 'Fri', zh: '五' },
  'heatmap.weekday_sat': { en: 'Sat', zh: '六' },
  'heatmap.weekday_sun': { en: 'Sun', zh: '日' },
  'heatmap.stats': { en: 'Statistics', zh: '统计' },
  'heatmap.contributions_in_range': { en: 'contributions in the last year', zh: '次贡献（过去一年）' },

  // Projects page
  'projects.kpi_total_projects': { en: 'Total Projects', zh: '项目总数' },
  'projects.kpi_total_cost': { en: 'Total Cost', zh: '总费用' },
  'projects.kpi_avg_cost': { en: 'Avg Cost / Project', zh: '项目均费' },
  'projects.ranking_title': { en: 'Project Ranking', zh: '项目排行' },
  'projects.col_name': { en: 'Project Name', zh: '项目名称' },
  'projects.col_sessions': { en: 'Sessions', zh: '会话数' },
  'projects.col_turns': { en: 'Turns', zh: '轮次' },
  'projects.col_agent_turns': { en: 'Agent Turns', zh: 'Agent 轮次' },
  'projects.col_cost_per_session': { en: '$/Session', zh: '$/会话' },
  'projects.col_model': { en: 'Model', zh: '模型' },
  'projects.col_total_cost': { en: 'Total Cost', zh: '总费用' },
  'projects.col_session_id': { en: 'Session ID', zh: '会话 ID' },
  'projects.col_duration': { en: 'Duration', zh: '时长' },
  'projects.col_cost': { en: 'Cost', zh: '费用' },
  'projects.col_cache_hit': { en: 'Cache Hit%', zh: '缓存命中%' },
  'projects.sessions_for': { en: 'Sessions for', zh: '会话列表 -' },
  'projects.no_sessions': { en: 'No session data available for this project.', zh: '该项目暂无会话数据。' },

  // Trends page
  'trends.title': { en: 'Trends', zh: '趋势' },
  'trends.daily': { en: 'Daily', zh: '日' },
  'trends.monthly': { en: 'Monthly', zh: '月' },
  'trends.log_scale': { en: 'Log', zh: '对数' },
  'trends.linear_scale': { en: 'Linear', zh: '线性' },
  'trends.usage_trend': { en: 'Usage Trend', zh: '使用趋势' },
  'trends.sessions_per_day': { en: 'Sessions per Day', zh: '每日会话数' },
  'trends.sessions_per_month': { en: 'Sessions per Month', zh: '每月会话数' },
  'trends.cost_per_turn_trend': { en: 'Cost per Turn Trend', zh: '每轮费用趋势' },
  'trends.summary': { en: 'Summary', zh: '趋势摘要' },
  'trends.total_cost': { en: 'Total Cost', zh: '总费用' },
  'trends.avg_daily_cost': { en: 'Avg Daily Cost', zh: '日均费用' },
  'trends.avg_monthly_cost': { en: 'Avg Monthly Cost', zh: '月均费用' },
  'trends.total_turns': { en: 'Total Turns', zh: '总轮次' },
  'trends.avg_cost_per_turn': { en: 'Avg Cost/Turn', zh: '平均每轮费用' },
  'trends.cost': { en: 'Cost ($)', zh: '费用 ($)' },
  'trends.turns': { en: 'Turns', zh: '轮次' },
  'trends.cost_per_turn': { en: 'Cost/Turn ($)', zh: '每轮费用 ($)' },
  'trends.sessions': { en: 'Sessions', zh: '会话数' },

  // Sessions page
  'sessions.kpi_total_sessions': { en: 'Total Sessions', zh: '总会话数' },
  'sessions.kpi_total_cost': { en: 'Total Cost', zh: '总费用' },
  'sessions.kpi_avg_cost': { en: 'Avg Cost / Session', zh: '会话均费' },
  'sessions.kpi_avg_duration': { en: 'Avg Duration', zh: '平均时长' },
  'sessions.table_title': { en: 'Session List', zh: '会话列表' },
  'sessions.search_placeholder': { en: 'Search by session ID or project...', zh: '按会话 ID 或项目搜索...' },
  'sessions.sort_by_cost': { en: 'By Cost', zh: '按费用' },
  'sessions.sort_by_date': { en: 'By Date', zh: '按日期' },
  'sessions.sort_by_turns': { en: 'By Turns', zh: '按轮次' },
  'sessions.filter_all': { en: 'All', zh: '全部' },
  'sessions.col_session_id': { en: 'Session ID', zh: '会话 ID' },
  'sessions.col_project': { en: 'Project', zh: '项目' },
  'sessions.col_turns': { en: 'Turns', zh: '轮次' },
  'sessions.col_duration': { en: 'Duration', zh: '时长' },
  'sessions.col_cost': { en: 'Cost', zh: '费用' },
  'sessions.col_model': { en: 'Model', zh: '模型' },
  'sessions.col_cache_hit': { en: 'Cache Hit%', zh: '缓存命中%' },
  'sessions.col_date': { en: 'Date', zh: '日期' },
  'sessions.detail_title': { en: 'Title', zh: '标题' },
  'sessions.detail_tags': { en: 'Tags', zh: '标签' },
  'sessions.detail_mode': { en: 'Mode', zh: '模式' },
  'sessions.detail_branch': { en: 'Branch', zh: '分支' },
  'sessions.detail_agent_breakdown': { en: 'Agent Breakdown', zh: 'Agent 分解' },
  'sessions.detail_agent_type': { en: 'Agent Type', zh: 'Agent 类型' },
  'sessions.detail_agent_desc': { en: 'Description', zh: '描述' },
  'sessions.detail_agent_turns': { en: 'Turns', zh: '轮次' },
  'sessions.detail_agent_output': { en: 'Output Tokens', zh: '输出 Token' },
  'sessions.detail_agent_cost': { en: 'Cost', zh: '费用' },
  'sessions.detail_metadata': { en: 'Metadata', zh: '元数据' },
  'sessions.detail_autonomy': { en: 'Autonomy Ratio', zh: '自主比' },
  'sessions.detail_api_errors': { en: 'API Errors', zh: 'API 错误' },
  'sessions.detail_max_context': { en: 'Max Context', zh: '最大上下文' },
  'sessions.detail_compactions': { en: 'Compactions', zh: '压缩次数' },
  'sessions.detail_service_tier': { en: 'Service Tier', zh: '服务层级' },
  'sessions.detail_output_tokens': { en: 'Output Tokens', zh: '输出 Token' },
  'sessions.detail_agent_cost_label': { en: 'Agent Cost', zh: 'Agent 费用' },
  'sessions.detail_cache_hit': { en: 'Cache Hit Rate', zh: '缓存命中率' },
  'sessions.no_sessions': { en: 'No session data available.', zh: '暂无会话数据。' },
  'sessions.no_agents': { en: 'No agent data.', zh: '无 Agent 数据。' },
  'sessions.no_title': { en: '(untitled)', zh: '(无标题)' },

  // Common
  'common.coming_soon': { en: 'Coming soon', zh: '即将推出' },
  'common.theme_toggle': { en: 'Toggle theme', zh: '切换主题' },
  'common.lang_toggle': { en: 'Language', zh: '语言' },
  'common.turns': { en: 'turns', zh: '轮' },
  'common.sessions': { en: 'sessions', zh: '个会话' },

  // Footer
  'footer.generated_by': { en: 'Generated by', zh: '由' },
  'footer.suffix': { en: '', zh: '生成' },

  // ─── Wrapped Page ─────────────────────────────────────────────────────
  'wrapped.hero_title_pre': { en: 'Your', zh: '你的' },
  'wrapped.hero_title_suf': { en: 'Claude Code Wrapped', zh: 'Claude Code 年度总结' },
  'wrapped.active_of': { en: 'active of', zh: '活跃 /' },
  'wrapped.days_in': { en: 'days in', zh: '天，' },

  // Archetype descriptions
  'wrapped.archetype_desc.Architect': { en: 'You design systems that think for themselves', zh: '你设计能自主思考的系统' },
  'wrapped.archetype_desc.Sprinter': { en: 'Fast iterations, rapid results', zh: '快速迭代，极速产出' },
  'wrapped.archetype_desc.NightOwl': { en: 'The city sleeps, your code doesn\'t', zh: '城市入睡，代码不眠' },
  'wrapped.archetype_desc.Delegator': { en: 'You orchestrate agents like a symphony', zh: '你像指挥交响乐一样编排 Agent' },
  'wrapped.archetype_desc.Explorer': { en: 'Every project is a new frontier', zh: '每个项目都是新的疆界' },
  'wrapped.archetype_desc.Marathoner': { en: 'Endurance is your superpower', zh: '耐力是你的超能力' },

  // Activity stats
  'wrapped.activity_stats': { en: 'Activity Stats', zh: '活跃统计' },
  'wrapped.active_days': { en: 'Active Days', zh: '活跃天数' },
  'wrapped.longest_streak': { en: 'Longest Streak', zh: '最长连续' },
  'wrapped.consecutive_days': { en: 'consecutive days', zh: '连续天数' },
  'wrapped.ghost_days': { en: 'Ghost Days', zh: '沉寂天数' },
  'wrapped.days_offline': { en: 'days offline', zh: '天未活跃' },
  'wrapped.total_sessions': { en: 'Total Sessions', zh: '总会话数' },
  'wrapped.sessions': { en: 'sessions', zh: '个会话' },
  'wrapped.total_turns': { en: 'Total Turns', zh: '总对话轮次' },
  'wrapped.agent_driven': { en: 'agent-driven', zh: '由 Agent 驱动' },
  'wrapped.total_cost': { en: 'Total Cost', zh: '总费用' },
  'wrapped.output_tokens': { en: 'output tokens', zh: '输出 token' },

  // Peak patterns
  'wrapped.peak_patterns': { en: 'Peak Patterns', zh: '高峰模式' },
  'wrapped.peak_hour': { en: 'Peak Hour', zh: '高峰时段' },
  'wrapped.peak_day': { en: 'Peak Day', zh: '高峰日' },
  'wrapped.autonomy_ratio': { en: 'Autonomy', zh: '自主比' },
  'wrapped.turns_per_prompt': { en: 'turns per prompt', zh: '轮次/提示' },
  'wrapped.avg_duration': { en: 'Avg Duration', zh: '平均时长' },
  'wrapped.per_session': { en: 'per session', zh: '每会话' },
  'wrapped.avg_cost': { en: 'Avg Cost', zh: '平均费用' },

  // Weekdays
  'wrapped.weekday.Monday': { en: 'Monday', zh: '周一' },
  'wrapped.weekday.Tuesday': { en: 'Tuesday', zh: '周二' },
  'wrapped.weekday.Wednesday': { en: 'Wednesday', zh: '周三' },
  'wrapped.weekday.Thursday': { en: 'Thursday', zh: '周四' },
  'wrapped.weekday.Friday': { en: 'Friday', zh: '周五' },
  'wrapped.weekday.Saturday': { en: 'Saturday', zh: '周六' },
  'wrapped.weekday.Sunday': { en: 'Sunday', zh: '周日' },

  // Rankings
  'wrapped.rankings': { en: 'Rankings', zh: '排行榜' },
  'wrapped.top_tools': { en: 'Top 5 Tools', zh: 'Top 5 工具' },
  'wrapped.top_projects': { en: 'Top 5 Projects', zh: 'Top 5 项目' },
  'wrapped.models': { en: 'Models by Turns', zh: '模型（按轮次）' },

  // Records
  'wrapped.records': { en: 'Records', zh: '记录' },
  'wrapped.most_expensive_session': { en: 'Most Expensive Session', zh: '最贵会话' },
  'wrapped.longest_session': { en: 'Longest Session', zh: '最长会话' },
  'wrapped.of_total_spend': { en: 'of total spend', zh: '占总花费' },
  'wrapped.hours_total': { en: 'hours total', zh: '小时' },

  // Archetype titles (with prefix)
  'wrapped.the_archetype.Architect': { en: 'The Architect', zh: '架构师' },
  'wrapped.the_archetype.Sprinter': { en: 'The Sprinter', zh: '冲刺者' },
  'wrapped.the_archetype.NightOwl': { en: 'The Night Owl', zh: '夜猫子' },
  'wrapped.the_archetype.Delegator': { en: 'The Delegator', zh: '指挥官' },
  'wrapped.the_archetype.Explorer': { en: 'The Explorer', zh: '探索者' },
  'wrapped.the_archetype.Marathoner': { en: 'The Marathoner', zh: '马拉松选手' },

  // No data
  'wrapped.no_data': { en: 'No wrapped data available.', zh: '暂无年度总结数据。' },
}

function getInitialLocale(): Locale {
  if (typeof window === 'undefined') return 'en'
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored === 'en' || stored === 'zh') return stored
  // Detect browser language
  const lang = navigator.language.toLowerCase()
  if (lang.startsWith('zh')) return 'zh'
  return 'en'
}

const locale = ref<Locale>(getInitialLocale())

export function useI18n() {
  function t(key: string): string {
    const entry = messages[key]
    if (!entry) return key
    return entry[locale.value] ?? entry.en ?? key
  }

  function toggleLocale() {
    locale.value = locale.value === 'en' ? 'zh' : 'en'
    localStorage.setItem(STORAGE_KEY, locale.value)
  }

  const localeLabel = computed(() => (locale.value === 'en' ? 'EN' : '中'))

  return {
    locale,
    t,
    toggleLocale,
    localeLabel,
  }
}
