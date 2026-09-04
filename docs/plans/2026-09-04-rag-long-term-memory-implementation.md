# RAG 长期记忆(rag_remember + evolve 联动)实施计划

> **REQUIRED SUB-SKILL:** Use the executing-plans skill to implement this plan task-by-task.

**Goal:** agent 获得"写"路径:`rag_remember` 工具把知识写成 `~/.conga/notes/*.md` 并增量入 RAG 索引;evolve 蒸馏产物(`memory/`)自动入索引;`rag_search` 统一检索三路来源。

**Architecture:** 磁盘文件是事实源,索引是派生物。conga-rag 在配置加载后注入两个内置 source(`notes`/`memory`,目录存在且名字未被占用时);conga-ext 新增 `rag_remember`(Medium 风险,写文件 + `run_ingest(Some("notes"))`);conga-host 在 feature `rag` 下于 `apply_proposals` 尾部 fail-soft 刷新 `memory` 源。布线简化:**conga-ext 的 `rag` feature 传递启用 `conga-host/rag`**,src-tauri 与 conga-cli 零改动(设计文档 §5 的"src-tauri 启用 conga-host/rag"由传递 feature 取代)。

**Tech Stack:** Rust 2021 / tokio / sqlx+qdrant-edge(既有)/ 无新第三方依赖。

**前置:** 设计文档 `docs/plans/2026-09-04-rag-long-term-memory-design.md`(已定稿)。在独立 worktree 执行(当前工作区暂存区有 WIP)。测试确定性关键:新增环境变量 `CONGA_RAG_BUILTIN_BASE`(覆盖内置源根目录;空串 = 显式禁用,用于 hermetic 测试),生产默认 `conga::storage::config_dir()`(`~/.conga`,含 `~/.gasket` 遗留回退)。

**约定:** 每个任务在 `conga/` workspace 根(/Users/yeheng/workspaces/Github/conga/conga)执行,除非另注。提交信息用英文。

---

### Task 1: conga-rag — 内置 source 注入(`builtin_base` + `inject_builtins`)

**Files:**
- Modify: `conga-rag/src/config.rs`(`load_with` 约 129-136 行,`expand_tilde` 与 `validate` 之间)
- Test: `conga-rag/src/config.rs` tests 模块(文件尾部已有 tests,追加)

**Step 1: 写失败测试**(追加到 config.rs 的 `mod tests`)

```rust
// --- built-in source injection ---

/// Hermetic core: explicit base, no env. Mirrors append_memory_in pattern.
fn mk_cfg_with_source(name: &str, path: &std::path::Path) -> RagConfig {
    let mut cfg = RagConfig::default();
    cfg.sources.insert(
        name.into(),
        SourceConfig { path: path.into(), ..Default::default() },
    );
    cfg
}

#[test]
fn inject_builtins_adds_only_existing_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("notes")).unwrap();
    // memory/ 不存在 → 不注入
    let mut cfg = mk_cfg_with_source("docs", &tmp.path().join("docs"));
    cfg.inject_builtins_in(tmp.path());
    assert!(cfg.sources.contains_key("notes"));
    assert!(!cfg.sources.contains_key("memory"));
    assert_eq!(cfg.sources["notes"].path, tmp.path().join("notes"));
    assert_eq!(cfg.sources["notes"].kind, "dir");
}

#[test]
fn inject_builtins_respects_name_ownership() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("notes")).unwrap();
    std::fs::create_dir_all(tmp.path().join("memory")).unwrap();
    let user_notes = tmp.path().join("my-notes");
    let mut cfg = mk_cfg_with_source("notes", &user_notes);
    cfg.inject_builtins_in(tmp.path());
    assert_eq!(cfg.sources["notes"].path, user_notes, "用户占名优先");
    assert!(cfg.sources.contains_key("memory"), "未占用名照常注入");
}

#[test]
fn inject_builtins_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("notes")).unwrap();
    let mut cfg = RagConfig::default();
    cfg.inject_builtins_in(tmp.path());
    cfg.inject_builtins_in(tmp.path());
    assert_eq!(cfg.sources.len(), 1);
}
```

注:config.rs 的 tests 模块若无 `tempfile` dev-dep,确认 `conga-rag/Cargo.toml` `[dev-dependencies]` 已有(现有 store/pipeline 测试在用,应已存在)。

**Step 2: 跑测试确认失败**

Run: `cargo test -p conga-rag inject_builtins`
Expected: 编译错误 `no method named inject_builtins_in`

**Step 3: 最小实现**(config.rs,`impl RagConfig` 内、`expand_tilde` 之后加方法;`store_path` 附近加自由函数)

```rust
/// Root hosting the built-in `notes`/`memory` sources. Default is conga's
/// config dir (`~/.conga`, legacy `~/.gasket` fallback applies);
/// `CONGA_RAG_BUILTIN_BASE` overrides (advanced installs, hermetic tests).
/// Empty string = explicitly disabled → `None`.
pub fn builtin_base() -> Option<PathBuf> {
    match std::env::var("CONGA_RAG_BUILTIN_BASE") {
        Ok(b) if b.trim().is_empty() => None,
        Ok(b) => Some(PathBuf::from(b)),
        Err(_) => Some(conga::storage::config_dir()),
    }
}

/// Built-in memory sources: `notes` (rag_remember output) and `memory`
/// (evolve lessons). Injected when the dir exists and the user's rag.toml
/// has not claimed the name. Called by `load_with` and again by
/// `rag_remember` after it creates the notes dir (idempotent).
pub fn inject_builtins(&mut self) {
    if let Some(base) = builtin_base() {
        self.inject_builtins_in(&base);
    }
}

/// Testable core: explicit base, no env.
pub fn inject_builtins_in(&mut self, base: &Path) {
    for name in ["notes", "memory"] {
        let dir = base.join(name);
        if dir.is_dir() && !self.sources.contains_key(name) {
            self.sources.insert(
                name.to_string(),
                SourceConfig { path: dir, ..Default::default() },
            );
        }
    }
}
```

并在 `load_with` 中插入(`expand_tilde();` 之后、`validate()?;` 之前):

```rust
        cfg.inject_builtins();
```

config.rs 顶部 `use std::path::PathBuf` 已有;`Path` 若未导入则补 `use std::path::{Path, PathBuf};`。

**Step 4: 跑测试确认通过**

Run: `cargo test -p conga-rag`
Expected: 全部 PASS(现有 config 测试不受影响——它们不创建 base/notes、base 为真实 home 时 `~/.conga/notes` 尚不存在;若开发机已有该目录导致现有测试波动,见 Task 3 Step 4 的 env 加固统一处理)

**Step 5: Commit**

```bash
git add conga-rag/src/config.rs
git commit -m "feat(conga-rag): inject built-in notes/memory sources at config load"
```

---

### Task 2: conga-ext — `rag_remember` 工具(注册契约 + 参数校验 + slug)

**Files:**
- Modify: `conga-ext/src/rag.rs`(模块文档首段、`register`、文件级助手、tests)
- Test: 同文件 tests 模块(复用现有 `ENV_LOCK`/`make_ctx`/`registered_tool` 助手)

**Step 1: 写失败测试**(追加到 rag.rs tests;`registered_tool()` 助手参数化或新增 `registered_tool_named(name)`)

```rust
fn registered_tool_named(name: &str) -> ToolDefinition {
    let mut api = ExtensionApiImpl::new();
    register(&mut api);
    api.tools
        .into_iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("{name} registered"))
}

#[test]
fn register_exposes_rag_remember_with_medium_risk() {
    let tool = registered_tool_named("rag_remember");
    // 写文件的工具必须与 write 同级:AutoEdit 放行,Suggest/Plan 需确认。
    assert!(matches!(tool.risk, RiskLevel::Medium));
    assert_eq!(tool.parameters["required"][0], "title");
    assert_eq!(tool.parameters["required"][1], "content");
}

#[test]
fn slugify_keeps_cjk_and_collapses_separators() {
    assert_eq!(slugify("  架构 分层!设计 "), "架构-分层-设计");
    assert_eq!(slugify("A/B\\C:D"), "A-B-C-D");
    assert!(slugify("!!!").is_empty());
}

#[tokio::test]
async fn remember_rejects_blank_and_oversize_args() {
    let tool = registered_tool_named("rag_remember");
    for bad in [
        serde_json::json!({ "title": "  ", "content": "x" }),
        serde_json::json!({ "title": "t", "content": "" }),
        serde_json::json!({ "title": "t", "content": "中".repeat(MAX_CONTENT_CHARS + 1) }),
        serde_json::json!({ "title": "字".repeat(MAX_TITLE_CHARS + 1), "content": "x" }),
    ] {
        assert!((tool.execute)(make_ctx(bad)).await.is_err(), "{bad}");
    }
}
```

**Step 2: 跑测试确认失败**

Run: `cargo test -p conga-ext --features rag remember rag_remember slugify`
Expected: 编译错误(`slugify`/`MAX_*` 未定义、工具未注册)

**Step 3: 实现**(rag.rs;模块文档"Read-only — it never writes the store"改为说明双工具:rag_search 只读,rag_remember 只写 notes/)

```rust
/// `rag_remember`: append (or overwrite by title) a memory note under the
/// built-in `notes` source, then incrementally index it. Files are the
/// source of truth; the store is derived.
const MAX_TITLE_CHARS: usize = 80;
const MAX_CONTENT_CHARS: usize = 8_000;
const MAX_SLUG_CHARS: usize = 60;

/// CJK-safe filename slug: alphanumeric (incl. CJK) kept, everything else
/// collapses to '-'. Empty after trim → caller falls back to "note".
fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in title.trim().chars() {
        if c.is_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c);
        } else {
            dash = true;
        }
    }
    out.chars().take(MAX_SLUG_CHARS).collect()
}

fn notes_dir() -> std::path::PathBuf {
    conga_rag::config::builtin_base()
        .unwrap_or_else(|| conga::storage::config_dir())
        .join("notes")
}
```

`register()` 内追加第二个 `api.register_tool`(完整定义):

```rust
    api.register_tool(ToolDefinition {
        name: "rag_remember".into(),
        label: "RAG Remember".into(),
        description: "把跨会话有用的知识/偏好/事实写入长期记忆(同 title 覆盖旧记忆)。适用:用户说\"记住这个\",或你判断该知识未来会话仍需要。不适用:易变状态、一次性任务细节(不要记)。写入后可用 rag_search 检索。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "title":   { "type": "string", "description": "记忆标题(主键,同题覆盖)" },
                "content": { "type": "string", "description": "自包含的知识陈述,脱离当前对话也能读懂" },
                "tags":    { "type": "array", "items": { "type": "string" }, "description": "可选标签" }
            },
            "required": ["title", "content"]
        }),
        risk: RiskLevel::Medium,
        execute: Arc::new(move |ctx| {
            Box::pin(async move {
                if ctx.aborted() {
                    return Err(ToolError::Message("aborted".into()));
                }
                let title = ctx.args["title"].as_str().unwrap_or_default().trim().to_string();
                let content = ctx.args["content"].as_str().unwrap_or_default().trim().to_string();
                let tags: Vec<String> = ctx.args.get("tags")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let title_chars = title.chars().count();
                if title.is_empty() || title_chars > MAX_TITLE_CHARS {
                    return Err(ToolError::Message(format!(
                        "title 不能为空且不超过 {MAX_TITLE_CHARS} 字符"
                    )));
                }
                if content.is_empty() || content.chars().count() > MAX_CONTENT_CHARS {
                    return Err(ToolError::Message(format!(
                        "content 不能为空且不超过 {MAX_CONTENT_CHARS} 字符"
                    )));
                }

                let (_cfg_path, mut cfg) = conga_rag::config::RagConfig::load()
                    .map_err(|e| ToolError::Message(format!("加载 RAG 配置失败:{e}")))?;
                let dir = notes_dir();
                std::fs::create_dir_all(&dir).map_err(|e| ToolError::Message(e.to_string()))?;
                let slug = { let s = slugify(&title); if s.is_empty() { "note".into() } else { s } };
                let path = dir.join(format!("{slug}.md"));
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
                    .to_string();
                let markdown = conga_host::memory::entry_markdown(
                    &title, &tags, &now, &ctx.ctx.session_id, &content,
                );
                std::fs::write(&path, markdown)
                    .map_err(|e| ToolError::Message(format!("写入失败 {}: {e}", path.display())))?;
                // 目录刚建,确保本轮注入(幂等)
                cfg.inject_builtins();
                info!("rag_remember: title='{}' -> {}", title, path.display());
                match conga_rag::pipeline::run_ingest(&cfg, Some("notes"), false).await {
                    Ok(_) => Ok(ToolResult::text(format!(
                        "已记住:{title}\n文件:{}\n(同 title 再次写入会覆盖;用 rag_search 检索)",
                        path.display()
                    ))),
                    Err(e) => Ok(ToolResult::error(format!(
                        "文件已写入 {} 但索引失败:{e}(下次 ingest 会自动补偿)",
                        path.display()
                    ))),
                }
            })
        }),
    });
```

注:`RagConfig::load()` 失败先于任何磁盘写(无 rag.toml 时不产生目录副作用);`builtin_base()` 为 `None`(显式禁用)时回落 `config_dir()` 保持写入路径稳定。

**Step 4: 跑测试确认通过**

Run: `cargo test -p conga-ext --features rag`
Expected: 全 PASS

**Step 5: Commit**

```bash
git add conga-ext/src/rag.rs
git commit -m "feat(conga-ext): rag_remember tool writes indexed memory notes"
```

---

### Task 3: conga-ext — 端到端 + 覆盖语义 + 现有测试 env 加固 + rag_search 描述更新

**Files:**
- Modify: `conga-ext/src/rag.rs`(tests;`rag_search` description 一行)

**Step 1: 写失败测试**(复用 `env_lock()`、`spawn_mock_embeddings`、`make_ctx`;每个测试开头 `std::env::set_var("CONGA_RAG_BUILTIN_BASE", tmpbase)`、结尾 `remove_var` 两项 env)

```rust
async fn remember_args(title: &str, content: &str) -> serde_json::Value {
    serde_json::json!({ "title": title, "content": content, "tags": ["t"] })
}

#[tokio::test]
async fn remember_then_search_round_trip_and_overwrite() {
    let _g = env_lock().await;
    let base = tempfile::tempdir().unwrap();       // CONGA_RAG_BUILTIN_BASE
    let dbdir = tempfile::tempdir().unwrap();
    let dummy = tempfile::tempdir().unwrap();      // 占位 user source
    std::fs::create_dir_all(base.path().join("notes")).unwrap();
    let (emb, _req) = conga_rag::testsupport::spawn_mock_embeddings(0).await;
    let cfg_path = dbdir.path().join("rag.toml");
    std::fs::write(&cfg_path, format!(
        r#"[sources.dummy]
type = "dir"
path = {:?}

[embedding]
base_url = {:?}
api_key = "k"
model = "mock"

[store]
path = {:?}
"#, dummy.path(), emb, dbdir.path().join("t.db"))).unwrap();
    std::env::set_var("CONGA_RAG_CONFIG", &cfg_path);
    std::env::set_var("CONGA_RAG_BUILTIN_BASE", base.path());

    let remember = registered_tool_named("rag_remember");
    let r = (remember.execute)(make_ctx(remember_args("架构决策", "网关走单进程 embedded 模式 xyzzy")).await
        .expect("first remember ok");
    assert!(text_of(r).contains("已记住"));
    assert!(base.path().join("notes/架构决策.md").exists(), "frontmatter 文件落盘");

    // 同 title 覆盖:单文档、内容更新
    (remember.execute)(make_ctx(remember_args("架构决策", "改主意了,网关拆双进程 quux")).await.unwrap();
    {
        let (_p, cfg) = conga_rag::config::RagConfig::load().unwrap();
        let mut store = conga_rag::store::Store::open(&cfg.store_path()).await.unwrap();
        let docs = store.docs_for_source("notes").await.unwrap();
        assert_eq!(docs.len(), 1, "同 title 覆盖,不是追加");
        store.close().await.unwrap();
    }

    let search = registered_tool_named("rag_search");
    let res = (search.execute)(make_ctx(serde_json::json!({ "query": "quux", "source": "notes" })))
        .await.expect("search ok");
    std::env::remove_var("CONGA_RAG_CONFIG");
    std::env::remove_var("CONGA_RAG_BUILTIN_BASE");
    let text = text_of(res);
    assert!(text.contains("quux") && !text.contains("xyzzy"), "检索到的是覆盖后内容: {text}");
}

#[tokio::test]
async fn remember_reports_partial_success_when_ingest_fails() {
    let _g = env_lock().await;
    let base = tempfile::tempdir().unwrap();
    let dummy = tempfile::tempdir().unwrap();
    let (emb, _req) = conga_rag::testsupport::spawn_mock_embeddings(0).await;
    std::fs::create_dir_all(base.path().join("notes")).unwrap();
    let cfg_path = base.path().join("rag.toml");
    std::fs::write(&cfg_path, format!(
        r#"[sources.dummy]
type = "dir"
path = {:?}

[embedding]
base_url = {:?}
api_key = "k"
model = "mock"

[store]
path = {:?}
"#, dummy.path(), emb, base.path().join("t.db"))).unwrap();
    // 让 store 无法打开:path 是目录
    std::fs::create_dir_all(base.path().join("t.db")).unwrap();
    std::env::set_var("CONGA_RAG_CONFIG", &cfg_path);
    std::env::set_var("CONGA_RAG_BUILTIN_BASE", base.path());

    let remember = registered_tool_named("rag_remember");
    let res = (remember.execute)(make_ctx(remember_args("p", "c"))).await.expect("tool returns Ok");
    std::env::remove_var("CONGA_RAG_CONFIG");
    std::env::remove_var("CONGA_RAG_BUILTIN_BASE");
    let text = text_of(res);
    assert!(text.contains("已写入") && text.contains("索引失败"), "部分成功如实上报: {text}");
    assert!(base.path().join("notes/p.md").exists(), "文件确实落盘");
}
```

`text_of` 需兼容 error 工具结果(`ToolResult::error` 也是 Text block;若 is_error 走独立字段则只取文本即可)。

**Step 2: 跑测试确认失败**

Run: `cargo test -p conga-ext --features rag remember_`
Expected: FAIL(store 未建/注入未生效相关断言)

**Step 3: 修正实现直至通过**(若 Task 2 实现正确,此步应只需微调;失败则按断言修)

**Step 4: 现有测试 env 加固**——`tool_searches_mock_index_end_to_end`、`missing_store_errors_before_search`、`missing_config_fails_loud` 三个测试在 `env_lock()` 后追加 `std::env::set_var("CONGA_RAG_BUILTIN_BASE", "")`,结尾 `remove_var("CONGA_RAG_BUILTIN_BASE")`。防止开发机真实 `~/.conga/notes` 存在时注入污染临时 store(测试断言含 "match" 关键词,真实笔记可能含 "matcher")。

**Step 5: rag_search description 更新**

```rust
description: "语义检索个人知识库(用户笔记/文档 + agent 长期记忆 notes + 蒸馏教训 memory,由 `conga-rag ingest` 或 `rag_remember` 维护)。适用:查找个人笔记、过往总结、项目文档、长期记忆。不适用:找代码文件(用 grep)、联网信息(用 web_search)。返回相关片段与源文件路径;需要全文时用 read 工具读取源文件。source 可选值含 notes/memory。".into(),
```

**Step 6: 全量跑 + Commit**

Run: `cargo test -p conga-ext --features rag`
Expected: 全 PASS

```bash
git add conga-ext/src/rag.rs
git commit -m "test(conga-ext): rag_remember e2e, overwrite semantics, env hardening"
```

---

### Task 4: conga-host — feature `rag` + evolve fail-soft 索引刷新

**Files:**
- Modify: `conga-host/Cargo.toml`(`[dependencies]` 加可选依赖;`[features]` 加一行)
- Modify: `conga-host/src/evolve.rs`(`apply_proposals` 尾部 `out` 前 + 模块级助手 + tests)
- Test: `conga-host/src/evolve.rs` tests(feature-gated)

**Step 1: Cargo 变更**

```toml
# [dependencies] 追加:
conga-rag = { path = "../conga-rag", optional = true }

# [features] 追加:
rag = ["dep:conga-rag"]
```

**Step 2: 写失败测试**(evolve.rs tests;镜像现有 `apply_proposals` 测试的 proposal/policy_always 结构;新增 env 锁)

```rust
    // --- feature rag: memory source reindex ---

    /// CONGA_RAG_CONFIG / CONGA_RAG_BUILTIN_BASE are process-global.
    static RAG_ENV_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

    #[cfg(feature = "rag")]
    fn rag_lock() -> tokio::sync::MutexGuard<'static, ()> {
        RAG_ENV_LOCK.get_or_init(|| tokio::sync::Mutex::new(())).blocking_lock()
    }

    #[cfg(feature = "rag")]
    fn mini_rag_config(base: &std::path::Path, store: &std::path::Path, emb: &str) -> std::path::PathBuf {
        let dummy = base.join("dummy");
        std::fs::create_dir_all(&dummy).unwrap();
        let p = base.join("rag.toml");
        std::fs::write(&p, format!(
            r#"[sources.dummy]
type = "dir"
path = {:?}

[embedding]
base_url = {:?}
api_key = "k"
model = "mock"

[store]
path = {:?}
"#, dummy, emb, store)).unwrap();
        p
    }

    #[tokio::test]
    #[cfg(feature = "rag")]
    async fn evolve_writes_then_reindexes_memory_source() {
        let _g = rag_lock();
        let base = tempfile::tempdir().unwrap();
        let memory_root = base.path().join("memory");
        std::fs::create_dir_all(&memory_root).unwrap();
        std::fs::create_dir_all(base.path().join("notes")).unwrap(); // notes 空目录,自然 0 文件
        let (emb, _req) = conga_rag::testsupport::spawn_mock_embeddings(0).await;
        let cfg_path = mini_rag_config(base.path(), &base.path().join("t.db"), &emb);
        std::env::set_var("CONGA_RAG_CONFIG", &cfg_path);
        std::env::set_var("CONGA_RAG_BUILTIN_BASE", base.path());

        let prop = conga::json!({ /* 与现有 apply_proposals 测试同构:一条 insights */ });
        let out = apply_proposals(&prop_parsed, &memory_root, &base.path().join("skills"), "s1", &policy_always(true)).await;

        assert_eq!(out.added_insights.len(), 1);
        let (_p, cfg) = conga_rag::config::RagConfig::load().unwrap();
        let mut store = conga_rag::store::Store::open(&cfg.store_path()).await.unwrap();
        assert!(!store.docs_for_source("memory").await.unwrap().is_empty(), "蒸馏产物已入索引");
        store.close().await.unwrap();
        std::env::remove_var("CONGA_RAG_CONFIG");
        std::env::remove_var("CONGA_RAG_BUILTIN_BASE");
    }

    #[tokio::test]
    #[cfg(feature = "rag")]
    async fn evolve_survives_index_failure() {
        let _g = rag_lock();
        // rag 配置指向不存在的路径 → hook 静默跳过,evolve 照常成功
        std::env::set_var("CONGA_RAG_CONFIG", "/nonexistent/rag.toml");
        std::env::set_var("CONGA_RAG_BUILTIN_BASE", "");
        let base = tempfile::tempdir().unwrap();
        let out = apply_proposals(&prop_parsed, &base.path().join("memory"), &base.path().join("skills"), "s1", &policy_always(true)).await;
        std::env::remove_var("CONGA_RAG_CONFIG");
        std::env::remove_var("CONGA_RAG_BUILTIN_BASE");
        assert_eq!(out.added_insights.len(), 1, "fail-soft:索引失败不影响 evolve");
    }
```

(proposal 构造照抄现有测试的 helper/字面量,`EvolveProposal` 结构见 evolve.rs 顶部;此处不给伪代码占位,以现有测试为准。)

**Step 3: 跑测试确认失败**

Run: `cargo test -p conga-host --features rag evolve_`
Expected: 编译错误(`index_memory_fail_soft` 未定义)或断言 FAIL(docs_for_source 空)

**Step 4: 实现**(evolve.rs;`apply_proposals` 末尾 `out` 之前调用,加模块级助手)

```rust
    // 4. Best-effort reindex of the `memory` source (conga-rag feature).
    #[cfg(feature = "rag")]
    index_memory_fail_soft(memory_root).await;
    out
```

```rust
/// Fail-soft refresh of the built-in `memory` source after evolve writes.
/// Files are the source of truth — an index lag self-heals on the next
/// `conga-rag ingest` or `rag_remember`. Skipped silently when RAG is
/// unconfigured or the store was never built: evolve must not bootstrap a
/// vector store as a side effect.
#[cfg(feature = "rag")]
async fn index_memory_fail_soft(memory_root: &Path) {
    let Ok((_p, cfg)) = conga_rag::config::RagConfig::load() else {
        return;
    };
    if !memory_root.is_dir() || !cfg.store_path().exists() {
        return;
    }
    match conga_rag::pipeline::run_ingest(&cfg, Some("memory"), false).await {
        Ok(stats) => tracing::info!(added = stats.added, updated = stats.updated, "evolve: memory 源已刷新"),
        Err(e) => tracing::warn!("evolve: memory 索引刷新失败(下次 ingest 补偿): {e}"),
    }
}
```

注:retire(删除)路径同样被增量 ingest 覆盖(`remove_missing`);`Some("memory")` 源不存在时 run_ingest 只是空跑。

**Step 5: 双向验证**

Run: `cargo test -p conga-host --features rag` → 全 PASS
Run: `cargo test -p conga-host` → 全 PASS(无 feature 时 hook 编译剔除,零行为变化)

**Step 6: Commit**

```bash
git add conga-host/Cargo.toml conga-host/src/evolve.rs
git commit -m "feat(host): reindex memory source after evolve behind rag feature"
```

---

### Task 5: feature 传递 + 文档 + 全量验证

**Files:**
- Modify: `conga-ext/Cargo.toml`(features 一行)
- Modify: `rag.example.toml`(追加注释块)

**Step 1: feature 传递**

```toml
# conga-ext [features]:
rag = ["dep:conga-rag", "conga-host/rag"]
```

(src-tauri 已启用 `conga-ext/rag`、conga-cli 的 `ext` 已含 `conga-ext?/rag` → 两端零改动获得 evolve 联动。)

**Step 2: rag.example.toml 追加**

```toml
# 内置源:~/.conga/notes(agent 长期记忆,rag_remember 写入)与
# ~/.conga/memory(evolve 蒸馏教训)在目录存在且名字未被占用时自动注入,
# 无需在此声明。CONGA_RAG_BUILTIN_BASE 可覆盖根目录(空串=禁用,高级用法)。
```

**Step 3: 全量验证矩阵**(每条都必须绿)

```bash
cargo test -p conga-rag                          # Task 1
cargo test -p conga-ext --features rag           # Task 2+3
cargo test -p conga-host                         # 无 feature 回归
cargo test -p conga-host --features rag          # Task 4
cargo check --workspace --all-targets            # workspace 整体
cd ../web/src-tauri && cargo check               # 桌面组合根传递生效
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
```

**Step 4: Commit**

```bash
git add conga-ext/Cargo.toml rag.example.toml
git commit -m "feat(ext): propagate rag feature to host; document built-in sources"
```

---

### 手工冒烟(实施完成后,可选)

1. `conga-rag ingest` 建库(或已有)。
2. 桌面端对话:"记住:conga 的桌面组合根在 web/src-tauri,聊天循环在 chat.rs" → 确认 `rag_remember` 被调用、`~/.conga/notes/` 出现 md。
3. 新会话问"conga 桌面端聊天循环在哪实现" → agent 调 `rag_search` 命中。
4. 触发 evolve → `conga-rag status` 显示 `memory` 源有 chunk。

### 风险与回滚

- 全部新行为在 feature `rag` 之后,src-tauri/cli 已启用;不启用的构建(如第三方仅用 conga-host)零变化。
- store 并发(单写者)沿用既有"报错重试"约定,无新风险面。
- 回滚 = revert 对应 commit;notes 文件与索引可独立清理(`rm -rf ~/.conga/notes && conga-rag ingest`)。
