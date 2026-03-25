# Agent Pipeline Lifecycle Hooks 设计文档

## 背景

当前 agent pipeline (`loop_.rs`) 存在以下问题：

1. **硬编码的 hook 点**：`ExternalHookRunner` 只有 `pre_request` 和 `post_response` 两个固定点
2. **分散的扩展机制**：`VaultInjector`、`TextEmbedder` 各自用 `Option<T>` 和 `if let Some(...)` 实现
3. **扩展成本高**：新增拦截点需要修改核心代码，增加 `if-else` 分支

## 目标

建立统一的 lifecycle hook 机制：
- 支持在 pipeline 关键节点注册回调
- 数据结构驱动执行策略，消除运行时判断
- 向后兼容现有 `~/.gasket/hooks/` 脚本

## 设计原则

1. **KISS**：一个 trait，一个 registry，无复杂中间件
2. **数据结构驱动**：`HookPoint` 决定执行策略，不是运行时 `if-else`
3. **零特殊情况**：空 registry 直接返回 `Continue`
4. **向后兼容**：现有 shell hooks 继续工作

## 核心类型

### HookPoint

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPoint {
    /// 收到用户消息后，处理前
    BeforeRequest,
    /// 加载历史记录后，组装 prompt 前
    AfterHistory,
    /// 发送到 LLM 前
    BeforeLLM,
    /// 工具调用完成后
    AfterToolCall,
    /// 响应发送给用户前
    AfterResponse,
}
```

### ExecutionStrategy

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStrategy {
    /// 顺序执行，失败即停，可修改 messages
    Sequential,
    /// 并行执行，只读，用于审计/日志/通知
    Parallel,
}

impl HookPoint {
    pub fn default_strategy(&self) -> ExecutionStrategy {
        match self {
            Self::BeforeRequest => ExecutionStrategy::Sequential,
            Self::AfterHistory => ExecutionStrategy::Sequential,
            Self::BeforeLLM => ExecutionStrategy::Sequential,
            Self::AfterToolCall => ExecutionStrategy::Parallel,
            Self::AfterResponse => ExecutionStrategy::Parallel,
        }
    }
}
```

### HookAction

```rust
#[derive(Debug, Clone)]
pub enum HookAction {
    /// 继续执行
    Continue,
    /// 中止 pipeline，返回错误消息给用户
    Abort(String),
}
```

### HookContext

```rust
/// 只读视图（Parallel 点使用）
pub struct HookContextView<'a> {
    pub session_key: &'a str,
    pub messages: &'a [ChatMessage],
    pub user_input: Option<&'a str>,
    pub response: Option<&'a str>,
    pub tool_calls: Option<&'a [ToolCallInfo]>,
    pub token_usage: Option<&'a TokenUsage>,
}

/// 可写上下文（Sequential 点使用）
pub struct HookContext<'a> {
    pub session_key: &'a str,
    pub messages: &'a mut Vec<ChatMessage>,
    pub user_input: Option<&'a str>,
    pub response: Option<&'a str>,
    pub tool_calls: Option<&'a [ToolCallInfo]>,
    pub token_usage: Option<&'a TokenUsage>,
}

impl<'a> HookContext<'a> {
    pub fn as_view(&self) -> HookContextView<'a> {
        HookContextView {
            session_key: self.session_key,
            messages: self.messages,
            user_input: self.user_input,
            response: self.response,
            tool_calls: self.tool_calls,
            token_usage: self.token_usage,
        }
    }
}
```

### PipelineHook Trait

```rust
#[async_trait]
pub trait PipelineHook: Send + Sync {
    /// Hook 名称（用于日志和调试）
    fn name(&self) -> &str;

    /// 执行点
    fn point(&self) -> HookPoint;

    /// Sequential 点调用（可修改 messages）
    async fn run(&self, ctx: &mut HookContext<'_>) -> Result<HookAction, AgentError>;

    /// Parallel 点调用（只读）
    async fn run_parallel(&self, ctx: &HookContextView<'_>) -> Result<HookAction, AgentError>;
}
```

## HookRegistry

```rust
pub struct HookRegistry {
    hooks: HashMap<HookPoint, Vec<Arc<dyn PipelineHook>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: HashMap::new() }
    }

    pub fn register(&mut self, hook: Arc<dyn PipelineHook>) {
        self.hooks.entry(hook.point()).or_default().push(hook);
    }

    pub fn get_hooks(&self, point: HookPoint) -> &[Arc<dyn PipelineHook>] {
        self.hooks.get(&point).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub async fn execute(
        &self,
        point: HookPoint,
        ctx: &mut HookContext<'_>,
    ) -> Result<HookAction, AgentError> {
        let hooks = self.get_hooks(point);
        if hooks.is_empty() {
            return Ok(HookAction::Continue);
        }

        match point.default_strategy() {
            ExecutionStrategy::Sequential => {
                for hook in hooks {
                    let action = hook.run(ctx).await?;
                    if let HookAction::Abort(msg) = action {
                        return Ok(HookAction::Abort(msg));
                    }
                }
                Ok(HookAction::Continue)
            }
            ExecutionStrategy::Parallel => {
                let view = ctx.as_view();
                let results = futures::future::join_all(
                    hooks.iter().map(|h| h.run_parallel(&view))
                ).await;

                for result in results {
                    if let Ok(HookAction::Abort(msg)) = result {
                        return Ok(HookAction::Abort(msg));
                    }
                }
                Ok(HookAction::Continue)
            }
        }
    }
}
```

## 现有代码迁移

### ExternalShellHook

将 `ExternalHookRunner` 包装为 `PipelineHook`：

```rust
pub struct ExternalShellHook {
    runner: ExternalHookRunner,
    point: HookPoint,
}

#[async_trait]
impl PipelineHook for ExternalShellHook {
    fn name(&self) -> &str { "external_shell" }
    fn point(&self) -> HookPoint { self.point }

    async fn run(&self, ctx: &mut HookContext<'_>) -> Result<HookAction, AgentError> {
        // BeforeRequest: 调用 run_pre_request
        let input = ctx.user_input.unwrap_or("");
        let output = self.runner.run_pre_request(ctx.session_key, input).await
            .map_err(|e| AgentError::Hook(e.to_string()))?;

        match output {
            Some(out) if out.is_abort() => Ok(HookAction::Abort(out.error.unwrap_or_default())),
            Some(out) => {
                if let Some(modified) = out.modified_message {
                    // 修改第一条 user message
                }
                Ok(HookAction::Continue)
            }
            None => Ok(HookAction::Continue),
        }
    }

    async fn run_parallel(&self, ctx: &HookContextView<'_>) -> Result<HookAction, AgentError> {
        // AfterResponse: 调用 run_post_response
        let response = ctx.response.unwrap_or("");
        let tools = ctx.tool_calls.map(|t| /* ... */).unwrap_or_default();
        self.runner.run_post_response(ctx.session_key, response, &tools).await
            .map_err(|e| AgentError::Hook(e.to_string()))?;
        Ok(HookAction::Continue)
    }
}
```

### VaultHook

```rust
pub struct VaultHook {
    injector: VaultInjector,
}

#[async_trait]
impl PipelineHook for VaultHook {
    fn name(&self) -> &str { "vault_injector" }
    fn point(&self) -> HookPoint { HookPoint::BeforeLLM }

    async fn run(&self, ctx: &mut HookContext<'_>) -> Result<HookAction, AgentError> {
        let report = self.injector.inject(ctx.messages);
        // 存储 injected_values 供后续 redaction
        Ok(HookAction::Continue)
    }
}
```

### HistoryRecallHook

```rust
pub struct HistoryRecallHook {
    embedder: Arc<TextEmbedder>,
    k: usize,
}

#[async_trait]
impl PipelineHook for HistoryRecallHook {
    fn name(&self) -> &str { "history_recall" }
    fn point(&self) -> HookPoint { HookPoint::AfterHistory }

    async fn run(&self, ctx: &mut HookContext<'_>) -> Result<HookAction, AgentError> {
        // 语义召回历史消息，注入到 messages
        Ok(HookAction::Continue)
    }
}
```

## AgentLoop 改造

### 改造前

```rust
pub struct AgentLoop {
    // ...
    external_hooks: ExternalHookRunner,
    vault_injector: Option<VaultInjector>,
    embedder: Option<Arc<TextEmbedder>>,
    history_recall_k: usize,
}
```

### 改造后

```rust
pub struct AgentLoop {
    // ...
    hooks: Arc<HookRegistry>,
}
```

### 执行流程

```rust
pub async fn process_direct(&self, content: &str, session_key: &SessionKey) -> Result<AgentResponse, AgentError> {
    // 1. BeforeRequest
    self.hooks.execute(HookPoint::BeforeRequest, &mut ctx).await?;

    // 2. Load history + save user message
    // ...

    // 3. AfterHistory
    self.hooks.execute(HookPoint::AfterHistory, &mut ctx).await?;

    // 4. BeforeLLM
    self.hooks.execute(HookPoint::BeforeLLM, &mut ctx).await?;

    // 5. LLM loop
    let result = self.run_agent_loop(messages).await?;

    // 6. AfterToolCall (inside agent loop)

    // 7. AfterResponse
    self.hooks.execute(HookPoint::AfterResponse, &mut ctx).await?;

    Ok(result)
}
```

## HookBuilder

```rust
pub struct HookBuilder {
    hooks: Vec<Arc<dyn PipelineHook>>,
}

impl HookBuilder {
    pub fn new() -> Self { Self { hooks: Vec::new() } }

    pub fn with_external_hooks(mut self, runner: ExternalHookRunner) -> Self {
        self.hooks.push(Arc::new(ExternalShellHook::new(runner.clone(), HookPoint::BeforeRequest)));
        self.hooks.push(Arc::new(ExternalShellHook::new(runner, HookPoint::AfterResponse)));
        self
    }

    pub fn with_vault(mut self, injector: VaultInjector) -> Self {
        self.hooks.push(Arc::new(VaultHook::new(injector)));
        self
    }

    pub fn with_history_recall(mut self, embedder: Arc<TextEmbedder>, k: usize) -> Self {
        self.hooks.push(Arc::new(HistoryRecallHook::new(embedder, k)));
        self
    }

    pub fn with_hook(mut self, hook: Arc<dyn PipelineHook>) -> Self {
        self.hooks.push(hook);
        self
    }

    pub fn build(self) -> HookRegistry {
        let mut registry = HookRegistry::new();
        for hook in self.hooks {
            registry.register(hook);
        }
        registry
    }
}
```

## 文件结构

```
gasket-core/src/hooks/
├── mod.rs           # 导出
├── types.rs         # HookPoint, HookContext, HookAction, ExecutionStrategy
├── registry.rs      # HookRegistry, HookBuilder
├── external.rs      # ExternalShellHook (改造自现有)
├── vault.rs         # VaultHook
└── history.rs       # HistoryRecallHook
```

## 向后兼容

| 现有功能 | 兼容性 |
|---------|--------|
| `~/.gasket/hooks/pre_request.sh` | 完全兼容，包装为 ExternalShellHook |
| `~/.gasket/hooks/post_response.sh` | 完全兼容，包装为 ExternalShellHook |
| Subagent 的 noop hooks | 兼容，使用空 HookRegistry |
| `process_direct` API | 签名不变 |

## 测试策略

1. **单元测试**
   - HookRegistry 的注册和执行
   - Sequential/Parallel 策略的正确调用
   - Abort 语义的正确传播

2. **集成测试**
   - ExternalShellHook 与真实脚本交互
   - VaultHook 注入和 redaction
   - 端到端 pipeline 执行

## 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| 执行顺序改变 | 保持与现有代码相同的注册顺序 |
| 并行 hook 的 abort 语义 | 文档明确说明，abort 是"best effort" |
| Hook 失败处理 | Sequential 失败立即停止，Parallel 记录日志继续 |

## 未来扩展

1. **配置文件驱动**：从 `config.yaml` 加载 hook 配置
2. **动态加载**：支持 WASM 或动态库形式的 hook
3. **Hook 优先级**：在同一 HookPoint 内支持排序
4. **Metrics**：记录每个 hook 的执行时间
