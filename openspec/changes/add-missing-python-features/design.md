## Context

nanobot-rs 已经完成了从 Python 到 Rust 的核心功能迁移（Phase 1-4），包括：
- Agent 核心循环
- 基础工具系统（文件、Shell、Web）
- 3 个 LLM 提供商（OpenAI、OpenRouter、Anthropic）
- 4 个聊天频道（Telegram、Discord、Slack、Email）
- 配置管理和 CLI 基础命令

但是，通过深入对比 Python 版本，发现仍有以下功能缺失：
- Skills 系统（动态扩展能力）
- 更多 LLM 提供商（特别是国产模型）
- 国产聊天平台
- 完整的 CLI 命令
- 语音转录能力

### 迁移约束
- 配置文件格式必须保持兼容
- CLI 命令接口保持兼容
- 不能破坏现有功能
- 代码复杂度可控（总代码量 < 15,000 行）

## Goals / Non-Goals

### Goals
1. 实现完整的 Skills 系统，支持动态扩展能力
2. 支持所有主流 LLM 提供商（特别是 DeepSeek、Gemini）
3. 提供完整的 CLI 管理命令
4. 实现语音转录功能
5. 支持国产聊天平台（至少飞书和钉钉）
6. 保持代码简洁、可维护

### Non-Goals
1. 不实现 Python 扩展绑定（pyo3）
2. 不重写 WhatsApp bridge（保留 TypeScript 版本）
3. 不改变配置文件格式
4. 不实现所有 Python 版本的渠道（只实现主流平台）
5. 不实现 Web UI（保持 CLI 优先）

## Decisions

### 1. Skills 系统设计

**决定**: 基于 Trait + 文件系统的轻量级技能系统

```rust
// 核心 trait
pub trait Skill: Send + Sync {
    fn metadata(&self) -> &SkillMetadata;
    fn content(&self) -> &str;
    fn is_available(&self) -> bool {
        // 检查依赖（bins、env vars）
    }
}

// 元数据结构
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub always: bool,  // 是否始终加载完整内容
    pub bins: Vec<String>,  // 依赖的二进制命令
    pub env_vars: Vec<String>,  // 依赖的环境变量
}
```

**加载流程**:
1. 扫描技能目录（内置 + 用户）
2. 解析 YAML frontmatter
3. 检查依赖可用性
4. 根据 `always` 标记决定加载策略
5. 注册到 Skills Registry

**渐进式加载策略**:
- `always=true`：完整内容加载到上下文
- `always=false`：只加载摘要（name + description）
- Agent 通过 `read_file` 按需加载完整内容

**理由**:
- 与 Python 版本完全兼容
- 基于文件系统，易于调试和扩展
- 无需复杂的插件系统
- 用户可以直接编辑 Markdown 文件

**替代方案**:
- WASM 插件系统：过于复杂，不符合轻量级原则
- 动态库加载：不安全且跨平台困难
- 仅内置技能：缺乏可扩展性

### 2. LLM 提供商架构

**决定**: 分层提供商系统 + Registry 模式

```rust
// 提供商层次
trait LlmProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
}

// OpenAI 兼容层（基础）
struct OpenAICompatibleProvider {
    api_base: String,
    api_key: String,
    default_model: String,
}

// 特定提供商（继承兼容层）
struct DeepSeekProvider {
    base: OpenAICompatibleProvider,
}

// 注册表
struct ProviderRegistry {
    providers: HashMap<String, Box<dyn LlmProvider>>,
    metadata: HashMap<String, ProviderMetadata>,
}

impl ProviderRegistry {
    fn detect_provider(&self, model: &str) -> Option<&dyn LlmProvider> {
        // 从模型名称推断提供商
        // 例如: "deepseek/chat" -> DeepSeekProvider
    }
}
```

**提供商分类**:
1. **OpenAI 兼容** (DeepSeek, vLLM, Groq): 继承 `OpenAICompatibleProvider`
2. **适配器** (Gemini, Zhipu): 转换请求/响应格式
3. **网关** (OpenRouter, AiHubMix): 标准 OpenAI API + 特殊 header

**配置示例**:
```json
{
  "providers": {
    "deepseek": {
      "apiKey": "${DEEPSEEK_API_KEY}",
      "apiBase": "https://api.deepseek.com/v1"
    },
    "gemini": {
      "apiKey": "${GEMINI_API_KEY}"
    }
  }
}
```

**理由**:
- 最大化代码复用（OpenAI 兼容提供商）
- 支持自动提供商检测
- 易于添加新提供商
- 与 Python 版本的 LiteLLM 风格一致

**替代方案**:
- 每个提供商完全独立实现：代码重复严重
- 使用 async-openai crate：依赖较重，部分提供商不兼容

### 3. Message Tool 设计

**决定**: 通过 MessageBus 发送消息

```rust
pub struct MessageTool {
    bus: Arc<MessageBus>,
}

impl Tool for MessageTool {
    async fn execute(&self, args: Value) -> Result<String> {
        let channel = args["channel"].as_str().unwrap();
        let chat_id = args["chat_id"].as_str().unwrap();
        let content = args["content"].as_str().unwrap();

        let msg = OutboundMessage {
            channel: channel.to_string(),
            chat_id: chat_id.to_string(),
            content: content.to_string(),
            ..Default::default()
        };

        self.bus.publish_outbound(msg).await?;
        Ok(format!("Message sent to {}:{} ", channel, chat_id))
    }
}
```

**参数 schema**:
```json
{
  "type": "object",
  "properties": {
    "channel": {"type": "string", "description": "Target channel (telegram/discord/slack)"},
    "chat_id": {"type": "string", "description": "Target chat ID"},
    "content": {"type": "string", "description": "Message content"}
  },
  "required": ["channel", "chat_id", "content"]
}
```

**理由**:
- 复用现有 MessageBus 架构
- 与其他频道实现解耦
- 支持异步消息发送
- 简单直观的 API

**安全考虑**:
- 需要验证 channel 是否已配置
- 需要权限检查（可选）

### 4. Transcription 服务设计

**决定**: Groq API 作为默认转录服务

```rust
pub trait TranscriptionService: Send + Sync {
    async fn transcribe(&self, audio_data: &[u8]) -> Result<String>;
}

pub struct GroqTranscription {
    client: reqwest::Client,
    api_key: String,
}

impl TranscriptionService for GroqTranscription {
    async fn transcribe(&self, audio_data: &[u8]) -> Result<String> {
        // 调用 Groq Whisper API
        // POST https://api.groq.com/openai/v1/audio/transcriptions
    }
}
```

**集成点**:
- Telegram Channel: 检测语音消息 -> 自动转录 -> 添加到消息内容
- 配置: `tools.transcription.enabled` + `tools.transcription.provider`

**配置示例**:
```json
{
  "tools": {
    "transcription": {
      "enabled": true,
      "provider": "groq",
      "language": "zh"
    }
  },
  "providers": {
    "groq": {
      "apiKey": "${GROQ_API_KEY}"
    }
  }
}
```

**理由**:
- Groq Whisper API 快速且免费
- 与现有 providers 配置集成
- 可选功能，不影响核心流程
- 易于扩展其他转录服务

**替代方案**:
- whisper-rs 本地转录：需要下载模型，占用磁盘空间
- OpenAI Whisper API：成本较高
- Google Speech-to-Text：需要额外依赖

### 5. CLI 子命令架构

**决定**: 使用 clap 的子命令系统

```rust
#[derive(Parser)]
#[command(name = "nanobot")]
enum Commands {
    Onboard,
    Status,
    Agent(AgentArgs),
    Gateway,
    Channels(ChannelsCommands),  // 新增
    Cron(CronCommands),          // 新增
}

#[derive(Parser)]
enum ChannelsCommands {
    Status,
    Login,  // 用于 WhatsApp 二维码登录
}

#[derive(Parser)]
enum CronCommands {
    List,
    Add {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        schedule: String,  // cron 表达式
        #[arg(short, long)]
        message: String,
    },
    Remove {
        id: String,
    },
    Enable { id: String },
    Disable { id: String },
    Run { id: String },
}
```

**输出格式**:
- `channels status`: 表格形式，显示配置和状态
- `cron list`: 表格形式，显示任务详情
- 所有命令: 支持彩色输出

**理由**:
- 与现有 CLI 架构一致
- clap 提供强大的参数解析和帮助生成
- 易于扩展新子命令
- 与 Python 版本 CLI 兼容

### 6. 国产平台集成策略

**决定**: 使用 HTTP API + 长连接

**飞书**:
- 使用 WebSocket 长连接
- 官方文档: https://open.feishu.cn/document/ukTMukTMukTM/uUTNz4SN1MjL1UzM
- 认证: App ID + App Secret
- 依赖: `tokio-tungstenite` (已有)

**钉钉**:
- 使用 Stream 模式（WebSocket）
- 官方文档: https://open.dingtalk.com/document/org/stream-to-obtain-the-chat
- 认证: Client ID + Client Secret
- 依赖: `tokio-tungstenite` (已有)

**QQ**:
- 使用官方 HTTP API 或 botpy SDK
- 官方文档: https://bot.q.qq.com/wiki/develop/api/
- 认证: AppID + AppSecret
- 依赖: 仅 `reqwest` (已有)

**理由**:
- 避免引入额外的大型 SDK
- 复用现有的 WebSocket 和 HTTP 客户端
- 代码量可控（每个约 300 行）
- Feature flag 控制编译

**替代方案**:
- 使用各平台官方 SDK：
  - 飞书: https://github.com/larksuite/oapi-sdk-rust（不成熟）
  - 钉钉: 无官方 Rust SDK
  - QQ: 无官方 Rust SDK

### 7. Subagent Manager 设计

**决定**: 基于任务队列的简单实现

```rust
pub struct SubagentManager {
    tasks: Arc<RwLock<HashMap<String, SubagentTask>>>,
    bus: Arc<MessageBus>,
}

pub struct SubagentTask {
    pub id: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
}

pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

impl SubagentManager {
    pub async fn spawn(&self, task: SubagentConfig) -> Result<String> {
        // 创建任务 ID
        // 添加到任务队列
        // 在后台 tokio task 中执行
        // 完成后通过 MessageBus 通知
    }

    pub async fn get_status(&self, id: &str) -> Option<SubagentTask> {
        // 查询任务状态
    }
}
```

**SpawnTool 增强**:
- 支持任务状态查询
- 支持任务列表
- 可选: 支持任务取消

**理由**:
- 轻量级实现，无额外依赖
- 与 MessageBus 集成，通知主 Agent
- 易于理解和维护
- 可选功能，不影响核心流程

**替代方案**:
- 使用 actor 框架（如 actix）：过于复杂
- 使用任务调度库（如 tokio-cron-scheduler）：功能重复

## Risks / Trade-offs

### 风险

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| Skills 系统过于复杂 | 中 | 高 | 先实现 MVP，逐步完善 |
| 国产平台 API 不稳定 | 中 | 中 | 使用稳定的 HTTP API，避免 SDK |
| Groq 转录服务限流 | 低 | 低 | 可选功能，失败降级 |
| 代码量激增 | 中 | 中 | 严格控制每个模块代码量 |
| 提供商测试困难 | 高 | 中 | 提供 Mock 测试，手动测试指南 |

### Trade-offs

| 选择 | 优点 | 缺点 |
|------|------|------|
| 轻量级 Skills 系统 | 简单、易扩展 | 无法执行复杂逻辑 |
| OpenAI 兼容提供商 | 代码复用 | 部分提供商不完全兼容 |
| Groq 转录 | 快速、免费 | 依赖外部服务 |
| 自实现国产平台 | 可控、轻量 | 维护成本 |
| 简单 Subagent | 易于实现 | 功能有限 |

## Migration Plan

### 迭代 1（2 周）
**目标**: 核心能力可用

**关键路径**:
1. Skills 系统框架（1 周）
2. DeepSeek 提供商（2 天）
3. Message Tool（1 天）
4. CLI channels status（2 天）
5. 集成测试（2 天）

**验收**:
```bash
# Skills 测试
nanobot agent -m "List available skills"

# DeepSeek 测试
nanobot agent -m "Hello" --model deepseek/chat

# Message Tool 测试
nanobot agent -m "Send hello to telegram:123456"

# CLI 测试
nanobot channels status
```

### 迭代 2（2 周）
**目标**: 用户体验完善

**关键路径**:
1. 剩余内置技能（3 天）
2. 所有提供商（4 天）
3. CLI cron 命令（2 天）
4. Transcription（2 天）
5. 集成测试（1 天）

**验收**:
```bash
# 技能测试
nanobot agent -m "Use github skill to create an issue"

# 提供商测试
nanobot agent -m "Hello" --model gemini/gemini-pro

# Cron 测试
nanobot cron add --name "test" --schedule "0 9 * * *" --message "Good morning"

# 转录测试
# 在 Telegram 发送语音消息，验证转录文本
```

### 迭代 3（1 周）
**目标**: 平台扩展和文档

**关键路径**:
1. 飞书频道（2 天）
2. 钉钉频道（2 天）
3. 文档和示例（2 天）
4. 最终测试（1 天）

**验收**:
```bash
# 飞书测试
nanobot gateway  # 启用飞书频道

# 文档测试
cargo doc --open

# 完整性测试
# 运行所有测试用例
```

### 回滚计划

每个迭代独立，可以按需回滚：

1. **迭代 1 回滚**: 禁用 Skills 系统，回退到基础工具
2. **迭代 2 回滚**: 保留迭代 1 功能，禁用新提供商和 CLI 命令
3. **迭代 3 回滚**: 保留核心功能，禁用国产平台

## Open Questions

### 1. Skills 安全性
**问题**: 用户技能可能包含恶意指令，如何防护？

**建议方案**:
- 添加 `skills.trusted_directories` 配置
- 技能内容只读，不允许执行代码
- 在系统提示词中明确技能边界

### 2. 提供商优先级
**问题**: 多个提供商支持同一模型时，如何选择？

**建议方案**:
- 配置 `providers.default` 指定默认提供商
- 模型名称带前缀明确指定（如 `deepseek/chat`）
- 无前缀时按注册顺序选择

### 3. Transcription 性能
**问题**: 长语音消息转录可能耗时，如何处理？

**建议方案**:
- 添加转录超时配置
- 异步处理，不阻塞主流程
- 转录失败时降级为"语音消息（转录失败）"

### 4. 国产平台认证
**问题**: 飞书/钉钉的认证流程复杂，如何简化？

**建议方案**:
- 提供详细的配置文档
- CLI 添加 `nanobot channels login` 命令（飞书）
- 配置验证和错误提示

### 5. Subagent 隔离
**问题**: 子代理任务失败是否影响主 Agent？

**建议方案**:
- 完全隔离，子代理失败只记录日志
- 通过 MessageBus 通知主 Agent 任务完成
- 主 Agent 可选择处理或忽略结果

## Implementation Notes

### 代码组织
- Skills 系统: `nanobot-core/src/skills/` (约 800 行)
- 提供商扩展: `nanobot-core/src/providers/` (每个约 150 行)
- 国产平台: `nanobot-core/src/channels/` (每个约 300 行)
- CLI 命令: `nanobot-cli/src/commands/` (约 400 行)
- Transcription: `nanobot-core/src/transcription/` (约 200 行)

### Feature Flags
```toml
[features]
default = []
skills = []  # Skills 系统
transcription = ["groq"]  # 语音转录
all-providers = ["deepseek", "gemini", "zhipu", ...]
all-channels = ["telegram", "discord", "feishu", "dingtalk", ...]
feishu = []
dingtalk = []
qq = []
```

### 测试策略
- 单元测试: 所有新模块（目标覆盖率 >80%）
- 集成测试: Skills 加载、提供商调用、CLI 命令
- E2E 测试: 完整流程（手动，需要真实 API key）

### 文档更新
- README.md: 添加新功能说明
- skills.md: 技能开发指南
- providers.md: 所有提供商配置示例
- channels.md: 所有频道配置示例
- migration.md: Python 到 Rust 迁移指南
