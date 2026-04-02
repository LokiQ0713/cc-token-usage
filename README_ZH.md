# cc-token-usage

[![Release](https://github.com/LokiQ0713/cc-token-usage/actions/workflows/release.yml/badge.svg)](https://github.com/LokiQ0713/cc-token-usage/actions/workflows/release.yml)
[![npm](https://img.shields.io/npm/v/cc-token-usage)](https://www.npmjs.com/package/cc-token-usage)
[![crates.io](https://img.shields.io/crates/v/cc-token-usage)](https://crates.io/crates/cc-token-usage)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

分析 Claude Code 本地 session 数据的 CLI 工具 -- token 用量、费用、效率和编码模式。不调 API，不联网，纯本地分析。

[English](README.md)

<!-- screenshot -->
![Overview Dashboard](assets/preview1.png)

## 功能特性

- **交互式 HTML 仪表盘** -- Vue 3 单文件应用，6 个页面（概览、趋势、项目、会话、热力图、年度总结），暗色/亮色主题，中英文切换
- **终端报表** -- 通过 comfy-table 输出格式化表格，随处可用
- **热力图** -- GitHub 风格终端活跃度热力图（`░▒▓█`）
- **年度总结 (Wrapped)** -- Spotify 风格年度回顾，含开发者画像分类
- **项目钻取** -- 按费用排名项目，支持按名称过滤，逐层深入到会话和轮次
- **趋势分析** -- 每日/每月费用趋势，月度环比对比
- **上下文压缩检测** -- 识别上下文被压缩的会话，评估风险
- **代码归属追踪** -- 追踪每个会话中 Claude 的代码贡献
- **效率指标** -- 输出占比、每轮费用、缓存节省分析
- **会话元数据挖掘** -- 标题、标签、模式、分支、错误、推测性执行
- **双通道校验** -- 独立 raw JSON 计数器交叉验证 pipeline
- **JSON 导出** -- 所有子命令支持 `--format json`，方便工具集成
- **并行处理** -- rayon 驱动并行解析（大数据集 3.9 倍加速）
- **164+ 测试** -- 两个 crate 全面覆盖

### 月度趋势
![Monthly](assets/preview2.png)

### 项目钻取
![Projects](assets/preview3.png)

## 安装

一键安装（macOS / Linux）：

```bash
curl -fsSL https://raw.githubusercontent.com/LokiQ0713/cc-token-usage/master/install.sh | sh
```

通过 npm：

```bash
npx cc-token-usage            # 免安装，始终最新
npm install -g cc-token-usage  # 全局安装
```

通过 cargo：

```bash
cargo install cc-token-usage
```

### 升级

```bash
cc-token-usage update          # 下载并替换
cc-token-usage update --check  # 仅检查，不下载
```

### 作为 Claude Code Skill 安装

安装为 Skill 后，跟 Claude 说"我用了多少 token"它就会自动帮你分析：

```bash
npx skills add LokiQ0713/cc-token-usage -g -y
```

## 使用

默认：终端输出汇总 + 打开 HTML 仪表盘：

```bash
cc-token-usage
```

### 子命令

| 命令 | 说明 |
|------|------|
| `overview` | 所有项目的整体用量概览 |
| `project` | 按费用排名的项目级分析 |
| `session` | 单会话详情，逐轮分析 |
| `trend` | 每日/每月用量趋势 |
| `heatmap` | GitHub 风格终端活跃度热力图 |
| `wrapped` | 年度 "Wrapped" 总结，含开发者画像 |
| `validate` | 交叉校验 token 计数与原始 JSONL |
| `update` | 自更新二进制文件 |

### 示例

```bash
# 仅生成 HTML 仪表盘
cc-token-usage --format html

# JSON 导出
cc-token-usage --format json

# 按费用排名所有项目
cc-token-usage project --top 0

# 过滤特定项目
cc-token-usage project --name "my-project"

# 最近一次会话详情
cc-token-usage session --latest

# 按月汇总
cc-token-usage trend --group-by month

# 按天趋势（最近 30 天）
cc-token-usage trend --days 30

# 终端热力图（最近一年）
cc-token-usage heatmap

# 全部历史热力图
cc-token-usage heatmap --days 0

# 年度总结
cc-token-usage wrapped

# 指定年份总结
cc-token-usage wrapped --year 2025

# 校验 token 计数准确性
cc-token-usage validate --failures-only
```

### 示例输出

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

## HTML 仪表盘

`--format html` 生成自包含的单文件 HTML 仪表盘（370KB，零网络依赖），包含：

- **概览** -- 汇总统计、模型分布、每 agent 费用、用量洞察
- **趋势** -- 每日费用图表，月度环比
- **项目** -- 所有项目按费用排名，点击深入查看会话
- **会话** -- 会话列表，含元数据（标题、标签、模式、分支）
- **热力图** -- 交互式活跃度热力图
- **年度总结** -- 含开发者画像分类的年度回顾

基于 Vue 3 + TailwindCSS + Chart.js 构建，支持暗色/亮色主题和中英文切换。

## 工作原理

直接读取 `~/.claude/projects/` 目录。解析所有 JSONL session 文件，包括子 agent 文件（新旧两种格式都支持）。自动校验数据、按 `requestId` 去重、归属孤立 agent、检测上下文压缩。

**解析库：** `cc-session-jsonl` crate 独立处理所有 JSONL 解析 -- 23 种 entry 类型、3 种文件布局格式、通过 `Unknown` 变体前向兼容。作为独立 crate 发布在 [crates.io](https://crates.io/crates/cc-session-jsonl)。

**定价数据：** 使用 [Anthropic 官方费率](https://platform.claude.com/docs/en/about-claude/pricing)。区分 5 分钟和 1 小时缓存 TTL，精确计算费用。

## 配置

可选。创建 `~/.config/cc-token-usage/config.toml` 覆盖模型定价：

```toml
[pricing_override.claude-opus-4-6]
base_input = 5.0
cache_write_5m = 6.25
cache_write_1h = 10.0
cache_read = 0.50
output = 25.0
```

## 开发

### 工作区结构

```
crates/
  cc-session-jsonl/   # 纯 JSONL 解析库（98 个测试）
  cc-token-usage/     # 分析 CLI（61+ 个测试）
frontend/             # Vue 3 + Vite + TailwindCSS 仪表盘
npm-package/          # npm 二进制包装
```

### 构建与测试

```bash
cargo build
cargo test --workspace --all-features    # 164+ 测试
cargo clippy --workspace --all-features -- -D warnings
cargo fmt
```

### 前端开发

```bash
cd frontend
npm install
npm run dev      # 开发服务器（HMR）
npm run build    # 构建单文件 HTML → dist/index.html
```

构建完成后，Rust 二进制通过 `include_str!` 嵌入 `frontend/dist/index.html` -- 运行时无需额外文件。

## 技术栈

Rust (serde, clap, chrono, comfy-table, rayon) + Vue 3 + TailwindCSS + Chart.js

## 许可证

MIT
