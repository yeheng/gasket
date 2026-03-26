# Subagent 消息隔离系统设计

**日期**: 2026-03-26
**状态**: 待审查
**作者**: Claude

## 背景

当前 subagent（通过 `spawn_parallel` 工具执行）的消息与主 agent 消息混在一起显示，造成用户困惑。例如：

```
[Task 4] Iteration 1 completed[Task 8] Iteration 1 completed[Task 3]
```

这些消息直接混入主对话流，无法区分来源，影响用户体验。

## 目标

1. **消息隔离**: Subagent 消息与主 agent 消息完全分离
2. **清晰展示**: 用户能清楚看到每个 subagent 的状态和进度
3. **动态分组**: 执行中显示独立卡片，完成后合并为摘要
4. **标准模式**: 默认显示任务描述 + 最终输出 + 工具调用摘要
5. **流式展示**: Subagent 的思考过程和输出内容实时流式展示，用户可以实时看到进度

## 非目标

- 不改变 subagent 的执行逻辑
- 不改变 spawn_parallel 工具的接口
- 不支持嵌套 subagent（subagent 再 spawn subagent）

## 设计方案

### 1. 后端消息格式

#### 1.1 扩展 WebSocketMessage 枚举

在 `gasket/types/src/events.rs` 中新增 subagent 专用消息类型：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSocketMessage {
    // === 主 Agent 消息（保持不变） ===
    Thinking { content: String },
    Content { content: String },
    ToolStart { name: String, arguments: Option<String> },
    ToolEnd { name: String, output: Option<String> },
    Done,
    Text { content: String },

    // === Subagent 专用消息（新增） ===
    /// Subagent 开始执行
    SubagentStarted {
        id: String,        // UUID
        task: String,      // 任务描述
        index: u32,        // 任务序号 (1, 2, 3...)
    },
    /// Subagent 思考内容（增量）
    SubagentThinking {
        id: String,
        content: String,
    },
    /// Subagent 输出内容（增量）
    SubagentContent {
        id: String,
        content: String,
    },
    /// Subagent 工具调用开始
    SubagentToolStart {
        id: String,
        name: String,
        arguments: Option<String>,
    },
    /// Subagent 工具调用结束
    SubagentToolEnd {
        id: String,
        name: String,
        output: Option<String>,
    },
    /// Subagent 执行完成
    SubagentCompleted {
        id: String,
        index: u32,
        summary: String,    // 简短摘要（前 100 字符）
        tool_count: u32,    // 工具调用次数
    },
    /// Subagent 执行出错
    SubagentError {
        id: String,
        index: u32,
        error: String,
    },
}
```

#### 1.2 消息发送逻辑修改

在 `gasket/core/src/tools/spawn_parallel.rs` 中：

1. 将 `SubagentEvent` 转换为对应的 `WebSocketMessage` 类型
2. 移除文本前缀拼接（不再拼接 `[Task N]` 到内容中）
3. `Iteration` 消息不再发送到前端（静默处理）

### 2. 前端数据结构

#### 2.1 SubagentState 接口

新增 `web/src/types/index.ts`：

```typescript
export interface SubagentState {
  id: string;
  index: number;
  task: string;
  status: 'running' | 'completed' | 'error';
  thinking?: string;
  content?: string;
  toolCalls: ToolCall[];
  toolCount: number;
  summary?: string;
  error?: string;
  startTime: number;
  endTime?: number;
}
```

#### 2.2 Message 接口扩展

```typescript
export interface Message {
  id: string;
  role: 'user' | 'bot' | 'system';
  content: string;
  thinking?: string;
  toolCalls?: ToolCall[];
  timestamp: number;

  // 新增：subagent 合并摘要标记
  isSubagentGroup?: boolean;
  subagentCount?: number;
}
```

### 3. 前端组件设计

#### 3.1 组件层级

```
ChatArea.vue
├── MessageBubble.vue (主消息流)
│   ├── UserMessage
│   ├── BotMessage (thinking + toolCalls + content)
│   └── SubagentGroupCard (isSubagentGroup=true 时)
│
└── SubagentPanel.vue (实时 subagent 状态)
    ├── RunningSubagentCard (运行中，多个独立卡片)
    └── CompletedSummaryCard (完成后合并)
```

#### 3.2 SubagentPanel.vue

职责：
- 显示当前运行中的 subagent 独立卡片（带动画）
- 所有 subagent 完成后，触发合并为摘要卡片
- 默认折叠，点击展开查看详情

显示内容（标准模式）：
- 任务序号和描述
- 状态指示器（运行中/完成/错误）
- 工具调用次数
- 可展开查看：思考过程、输出内容

#### 3.3 SubagentGroupCard

在 `MessageBubble.vue` 中新增，当 `message.isSubagentGroup === true` 时渲染：

- 标题："完成 N 个并行任务"
- 每个任务的摘要列表
- 可展开查看详细输出

### 4. 消息处理流程

#### 4.1 后端流程

```
SubagentTracker
    │
    ▼ SubagentEvent
spawn_parallel.rs (转换)
    │
    ▼ WebSocketMessage::Subagent*
OutboundMessage
    │
    ▼ JSON over WebSocket
前端
```

#### 4.2 前端流程

```
WebSocket.onmessage
    │
    ▼ JSON.parse
processWebSocketMessage(msg)
    │
    ├─ 主消息类型 → Message (原有逻辑)
    │
    └─ Subagent消息类型 → activeSubagents Map
           │
           ▼ 所有 subagent 完成
       finalizeSubagents()
           │
           ▼ 生成合并摘要
       append-to-message (追加到主消息)
           │
           ▼ 清理状态
       activeSubagents.clear()
```

### 5. 用户体验

#### 5.1 执行中

```
┌─────────────────────────────────────────────┐
│ 🔧 并行任务 (3个)                            │
│                                             │
│ ┌─────────────────────────────────────────┐ │
│ │ 🔄 Task 1: 搜索相关文档                  │ │
│ │    查看详情 (2 个工具调用) ▼             │ │
│ └─────────────────────────────────────────┘ │
│                                             │
│ ┌─────────────────────────────────────────┐ │
│ │ 🔄 Task 2: 分析代码结构                  │ │
│ │    查看详情 (0 个工具调用) ▼             │ │
│ └─────────────────────────────────────────┘ │
│                                             │
│ ┌─────────────────────────────────────────┐ │
│ │ 🔄 Task 3: 生成报告                      │ │
│ │    查看详情 (1 个工具调用) ▼             │ │
│ └─────────────────────────────────────────┘ │
└─────────────────────────────────────────────┘
```

#### 5.2 完成后

```
┌─────────────────────────────────────────────┐
│ ✅ 完成 3 个并行任务                         │
│                                             │
│ • Task 1: 找到 5 篇相关文档 (2 个工具)       │
│ • Task 2: 识别了 3 个模块 (0 个工具)         │
│ • Task 3: 生成完整报告 (1 个工具)            │
│                                             │
│ [展开详情 ▼]                                │
└─────────────────────────────────────────────┘
```

## 改动文件清单

| 文件 | 改动类型 | 说明 |
|------|----------|------|
| `gasket/types/src/events.rs` | 修改 | 扩展 `WebSocketMessage` 枚举，新增 6 个 Subagent 类型 |
| `gasket/core/src/tools/spawn_parallel.rs` | 修改 | 更新消息发送逻辑，使用新消息类型 |
| `web/src/types/index.ts` | 新增 | 定义 `SubagentState` 接口 |
| `web/src/components/SubagentPanel.vue` | 新增 | Subagent 实时状态组件 |
| `web/src/components/ChatArea.vue` | 修改 | 添加 subagent 消息处理函数 |
| `web/src/components/MessageBubble.vue` | 修改 | 添加 `SubagentGroupCard` 展示 |

## 向后兼容性

- 主 agent 消息格式保持不变
- 前端原有逻辑不受影响
- 新消息类型为可选，旧客户端会忽略未知类型

## 测试计划

1. **单元测试**: 后端 WebSocketMessage 序列化/反序列化
2. **集成测试**: spawn_parallel 工具执行，验证消息格式
3. **E2E 测试**: 前端接收 subagent 消息，验证 UI 展示
4. **手动测试**:
   - 单个 subagent 执行
   - 多个 subagent 并行执行
   - Subagent 出错场景
   - 网络断开重连场景

## 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 消息量大导致性能问题 | 高 | 使用增量更新，避免全量刷新 |
| 前端状态管理复杂 | 中 | 使用独立的 Map 管理 subagent 状态 |
| 向后兼容性破坏 | 高 | 保持原有消息类型不变，新增类型为可选 |

## 未来扩展

- [ ] 支持手动取消单个 subagent
- [ ] 支持 subagent 进度百分比显示
- [ ] 支持嵌套 subagent（subagent 再 spawn subagent）
