# Agent Pipeline Lifecycle Hooks 设计文档

## 背景

当前 agent pipeline (`loop_.rs`) 存在以下问题：

1. **硬编码的 hook 点**：`ExternalHookRunner` 只有 `pre_request` 和 `post_response` 两个固定点
2. **分散的扩展机制**：`VaultInjector`、`TextEmbedder` 各自用 `Option<T>` 和 `if let Some(...)` 实现
3. **扩展成本高**：新增拦截点需要修改核心代码，增加 `if-else` 分支
4. **Subagent 无 hook 支持**：后台任务无法插入自定义逻辑

## 目标

建立统一的 lifecycle hook 机制：
- 支持在 pipeline 关键节点注册回调
- 数据结构驱动执行策略，消除运行时判断
- 支持 Subagent 自定义 hook
- 向后兼容现有 `~/.gasket/hooks/` 脚本

## 设计原则

1. **KISS**：一个 trait，一个 registry，无复杂中间件
2. **数据结构驱动**：`HookPoint` 决定执行策略，不是运行时 `if-else`
3. **DRY**：消除重复的类型定义
4. **零特殊情况**：空 registry 直接返回 `Continue`
5. **向后兼容**：现有 shell hooks 继续工作

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

### HookContext（泛型版本，消除重复）

```rust
/// Hook 执行上下文（泛型版本）
///
/// - `MutableContext<'a>` = 可修改 messages（Sequential 点）
/// - `ReadonlyContext<'a>` = 只读 messages（Parallel 点）
pub struct HookContext<'a, M> {
    pub session_key: &'a str,
    pub messages: M,
    pub user_input: Option<&'a str>,
    pub response: Option<&'a str>,
    pub tool_calls: Option<&'a [ToolCallInfo]>,
    pub token_usage: Option<&'a TokenUsage>,
}

/// 可写上下文（Sequential 点使用）
pub type MutableContext<'a> = HookContext<'a, &'a mut Vec<ChatMessage>>;

/// 只读上下文（Parallel 点使用）
pub type ReadonlyContext<'a> = HookContext<'a, &'a [ChatMessage]>;

impl<'a> MutableContext<'a> {
    /// 转换为只读视图（用于 Parallel 执行）
    pub fn as_readonly(&self) -> ReadonlyContext<'a> {
        HookContext {
            session_key: self.session_key,
            messages: &*self.messages,
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
    /// 默认实现返回 Continue，hook 只需 override 需要的方法
    async fn run(&self, ctx: &mut MutableContext<'_>) -> Result<HookAction, AgentError> {
        Ok(HookAction::Continue)
    }

    /// Parallel 点调用（只读）
    async fn run_parallel(&self, ctx: &ReadonlyContext<'_>) -> Result<HookAction, AgentError> {
        Ok(HookAction::Continue)
    }
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

    /// 创建空 registry（用于 subagent 默认配置）
    pub fn empty() -> Arc<Self> {
        Arc::new(Self::new())
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
        ctx: &mut MutableContext<'_>,
    ) -> Result<HookAction, AgentError> {
        let hooks = self.get_hooks(point);
        if hooks.is_empty() {
            return Ok(HookAction::Continue);
        }

        match point.default_strategy() {
            ExecutionStrategy::Sequential => {
                for hook in hooks {
                    tracing::debug!("[Hook] Running {} at {:?}", hook.name(), point);
                    let action = hook.run(ctx).await?;
                    if let HookAction::Abort(msg) = action {
                        tracing::warn!("[Hook] {} aborted: {}", hook.name(), msg);
                        return Ok(HookAction::Abort(msg));
                    }
                }
                Ok(HookAction::Continue)
            }
            ExecutionStrategy::Parallel => {
                let view = ctx.as_readonly();
                let results = futures::future::join_all(
                    hooks.iter().map(|h| {
                        tracing::debug!("[Hook] Running {} at {:?}", h.name(), point);
                        h.run_parallel(&view)
                    })
                ).await;

                // 检查是否有 abort
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

impl ExternalShellHook {
    pub fn new(runner: ExternalHookRunner, point: HookPoint) -> Self {
        Self { runner, point }
    }
}

#[async_trait]
impl PipelineHook for ExternalShellHook {
    fn name(&self) -> &str { "external_shell" }
    fn point(&self) -> HookPoint { self.point }

    async fn run(&self, ctx: &mut MutableContext<'_>) -> Result<HookAction, AgentError> {
        // BeforeRequest: 调用 run_pre_request
        let input = ctx.user_input.unwrap_or("");
        let output = self.runner.run_pre_request(ctx.session_key, input).await
            .map_err(|e| AgentError::Hook(e.to_string()))?;

        match output {
            Some(out) if out.is_abort() => Ok(HookAction::Abort(out.error.unwrap_or_default())),
            Some(out) => {
                if let Some(modified) = out.modified_message {
                    // 修改第一条 user message 的 content
                    for msg in ctx.messages.iter_mut() {
                        if msg.role == MessageRole::User {
                            msg.content = modified;
                            break;
                        }
                    }
                }
                Ok(HookAction::Continue)
            }
            None => Ok(HookAction::Continue),
        }
    }

    async fn run_parallel(&self, ctx: &ReadonlyContext<'_>) -> Result<HookAction, AgentError> {
        // AfterResponse: 调用 run_post_response
        let response = ctx.response.unwrap_or("");
        let tools = ctx.tool_calls
            .map(|t| t.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
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
    /// 存储 injected values 用于后续 redaction
    injected_values: Arc<RwLock<Vec<String>>>,
}

impl VaultHook {
    pub fn new(injector: VaultInjector) -> Self {
        Self {
            injector,
            injected_values: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn injected_values(&self) -> Arc<RwLock<Vec<String>>> {
        self.injected_values.clone()
    }
}

#[async_trait]
impl PipelineHook for VaultHook {
    fn name(&self) -> &str { "vault_injector" }
    fn point(&self) -> HookPoint { HookPoint::BeforeLLM }

    async fn run(&self, ctx: &mut MutableContext<'_>) -> Result<HookAction, AgentError> {
        let report = self.injector.inject(ctx.messages);
        if !report.keys_used.is_empty() {
            tracing::debug!(
                "[VaultHook] Injected {} keys into {} messages",
                report.keys_used.len(),
                report.messages_modified
            );
        }
        // 存储 injected_values 供后续 redaction 使用
        *self.injected_values.write().await = report.injected_values;
        Ok(HookAction::Continue)
    }
}
```

### HistoryRecallHook

```rust
pub struct HistoryRecallHook {
    embedder: Arc<TextEmbedder>,
    k: usize,
    context: Arc<dyn AgentContext>,
}

#[async_trait]
impl PipelineHook for HistoryRecallHook {
    fn name(&self) -> &str { "history_recall" }
    fn point(&self) -> HookPoint { HookPoint::AfterHistory }

    async fn run(&self, ctx: &mut MutableContext<'_>) -> Result<HookAction, AgentError> {
        if self.k == 0 {
            return Ok(HookAction::Continue);
        }

        let query = ctx.user_input.unwrap_or("");
        match self.embedder.embed(query) {
            Ok(query_vec) => {
                match self.context.recall_history(ctx.session_key, &query_vec, self.k).await {
                    Ok(recalled) if !recalled.is_empty() => {
                        tracing::debug!("[HistoryRecall] Recalled {} messages", recalled.len());
                        // 注入召回的历史到 messages
                        let recall_msg = format!(
                            "# Relevant Historical Context\n{}",
                            recalled.join("\n")
                        );
                        ctx.messages.push(ChatMessage::assistant(recall_msg));
                    }
                    _ => {}
                }
                Ok(HookAction::Continue)
            }
            Err(e) => {
                tracing::debug!("[HistoryRecall] Failed to embed query: {}", e);
                Ok(HookAction::Continue)
            }
        }
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
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    config: AgentConfig,
    workspace: PathBuf,
    history_config: HistoryConfig,
    context: Arc<dyn AgentContext>,
    system_prompt: String,
    skills_context: Option<String>,
    pricing: Option<ModelPricing>,
    /// Hook Registry — 替代所有分散的 hook 字段
    hooks: Arc<HookRegistry>,
    /// Vault injected values（用于 redaction）
    vault_values: Arc<RwLock<Vec<String>>>,
}
```

### AgentLoop::builder 改造

```rust
impl AgentLoop {
    /// 创建用于 subagent 的 builder（空 hooks）
    pub fn builder(
        provider: Arc<dyn LlmProvider>,
        workspace: PathBuf,
        config: AgentConfig,
        tools: Arc<ToolRegistry>,
    ) -> Result<Self, AgentError> {
        let context: Arc<dyn AgentContext> = Arc::new(StatelessContext::new());

        Ok(Self {
            provider,
            tools,
            config,
            workspace,
            history_config: HistoryConfig::default(),
            context,
            system_prompt: String::new(),
            skills_context: None,
            pricing: None,
            hooks: HookRegistry::empty(),  // 空 registry
            vault_values: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// 设置自定义 hooks（用于 subagent）
    pub fn with_hooks(mut self, hooks: Arc<HookRegistry>) -> Self {
        self.hooks = hooks;
        self
    }

    /// 设置 system prompt
    pub fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = prompt;
    }
}
```

### 执行流程

```rust
impl AgentLoop {
    pub async fn process_direct(
        &self,
        content: &str,
        session_key: &SessionKey,
    ) -> Result<AgentResponse, AgentError> {
        let session_key_str = session_key.to_string();

        // 1. 加载 session + 构建初始 messages
        let session = self.context.load_session(session_key).await;
        let history_snapshot = session.get_history(self.config.memory_window);
        self.context.save_message(session_key, "user", content, None).await;

        // 2. BeforeRequest Hook
        let mut messages = vec![ChatMessage::user(content)];
        let mut ctx = MutableContext {
            session_key: &session_key_str,
            messages: &mut messages,
            user_input: Some(content),
            response: None,
            tool_calls: None,
            token_usage: None,
        };

        match self.hooks.execute(HookPoint::BeforeRequest, &mut ctx).await? {
            HookAction::Abort(msg) => return Ok(AgentResponse::aborted(msg)),
            HookAction::Continue => {}
        }

        // 3. 处理历史
        let processed = process_history(history_snapshot, &self.history_config);
        let summary = self.context.load_summary(&session_key_str).await;
        if !processed.evicted.is_empty() {
            self.context.compress_context(&session_key_str, &processed.evicted);
        }

        // 4. 组装 prompt
        let mut full_messages = Self::assemble_prompt(
            &self.system_prompt,
            &self.skills_context,
            processed.messages,
            content,
            summary.as_deref(),
        );

        // 5. AfterHistory Hook
        let mut ctx = MutableContext {
            session_key: &session_key_str,
            messages: &mut full_messages,
            user_input: Some(content),
            response: None,
            tool_calls: None,
            token_usage: None,
        };
        self.hooks.execute(HookPoint::AfterHistory, &mut ctx).await?;

        // 6. BeforeLLM Hook（Vault 注入）
        self.hooks.execute(HookPoint::BeforeLLM, &mut ctx).await?;

        // 7. 执行 LLM 循环（内部触发 AfterToolCall）
        let result = self.run_agent_loop_with_hooks(full_messages, &session_key_str).await?;

        // 8. AfterResponse Hook
        let mut ctx_for_response = MutableContext {
            session_key: &session_key_str,
            messages: &mut result.messages,
            user_input: Some(content),
            response: Some(&result.content),
            tool_calls: None,
            token_usage: result.token_usage.as_ref(),
        };
        self.hooks.execute(HookPoint::AfterResponse, &mut ctx_for_response).await?;

        // 9. 保存 assistant message
        self.context.save_message(
            session_key,
            "assistant",
            &result.content,
            Some(result.tools_used.clone()),
        ).await;

        Ok(AgentResponse::from(result))
    }
}
```

## Subagent Hook 支持

### SubagentTaskBuilder 扩展

```rust
pub struct SubagentTaskBuilder<'a> {
    manager: &'a SubagentManager,
    subagent_id: String,
    task: String,
    provider: Option<Arc<dyn LlmProvider>>,
    agent_config: Option<AgentConfig>,
    event_tx: Option<mpsc::Sender<SubagentEvent>>,
    system_prompt: Option<String>,
    session_key: Option<SessionKey>,
    cancellation_token: Option<CancellationToken>,
    /// 自定义 hooks（新增）
    hooks: Option<Arc<HookRegistry>>,
}

impl<'a> SubagentTaskBuilder<'a> {
    /// 设置自定义 hooks
    ///
    /// 允许 subagent 使用与主 agent 相同的 hooks，
    /// 或配置专属的 hooks（如监控、日志）。
    pub fn with_hooks(mut self, hooks: Arc<HookRegistry>) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// 继承主 agent 的 hooks
    ///
    /// 用于需要与主 agent 共享 vault、监控等功能的场景。
    pub fn inherit_hooks(mut self, agent_hooks: Arc<HookRegistry>) -> Self {
        self.hooks = Some(agent_hooks);
        self
    }

    pub async fn spawn(self, result_tx: mpsc::Sender<SubagentResult>) -> anyhow::Result<String> {
        // ... existing code ...

        let mut agent = AgentLoop::builder(provider, workspace, agent_config, tools)?;

        // 应用自定义 hooks
        if let Some(hooks) = self.hooks {
            agent = agent.with_hooks(hooks);
        }

        // ... rest of spawn logic ...
    }
}
```

### Subagent 支持的 HookPoint

| HookPoint | Subagent 支持？ | 说明 |
|-----------|----------------|------|
| BeforeRequest | ✅ | 可修改输入、注入上下文 |
| AfterHistory | ❌ | Subagent 无历史 |
| BeforeLLM | ✅ | 可注入 vault |
| AfterToolCall | ✅ | 监控/日志 |
| AfterResponse | ✅ | 审计/通知 |

### 使用示例

```rust
// 场景 1：Subagent 使用独立的监控 hook
let monitor_hook = Arc::new(MonitoringHook::new());
let hooks = HookBuilder::new()
    .with_hook(monitor_hook)
    .build();

manager.task("task-1", "Analyze code")
    .with_hooks(Arc::new(hooks))
    .spawn(result_tx)
    .await?;

// 场景 2：Subagent 继承主 agent 的 hooks（共享 vault）
manager.task("task-2", "Process secrets")
    .inherit_hooks(main_agent_hooks)
    .spawn(result_tx)
    .await?;
```

## HookBuilder

```rust
pub struct HookBuilder {
    hooks: Vec<Arc<dyn PipelineHook>>,
}

impl HookBuilder {
    pub fn new() -> Self { Self { hooks: Vec::new() } }

    pub fn with_external_hooks(mut self, runner: ExternalHookRunner) -> Self {
        self.hooks.push(Arc::new(ExternalShellHook::new(
            runner.clone(),
            HookPoint::BeforeRequest,
        )));
        self.hooks.push(Arc::new(ExternalShellHook::new(
            runner,
            HookPoint::AfterResponse,
        )));
        self
    }

    pub fn with_vault(mut self, injector: VaultInjector) -> Self {
        self.hooks.push(Arc::new(VaultHook::new(injector)));
        self
    }

    pub fn with_history_recall(mut self, embedder: Arc<TextEmbedder>, k: usize, context: Arc<dyn AgentContext>) -> Self {
        self.hooks.push(Arc::new(HistoryRecallHook::new(embedder, k, context)));
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

    /// 构建 Arc<HookRegistry>（用于共享）
    pub fn build_shared(self) -> Arc<HookRegistry> {
        Arc::new(self.build())
    }
}

impl Default for HookBuilder {
    fn default() -> Self {
        Self::new()
    }
}
```

## 文件结构

```
gasket-core/src/hooks/
├── mod.rs           # 导出
├── types.rs         # HookPoint, HookContext, MutableContext, ReadonlyContext, HookAction, ExecutionStrategy
├── registry.rs      # HookRegistry, HookBuilder
├── external.rs      # ExternalShellHook (改造自现有)
├── vault.rs         # VaultHook
└── history.rs       # HistoryRecallHook

gasket-core/src/agent/
├── loop_.rs         # AgentLoop 改造
└── subagent.rs      # SubagentTaskBuilder 扩展
```

## 向后兼容

| 现有功能 | 兼容性 |
|---------|--------|
| `~/.gasket/hooks/pre_request.sh` | 完全兼容，包装为 ExternalShellHook |
| `~/.gasket/hooks/post_response.sh` | 完全兼容，包装为 ExternalShellHook |
| Subagent 的默认行为 | 兼容，使用空 HookRegistry |
| `process_direct` API | 签名不变 |
| `AgentLoop::builder()` | 签名不变，返回空 hooks |

## 测试策略

### 单元测试

1. **HookRegistry 测试**
   - 注册和检索
   - Sequential/Parallel 策略的正确调用
   - Abort 语义的正确传播
   - 空 registry 返回 Continue

2. **Context 测试**
   - MutableContext 到 ReadonlyContext 的转换
   - 泛型类型的正确行为

### 集成测试

1. **ExternalShellHook 测试**
   - 与真实脚本交互
   - abort 和 modify 语义

2. **VaultHook 测试**
   - 注入和 redaction
   - injected_values 存储

3. **Subagent 测试**
   - with_hooks() 配置
   - inherit_hooks() 共享

4. **端到端测试**
   - 完整 pipeline 执行
   - 多 hook 协作

## 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| 执行顺序改变 | 保持与现有代码相同的注册顺序 |
| 并行 hook 的 abort 语义 | 文档明确说明，abort 在所有 hook 完成后检查 |
| Hook 失败处理 | Sequential 失败立即停止，Parallel 记录日志继续 |
| Subagent hooks 继承 | 提供 inherit_hooks() 显式共享，避免隐式依赖 |

## 未来扩展

1. **配置文件驱动**：从 `config.yaml` 加载 hook 配置
2. **动态加载**：支持 WASM 或动态库形式的 hook
3. **Hook 优先级**：在同一 HookPoint 内支持排序
4. **Metrics**：记录每个 hook 的执行时间
5. **Hook 链**：支持 hook 的输出作为下一个 hook 的输入
