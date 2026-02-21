# Change: Add Missing Python Features to Rust Implementation

## Why

nanobot-rs 已经完成了核心框架的迁移（Phase 1-4），但经过与 Python 版本的深入对比分析，发现以下关键功能仍然缺失：

1. **Skills 系统** - Python 版本最强大的可扩展特性
2. **更多 LLM 提供商** - 当前仅支持 3/13 提供商
3. **国产聊天平台** - 缺少飞书、钉钉、QQ 等中国主流平台
4. **完整的 CLI 命令** - 缺少 channels 和 cron 子命令
5. **语音转录** - 缺少 Whisper 集成
6. **Subagent 管理器** - 缺少完整的后台任务框架

这些功能的缺失会影响：
- **可扩展性** - 无法动态加载和管理技能
- **市场适配** - 无法使用国产模型和国产聊天平台
- **用户体验** - CLI 功能不完整，无法处理语音消息

## What Changes

### 高优先级功能（核心能力）

#### 1. Skills 系统（全新功能）
- **Skills Loader** - 动态加载技能模块
  - YAML frontmatter 元数据解析
  - 依赖检查（bins、env vars）
  - 渐进式加载（always=true vs 按需加载）
  - 内置技能和工作区技能
- **内置技能库**
  - `github` - GitHub CLI 集成
  - `cron` - 定时任务管理
  - `memory` - 记忆管理指南
  - `summarize` - 文本摘要
  - `skill-creator` - 创建新技能
  - `tmux` - Tmux 操作
  - `weather` - 天气查询
- **Skills Registry** - 技能注册和管理

#### 2. LLM Providers 扩展（新增 10+ 提供商）
- **国产模型**
  - DeepSeek（深度求索）
  - Zhipu（智谱 GLM）
  - DashScope（阿里云灵积）
  - Moonshot（月之暗面）
  - MiniMax
- **国际模型**
  - Gemini（Google）
  - Groq（转录专用）
- **网关服务**
  - AiHubMix
- **本地部署**
  - vLLM
- **Providers Registry** - 统一的提供商注册表
  - 自动提供商检测
  - 模型前缀处理
  - 模型特定参数覆盖

#### 3. Message Tool（新增工具）
- 主动发送消息到特定频道
- 支持上下文路由（channel、chat_id）
- 集成到 Agent 工具集

### 中优先级功能（用户体验）

#### 4. CLI 子命令扩展
- **`nanobot channels`** 子命令
  - `channels status` - 显示所有频道状态
  - `channels login` - 二维码登录（WhatsApp）
- **`nanobot cron`** 子命令
  - `cron list` - 列出所有定时任务
  - `cron add` - 添加新任务
  - `cron remove` - 删除任务
  - `cron enable/disable` - 启用/禁用任务
  - `cron run <id>` - 手动运行任务

#### 5. Transcription 服务（语音转录）
- Groq API Whisper 集成
- 音频文件转文字
- Telegram 语音消息自动转录
- 支持多种音频格式

#### 6. Subagent Manager 增强
- 完整的后台任务框架
- 任务状态追踪
- 结果通知机制
- 与主 Agent 的消息总线集成

### 低优先级功能（平台扩展）

#### 7. 聊天频道扩展
- **Feishu（飞书）** - WebSocket 长连接
- **DingTalk（钉钉）** - Stream 模式
- **QQ** - botpy SDK 集成
- **WhatsApp** - WebSocket 桥接

## Impact

### 受影响的规范
- **新建规范**：
  - `skills-system` - 技能系统规范
  - `llm-providers` - LLM 提供商扩展规范
  - `chat-channels` - 聊天频道扩展规范
  - `cli-commands` - CLI 命令扩展规范
- **修改规范**：
  - `agent/tools` - 添加 Message Tool
  - `providers` - 扩展提供商注册表
  - `cli` - 添加子命令

### 受影响的代码

#### 新增模块
- `nanobot-core/src/skills/` - 技能系统（约 800 行）
  - `loader.rs` - 技能加载器
  - `registry.rs` - 技能注册表
  - `metadata.rs` - 元数据解析
  - `builtin/` - 内置技能（7 个技能文件）
- `nanobot-core/src/tools/message.rs` - Message Tool（约 100 行）
- `nanobot-core/src/transcription/` - 语音转录（约 200 行）
- `nanobot-core/src/providers/` - 扩展提供商（每个约 150 行）
  - `deepseek.rs`
  - `gemini.rs`
  - `zhipu.rs`
  - `dashscope.rs`
  - `moonshot.rs`
  - `minimax.rs`
  - `vllm.rs`
  - `groq.rs`
  - `aihubmix.rs`
- `nanobot-core/src/channels/` - 扩展频道（每个约 300 行）
  - `feishu.rs`
  - `dingtalk.rs`
  - `qq.rs`
  - `whatsapp.rs`
- `nanobot-cli/src/commands/` - CLI 子命令（约 400 行）
  - `channels.rs`
  - `cron.rs`

#### 修改模块
- `nanobot-core/src/providers/registry.rs` - 提供商注册表
- `nanobot-core/src/tools/registry.rs` - 工具注册
- `nanobot-core/src/agent/loop.rs` - Agent 循环
- `nanobot-cli/src/main.rs` - CLI 入口

### 迁移影响
- **配置兼容性**: 完全兼容现有配置格式
- **API 兼容性**: 所有现有 CLI 命令保持兼容
- **工作区结构**: 无需修改用户工作区

### 风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| Skills 系统复杂度高 | 高 | 分阶段实现，先框架后内置技能 |
| 国产平台 API 差异 | 中 | 使用官方 SDK 或 HTTP API |
| Groq 转录依赖外部服务 | 低 | 可选功能，失败降级 |
| 代码量增加（~3000 行） | 中 | 模块化设计，按需启用 feature |

## Success Criteria

### 必须达成（P0）
- [ ] Skills 系统框架完成，至少加载 3 个内置技能
- [ ] DeepSeek 和 Gemini 提供商可用
- [ ] `nanobot channels status` 和 `nanobot cron list` 命令可用
- [ ] Message Tool 可以主动发送消息

### 应该达成（P1）
- [ ] 至少 5 个内置技能可用
- [ ] 所有 13 个 LLM 提供商实现完成
- [ ] 完整的 `nanobot cron` 子命令
- [ ] 语音转录功能可用

### 可以达成（P2）
- [ ] 飞书和钉钉频道实现
- [ ] WhatsApp 桥接
- [ ] QQ 机器人
- [ ] Subagent Manager 完整框架

## Timeline

建议分 3 个迭代完成：

### 迭代 1（2 周）- 核心能力
- Skills 系统框架 + 3 个内置技能
- DeepSeek + Gemini 提供商
- Message Tool
- `channels status` 命令

### 迭代 2（2 周）- 用户体验
- 剩余 4 个内置技能
- 所有 LLM 提供商
- 完整 `cron` 子命令
- 语音转录

### 迭代 3（1 周）- 平台扩展
- 飞书和钉钉频道
- Subagent Manager 增强
- 文档和测试

## Dependencies

- `serde_yaml` - YAML 解析（Skills 元数据）
- `whisper-rs` 或 Groq API - 语音转录
- 各平台官方 SDK - 飞书、钉钉、QQ
- 现有依赖的 feature 扩展

## Related Changes

- 依赖 `migrate-to-rust` change（核心框架已完成）
- 可能需要更新 `project.md`（添加 Skills 系统说明）
