# Proposal: Enhance Security and Observability

## Change ID
`enhance-security-and-observability`

## Summary
增强 NanoBot-RS 的安全策略深度和生产就绪特性，包括：
1. **命令风险分级系统** - 细粒度的命令执行安全控制
2. **完善的安全测试套件** - 全面的安全漏洞测试覆盖
3. **可观测性系统** - 统一的事件记录和监控接口
4. **连接池预热** - 减少首次 API 请求延迟
5. **Rate Limiting** - 防止工具执行滥用

## Motivation
当前 NanoBot-RS 的安全实现较为基础：
- Shell 工具使用简单的 `enabled` 开关，缺乏风险分级
- 缺少专门的安全测试套件来验证防护措施
- 缺乏可观测性接口，难以在生产环境监控 Agent 行为
- HTTP 客户端没有预热机制，首次请求延迟较高
- 缺少 Rate Limiting 机制，可能被滥用

借鉴 ZeroClaw 项目的成熟实现，这些改进将使 NanoBot 从"设计良好"提升到"生产就绪"。

## Scope

### In Scope
- 新增 `SecurityPolicy` 模块，实现命令风险分级
- 新增 `observability` 模块，定义 Observer trait
- 扩展 `LlmProvider` trait，添加 `warmup()` 方法
- 新增 `ActionTracker` 实现滑动窗口限流
- 新增安全测试套件

### Out of Scope
- Docker 沙箱运行时（可作为后续独立 proposal）
- OpenTelemetry 完整集成（本 proposal 仅定义接口）
- Gateway 配对机制（可作为后续独立 proposal）

## Impact

### Breaking Changes
- **配置文件**：新增 `security` 和 `observability` 配置节（可选，有默认值）
- **ExecTool**：行为变化，高风险命令将被默认阻止

### Migration Path
- 所有新配置都有合理默认值，无需修改现有配置
- `SecurityPolicy` 默认行为与当前 `enabled: true` 兼容

## Dependencies
- 无外部依赖新增
- 需要更新 `config/schema.rs` 添加新配置结构

## Related Changes
- 无

## Timeline Estimate
- Security Policy: 2-3 days
- Observability: 1-2 days
- Rate Limiting: 1 day
- Security Tests: 1-2 days
- Provider Warmup: 0.5 day
