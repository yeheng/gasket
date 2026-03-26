# Subagent 消息隔离系统实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 subagent 消息与主 agent 消息的完全隔离，提供清晰独立的 UI 展示

**Architecture:** 后端扩展 WebSocketMessage 枚举添加 6 个 Subagent 专用消息类型；前端使用独立的 Map 管理 subagent 状态，通过 SubagentPanel 组件实时展示，完成后合并为摘要卡片

**Tech Stack:** Rust (serde JSON), Vue 3 (Composition API), TypeScript, Tailwind CSS

---

## 文件结构

```
gasket/types/src/events.rs          [修改] WebSocketMessage 枚举
gasket/core/src/tools/spawn_parallel.rs  [修改] 消息发送逻辑
web/src/types/index.ts              [新增] SubagentState 接口
web/src/components/SubagentPanel.vue    [新增] 实时状态组件
web/src/components/ChatArea.vue     [修改] 消息处理逻辑
web/src/components/MessageBubble.vue    [修改] 摘要卡片展示
```

---

## Task 1: 后端 - 扩展 WebSocketMessage 枚举

**Files:**
- Modify: `gasket/types/src/events.rs`

- [ ] **Step 1: 添加 Subagent 消息类型到 WebSocketMessage 枚举**

在 `WebSocketMessage` 枚举中添加 6 个新的 Subagent 消息类型：

```rust
// gasket/types/src/events.rs

// 在 WebSocketMessage 枚举中添加（保持原有变体不变）：

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
```

- [ ] **Step 2: 添加构造辅助方法**

在 `impl WebSocketMessage` 中添加：

```rust
    // === Subagent 消息构造方法 ===

    /// Create a subagent_started message
    pub fn subagent_started(id: impl Into<String>, task: impl Into<String>, index: u32) -> Self {
        Self::SubagentStarted {
            id: id.into(),
            task: task.into(),
            index,
        }
    }

    /// Create a subagent_thinking message
    pub fn subagent_thinking(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::SubagentThinking {
            id: id.into(),
            content: content.into(),
        }
    }

    /// Create a subagent_content message
    pub fn subagent_content(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::SubagentContent {
            id: id.into(),
            content: content.into(),
        }
    }

    /// Create a subagent_tool_start message
    pub fn subagent_tool_start(id: impl Into<String>, name: impl Into<String>, arguments: Option<String>) -> Self {
        Self::SubagentToolStart {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }

    /// Create a subagent_tool_end message
    pub fn subagent_tool_end(id: impl Into<String>, name: impl Into<String>, output: Option<String>) -> Self {
        Self::SubagentToolEnd {
            id: id.into(),
            name: name.into(),
            output,
        }
    }

    /// Create a subagent_completed message
    pub fn subagent_completed(id: impl Into<String>, index: u32, summary: impl Into<String>, tool_count: u32) -> Self {
        Self::SubagentCompleted {
            id: id.into(),
            index,
            summary: summary.into(),
            tool_count,
        }
    }

    /// Create a subagent_error message
    pub fn subagent_error(id: impl Into<String>, index: u32, error: impl Into<String>) -> Self {
        Self::SubagentError {
            id: id.into(),
            index,
            error: error.into(),
        }
    }
```

- [ ] **Step 3: 添加单元测试**

在 `tests` 模块中添加：

```rust
    #[test]
    fn test_subagent_started_serialization() {
        let msg = WebSocketMessage::subagent_started("id-123", "Search docs", 1);
        let json = msg.to_json();
        assert!(json.contains("\"type\":\"subagent_started\""));
        assert!(json.contains("\"id\":\"id-123\""));
        assert!(json.contains("\"task\":\"Search docs\""));
        assert!(json.contains("\"index\":1"));
    }

    #[test]
    fn test_subagent_thinking_serialization() {
        let msg = WebSocketMessage::subagent_thinking("id-123", "Analyzing...");
        let json = msg.to_json();
        assert!(json.contains("\"type\":\"subagent_thinking\""));
        assert!(json.contains("\"content\":\"Analyzing...\""));
    }

    #[test]
    fn test_subagent_completed_serialization() {
        let msg = WebSocketMessage::subagent_completed("id-123", 1, "Done", 5);
        let json = msg.to_json();
        assert!(json.contains("\"type\":\"subagent_completed\""));
        assert!(json.contains("\"tool_count\":5"));
    }

    #[test]
    fn test_subagent_message_deserialization() {
        let json = r#"{"type":"subagent_started","id":"id-123","task":"Test task","index":1}"#;
        let msg: WebSocketMessage = serde_json::from_str(json).unwrap();
        match msg {
            WebSocketMessage::SubagentStarted { id, task, index } => {
                assert_eq!(id, "id-123");
                assert_eq!(task, "Test task");
                assert_eq!(index, 1);
            }
            _ => panic!("Expected SubagentStarted"),
        }
    }
```

- [ ] **Step 4: 运行测试验证**

Run: `cd gasket/types && cargo test --lib`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add gasket/types/src/events.rs
git commit -m "feat(types): add Subagent message types to WebSocketMessage

- Add 6 new message types: SubagentStarted, SubagentThinking, SubagentContent,
  SubagentToolStart, SubagentToolEnd, SubagentCompleted, SubagentError
- Add helper constructors for each message type
- Add unit tests for serialization/deserialization"
```

---

## Task 2: 后端 - 修改 spawn_parallel 消息发送逻辑

**Files:**
- Modify: `gasket/core/src/tools/spawn_parallel.rs`

- [ ] **Step 1: 修改事件处理逻辑**

在 `tokio::spawn` 内的消息转换逻辑中，将 `SubagentEvent` 转换为新的 `WebSocketMessage` 类型：

```rust
// gasket/core/src/tools/spawn_parallel.rs
// 在 tokio::spawn(async move { ... }) 内部修改

tokio::spawn(async move {
    while let Some(event) = event_rx.recv().await {
        // 提取 subagent ID
        let subagent_id = match &event {
            SubagentEvent::Started { id, .. } => id,
            SubagentEvent::Thinking { id, .. } => id,
            SubagentEvent::Content { id, .. } => id,
            SubagentEvent::Iteration { id, .. } => id,
            SubagentEvent::ToolStart { id, .. } => id,
            SubagentEvent::ToolEnd { id, .. } => id,
            SubagentEvent::Completed { id, .. } => id,
            SubagentEvent::Error { id, .. } => id,
        };

        // 获取任务序号
        let task_index = task_id_map
            .get(subagent_id)
            .copied()
            .unwrap_or(0) as u32;

        // 转换为新的 WebSocketMessage 类型（不再拼接 [Task N] 前缀）
        let ws_msg = match &event {
            SubagentEvent::Started { id, task } => {
                info!("Subagent {} started: {}", task_index, task);
                Some(WebSocketMessage::subagent_started(
                    id.clone(),
                    task.clone(),
                    task_index,
                ))
            }
            SubagentEvent::Thinking { id, content } => {
                Some(WebSocketMessage::subagent_thinking(
                    id.clone(),
                    content.clone(),
                ))
            }
            SubagentEvent::Content { id, content } => {
                Some(WebSocketMessage::subagent_content(
                    id.clone(),
                    content.clone(),
                ))
            }
            SubagentEvent::ToolStart { id, tool_name, arguments } => {
                Some(WebSocketMessage::subagent_tool_start(
                    id.clone(),
                    tool_name.clone(),
                    arguments.clone(),
                ))
            }
            SubagentEvent::ToolEnd { id, tool_name, output } => {
                Some(WebSocketMessage::subagent_tool_end(
                    id.clone(),
                    tool_name.clone(),
                    Some(output.clone()),
                ))
            }
            SubagentEvent::Completed { id, result } => {
                info!(
                    "Subagent {} completed, model={}",
                    task_index,
                    result.model.as_deref().unwrap_or("unknown")
                );
                // 生成简短摘要（取 content 前 100 字符）
                let summary: String = result.response.content
                    .chars()
                    .take(100)
                    .collect();
                Some(WebSocketMessage::subagent_completed(
                    id.clone(),
                    task_index,
                    summary,
                    result.tools_used.len() as u32,
                ))
            }
            SubagentEvent::Error { id, error } => {
                warn!("Subagent {} error: {}", task_index, error);
                Some(WebSocketMessage::subagent_error(
                    id.clone(),
                    task_index,
                    error.clone(),
                ))
            }
            SubagentEvent::Iteration { .. } => {
                // Iteration 消息静默处理，不发送到前端
                None
            }
        };

        // 发送消息
        if let (Some(msg), Some(ref key)) = (ws_msg, session_key) {
            let outbound = OutboundMessage::with_ws_message(
                key.channel.clone(),
                &key.chat_id,
                msg,
            );
            let _ = timeout(Duration::from_millis(100), outbound_tx.send(outbound)).await;
        }
    }
});
```

- [ ] **Step 2: 移除旧的行首状态追踪逻辑**

删除 `subagent_at_line_start` 相关代码，因为不再需要拼接前缀：

```rust
// 删除这些行：
// let mut subagent_at_line_start: HashMap<String, bool> = HashMap::new();
// subagent_at_line_start.insert(id.clone(), true);
// subagent_at_line_start.insert(id.clone(), content.ends_with('\n'));
// let at_start = subagent_at_line_start.get(id).copied().unwrap_or(true);
```

- [ ] **Step 3: 运行构建验证**

Run: `cd gasket && cargo build --package gasket-core --features webhook`
Expected: Build succeeds with no errors

- [ ] **Step 4: Commit**

```bash
git add gasket/core/src/tools/spawn_parallel.rs
git commit -m "refactor(core): use structured Subagent messages in spawn_parallel

- Replace text prefix concatenation with structured WebSocketMessage types
- Remove subagent_at_line_start tracking (no longer needed)
- Silently handle Iteration events (not sent to frontend)
- Use WebSocketMessage::subagent_* constructors for all events"
```

---

## Task 3: 前端 - 添加 SubagentState 类型定义

**Files:**
- Create: `web/src/types/index.ts`

- [ ] **Step 1: 创建类型定义文件**

```typescript
// web/src/types/index.ts

export interface ToolCall {
  id: string;
  name: string;
  arguments?: string;
  status: 'running' | 'complete' | 'error';
  result?: string | null;
  duration?: string;
}

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

export interface SubagentGroupMessage {
  isSubagentGroup: true;
  subagentCount: number;
  subagents: SubagentState[];
  timestamp: number;
}
```

- [ ] **Step 2: Commit**

```bash
git add web/src/types/index.ts
git commit -m "feat(web): add SubagentState type definitions"
```

---

## Task 4: 前端 - 创建 SubagentPanel 组件

**Files:**
- Create: `web/src/components/SubagentPanel.vue`

- [ ] **Step 1: 创建 SubagentPanel 组件**

```vue
<script setup lang="ts">
import { computed } from 'vue';
import type { SubagentState } from '../types';
import { ChevronDown, ChevronRight, Loader2, CheckCircle, XCircle, Wrench } from 'lucide-vue-next';

const props = defineProps<{
  subagents: Map<string, SubagentState>;
}>();

const emit = defineEmits<{
  (e: 'expand', id: string): void;
  (e: 'collapse', id: string): void;
}>();

// 展开状态
const expandedIds = ref<Set<string>>(new Set());

// 按状态分组
const runningSubagents = computed(() =>
  [...props.subagents.values()]
    .filter(s => s.status === 'running')
    .sort((a, b) => a.index - b.index)
);

const completedSubagents = computed(() =>
  [...props.subagents.values()]
    .filter(s => s.status !== 'running')
    .sort((a, b) => a.index - b.index)
);

const hasAnySubagents = computed(() => props.subagents.size > 0);
const allCompleted = computed(() =>
  hasAnySubagents.value && runningSubagents.value.length === 0
);

const toggleExpand = (id: string) => {
  if (expandedIds.value.has(id)) {
    expandedIds.value.delete(id);
  } else {
    expandedIds.value.add(id);
  }
};

const formatDuration = (start: number, end?: number) => {
  const ms = (end || Date.now()) - start;
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
};

const statusIcon = (status: SubagentState['status']) => {
  switch (status) {
    case 'running': return Loader2;
    case 'completed': return CheckCircle;
    case 'error': return XCircle;
  }
};

const statusColor = (status: SubagentState['status']) => {
  switch (status) {
    case 'running': return 'text-blue-400';
    case 'completed': return 'text-emerald-400';
    case 'error': return 'text-red-400';
  }
};
</script>

<template>
  <div v-if="hasAnySubagents" class="subagent-panel my-4">
    <!-- 运行中的 subagent 列表 -->
    <div v-if="runningSubagents.length > 0" class="space-y-2">
      <div class="flex items-center gap-2 text-sm text-slate-400 mb-2">
        <Loader2 class="w-4 h-4 animate-spin" />
        <span>并行任务 ({{ runningSubagents.length }}个)</span>
      </div>

      <div
        v-for="subagent in runningSubagents"
        :key="subagent.id"
        class="subagent-card bg-slate-800/50 border border-slate-700/50 rounded-lg overflow-hidden"
      >
        <div
          class="subagent-header flex items-center gap-2 p-3 cursor-pointer hover:bg-slate-700/30 transition-colors"
          @click="toggleExpand(subagent.id)"
        >
          <component
            :is="statusIcon(subagent.status)"
            class="w-4 h-4"
            :class="[statusColor(subagent.status), { 'animate-spin': subagent.status === 'running' }]"
          />
          <span class="font-medium text-slate-200">Task {{ subagent.index }}</span>
          <span class="text-slate-400 text-sm truncate flex-1">{{ subagent.task }}</span>
          <span class="text-xs text-slate-500 flex items-center gap-1">
            <Wrench class="w-3 h-3" />
            {{ subagent.toolCount }}
          </span>
          <component
            :is="expandedIds.has(subagent.id) ? ChevronDown : ChevronRight"
            class="w-4 h-4 text-slate-500"
          />
        </div>

        <!-- 展开详情 -->
        <Transition
          enter-active-class="transition-all duration-200 ease-out"
          leave-active-class="transition-all duration-150 ease-in"
          enter-from-class="opacity-0 max-h-0"
          leave-to-class="opacity-0 max-h-0"
        >
          <div v-if="expandedIds.has(subagent.id)" class="subagent-details border-t border-slate-700/50">
            <div class="p-3 pl-6 space-y-2">
              <!-- 思考过程 -->
              <div v-if="subagent.thinking" class="text-xs">
                <span class="text-slate-500">思考：</span>
                <p class="text-slate-400 mt-1 whitespace-pre-wrap">{{ subagent.thinking.slice(0, 300) }}{{ subagent.thinking.length > 300 ? '...' : '' }}</p>
              </div>
              <!-- 输出内容 -->
              <div v-if="subagent.content" class="text-sm">
                <span class="text-slate-500">输出：</span>
                <p class="text-slate-300 mt-1 whitespace-pre-wrap">{{ subagent.content.slice(0, 500) }}{{ subagent.content.length > 500 ? '...' : '' }}</p>
              </div>
              <!-- 工具调用 -->
              <div v-if="subagent.toolCalls.length > 0" class="text-xs">
                <span class="text-slate-500">工具调用：</span>
                <div class="mt-1 space-y-1">
                  <div
                    v-for="tool in subagent.toolCalls.slice(0, 5)"
                    :key="tool.id"
                    class="flex items-center gap-2 text-slate-400"
                  >
                    <component
                      :is="tool.status === 'running' ? Loader2 : (tool.status === 'error' ? XCircle : CheckCircle)"
                      class="w-3 h-3"
                      :class="{ 'animate-spin': tool.status === 'running' }"
                    />
                    <span>{{ tool.name }}</span>
                  </div>
                  <div v-if="subagent.toolCalls.length > 5" class="text-slate-500">
                    ... 还有 {{ subagent.toolCalls.length - 5 }} 个
                  </div>
                </div>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>

    <!-- 已完成的摘要 -->
    <div v-if="allCompleted && completedSubagents.length > 0" class="completed-summary">
      <div class="flex items-center gap-2 text-emerald-400 mb-3">
        <CheckCircle class="w-4 h-4" />
        <span class="font-medium">完成 {{ completedSubagents.length }} 个并行任务</span>
      </div>

      <div class="space-y-1.5">
        <div
          v-for="s in completedSubagents"
          :key="s.id"
          class="flex items-center gap-2 text-sm py-1.5 px-2 rounded bg-slate-800/30"
        >
          <span class="text-slate-400 w-14">Task {{ s.index }}:</span>
          <span class="text-slate-300 truncate flex-1">{{ s.summary || s.task }}</span>
          <Wrench class="w-3 h-3 text-slate-500" />
          <span class="text-xs text-slate-500 w-8">{{ s.toolCount }}</span>
          <span class="text-xs text-slate-500">{{ formatDuration(s.startTime, s.endTime) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.subagent-panel {
  animation: fadeIn 0.2s ease-out;
}

@keyframes fadeIn {
  from {
    opacity: 0;
    transform: translateY(-4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.subagent-details {
  max-height: 300px;
  overflow-y: auto;
}
</style>
```

- [ ] **Step 2: Commit**

```bash
git add web/src/components/SubagentPanel.vue
git commit -m "feat(web): add SubagentPanel component for real-time status

- Display running subagents as individual cards with animation
- Show completed subagents as merged summary
- Expandable details showing thinking, content, tool calls
- Duration and tool count display"
```

---

## Task 5: 前端 - 修改 ChatArea 消息处理

**Files:**
- Modify: `web/src/components/ChatArea.vue`

- [ ] **Step 1: 添加 Subagent 状态管理**

在 `<script setup>` 中添加：

```typescript
// web/src/components/ChatArea.vue

import type { SubagentState } from '../types';
import SubagentPanel from './SubagentPanel.vue';

// Subagent 状态管理
const activeSubagents = ref<Map<string, SubagentState>>(new Map());
const completedSubagentGroups = ref<SubagentState[][]>([]);

// 计算属性：是否有活跃的 subagent
const hasActiveSubagents = computed(() => activeSubagents.value.size > 0);
```

- [ ] **Step 2: 添加 Subagent 消息处理函数**

```typescript
// Subagent 消息处理函数
function handleSubagentStarted(msg: { id: string; task: string; index: number }) {
  activeSubagents.value.set(msg.id, {
    id: msg.id,
    index: msg.index,
    task: msg.task,
    status: 'running',
    toolCalls: [],
    toolCount: 0,
    startTime: Date.now(),
  });
}

function handleSubagentThinking(msg: { id: string; content: string }) {
  const subagent = activeSubagents.value.get(msg.id);
  if (subagent) {
    subagent.thinking = (subagent.thinking || '') + msg.content;
  }
}

function handleSubagentContent(msg: { id: string; content: string }) {
  const subagent = activeSubagents.value.get(msg.id);
  if (subagent) {
    subagent.content = (subagent.content || '') + msg.content;
  }
}

function handleSubagentToolStart(msg: { id: string; name: string; arguments?: string }) {
  const subagent = activeSubagents.value.get(msg.id);
  if (subagent) {
    const toolId = Date.now().toString() + '_' + Math.random().toString(36).substr(2, 9);
    subagent.toolCalls.push({
      id: toolId,
      name: msg.name,
      arguments: msg.arguments,
      status: 'running',
      result: null,
    });
    subagent.toolCount++;
  }
}

function handleSubagentToolEnd(msg: { id: string; name: string; output?: string }) {
  const subagent = activeSubagents.value.get(msg.id);
  if (subagent && subagent.toolCalls.length > 0) {
    // 找到匹配的运行中工具
    const tool = [...subagent.toolCalls].reverse().find(t => t.name === msg.name && t.status === 'running');
    if (tool) {
      tool.status = 'complete';
      tool.result = msg.output;
      tool.duration = ((Date.now() - parseInt(tool.id.split('_')[0])) / 1000).toFixed(1) + 's';
    }
  }
}

function handleSubagentCompleted(msg: { id: string; index: number; summary: string; tool_count: number }) {
  const subagent = activeSubagents.value.get(msg.id);
  if (subagent) {
    subagent.status = 'completed';
    subagent.summary = msg.summary;
    subagent.toolCount = msg.tool_count;
    subagent.endTime = Date.now();
  }
  checkAndFinalizeSubagents();
}

function handleSubagentError(msg: { id: string; index: number; error: string }) {
  const subagent = activeSubagents.value.get(msg.id);
  if (subagent) {
    subagent.status = 'error';
    subagent.error = msg.error;
    subagent.endTime = Date.now();
  }
  checkAndFinalizeSubagents();
}

function checkAndFinalizeSubagents() {
  const allCompleted = [...activeSubagents.value.values()]
    .every(s => s.status !== 'running');

  if (allCompleted && activeSubagents.value.size > 0) {
    finalizeSubagents();
  }
}

function finalizeSubagents() {
  const subagents = [...activeSubagents.value.values()].sort((a, b) => a.index - b.index);
  if (subagents.length === 0) return;

  // 生成合并摘要
  const summary = generateSubagentSummary(subagents);

  // 追加到最新的 bot 消息
  const lastMsg = props.messages[props.messages.length - 1];
  if (lastMsg && lastMsg.role === 'bot') {
    emit('append-to-message', lastMsg.id, summary, 'content');
  }

  // 保存完成组用于历史显示
  completedSubagentGroups.value.push(subagents);

  // 清理活跃状态
  activeSubagents.value.clear();
}

function generateSubagentSummary(subagents: SubagentState[]): string {
  const lines = ['\n\n---\n**✅ 并行任务完成**\n'];
  for (const s of subagents) {
    const status = s.status === 'error' ? '❌' : '✓';
    const duration = s.endTime ? ((s.endTime - s.startTime) / 1000).toFixed(1) : '?';
    lines.push(`${status} **Task ${s.index}**: ${s.summary || s.task} _(${s.toolCount} 工具, ${duration}s)_`);
  }
  return lines.join('\n');
}
```

- [ ] **Step 3: 修改 processWebSocketMessage 函数**

在 `processWebSocketMessage` 的 switch 语句中添加新的 case：

```typescript
const processWebSocketMessage = (msg: any, botMsg: Message) => {
  isSending.value = false;
  isReceiving.value = true;

  switch (msg.type) {
    // === 主 Agent 消息（保持原有逻辑） ===
    case 'thinking':
      isThinking.value = true;
      emit('append-to-message', botMsg.id, msg.content, 'thinking');
      break;

    case 'tool_start':
      isThinking.value = true;
      emit('ensure-tool-calls', botMsg.id);
      const toolId = Date.now().toString() + '_' + Math.random().toString(36).substr(2, 9);
      emit('push-tool-call', botMsg.id, {
        id: toolId,
        name: msg.name,
        arguments: msg.arguments || '',
        status: 'running',
        result: null
      });
      toolStartTimes.value[toolId] = Date.now();
      break;

    case 'tool_end':
      isThinking.value = true;
      const toolCalls = props.messages.find(m => m.id === botMsg.id)?.toolCalls;
      if (toolCalls && toolCalls.length > 0) {
        const matchingTool = [...toolCalls].reverse().find(t => t.name === msg.name && t.status === 'running');
        const activeTool = matchingTool || [...toolCalls].reverse().find(t => t.status === 'running') || toolCalls[toolCalls.length - 1];
        const updates: any = { status: msg.error ? 'error' : 'complete', result: msg.error || msg.output };
        if (toolStartTimes.value[activeTool.id]) {
          updates.duration = ((Date.now() - toolStartTimes.value[activeTool.id]) / 1000).toFixed(1);
          delete toolStartTimes.value[activeTool.id];
        }
        emit('update-tool-call', botMsg.id, activeTool.id, updates);
      }
      break;

    case 'content':
    case 'text':
      isThinking.value = false;
      emit('append-to-message', botMsg.id, msg.content, 'content');
      break;

    case 'error':
      isThinking.value = false;
      showError(msg.content || msg.message || 'An error occurred');
      break;

    // === 新增：Subagent 消息处理 ===
    case 'subagent_started':
      handleSubagentStarted(msg);
      break;

    case 'subagent_thinking':
      handleSubagentThinking(msg);
      break;

    case 'subagent_content':
      handleSubagentContent(msg);
      break;

    case 'subagent_tool_start':
      handleSubagentToolStart(msg);
      break;

    case 'subagent_tool_end':
      handleSubagentToolEnd(msg);
      break;

    case 'subagent_completed':
      handleSubagentCompleted(msg);
      break;

    case 'subagent_error':
      handleSubagentError(msg);
      break;

    case 'done':
      isThinking.value = false;
      isReceiving.value = false;
      setTimeout(() => {
        const scrollEl = getScrollElement(scrollAreaRef.value);
        if (scrollEl) {
          scrollEl.scrollTo({ top: scrollEl.scrollHeight, behavior: 'smooth' });
        }
      }, 150);
      break;
  }
};
```

- [ ] **Step 4: 在模板中添加 SubagentPanel**

在 `<template>` 的消息列表区域添加：

```vue
<!-- 在消息列表上方或下方插入 SubagentPanel -->
<SubagentPanel
  v-if="hasActiveSubagents"
  :subagents="activeSubagents"
  class="max-w-4xl mx-auto w-full"
/>
```

- [ ] **Step 5: 运行前端构建验证**

Run: `cd web && npm run build`
Expected: Build succeeds with no errors

- [ ] **Step 6: Commit**

```bash
git add web/src/components/ChatArea.vue
git commit -m "feat(web): add subagent message handling in ChatArea

- Add activeSubagents Map for real-time state management
- Add handlers for all subagent WebSocket message types
- Auto-finalize and generate summary when all complete
- Integrate SubagentPanel component into message flow"
```

---

## Task 6: 前端 - 更新 MessageBubble 支持摘要展示

**Files:**
- Modify: `web/src/components/MessageBubble.vue`

- [ ] **Step 1: 添加 SubagentGroupCard 组件**

在 MessageBubble.vue 中添加对 subagent 合并消息的展示支持：

```vue
<!-- 在 MessageBubble.vue 的模板中 -->

<!-- 如果消息包含 subagent 摘要标记，显示特殊样式 -->
<div v-if="message.isSubagentGroup" class="subagent-group-card mt-4 p-4 bg-slate-800/50 border border-emerald-500/20 rounded-lg">
  <div class="flex items-center gap-2 text-emerald-400 mb-3">
    <CheckCircle class="w-4 h-4" />
    <span class="font-medium">完成 {{ message.subagentCount }} 个并行任务</span>
  </div>
  <div class="prose prose-invert prose-sm max-w-none" v-html="renderMarkdown(message.content)"></div>
</div>
```

- [ ] **Step 2: Commit**

```bash
git add web/src/components/MessageBubble.vue
git commit -m "feat(web): add SubagentGroupCard display in MessageBubble

- Show special styling for subagent group messages
- Display completion status and task summary"
```

---

## Task 7: 集成测试与验证

**Files:**
- Test: Manual E2E testing

- [ ] **Step 1: 启动后端服务**

Run: `cd gasket && cargo run --release --package gasket-cli -- gateway`
Expected: Gateway starts on port 3000

- [ ] **Step 2: 启动前端开发服务器**

Run: `cd web && npm run dev`
Expected: Dev server starts on port 5173

- [ ] **Step 3: 手动测试场景**

1. **单个 subagent 执行**:
   - 发送消息触发 spawn_parallel 单任务
   - 验证：显示单个运行卡片 → 完成后合并摘要

2. **多个 subagent 并行执行**:
   - 发送消息触发 spawn_parallel 多任务
   - 验证：显示多个独立运行卡片 → 全部完成后合并

3. **Subagent 出错场景**:
   - 触发 subagent 错误
   - 验证：错误状态正确显示，摘要中标记 ❌

4. **实时流式展示**:
   - 观察 thinking 和 content 实时更新
   - 验证：内容流畅显示，无跳跃

- [ ] **Step 4: Final Commit**

```bash
git add -A
git commit -m "feat: complete subagent message isolation system

- Backend: Add 6 new WebSocketMessage types for subagent events
- Frontend: Add SubagentPanel for real-time status display
- Frontend: Add subagent state management in ChatArea
- Frontend: Add SubagentGroupCard for completed summary
- Remove confusing [Task N] prefix concatenation
- Support real-time streaming of thinking and content"
```

---

## 验收标准

- [ ] Subagent 消息不再混入主对话流
- [ ] 执行中显示独立的 subagent 卡片（带动画）
- [ ] 完成后自动合并为摘要卡片
- [ ] 思考过程和输出内容实时流式展示
- [ ] 工具调用次数正确统计
- [ ] 错误状态正确显示
- [ ] 向后兼容：主 agent 消息不受影响
