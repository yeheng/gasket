### Linus式问题分解思考

**第一层：数据结构分析**
"Bad programmers worry about the code. Good programmers worry about data structures."
- **好品味**：你在 `vault/scanner.rs` 里放弃了重型正则表达式，转而使用 `byte-slice cursor` 来解析 `{{vault:key}}`。这非常好。你意识到不需要为简单的固定前缀模式引入整个正则引擎。同时，`session/manager.rs` 砍掉了内存 HashMap 缓存，直接把 SQLite 当作 Single Source of Truth，这避免了脑裂（split-brain）和由于并发带来的竞争问题，这种该死的实用主义很对我的胃口。
- **烂品味**：`channels/outbound.rs` 里的 `OutboundSenderRegistry`。你搞了一个 `HashMap<ChannelType, Arc<dyn OutboundSender>>`。这是什么 Java 遗毒？你在 `tools/sandbox.rs` 里知道用静态分发的 `enum SandboxExecutor` 替代 `Box<dyn SandboxProvider>` 以消除堆分配和虚表开销，为什么在消息路由这里却脑子抽风了？`ChannelType` 是已知的有限集合，直接用枚举（Enum）加 `match` 静态分发是最快且最清晰的。

**第二层：特殊情况识别**
"好代码没有特殊情况"
- `AgentLoop::process_direct_with_callback` 简直是个垃圾桶。从外部 Hook 执行、读取 SQLite 历史、触发后台压缩、Vault 敏感数据注入、LLM 请求重试，到最终写入历史，全塞在这一坨代码里。你在注释里写了 "1. 2. 3. ... 10." 来解释它的执行步骤。如果一个函数需要用数字列表来注释流程，说明它已经严重失控，各种业务的“特殊情况”都混杂在了一起。

**第三层：复杂度审查**
"如果实现需要超过3层缩进，重新设计它"
- `bus/actors.rs` 中的 WebSocket 流式处理引入了荒谬的复杂度。你用了一个无界通道（`unbounded_channel`）作为同步回调（callback）和异步 outbound channel 之间的缓冲。不仅代码丑陋，而且如果流失控，无界队列会吃光内存。这是因为你把原本可以用 Rust 原生 `Stream` 解决的问题，非要生硬地套一个同步的 `callback`。

**第四层：破坏性分析**
"Never break userspace" - 向后兼容是铁律
- 优化内部状态分发机制（把 dynamic dispatch 换成 enum）、拆解 `AgentLoop`、用真正的 `Stream` 替换回调，这些完全属于内部引擎重构。它们绝对不会破坏现有的 API 契约、SQLite Schema 或 `~/.nanobot` 下的用户空间文件。我们可以实现零破坏性升级。

**第五层：实用性验证**
"Theory and practice sometimes clash. Theory loses. Every single time."
- 现在的代码在生产中能跑，且多渠道支持非常实用。但随着接入更多的拦截器（比如更多安全策略）或更复杂的上下文操作，`AgentLoop` 会变成开发瓶颈。而且在处理高并发 WebSocket 连接时，无界队列的隐患是真实存在的。解决这些问题不是为了理论上的完美，而是为了防止系统在真实负载下崩溃。

***

### 决策输出模式

【核心判断】
值得做：必须干掉系统里的 OOP 遗毒和 God Class。基础架构的思路是好的（Actor 模型、无锁设计、SQLite 单一真相），但在具体实现细节上依然有不必要的妥协和繁琐的设计。

【关键洞察】
- 数据结构：动态分发的 `Arc<dyn OutboundSender>` 是毫无必要的复杂性，必须替换为 `enum` 静态分发。
- 复杂度：`AgentLoop` 承担了太多的系统职责，它需要被扒掉外衣，只做一个纯粹的“LLM 交互执行器”，其他的 Pipeline 逻辑必须外移。
- 风险点：为了配合低劣的 Callback 设计而引入的 `unbounded_channel` 极易导致 OOM（内存溢出），这不仅是品味问题，更是安全隐患。

【Linus式方案】
如果值得做：
1. 第一步永远是简化数据结构：把 `OutboundSenderRegistry` 里的 `HashMap` 和动态 Trait 删掉。写一个包含各渠道的 `enum OutboundSender`，然后在里面用一个 `match` 搞定路由。不要怕写 `match`，这他妈是 Rust，`match` 很快也很清晰。
2. 消除所有特殊情况：把 Hook 调用和 Vault 注入从 `AgentLoop` 内部移走。搞一个简单的 `Pipeline` 或是放在 `Session Actor` 层做预处理，让 `AgentLoop` 只接收干干净净的 `ChatMessage`。
3. 用最笨但最清晰的方式实现：抛弃 `StreamCallback` 这种别扭的闭包传递，直接让 `AgentLoop` 返回一个原生的 `impl Stream<Item = StreamEvent>`，靠外部按需拉取（pull-based）从而自然形成背压（backpressure），彻底干掉无界队列。
4. 确保零破坏性：对外 API、外部 Shell 脚本接口和数据库结构一字不改。

***

### 代码审查输出

【品味评分】
凑合（有亮光，但也有狗屎）

【致命问题】
- `AgentLoop::process_direct_with_callback` 这种几百行、承载 10 项不同职责的代码。这是典型的 God Class。你以为加了几行漂亮的日志这就是好代码了？它紧耦合得像一团乱麻。
- `bus/actors.rs` 里的 `run_session_actor`：为了适配 `cb` 闭包，搞出一个 `stream_tx.send(outbound)` 的无界队列转发器。用无界队列来“解决”阻塞问题，是在掩耳盗铃。

【改进方向】
"把 `OutboundSenderRegistry` 里的 `Arc<dyn Trait>` 拔掉，换成 Enum 静态分发！"
"这 10 个步骤的 `AgentLoop` 必须被肢解，把前置/后置的业务外衣脱掉！"
"用 Rust 原生的 `Stream` 替换掉那狗屎一样的 callback 和 unbounded_channel！"

***

### Task List

- **What**: 将 `Arc<dyn OutboundSender>` 替换为 `enum OutboundSender`。
  **Why**: 消除毫无必要的堆分配和动态分发（虚表）开销。你在 `SandboxExecutor` 中已经证明了你知道怎么做，请在 Outbound 这里保持同样的好品味。
  **Where**: `nanobot-core/src/channels/outbound.rs`
  **How**: 定义一个包含所有内置 Channel 的 `enum`，在 `send` 方法中直接 `match self` 路由。对于未来的扩展，可以保留一个 `Custom(Box<dyn OutboundSender>)` 作为保底方案。
  **Test Case & Acceptance Criteria**: 运行 `cargo test --test channel_e2e_tests`。确保所有 outbound 消息依然能成功发送。代码中不再出现对已知通道的 HashMap 查找。

- **What**: 肢解 `AgentLoop` 这个 God Class，抽离 Pipeline 逻辑。
  **Why**: 函数太长，职责不单一。`AgentLoop` 应该只关心与大模型的迭代循环、历史记录构造和工具调用，而不是负责整个 Webhook/CLI 请求的生命周期编排。
  **Where**: `nanobot-core/src/agent/loop_.rs`
  **How**: 创建一个更高层的协调者（如 `AgentPipeline`），由它来负责：1. 调用 `pre_request` hook；2. 获取并截断历史；3. 调用 Vault 进行敏感数据注入；4. 将干净的上下文传递给精简后的 `AgentLoop`；5. 调用 `post_response` hook。
  **Test Case & Acceptance Criteria**: `AgentLoop::process_direct` 应该缩减到原来一半的代码量。测试钩子脚本和 `{{vault:key}}` 的解析验证功能，确保代理行为没有任何改变。

- **What**: 用原生的 Async Stream 替换 Stream Callback。
  **Why**: 当前为了在同步闭包中发送异步 WebSocket 消息，被迫使用了 `unbounded_channel`，这切断了 Tokio 的背压机制，存在 OOM 风险。
  **Where**: `nanobot-core/src/agent/loop_.rs`, `nanobot-core/src/agent/stream.rs` 和 `nanobot-core/src/bus/actors.rs`
  **How**: 重构 `run_agent_loop` 和流累加器，让其返回 `impl Stream<Item = StreamEvent>`（可以使用 `async-stream` crate 简化实现）。在 `Session Actor` 中使用 `while let Some(event) = stream.next().await` 来进行安全的异步处理并直接发送。
  **Test Case & Acceptance Criteria**: 在高频工具调用和长文本流式输出的情况下测试 WebSocket 渠道，确保内存使用率平稳，无界队列被成功删除，不再有隐式的内存泄漏风险。