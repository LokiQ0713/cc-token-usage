# Session 数据模型

> cc-session-dag 设计基础参考。所有事实由真实 Claude Code JSONL 数据扫描验证（178 sessions / 37k+ assistant entries / 20k+ user entries）。

本文档沉淀对 Claude Code session JSONL 的关键认知，是 cc-session-dag crate 的设计参考。它不是 cc-session-jsonl 的类型定义文档（那是字段层），而是 **数据语义层的认知** —— 为什么字段是这样、entry 之间真实的关系如何、边角情况怎么处理。

配套图：[session-id-relationships.excalidraw](session-id-relationships.excalidraw)

## 目录

- [1. 三大关联键](#1-三大关联键)
- [2. assistant entry 模型](#2-assistant-entry-模型)
  - [2.1 拆分写入](#21-拆分写入)
  - [2.2 requestId 簇](#22-requestid-簇)
  - [2.3 7 种失败/异常场景](#23-7-种失败异常场景)
  - [2.4 L1→L2 折叠规则](#24-l1l2-折叠规则)
  - [2.5 必须保留并渲染](#25-必须保留并渲染)
- [3. user entry 模型](#3-user-entry-模型)
  - [3.1 严格二分](#31-严格二分)
  - [3.2 分类判别](#32-分类判别)
  - [3.3 数据保证](#33-数据保证)
- [4. 反直觉的事实](#4-反直觉的事实)

---

## 1. 三大关联键

JSONL 每个 entry 之间通过 **3 把钥匙** 关联。它们正交协作，缺一不可。

| 键 | 出现在哪 | 方向 | 作用 |
|---|---|---|---|
| **`parentUuid → uuid`** | 几乎所有 entry | child → parent | 串成"区块链"线性主轴（垂直） |
| **`requestId`** | 仅 assistant entry | 同簇共享 | 标识"同一次 API call 产物"（水平分组） |
| **`tool_use_id` ↔ `sourceToolUseID` / `toolUseResult`** | tool_use 在 asst 内，配对在 user entry | result → use | 精确关联工具调用与结果（跨 rid 簇） |

**正交的含义**：

- `parentUuid` 无差别串联——它同时穿过"一次 API call 内部的 block 切换"和"跨 API call 的对话推进"
- `requestId` 无序分组——它告诉你"谁和谁同属一次 call"，但不携带顺序
- `tool_use_id` 是字符串 id（`tu_xxx` 格式），属于 Anthropic API 命名空间，**与 entry uuid 完全不同的命名空间**

**单独看哪个键都不够**：

| 问题 | 用 parentUuid 答 | 用 requestId 答 |
|---|---|---|
| "这个 entry 的上一条是谁？" | ✓ | ✗ |
| "这几条 entry 是不是同一次 API call？" | ✗ | ✓ |
| "这次 API call 的 user 上家是谁？" | ✓ (取 header 的 parent) | ✗ |
| "thinking/text/tool_use 哪个先发生？" | ✓ (簇内 parent 链顺序) | ✗ |

---

## 2. assistant entry 模型

### 2.1 拆分写入

**一次 LLM API 响应的多个 `ContentBlock` 拆成多条独立 entry 写入 JSONL。** Anthropic API 返回一个 `message.content: [thinking, text, tool_use, ...]` 数组；Claude Code 把**每个 block 单独写一条 assistant entry**。

```
Anthropic API 单次响应:                   写入 JSONL:
   message:                              ┌──────────────────────────┐
   ├─ content[0]: thinking      ───────► │ entry 1: type=assistant   │
   ├─ content[1]: text          ───────► │   content: [thinking]     │
   └─ content[2]: tool_use      ───────► ├──────────────────────────┤
                                         │ entry 2: type=assistant   │
                                         │   content: [text]         │
                                         ├──────────────────────────┤
                                         │ entry 3: type=assistant   │
                                         │   content: [tool_use]     │
                                         └──────────────────────────┘
   3 个 block → 3 条 entry, 但都属于 "1 次 API call"
```

**这是 Claude Code 客户端的写入约定，不是 Anthropic API 协议**。其他 LLM 客户端可能合并成一条。

### 2.2 requestId 簇

**同一次 API call 拆出的多条 entry 共享 `requestId` 以及更多元数据**：

```
一个 requestId 簇内的 N 条 entry 共享:
  ✓ requestId (相同字符串)
  ✓ message.id (相同 msg_xxx)
  ✓ message.model (相同模型名)
  ✓ usage 字段（每条都复制了完整的 usage，不是平摊）
  ✓ timestamp（精确到秒级几乎相同）
```

**簇内部是 linear chain，不是平级 fan-out**（数据 100% 验证）：

```
parent (外部，上一轮的 tail/tool_result/attachment)
   │
   ▼ parent_uuid
┌─────────────────────────────────┐
│ entry A   thinking              │  ← header: parent 指向外部
│   parent: 外部                  │
└──────────────┬─────────────────┘
               │
               ▼
┌─────────────────────────────────┐
│ entry B   text                  │  ← parent 是同簇上一条 A
│   parent: A                     │
└──────────────┬─────────────────┘
               │
               ▼
┌─────────────────────────────────┐
│ entry C   tool_use              │  ← tail: parent 是同簇上一条 B
│   parent: B                     │
└──────────────┬─────────────────┘
               │
               ▼
   下一轮 (tool_result entry 把 parent 指向 C)
```

**三个簇内角色**：

- **header** — 簇的第一条，parent 指向「外部」
- **tail** — 簇的最后一条，下一轮的 entry 把 parent 指向它
- **single** — 簇里只有 1 个 entry 时同时是 header 和 tail

**计费 dedup 必须按 requestId**：因为 `usage` 被复制写到簇内每条 entry，按 entry 加总会重复算 N 倍。

### 2.3 7 种失败/异常场景

assistant entry **不全是真实 LLM 输出**。除了正常路径，还有 7 种"client-fabricated"场景。**`model: "<synthetic>"` 是识别这些场景的唯一稳定签名**。

| # | 场景 | rid | model | isApiErrorMessage | error | text 示例 | 样本数 |
|---|---|---|---|---|---|---|---|
| ⑤ | **正常 LLM 响应** | ✓ | 真实（如 `claude-sonnet-4-6`） | ∅ | ∅ | LLM 实际输出 | 37,385 |
| ① A | 用户取消请求 | ∅ | `<synthetic>` | ∅ | ∅ | `"No response requested."` | 25 |
| ① B | 用户打断 assistant 响应 | ∅ | `<synthetic>` | ∅ | ∅ | assistant 已输出的片段 | 11 |
| ② | 认证失败 (401/403) | ∅ | `<synthetic>` | ✓ | `"authentication_failed"` | `"Please run /login · API Error: 403..."` | 2 |
| ③ | **传输中断** | **✓** | `<synthetic>` | ✓ | `"unknown"` | `"API Error: socket closed..."` | 14 |
| ④ | tool_use 解析失败 | ∅ | `<synthetic>` | ✓ | ∅ | `"The model's tool call could not be parsed..."` | 23 |
| ⑤ | rate_limit | ∅ | `<synthetic>` | ✓ | `"rate_limit"` | `"You've hit your session limit · resets ..."` | (少数) |
| ⑥ | content filtering | ✓ | `<synthetic>` | ✓ | `"unknown"` | `"API Error: Output blocked by content filtering policy"` | (少数) |

#### 关键规律

**关于 requestId 的本质**：

- **有 rid** = server 至少处理过这次请求（即使响应是错的）
- **无 rid** = server 没（成功）处理过

具体到 5 个失败场景：

- ① A / ② / ④ / ⑤：客户端**根本没发请求**或**响应被弃用** → 无 rid
- ③ / ⑥：server **已签发 rid 才出错** → rid 被保留

**关于"整组 synthetic"问题**：

| 场景 | 同 rid 簇是不是全 synthetic？ |
|---|---|
| ① A 用户取消 | N/A，单独 1 条 |
| ① B 用户打断 | ❌ rid=A 簇仍是真，synthetic 在新无 rid 节点 |
| ② 认证失败 | ✅ 全 synthetic（rid 通常 ∅） |
| ③ 传输中断 | ❌ **混合！** 前段 real (thinking/text 已落地) + 后段 synthetic 错误标记，**共享 rid** |
| ④ 解析失败 | ✅ 原响应被整个弃用，重写 1 条 synthetic |
| ⑥ content filtering | ❌ 客户端自动重试 → 连续两条 synthetic 各带不同 rid |

**链 ③（传输中断）的混合形态 = cc-session-dag 必须能正确还原的边角**：

```
rid=req_xxx 簇 (混合)
  ├─ asst thinking    model=claude-sonnet-4-6  (真 LLM 输出，已成功落地)
  ├─ asst text        model=claude-sonnet-4-6  (真 LLM 输出)
  └─ asst synthetic   model=<synthetic>        (后段 socket 断，追加错误标记)
                       error=unknown
                       (这条 rid 和上面一样！)
```

**timing 信号**：实测样本中真 entry 到 synthetic 错误的间隔可达 **23-34 分钟**——这是 client timeout 的实际配置。

### 2.4 L1→L2 折叠规则

**把"物理 entry"折叠成"逻辑 API call 组"的算法**：

```
沿 parent_uuid 链走，连续的 assistant entry 算一组。
出组条件：
  (1) 遇到非 assistant 节点
  (2) 相邻 asst entry 的 rid 都非空且不同（content filtering 自动重试场景）
```

伪代码：

```rust
fn close_group_here(prev: &Node, curr: &Node) -> bool {
    if curr.kind() != NodeKind::Assistant {
        return true;  // (1) 跨越非 asst 节点
    }
    match (prev.request_id(), curr.request_id()) {
        (Some(a), Some(b)) if a != b => true,  // (2) rid 切换
        _ => false,
    }
}
```

**实测验证**：扫 178 sessions, 15,172 条连续 asst 链, **只有 1 条反例**（0.007%）。该反例就是场景 ⑥ content filtering 自动重试。

**链长分布**：

| 链长 | 数量 | 占比 |
|---|---|---|
| 1 (单 block 簇) | 6,198 | 41% |
| 2 (双 block) | 3,769 | 25% |
| 3 (经典三件套 thinking+text+tool_use) | 4,996 | 33% |
| 4-7 (复杂) | 207 | 1% |
| 11/16 (异常长) | 2 | 0.01% |

### 2.5 必须保留并渲染

**所有失败/异常 entry 必须保留在 DAG 并渲染到 session 视图**，不能当占位符或丢弃。理由：

1. **DAG 完整性** — 删了下家 parent 就成 orphan
2. **失败叙事** — 携带"为什么没成功"的信息（5+ 种失败原因）
3. **部分 LLM 输出** — 链 ③ 前段是真 LLM 输出，不能因为后段失败就整组扔
4. **用户行为信号** — 打断/取消/重试模式是会话历史
5. **网络/计费指标** — timeout 配置、rate_limit、auth 状态
6. **100% 有 child** — 所有失败 entry 都不是 leaf，必须挂下家

**实测**：61 个无 rid entry, **100% has child, 0 is_leaf**。

参考 [project_session_dag_render_no_rid.md](../../../.claude/projects/-Users-loki-cc-cc-token-usage/memory/project_session_dag_render_no_rid.md) 个人 memory。

---

## 3. user entry 模型

### 3.1 严格二分

**`type="user"` 的 entry 在语义上严格二分，且数据上 100% 不混合**：

```
type="user" 的 entry 只能是这两种之一:

  类型 A: RealInput (真用户输入)
    content: "hello" (string)
    或 [{text}, {text}, ...]
    或 [{image}, {text}]

  类型 B: ToolResult (工具结果)
    content: [{tool_result, tool_use_id: "tu_xxx"}]
    顶层还带:
      sourceToolUseID: "tu_xxx"
      toolUseResult: {...实际返回值...}

❌ 绝不会出现:
  content: [{tool_result}, {text}]   ← 工具结果 + 用户文本混合
```

**为什么 type 都是 user**：Anthropic API 协议要求把 tool 结果以 user role 注入回 LLM——这是单向通信协议的产物。Claude Code 忠实记录这个协议。

### 3.2 分类判别

**判别函数**（cc-session-dag 必须实现）：

```rust
enum UserKind {
    RealInput,
    ToolResult,
}

fn classify(e: &UserEntry) -> UserKind {
    // 优先看顶层字段（最稳定）
    if e.tool_use_result.is_some()
       || e.source_tool_use_id.is_some()
    {
        return UserKind::ToolResult;
    }
    // 兜底看 content 数组里有没有 tool_result block
    if let Some(content_arr) = e.content_array() {
        if content_arr.iter().any(|b| b.kind() == "tool_result") {
            return UserKind::ToolResult;
        }
    }
    UserKind::RealInput
}
```

### 3.3 数据保证

实测 20,582 个 user entry：

```
user entry 分布:
├─ content 是 string (旧格式):   3,355  (16.3%)
└─ content 是 array (新格式):   17,227  (83.7%)
    ├─ 仅 tool_result            16,995    ← ToolResult (主要)
    ├─ 仅 text                      210    ← RealInput (多段)
    ├─ image + text                  22    ← RealInput (带图)
    └─ tool_result + text             0    ← 永不混合
```

**总占比**（合并 string + array）：

| 语义类 | 数量 | 占比 |
|---|---|---|
| RealInput | 3,587 | 17% |
| ToolResult | 16,995 | 83% |

**16% 用 string、84% 用 array** — cc-session-dag 解析时必须同时支持新旧格式。

**user entry 永远没 requestId**——它不是 LLM 产物，所以不携带 server 签发的 id。

---

## 4. 反直觉的事实

汇总几个调试时最容易踩坑的非显然事实：

1. **`type="user"` 不代表"用户在说话"** — 83% 的 user entry 其实是工具结果回填。先 classify 再用，不要直接显示。

2. **同一 requestId 簇内可能 real + synthetic 混合** — 传输中断场景（链 ③）。下游 dedup / 折叠时只看 rid 不够，要再看 model 字段。

3. **`requestId` 在失败时不一定丢** — 7 种异常场景中场景 ③（传输中断）和场景 ⑥（content filtering）会保留 rid。"无 rid = 失败"是错的。

4. **`model: "<synthetic>"` 是字面字符串占位符** — 不是某个真模型，是客户端约定的哨兵值。这是识别 fabricated entry 的最稳定签名（比 `rid is None` 准）。

5. **assistant 响应 1 次 API call 可能写入 N 条 entry** — N 个 ContentBlock 各成一条。usage 字段被**重复写入每条**（不是平摊），dedup 必须按 requestId。

6. **parent_uuid 链上"假 fork"很多** — 一个 assistant(tool_use) 的 child 经常是 `[attachment, user(tool_result)]` 两条（hook 注入 + 工具回填都把 tool_use 当 parent），看似 fork 实际是协议产物。真正的 rewind fork 反而很少见。

7. **同 uuid 多次出现是常态** — JSONL 是 append-only，session resume 会把同 entry 多写几次。按 uuid 集合 dedup 是必须的（claude-code-log 用"earliest sessionId 优先"策略）。

8. **`tool_use_id` 不是 uuid** — 它是 Anthropic API 命名空间的字符串（`tu_xxx` / `toolu_xxx`），和 entry uuid 完全不同体系。两者经常同时出现在一条 entry 里。

9. **失败 entry 100% 有下家** — 错误从不终结 session，用户/客户端总会继续。"asst 失败 entry 是 leaf"是错的。

10. **`stop_reason` 在 synthetic entry 上是占位** — synthetic 类 assistant 的 `stop_reason` 通常填 `"stop_sequence"`，不要把它当真实的 LLM 停止原因。

---

## 相关文档

- [session-id-relationships.excalidraw](session-id-relationships.excalidraw) — requestId × parentUuid 关系图
- [../crates/cc-session-jsonl/](../crates/cc-session-jsonl/) — 底层字段类型定义
- [../crates/cc-session-dag/](../crates/cc-session-dag/) — 本文档指导设计的 crate
