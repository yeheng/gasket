# RAG ↔ Agent/前端 对接设计

> 日期:2026-09-04 · 状态:已评审通过(方案定稿,待实施)
>
> 前提:`conga-rag` 已可用(ingest/search/ask/status CLI,火山方舟 embedding,qdrant-edge + sqlite 存储)。
> 本文回答一个问题:**agent 怎么获得/搜索/理解 RAG 索引的个人知识,前端如何对接**。

---

## 1. 目标与非目标

**目标**:conga agent(桌面端 / CLI)能在对话中检索 `~/.conga/rag` 索引的个人知识(笔记等),阅读命中片段并追溯到源文件;前端(web/Tauri)**零改动**。

**非目标(YAGNI)**:

- 不做独立检索面板(与 agent 能力重复)
- 不做 gateway 新端点、不做设置页卡片
- 不做自动/后台 ingest(手动 CLI 或系统定时)
- 不做 ask 式二次生成——agent 自己就是推理器,返回原始 chunk 比再调一次 LLM 总结更省、更透明、不丢出处

## 2. 数据流

```
用户对话(web/Tauri 或 CLI)
   │  "帮我查下笔记里关于架构分层的说法"
   ▼
Host::run_turn ──► agent 选择工具
   │
   ▼
rag_search(query)                    [新增,conga-ext,Low 风险]
   ├─ RagConfig::load()              配置发现:$CONGA_RAG_CONFIG → ./rag.toml → ~/.conga/rag.toml
   ├─ embed_query(query)             1 次 embedding API 调用(单条输入,~1s)
   ├─ store.knn()                    本地向量检索(qdrant-edge,ms 级)
   ▼
文本结果(片段 + 出处 + 评分)──► 回到 agent 上下文
   │
   ▼
agent 理解/引用;需要全文时用现有 read 工具读源文件(检索→精读闭环)
   ▼
MessageBubble 按现有工具调用路径渲染(前端零改动)
```

## 3. 决策记录

| 决策点 | 选择 | 理由 |
|---|---|---|
| Agent 接入方式 | conga-ext 内置工具(进程内) | 复制 `web_search` 成熟模式:零进程管理、零新配置面、风险分级接入权限系统。MCP server 列为 Phase 2 |
| 前端对接程度 | 零改动 | 工具调用与渲染链路已存在,先跑通价值闭环 |
| ingest 触发 | 手动/定时 CLI | 索引更新是低频批处理,不值得引入后台任务调度 |
| 工具返回形态 | 原始 chunk(不生成答案) | 省一次 LLM 调用;出处可追溯;agent 可用 `read` 精读源文件 |

**核心权衡**:进程内工具引入 conga-ext → conga-rag 的编译耦合(qdrant-edge/sqlx 等依赖树),用 feature gate(`rag`,默认关)隔离给不需要 RAG 的构建。

## 4. 变更清单

| 位置 | 改动 |
|---|---|
| `conga/conga-ext/Cargo.toml` | 新 feature `rag = ["dep:conga-rag"]`(conga-rag 为 optional 依赖) |
| `conga/conga-ext/src/rag.rs` | **新文件**:`rag_search` 工具,仿 `web_search` 的 `register(api)` 模式 |
| `conga/conga-ext/src/lib.rs` | `#[cfg(feature = "rag")] pub mod rag;`;`prod_register` 与 `register_all` 中 cfg 注册 |
| `web/src-tauri/Cargo.toml` | conga-ext features 加 `"rag"`(桌面 agent 即获得工具,前端代码零改动) |
| `conga/conga-cli/Cargo.toml`(可选) | `ext` feature 追加 `conga-ext?/rag`,让 CLI `--features ext` 也带上 |

组合根现状(已核实):桌面端 `web/src-tauri/src/chat.rs` 调 `conga_ext::prod_register`;CLI `conga-cli/src/main.rs` 调 `conga_ext::register_all`(feature `ext` 后)。

## 5. 工具契约

```jsonc
{
  "name": "rag_search",
  "label": "RAG Search",
  "risk": "Low",              // 只读,所有权限模式下自动放行
  "parameters": {
    "type": "object",
    "properties": {
      "query":  { "type": "string", "description": "检索词" },
      "k":      { "type": "number", "description": "返回条数,默认取 rag.toml [ask].top_k,上限 20" },
      "source": { "type": "string", "description": "可选,按源名过滤(如 docs)" }
    },
    "required": ["query"]
  }
}
```

**description 写明使用边界**(agent 靠它决策何时选用):检索个人笔记/知识库时用;找代码用 `grep`;要联网信息用 `web_search`。

**输出格式**(纯文本,面向 agent 消费):

```
找到 3 条相关片段(索引模型 doubao-embedding-vision):

[1] score=0.646 docs /Users/.../notes/进行架构设计.md (片段 2/8)
<chunk 内容,单条上限 ~1600 字符>

[2] ...

提示:完整文件可用 read 工具读取;索引按上次 ingest 的时间点构建,最新内容以磁盘为准。
```

## 6. 错误处理(沿用 conga-rag fail-loud 风格)

| 场景 | 行为 |
|---|---|
| 无 rag.toml | 报错并指向 rag.example.toml,提示先建配置再 `conga-rag ingest` |
| 索引为空 | "索引为空:请先运行 conga-rag ingest"(对齐 CLI 文案) |
| embedding API 失败 | 透传服务端错误详情(embed.rs 已支持) |
| query 为空 / k 越界 | ToolError::Message 直接拒绝 |

## 7. 已知限制(如实记录,不在本期解决)

- **并发**:`Store` 设计为单顺序 pipeline(源码注释明示)。`rag_search` 每次调用短生命周期开库;与 CLI `ingest` 并发时可能冲突(qdrant-edge WAL 单写者)。ingest 低频手动,冲突报错重试即可;未来需要再加建议性文件锁。
- **索引时效**:手动/定时 ingest 导致结果可能滞后。输出末尾"以磁盘为准"提示即为此设计,agent 可直接 `read` 源文件兜底。
- **延迟与限流**:每次调用含 1 次 embedding API(网络)。单条输入 token 极少,触发 TPM 限流概率低;复用现有重试策略(7 次、最长 ~91s),最坏情况阻塞一个工具调用但会自愈。

## 8. 测试计划

- **注册/安全回归**:工具名 `rag_search`、风险级 `Low`、参数 schema(仿 `terminal_tool_is_high_risk` 风格)
- **行为**:`conga_rag::testsupport::spawn_mock_embeddings`(已公开、常编译)+ 临时目录造索引;`CONGA_RAG_CONFIG` 指向临时配置避免读到真实 `~/.conga/rag.toml`;断言命中格式、评分排序、k/source 参数
- **错误路径**:无配置 / 空索引 / 空 query

## 9. Phase 2(本期不做)

- `conga-rag mcp` 子命令:stdio MCP server,对 Claude Desktop 等外部 MCP 客户端开放同一能力(host 已有 MCP client,外部世界也能用)
- gateway `GET /api/rag/status` + `POST /api/rag/ingest` + 设置页索引状态卡片与手动重建
- 定时增量 ingest(launchd/cron 示例文档,零代码)
