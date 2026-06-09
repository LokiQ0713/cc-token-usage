# cc-session-jsonl v2 · 字段摸排与强类型设计基线

> cc-session-jsonl 强类型重写（v2，breaking）的设计基线。所有 required/optional 判定由真实
> Claude Code JSONL 全量扫描得出。与 [session-data-model.md](session-data-model.md)（语义层认知）
> 并列：那份回答「entry 之间什么关系」，本份回答「每个 entry 到底有哪些字段、哪些一定有」。

**数据快照**：2026-06-09，本机 `~/.claude/projects`，587 文件 / **56,961 entries** / 14 distinct types。
**方法**：按 entry type 并发（一种 type 一个 worker 进程），每 worker 全量扫一遍只统计自己的 type。
脚本见 `/tmp/field_survey_par.py`（一次性分析工具，未入库）。

---

## 0. 怎么读这份表（required 的判据）

一个字段标 **REQUIRED** ⟺ 在该 type 的**全部** entry 里都「键存在且非 null」（精确 `present == total`，不是四舍五入的 100%）。否则 **OPTIONAL**，附真实出现率。

但「样本 100%」**不直接等于** Rust 里可以写成非 `Option`。严格类型库的 required 判据是三者交集：

> **required ⟺ (本机样本 == 100%) ∧ (doc/已知反例不存在) ∧ (结构上必然)**

原因：本数据集只是**一台机器**。`teamName`/`agentName`/`agentColor` 在本机 **0 命中**，但它们在 teammates 功能下真实存在——只是这台机器没开。反过来，本机 100% 的字段也可能在别人机器的罕见路径上缺失。所以**最终拍板前，required 列必须和 doc 的 178-session 数据集交叉验证**；本表的 REQUIRED 是「强候选」，不是终判。

这条纪律是「宁可炸不可瞒」严格哲学的配套：**炸要炸在真·漂移上，不能炸在「我没见过但合法」上**。

---

## 1. 核心发现：「公共字段」基本是幻觉，只有 9 个真·通用

现行 `transcript_entry!` 宏给 4 个会话类（user/assistant/system/attachment）统一注入 17 个字段。
按真实出现率铺开后，**只有 9 个数据字段在 4 类里都 100%**（外加 `type` 判别符）：

| 宏注入字段 | user | assistant | system | attachment | 判定 |
|---|---|---|---|---|---|
| `uuid` | 100 | 100 | 100 | 100 | ✅ 真·通用 |
| `sessionId` | 100 | 100 | 100 | 100 | ✅ 真·通用 |
| `timestamp` | 100 | 100 | 100 | 100 | ✅ 真·通用 |
| `cwd` | 100 | 100 | 100 | 100 | ✅ 真·通用 |
| `version` | 100 | 100 | 100 | 100 | ✅ 真·通用 |
| `gitBranch` | 100 | 100 | 100 | 100 | ✅ 真·通用 |
| `userType` | 100 | 100 | 100 | 100 | ✅ 真·通用 |
| `entrypoint` | 100 | 100 | 100 | 100 | ✅ 真·通用 |
| `isSidechain` | 100 | 100 | 100 | 100 | ✅ 真·通用 |
| `parentUuid` | 96.3 | **100** | 98.7 | 98.9 | ⚠️ 仅 assistant 必填（见 §2） |
| `logicalParentUuid` | ~0 | ~0 | 1.3 | ~0 | 罕见（仅 compact_boundary） |
| `slug` | 54.2 | 53.7 | 35.2 | 27.9 | optional |
| `agentId` | 51.2 | 43.8 | **0** | 12.2 | 类型特异 |
| `promptId` | **99.9** | **0** | **0** | **0** | **仅 user 有** |
| `teamName` | **0** | **0** | **0** | **0** | 本机 0（teammates 未用） |
| `agentName` | **0** | **0** | **0** | **0** | 本机 0（teammates 未用） |
| `agentColor` | **0** | **0** | **0** | **0** | 本机 0（teammates 未用） |

**这是杀掉宏、改独立结构的实锤**：17 个里有 8 个是「类型特异」的——宏把 `promptId` 塞给了
assistant/system/attachment（它们根本没有）、把 `agentId` 塞给 system（system 没有）、把
`teamName/agentName/agentColor` 塞给所有类（本机全 0）。独立结构能精确到「`promptId` 只出现在
`UserEntry`」。**独立结构不是更优雅，是更正确。**

设计上共享访问仍可保留——用一个 trait（共享访问契约，不共享字段）：

```rust
/// DAG 构建只需从任意 entry 拿到这几把「链钥匙」。各 struct 各自声明字段并 impl 它。
pub trait DagNode {
    fn uuid(&self) -> &str;             // 9 个真·通用之一，可返回 &str（非 Option）
    fn session_id(&self) -> &str;
    fn timestamp(&self) -> &str;
    fn parent_uuid(&self) -> Option<&str>;   // 见 §2：根节点为 None
    fn is_sidechain(&self) -> bool;
}
```

---

## 2. `parentUuid` = Optional = DAG 根；assistant 永远有父

`parentUuid` 唯独在 **assistant 是 100%**，user(96.3%)/system(98.7%)/attachment(98.9%) 都 <100%。
语义：**缺 parentUuid = 链的根**；assistant 永远在回应某条消息，所以恒有父。这个**非对称**正是
独立结构能编码、宏编码不了的：

- `AssistantEntry.parent_uuid: String`（必填）
- `UserEntry` / `SystemEntry` / `AttachmentEntry`.parent_uuid: `Option<String>`

**破案**：system 的 23 条 null-`parentUuid` 全部来自 `compact_boundary` 子类型——它**改用
`logicalParentUuid` 接续**（context 折叠点跨越被压缩的历史）。user 的 536 条 null-`parentUuid`
则是各 session 的真·首条 + rewind 分叉根。数据自洽。

---

## 3. 全量 required / optional 清单（逐类型）

> REQUIRED 已隐含 §0 的交叉验证警告。`type` 是 enum 判别符，不重复列。

### assistant (n=23,258)

- **REQUIRED**：`uuid` `parentUuid` `sessionId` `timestamp` `cwd` `version` `gitBranch` `userType` `entrypoint` `isSidechain` `message`
- **OPTIONAL**：`requestId` 92.2% · `slug` 53.7% · `agentId` 43.8% · `attributionAgent` 30.0% · `attributionSkill` 12.6% · `attributionMcpServer` 3.1% · `attributionMcpTool` 3.1% · `attributionPlugin` 3.1% · `isApiErrorMessage` 0.3% · `error` 0.1% · `apiErrorStatus` 0.1%
- **`message.*`** REQUIRED：`id` `type` `role` `model` `content` `usage` ｜ OPTIONAL：`stop_reason` 74.2% · `diagnostics` 4.9% · `stop_sequence` 0.3% · `context_management` 0.1%
- **`message.usage.*`** REQUIRED：`input_tokens` `output_tokens` `cache_creation_input_tokens` `cache_read_input_tokens` `cache_creation` ｜ OPTIONAL：`service_tier` 99.7% · `inference_geo` 99.7% · `server_tool_use` 74.1% · `iterations` 73.9% · `speed` 73.9%
  - `cache_creation.*` REQUIRED：`ephemeral_1h_input_tokens` `ephemeral_5m_input_tokens`（均 100%）
  - `server_tool_use.*`（出现时）：`web_search_requests` `web_fetch_requests`
- content block 类型分布：`tool_use` 12,274 · `text` 6,897 · `thinking` 4,116
- ⚠️ **`message` 标 REQUIRED 是数据驱动的修正**：本机 23,258 条全有 message（含 59 条 api-error），
  且 doc §2.3 每个失败场景都带 `text`（即有 message）。但 doc 的 auth-fail/synthetic 路径样本极小，
  拍板前需在 doc 数据集复核——这是 §0 规则的头号适用对象。

### user (n=14,491)

- **REQUIRED**：`uuid` `sessionId` `timestamp` `cwd` `version` `gitBranch` `userType` `entrypoint` `isSidechain` `message`
- **OPTIONAL**：`promptId` 99.9% · `parentUuid` 96.3%(+536 null) · `sourceToolAssistantUUID` 84.6% · `slug` 54.2% · `agentId` 51.2% · `toolUseResult` 38.6% · `permissionMode` 8.5% · `promptSource` 3.2% · `isMeta` 1.5% · `sourceToolUseID` 0.4% · `imagePasteIds` 0.4% · `mcpMeta` 0.3% · `origin` 0.2% · `interruptedMessageId` 0.2% · `isVisibleInTranscriptOnly` 0.2% · `isCompactSummary` 0.2%
- **`message.*`** REQUIRED：`role` `content`（无 optional）
- `content` 形态分布：`array:tool_result_only` 12,262 · `string` 1,981 · `array:text_only` 190 · `array:[image,text]` 56 · `array:[image]` 2
  - array block 类型：`tool_result` 12,262 · `text` 247 · `image` 66 ｜ **`tool_result + text` 混合 = 0**（呼应 doc §3.1 严格二分）
- 设计：`content: serde_json::Value`（string|array 双形态），`kind()` 方法做 RealInput/ToolResult 分类（doc §3.2）。

### system (n=1,817) — 见 §4，应拆 subtype 子枚举

- 顶层 **REQUIRED**：`uuid` `sessionId` `timestamp` `cwd` `version` `gitBranch` `userType` `entrypoint` `isSidechain` `subtype`
- 顶层 **OPTIONAL**（24 个，因 subtype 而异）：`parentUuid` 98.7%(+23 null) · `isMeta` 78.8% · `durationMs` 61.9% · `messageCount` 61.9% · `slug` 35.2% · `level` 26.7% · `content` 16.9% · `hookCount`/`hookInfos`/`hookErrors`/`preventedContinuation`/`stopReason`/`hasOutput`/`toolUseID` 16.1% · `error`/`retryInMs`/`retryAttempt`/`maxRetries` 5.1% · `logicalParentUuid` 1.3% · `compactMetadata` 1.3% · `cause` 0.9% · `pendingBackgroundAgentCount` 0.7% · `pendingWorkflowCount` 0.3% · `url` 0.1%

### attachment (n=6,027) — 见 §5，应拆 attachment.type 子枚举

- 顶层 **REQUIRED**：`uuid` `sessionId` `timestamp` `cwd` `version` `gitBranch` `userType` `entrypoint` `isSidechain` `attachment`
- 顶层 **OPTIONAL**：`parentUuid` 98.9%(+64 null) · `slug` 27.9% · `agentId` 12.2%

### 稀疏 / 元数据类型

| type | n | REQUIRED | OPTIONAL |
|---|---|---|---|
| `last-prompt` | 2,250 | `sessionId` | `lastPrompt` 97.1% · `leafUuid` 83.9% |
| `permission-mode` | 2,247 | `permissionMode` `sessionId` | — |
| `ai-title` | 1,794 | `aiTitle` `sessionId` | — |
| `file-history-snapshot` | 1,722 | `isSnapshotUpdate` `messageId` `snapshot` | — |
| `mode` | 1,403 | `mode` `sessionId` | — |
| `queue-operation` | 1,190 | `operation` `sessionId` `timestamp` | `content` 52.0% |
| `custom-title` | 14 | `customTitle` `sessionId` | — |
| `started` ⚠️新 | 288 | `agentId` `key` | — |
| `result` ⚠️新 | 268 | `agentId` `key` `result` | — |
| `bridge-session` ⚠️新 | 192 | `bridgeSessionId` `lastSequenceNum` `sessionId` | — |

注：这些稀疏类型**没有** `uuid`/`parentUuid`（不参与 DAG 链），是 session 级元数据。

---

## 4. `system` 是「subtype 机器」→ 建模为 tagged 子枚举

顶层 24 个 optional 字段是假象。按 `subtype` 切开，每个子类型的字段都**干净 100% required**：

| subtype | n | 子类型专属 REQUIRED（在 9 通用 + parentUuid 之外） | 备注 |
|---|---|---|---|
| `turn_duration` | 1,124 | `durationMs` `messageCount` `isMeta` | opt: `pendingBackgroundAgentCount`/`pendingWorkflowCount` |
| `stop_hook_summary` | 293 | `hookCount` `hookInfos` `hookErrors` `preventedContinuation` `stopReason` `hasOutput` `toolUseID` `level` | |
| `away_summary` | 203 | `content` `isMeta` | |
| `api_error` | 93 | `error` `level` `maxRetries` `retryAttempt` `retryInMs` | opt: `cause` 18.3% |
| `local_command` | 69 | `content` `level` `isMeta` | |
| `compact_boundary` | 23 | `compactMetadata` `content` `level` **`logicalParentUuid`** | **无 parentUuid**，改用 logicalParentUuid |
| `informational` | 7 | `content` `level` `isMeta` | |
| `scheduled_task_fire` | 3 | `content` `isMeta` | |
| `bridge_status` | 2 | `content` `url` | |

**设计结论**：`SystemEntry` 不该是「一个 struct + 一堆 Option」，而是

```rust
pub struct SystemEntry { /* 9 通用 + parentUuid: Option + subtype 分发 */ pub body: SystemBody }

#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum SystemBody {
    TurnDuration { duration_ms: u64, message_count: u64, is_meta: bool, /* … */ },
    StopHookSummary { hook_count: u64, hook_infos: Vec<HookInfo>, /* … */ },
    ApiError { error: String, level: String, max_retries: u64, /* … */ },
    CompactBoundary { compact_metadata: CompactMetadata, /* logical_parent_uuid */ },
    // … 其余子类型 …
    #[serde(other)] Unknown,   // 未知 subtype 的软兜底
}
```

把「隐式 subtype tag」提升为「显式 Rust enum 变体」，条件性字段变成变体专属的 required 字段。

---

## 5. `attachment` 是「attachment.type 机器」（23 种）→ 同样建模为 tagged 子枚举

`attachment.type` 嵌套判别符下有 23 种子类型，每种字段都 100% 干净：

| attachment.type | n | REQUIRED（`type` 外） | OPTIONAL |
|---|---|---|---|
| `output_style` | 1,788 | `style` | — |
| `hook_success` | 1,721 | `command` `content` `durationMs` `exitCode` `hookEvent` `hookName` `stderr` `stdout` `toolUseID` | — |
| `task_reminder` | 719 | `content` `itemCount` | — |
| `deferred_tools_delta` | 466 | `addedLines` `addedNames` `removedNames` | `readdedNames` 96.8% · `pendingMcpServers` 15.0% |
| `skill_listing` | 457 | `content` `isInitial` `skillCount` | `names` 93.2% |
| `queued_command` | 338 | `commandMode` `prompt` | `imagePasteIds`/`source_uuid` 0.3% |
| `hook_additional_context` | 103 | `content` `hookEvent` `hookName` `toolUseID` | — |
| `mcp_instructions_delta` | 91 | `addedBlocks` `addedNames` `removedNames` | — |
| `file` | 80 | `content` `displayPath` `filename` | — |
| `diagnostics` | 78 | `files` `isNew` | — |
| `edited_text_file` | 59 | `filename` `snippet` | — |
| `date_change` | 43 | `newDate` | — |
| `command_permissions` | 31 | `allowedTools` | — |
| `compact_file_reference` | 30 | `displayPath` `filename` | — |
| `hook_non_blocking_error` | 6 | `command` `durationMs` `exitCode` `hookEvent` `hookName` `stderr` `stdout` `toolUseID` | — |
| `nested_memory` | 5 | `content` `displayPath` `path` | — |
| `invoked_skills` | 4 | `skills` | — |
| `plan_mode_exit` | 3 | `planExists` `planFilePath` | — |
| `plan_mode` | 2 | `isSubAgent` `planExists` `planFilePath` `reminderType` | — |
| `auto_mode` | 1 | （仅 type） | — |
| `dynamic_skill` | 1 | `displayPath` `skillDir` `skillNames` | — |
| `plan_file_reference` | 1 | `planContent` `planFilePath` | — |

**待定决策**：23 种全建模 vs 建常见 6 种（output_style/hook_success/task_reminder/deferred_tools_delta/
skill_listing/queued_command，覆盖 ~95%）+ 长尾走 `#[serde(other)] Unknown(Value)`。倾向后者——
长尾很多是 1~6 个样本，建模收益低、维护成本高。

---

## 6. 三个未建模的新类型 → 严格模型下落 `Ignored`

| 新类型 | n | uuid? | sessionId? | 严格归宿 | 价值 |
|---|---|---|---|---|---|
| `started` | 288 | ❌ | ❌ | `Ignored` | 异步 agent 启动标记（agentId+key） |
| `result` | 268 | ❌ | ❌ | `Ignored` | **异步 agent 结果**（result body，keyed by agentId） |
| `bridge-session` | 192 | ❌ | ✅ | `Ignored`（缺 uuid） | session 桥接元数据 |

它们都无 `uuid`，按 v2 严格规则（未知 type 需 uuid+sessionId 才进 Passthrough）安静落 `Ignored`，
**不报错**（软路径正常）。**待定决策**：`result` 携带异步 agent 的实际产出，若 cc-session-dag 要还原
异步 agent 链，可能值得为 `started`/`result` 建专门的（非 DAG）旁路类型；否则接受 Ignored。

---

## 7. 设计落点汇总（驱动 v2 结构定义）

1. **杀掉 `transcript_entry!` 宏**。9 个真·通用字段 + `DagNode` trait（共享访问，不共享字段）。其余 8 个
   宏字段按真实出现，落到各自类型上为 `Option`（`promptId` 仅 `UserEntry`，等等）。
2. **`parentUuid`**：`AssistantEntry` 必填，其余三会话类 `Option`（根 = None）。
3. **`SystemEntry` → `SystemBody` 子枚举**（按 `subtype` 内部 tag），9 个子类型 + `Unknown` 兜底。
4. **`AttachmentEntry` → `AttachmentBody` 子枚举**（按 `attachment.type`），常见 ~6 种 + `Unknown(Value)` 长尾。
5. **`message`/`usage` 强类型化**：本机数据支持把 `message` 及 `usage.{input,output,cache_*,cache_creation}`
   设为 required——但须经 §0 跨数据集复核（尤其失败/synthetic 路径）后定稿。
6. **新类型 started/result/bridge-session**：默认 `Ignored`；`result`（异步 agent 产出）是否建专门旁路类型，待定。
7. **稀疏元数据类型**（tag/mode/ai-title/…）：字段极少、无 DAG 位置，保留独立 struct（轻量）即可。

---

## 8. 已知局限与后续

- **单机数据集**：本表基于一台机器。`teamName/agentName/agentColor`(0)、auth-fail/synthetic 路径（样本极小）
  等都需在 doc 的 178-session 数据集**交叉验证**再定 required。本表 REQUIRED = 强候选，非终判。
- **版本切面**：`server_tool_use`/`iterations`/`speed`(~74%)、`attributionAgent`(30%) 等是版本门控字段
  （新版本才有），必然 `Option`——它们的出现率反映的是「跨版本历史」，不是「缺失」。
- **下一步**：①本 doc 评审定稿 → ②逐类型写 v2 struct（含 `SystemBody`/`AttachmentBody`）→ ③对
  started/result、attachment 长尾两个待定点拍板。

## 相关文档

- [session-data-model.md](session-data-model.md) — entry 之间的关系与语义（requestId 簇、user 二分、synthetic 检测）
- [../crates/cc-session-jsonl/](../crates/cc-session-jsonl/) — 当前 v0.4 类型（待 v2 重写）
- [../crates/cc-session-dag/](../crates/cc-session-dag/) — 下游消费者
