## 1. 迭代 1：核心能力（2 周）

### 1.1 Skills 系统框架
- [ ] 1.1.1 设计 Skill trait 和数据结构
  - 定义 `SkillMetadata` 结构（name, description, always, dependencies）
  - 定义 `Skill` trait（load, validate_dependencies, get_content）
- [ ] 1.1.2 实现 YAML frontmatter 解析
  - 使用 `serde_yaml` 解析元数据
  - 提取 Markdown 内容部分
- [ ] 1.1.3 实现 Skills Loader
  - 从 `~/.nanobot/skills/` 加载用户技能
  - 从 `nanobot-core/src/skills/builtin/` 加载内置技能
  - 实现依赖检查（bins、env vars）
- [ ] 1.1.4 实现 Skills Registry
  - 技能注册和注销
  - 按名称查找技能
  - 列出所有可用技能
  - 过滤不可用技能
- [ ] 1.1.5 实现渐进式加载机制
  - `always=true` 的技能完整加载
  - 其他技能只返回摘要
  - Agent 按需调用 read_file 加载完整内容
- [ ] 1.1.6 集成到 Agent Loop
  - 在 ContextBuilder 中添加技能摘要
  - 确保技能路径可被 read_file 访问

### 1.2 内置技能（优先 3 个）
- [ ] 1.2.1 `memory` 技能
  - 编写技能内容（如何使用长期记忆）
  - 添加 YAML 元数据
- [ ] 1.2.2 `cron` 技能
  - 编写技能内容（如何管理定时任务）
  - 添加 YAML 元数据
- [ ] 1.2.3 `summarize` 技能
  - 编写技能内容（如何总结内容）
  - 添加 YAML 元数据

### 1.3 LLM 提供商扩展
- [ ] 1.3.1 设计 Providers Registry
  - 定义 `ProviderMetadata` 结构
  - 实现提供商注册表
  - 自动提供商检测逻辑
  - 模型前缀处理（如 `deepseek/`）
- [ ] 1.3.2 实现 DeepSeek Provider
  - 继承 OpenAI 兼容实现
  - 配置 API base URL
  - 支持模型前缀 `deepseek/`
  - 添加配置示例到 README
- [ ] 1.3.3 实现 Gemini Provider
  - 研究 Gemini API 格式
  - 实现 API 调用（可能需要适配器）
  - 支持模型前缀 `gemini/`
- [ ] 1.3.4 单元测试
  - 测试提供商注册和查找
  - 测试模型前缀解析
  - Mock API 调用测试

### 1.4 Message Tool
- [ ] 1.4.1 定义 Message Tool
  - 实现 `Tool` trait
  - 定义参数 schema（channel, chat_id, content）
- [ ] 1.4.2 实现消息发送逻辑
  - 通过 MessageBus 发送 OutboundMessage
  - 处理频道不存在的情况
  - 错误处理和返回
- [ ] 1.4.3 注册到 ToolRegistry
  - 在默认工具集中添加 Message Tool
  - 更新文档

### 1.5 CLI channels 子命令
- [ ] 1.5.1 定义 CLI 结构
  - 添加 `nanobot channels` 子命令
  - 定义 `status` 子命令
- [ ] 1.5.2 实现 `channels status`
  - 读取配置文件
  - 列出所有配置的频道
  - 显示每个频道的状态（启用/禁用、配置情况）
  - 显示 API token 状态（✓ 有密钥，✗ 无密钥）
- [ ] 1.5.3 添加单元测试
  - 测试命令解析
  - 测试输出格式

### 1.6 验证和测试
- [ ] 1.6.1 Skills 系统集成测试
  - 测试技能加载
  - 测试依赖检查
  - 测试渐进式加载
- [ ] 1.6.2 提供商集成测试
  - 测试 DeepSeek API 调用（需要真实 API key，手动）
  - 测试 Gemini API 调用（需要真实 API key，手动）
- [ ] 1.6.3 Message Tool 测试
  - 测试消息发送
  - 测试错误处理
- [ ] 1.6.4 CLI 测试
  - 测试 `channels status` 命令

---

## 2. 迭代 2：用户体验（2 周）

### 2.1 剩余内置技能
- [ ] 2.1.1 `github` 技能
  - 编写技能内容（如何使用 gh CLI）
  - 添加依赖检查（需要 `gh` 命令）
- [ ] 2.1.2 `skill-creator` 技能
  - 编写技能创建模板
  - 添加技能元数据说明
- [ ] 2.1.3 `tmux` 技能
  - 编写 Tmux 操作指南
  - 添加依赖检查（需要 `tmux` 命令）
- [ ] 2.1.4 `weather` 技能
  - 编写天气查询技能
  - 添加依赖（可能需要 API key）

### 2.2 剩余 LLM 提供商
- [ ] 2.2.1 Zhipu（智谱）Provider
  - 研究 GLM API 格式
  - 实现适配器
- [ ] 2.2.2 DashScope（阿里云）Provider
  - 研究 DashScope API
  - 实现调用逻辑
- [ ] 2.2.3 Moonshot（月之暗面）Provider
  - 实现 Moonshot API 调用
- [ ] 2.2.4 MiniMax Provider
  - 实现 MiniMax API 调用
- [ ] 2.2.5 vLLM Provider
  - 支持本地 vLLM 服务器
  - 配置自定义 API base
- [ ] 2.2.6 Groq Provider
  - 实现 Groq API 调用
  - 主要用于转录，也支持文本生成
- [ ] 2.2.7 AiHubMix Provider
  - 实现网关调用
- [ ] 2.2.8 提供商文档
  - 更新 README 添加所有提供商配置示例
  - 创建 providers.md 文档

### 2.3 CLI cron 子命令
- [ ] 2.3.1 定义 CLI 结构
  - 添加 `nanobot cron` 子命令
  - 定义子命令：list, add, remove, enable, disable, run
- [ ] 2.3.2 实现 `cron list`
  - 从 CronService 读取任务列表
  - 格式化输出（表格形式）
  - 显示：ID, 名称, Cron 表达式, 下次运行, 状态
- [ ] 2.3.3 实现 `cron add`
  - 交互式添加任务
  - 参数：名称, cron 表达式, 消息内容, 目标频道
  - 验证 cron 表达式
- [ ] 2.3.4 实现 `cron remove`
  - 按任务 ID 删除
  - 确认提示
- [ ] 2.3.5 实现 `cron enable/disable`
  - 切换任务状态
  - 持久化到配置
- [ ] 2.3.6 实现 `cron run`
  - 手动触发任务
  - 显示执行结果
- [ ] 2.3.7 添加单元测试
  - 测试所有子命令
  - 测试错误处理

### 2.4 Transcription 服务
- [ ] 2.4.1 定义 Transcription trait
  - 定义接口：transcribe(audio_data) -> Result<String>
- [ ] 2.4.2 实现 Groq Transcription
  - 使用 Groq Whisper API
  - 支持多种音频格式
  - 配置 API key
- [ ] 2.4.3 集成到 Telegram Channel
  - 检测语音消息
  - 自动转录
  - 将转录文本添加到消息内容
- [ ] 2.4.4 添加配置选项
  - 启用/禁用转录
  - 选择转录服务
  - 语言设置
- [ ] 2.4.5 添加单元测试
  - Mock Groq API 测试
  - 测试音频格式处理

### 2.5 Subagent Manager 增强
- [ ] 2.5.1 设计 Subagent 结构
  - 定义任务状态（pending, running, completed, failed）
  - 定义任务结果
- [ ] 2.5.2 实现任务队列
  - 使用 tokio::sync::mpsc
  - 任务优先级
  - 并发限制
- [ ] 2.5.3 实现任务追踪
  - 分配任务 ID
  - 记录开始/结束时间
  - 存储任务结果
- [ ] 2.5.4 实现结果通知
  - 任务完成后发送通知到主 Agent
  - 通过 MessageBus 发送
- [ ] 2.5.5 增强 SpawnTool
  - 支持任务状态查询
  - 支持任务取消
- [ ] 2.5.6 添加单元测试
  - 测试任务队列
  - 测试状态追踪
  - 测试通知机制

### 2.6 验证和测试
- [ ] 2.6.1 技能系统全面测试
  - 测试所有 7 个内置技能
  - 测试用户自定义技能
- [ ] 2.6.2 提供商全面测试
  - 测试所有提供商（手动）
  - 测试提供商注册表
- [ ] 2.6.3 CLI 全面测试
  - 测试所有 cron 子命令
- [ ] 2.6.4 转录服务测试
  - 测试不同音频格式
  - 测试错误处理
- [ ] 2.6.5 Subagent 测试
  - 测试任务执行
  - 测试结果通知

---

## 3. 迭代 3：平台扩展（1 周）

### 3.1 飞书频道
- [ ] 3.1.1 研究飞书开放平台 API
  - WebSocket 长连接
  - 消息格式
  - 认证方式
- [ ] 3.1.2 实现 Feishu Channel
  - 实现 Channel trait
  - WebSocket 连接管理
  - 消息接收和解析
  - 消息发送
- [ ] 3.1.3 实现配置加载
  - App ID/Secret 配置
  - 白名单配置
- [ ] 3.1.4 添加 feature flag
  - 添加 `feishu` feature
  - 条件编译
- [ ] 3.1.5 手动测试
  - 使用真实飞书机器人测试

### 3.2 钉钉频道
- [ ] 3.2.1 研究钉钉机器人 API
  - Stream 模式
  - 消息格式
  - 认证方式
- [ ] 3.2.2 实现 DingTalk Channel
  - 实现 Channel trait
  - Stream 连接管理
  - 消息接收和解析
  - 消息发送
- [ ] 3.2.3 实现配置加载
  - Client ID/Secret 配置
  - 白名单配置
- [ ] 3.2.4 添加 feature flag
  - 添加 `dingtalk` feature
  - 条件编译
- [ ] 3.2.5 手动测试
  - 使用真实钉钉机器人测试

### 3.3 QQ 频道
- [ ] 3.3.1 研究 QQ 机器人 API
  - botpy SDK
  - 消息格式
  - 认证方式
- [ ] 3.3.2 实现 QQ Channel
  - 实现 Channel trait
  - 使用 botpy 或 HTTP API
  - 消息接收和解析
  - 消息发送
- [ ] 3.3.3 实现配置加载
  - AppID/Secret 配置
  - 白名单配置
- [ ] 3.3.4 添加 feature flag
  - 添加 `qq` feature
  - 条件编译
- [ ] 3.3.5 手动测试
  - 使用真实 QQ 机器人测试

### 3.4 WhatsApp 频道（可选）
- [ ] 3.4.1 研究集成方式
  - 是否使用现有 TypeScript bridge
  - 或实现 Rust 版本
- [ ] 3.4.2 实现 WhatsApp Channel（如果可行）
  - WebSocket 连接
  - 二维码登录
  - 消息收发
- [ ] 3.4.3 添加 `channels login` 命令
  - 显示二维码
  - 处理登录流程

### 3.5 文档和示例
- [ ] 3.5.1 更新主 README
  - 添加 Skills 系统说明
  - 添加所有提供商配置示例
  - 添加所有频道配置示例
- [ ] 3.5.2 创建迁移指南
  - Python 到 Rust 迁移步骤
  - 配置兼容性说明
  - 常见问题
- [ ] 3.5.3 创建技能开发指南
  - 如何创建自定义技能
  - YAML frontmatter 格式
  - 依赖声明
- [ ] 3.5.4 创建 API 文档
  - 生成 `cargo doc`
  - 添加示例代码

### 3.6 性能测试和优化
- [ ] 3.6.1 性能基准测试
  - 启动时间测试
  - 内存占用测试
  - 并发性能测试
- [ ] 3.6.2 优化建议
  - 识别性能瓶颈
  - 优化建议文档

### 3.7 最终验证
- [ ] 3.7.1 功能完整性检查
  - 对照 Python 版本功能清单
  - 确认所有计划功能已实现
- [ ] 3.7.2 兼容性测试
  - 配置文件兼容性
  - CLI 命令兼容性
  - 工作区结构兼容性
- [ ] 3.7.3 文档完整性检查
  - 所有功能有文档
  - 示例代码可运行
- [ ] 3.7.4 发布准备
  - 更新版本号
  - 准备 Release Notes
  - 更新 crates.io 元数据

---

## 依赖关系

### 迭代 1 依赖
- 无外部依赖，可立即开始

### 迭代 2 依赖
- 依赖迭代 1 的 Skills 系统框架
- 依赖迭代 1 的 Providers Registry

### 迭代 3 依赖
- 依赖迭代 2 的所有功能
- 各平台 SDK 可用性

## 验收标准

### 迭代 1
- Skills 系统可用，至少 3 个技能加载成功
- DeepSeek 和 Gemini 可用
- Message Tool 可以发送消息
- `channels status` 命令输出正确

### 迭代 2
- 所有 7 个内置技能可用
- 所有 13 个提供商实现完成
- 完整的 `cron` 子命令可用
- 语音转录功能可用
- Subagent Manager 基本功能完成

### 迭代 3
- 至少 2 个国产平台（飞书/钉钉）可用
- 文档完整
- 所有测试通过
- 性能达到预期
