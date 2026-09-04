# RAG 长期记忆模块设计(rag_remember + evolve 联动)

> 日期:2026-09-04 · 状态:已评审通过(设计定稿,待实施)
>
> 前提:`rag_search` 工具已落地(conga-ext,见 [2026-09-04-rag-agent-integration-design.md](2026-09-04-rag-agent-integration-design.md))。
> 本文回答一个问题:**agent 如何把对话中获得的知识写进 RAG 索引,形成读+写闭环的长期记忆**。

---

## 1. 目标与非目标

**目标**:conga agent 拥有跨会话长期记忆——对话中判断"值得记"即可写入(`rag_remember`),会话蒸馏教训自动入索引(evolve 联动),全部记忆经 `rag_search` 统一检索。

**非目标(YAGNI)**:

- 不做每轮自动召回注入(每轮 1 次 embedding 的延迟/成本不划算;现有 memory 目录注入已覆盖"始终可见"层)
- 不做 notes 的 LLM 去重/合并策展(mem0 v3 已验证 ADD-only 更优)
- 不做时序知识图谱(Graphiti/Cognee 路线,需 Neo4j,个人 agent 杀鸡用牛刀)
- 不做独立记忆服务/MCP(维持进程内组合根的既定决策)
- 不做多用户/记忆共享(MemOS 企业场景)

## 2. 横向调研结论(2026-09)

| 系统 | 写入 | 存储 | 召回 | 生命周期 |
|---|---|---|---|---|
| mem0 (64.7k★) | 对话后异步抽取 + **agent 主动写(同等优先)** | 向量 + 实体链接 | 语义+BM25+实体+时间,多信号融合 | **v3 改 ADD-only,砍掉 UPDATE/DELETE** |
| OpenViking (35.4k★,火山) | 会话 commit 后异步抽取 | `viking://` 虚拟 FS;L0/L1/L2 三层 | 目录递归下钻,轨迹可观测 | 文件系统语义 |
| Letta/MemGPT (24.6k★) | agent 工具自改 | **core 常驻(有界)/ archival 向量(无界)** | core 每轮注入,archival 检索 | agent 自编辑 |
| Graphiti/Zep | 持续摄取 | 时序 KG(Neo4j) | 语义+关键词+图遍历 | 自动失效 |
| Cognee / MemOS | 流水线抽取 / 统一 API | KG+向量 / MemCube | 混合 | 整理阶段 / 可检视图 |

**对本设计的验证**:

1. "文件即记忆 + agent 工具读写"已是共识(OpenViking 的卖点即此;conga 的 read/write/list 天然同构)。
2. 双写入路径(主动工具 + 会话蒸馏)是头部系统标配——正是本设计两条线。
3. mem0 v3 砍掉 LLM 策展式合并 → notes 走 ADD-only 是对的;**同 title 覆盖**是零成本的更新逃生门。
4. lessons(有界策展、每轮可见)/ notes(无界追加、仅检索)的双命名空间 = Letta core/archival 同构。
5. 纯向量检索是行业公认弱环 → Phase 2 混合检索(§6)。
6. OpenViking 为 AGPLv3——只借鉴思想,不引代码。

### 2.1 Coding harness 接入方式对比(2026-09,一手文档)

| Harness | 常驻指令(人写) | Agent 记忆(机器写) | 后台蒸馏 | 召回 |
|---|---|---|---|---|
| Claude Code | CLAUDE.md 层级 + `.claude/rules/` | **Auto memory**:agent 按纠正/偏好自记,repo 级,每会话注入,**200 行/25KB 封顶**,`/memory` 审计 | 无(写入即生效) | 截断注入,无检索 |
| Gemini CLI | GEMINI.md 层级 | agent **直接用 write_file/replace 改 markdown** | **Auto Memory**:挖转录 → .patch/SKILL.md 草稿 → **收件箱人审批准** | 层级注入,无检索 |
| Cursor | Rules(.mdc:Always/Auto-attached/**Agent-requested**/Manual) | Memories 后台抽取,设置页审计 | 后台抽取,无强人审 | 注入 |
| Amp / Cline 系 / Codex·pi | AGENTS.md / .clinerules | memories / Memory Bank 约定 / 无 | 有 / 无 / 无 | 注入 / 文件约定 / 无 |

**关键洞察**:① 无任何主流 coding harness 内建向量召回(OpenViking/mem0 均为外挂平台形态)——向量召回是 harness 原生弱项,我们做它是差异化且降级预案完整;② conga memory.rs 目录注入 = 行业主流形态,封顶同款(Claude 200 行/25KB ↔ 64 条/120 字 preview);③ `rag_remember` 写侧 ≈ Claude auto memory + Gemini memory files,正统;④ Gemini 的"蒸馏草稿人审收件箱"是唯一强人审设计,记为 Phase 2 备选(见 §6);⑤ 分层下钻无 harness 在做,支持混合检索排 Phase 2 的排序。

## 3. 总体架构

**核心原则:磁盘文件是事实源,索引只是派生物。** 写记忆 = 落一个 markdown 文件 + 增量 ingest(`run_ingest` 按 mtime+hash 只嵌新文件),不新增任何存储代码。

**两个内置 source**(`RagConfig::load` 后自动注入;规则:仅当目录存在且 rag.toml 未占用该名字时注入,空目录自然跳过):

| source | 目录 | 写入者 | 性质 |
|---|---|---|---|
| `notes` | `~/.conga/notes/` | `rag_remember`(agent 主动记) | 追加式,无上限,同 title 覆盖 |
| `memory` | `~/.conga/memory/` | evolve(自动蒸馏,现状) | 有界(64),目录注入 prompt 不变 |

```
对话中 agent 判断"值得记"
   → rag_remember(title, content, tags?)          [新工具, conga-ext]
       ├─ 写 ~/.conga/notes/<slug>.md(复用 memory.rs 的 entry_markdown frontmatter)
       └─ run_ingest(cfg, Some("notes"))           增量:1 次 embedding

evolve 蒸馏(显式触发:工具/CLI)
   → 写/合并 ~/.conga/memory/*.md(现状)
   → run_ingest(cfg, Some("memory"))               增量:hash 变化才重嵌

rag_search(现有,更新 description 说明三路来源)
   → notes(agent 记忆)+ memory(蒸馏教训)+ 用户 sources
```

## 4. `rag_remember` 工具契约

```jsonc
{
  "name": "rag_remember",
  "risk": "Medium",              // 写文件,与 write 同级:AutoEdit 放行,Suggest/Plan 需确认
  "parameters": {
    "type": "object",
    "properties": {
      "title":   { "type": "string",  "description": "记忆标题,同时是主键:同 title 覆盖旧记忆" },
      "content": { "type": "string",  "description": "自包含的知识陈述(不依赖对话上下文可理解)" },
      "tags":    { "type": "array",   "items": { "type": "string" }, "description": "可选标签" }
    },
    "required": ["title", "content"]
  }
}
```

- **description 写明使用边界**:何时记(跨会话有用的偏好/事实/经验,或用户明确说"记住");何时不记(易变状态、一次性任务细节);同 title 覆盖语义;检索用 `rag_search`。
- **行为**:校验(空 title/content 拒绝;content 设上限防上下文滥用)→ slug(title)保 CJK,同题覆盖 `~/.conga/notes/<slug>.md` → 增量 ingest → 返回确认 + 绝对路径。
- **风险定级理由**:agent 可被诱导写垃圾,Medium 保证 Suggest/Plan 模式下用户把关;爆炸半径有界(固定目录、人类可审计、可手删)。

## 5. evolve 联动与组合根布线

| 位置 | 改动 |
|---|---|
| `conga/conga-host/Cargo.toml` | 可选依赖 `conga-rag`,feature `rag`(仿现有 `session-index` 模式) |
| `conga-host/src/evolve.rs` | 写完 lessons 后 `run_ingest(cfg, Some("memory"))`;**fail-soft**:索引失败仅 log warn,不失败 evolve(文件是真相源,下次 ingest 补偿) |
| `conga-rag/src/config.rs` | `RagConfig::load` 注入内置 source(notes/memory) |
| `conga-ext/src/rag.rs` | 新增 `rag_remember`;更新 `rag_search` description(三路来源) |
| `web/src-tauri/Cargo.toml` | 启用 `conga-ext/rag`(已有)+ `conga-host/rag` |
| `conga-cli`(可选) | `ext` feature 追加,使 CLI 同获能力 |

## 6. 检索:现状与分层

**Phase 1(本设计范围)**:工具召回。`rag_search` 语义 knn + 现有 memory 目录注入(标题+标签+首行,渐进披露)。rag_search 结果带 namespace 路径,agent 用现有 `list/read` 工具下钻——**conga agent 自带文件系统工具,下钻是涌现行为,无需自建引擎**(OpenViking 的 ls/tree/find 是为没有 FS 工具的 agent 设计的)。

**降级预案**:若实测召回率不足(agent 不主动调 rag_search),把 notes 标题目录追加进 prompt(复用 memory.rs catalog 模式,有界截断)——零架构变更。

**Phase 2(混合检索)**:

- notes/memory 加 **SQLite FTS5 表 + jieba-rs 预分词**(写入与查询两侧同法分词),与 qdrant knn 做加权 RRF 融合。个人规模(万级 chunk)FTS5 内置 bm25() 足够,与元数据同库同事务。
- **中文分词是必答题**:FTS5 默认 unicode61 把连续 CJK 当单 token,句中词永远查不到(session_index.rs 现状即此坑);trigram 需 ≥3 字查询,中文高频 2 字词失效。jieba 预分词是正解。
- **分层映射**(OpenViking 式 L0/L1/L2,零新引擎):L0 = frontmatter title/tags(notes 天然免费)/目录 = source+路径(SQL 前缀查询)/L1 = 可选 `.overview.md` 边车(唯一真成本:每文件 1 次 LLM)/L2 = 文件 + `read`。
- **升级触发器**(满足才动):拼音容错、query 改写、重排序、百万级索引 → tantivy(内置 jieba);外部搜索引擎不考虑。
- **人审收件箱(备选,灵感来自 Gemini Auto Memory)**:evolve 蒸馏产物先落草稿目录,用户批准后转正。个人 agent 下暂不启用——蒸馏即落盘 + `rag_remember` 的 Medium 把关已够;规模化/多用户后再评估。

## 7. 错误处理

| 场景 | 行为 |
|---|---|
| 无 rag.toml | fail loud,指向 rag.example.toml(对齐 rag_search 现有文案) |
| store 不存在 | 首次 `rag_remember` 顺带引导建库(仅嵌该条,1 次 embedding) |
| `rag_remember` 的 ingest 失败 | 如实报告"文件已写入、索引失败,下次 ingest 补偿"(部分成功不静默) |
| evolve 的 ingest 失败 | fail-soft:log warn,evolve 本身成功 |
| 与 CLI ingest 并发 | 沿用既有约定:qdrant-edge 单写者,报错重试 |
| 空 title/content、超长 content | `ToolError::Message` 直接拒绝 |

## 8. 测试计划

- **注册契约**:`rag_remember` 名字、风险级 Medium、参数 schema(仿现有 `register_exposes_rag_search_with_low_risk`)
- **端到端**:`spawn_mock_embeddings` + 临时 rag.toml → 写 → `rag_search` 命中(frontmatter 不进 chunk 正文)
- **同 title 覆盖**:两次 remember 同题 → store 单文档、内容为后者
- **参数校验**:空 title/content、超长拒绝
- **内置 source 注入**:目录存在才注入;rag.toml 已占用名字时不覆盖用户配置
- **evolve 联动**(feature-gated):run_evolve 后 store 含 `memory` 源 chunk;ingest 失败时 evolve 仍成功
- **风险一致性**:Medium 在 Suggest/Plan 模式下需确认(permission 现有模式)

## 9. 决策记录

| 决策点 | 选择 | 理由 |
|---|---|---|
| notes 生命周期 | ADD-only + 同 title 覆盖 | mem0 v3 验证 LLM 策展不如只增不删;覆盖语义给人工更新留零成本口子 |
| 双命名空间 | notes(无界)/memory(有界) | Letta core/archival 同构:prompt 成本层 vs 检索成本层 |
| 写入触发 | 主动工具 + evolve 蒸馏都要 | 头部系统标配双路径;OpenViking session-commit 抽取 ≈ evolve |
| 召回方式 | 工具检索,不做每轮自动召回 | 每轮 embedding 延迟/成本;目录注入已覆盖常驻层;降级预案见 §6 |
| 风险级 | Medium | agent 可被诱导写入;Suggest/Plan 下用户把关 |
| source 命名 | 目录名即源名(notes/memory) | rag_search source 过滤可预测 |
| 检索引擎 | SQLite FTS5 + jieba(Phase 2) | 个人规模足够、同库事务;tantivy 仅按触发器升级 |
| 分层下钻 | 不自建引擎,靠 list/read 涌现 | conga agent 已有 FS 工具,结构性优势 |

## 10. 已知限制

- qdrant-edge 单写者并发约束(现状沿用,报错重试)。
- evolve fail-soft 意味着 lessons 索引可能短暂滞后于文件(下次 ingest 补偿)。
- session_index.rs 的中文分词坑与本设计同根,但不在本设计范围;Phase 2 的 jieba 工具函数留了顺手修的口子。
- OpenViking 的 L1 概览层(每文件 1 次 LLM 调用)未纳入,待真实需求出现再评估。
