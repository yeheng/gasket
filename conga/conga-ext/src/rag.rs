//! RAG retrieval tool over the personal conga-rag index.
//!
//! Reads the same config as the `conga-rag` CLI (`$CONGA_RAG_CONFIG` →
//! ./rag.toml → ~/.conga/rag.toml) and searches the vector store built by
//! `conga-rag ingest`. Read-only — it never writes the store, so it is safe
//! to expose in every permission mode. Hit paths are printed absolute so the
//! agent can follow up with the `read` tool (检索 → 精读闭环).

use std::sync::Arc;

use conga::{RiskLevel, ToolDefinition, ToolError, ToolResult};
use tracing::info;

/// Upper bound for the `k` argument (guards the context budget).
const MAX_K: usize = 20;
/// Cap per hit. Chunks are ~1.2k chars by default; this only guards against
/// pathological chunks blowing up the context.
const MAX_HIT_CHARS: usize = 1600;

pub fn register(api: &mut dyn conga::ExtensionApi) {
    api.register_tool(ToolDefinition {
        name: "rag_search".into(),
        label: "RAG Search".into(),
        description: "语义检索个人知识库(笔记/文档,由 `conga-rag ingest` 建立索引)。适用:查找个人笔记、过往总结、项目文档的内容。不适用:找代码文件(用 grep)、联网信息(用 web_search)。返回相关片段与源文件路径;需要全文时用 read 工具读取源文件。".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "检索词(自然语言或关键词)" },
                "k": { "type": "number", "description": "返回条数,默认取配置 ask.top_k,上限 20" },
                "source": { "type": "string", "description": "可选,按源名过滤(如 notes)" }
            },
            "required": ["query"]
        }),
        risk: RiskLevel::Low,
        execute: Arc::new(move |ctx| {
            Box::pin(async move {
                if ctx.aborted() {
                    return Err(ToolError::Message("aborted".into()));
                }
                let query = ctx.args["query"].as_str().unwrap_or_default().trim().to_string();
                if query.is_empty() {
                    return Err(ToolError::Message("query 不能为空".into()));
                }
                let k_arg = ctx.args.get("k").and_then(|v| v.as_u64()).map(|n| n as usize);
                let source = ctx
                    .args
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);

                let (_cfg_path, cfg) = conga_rag::config::RagConfig::load()
                    .map_err(|e| ToolError::Message(format!("加载 RAG 配置失败:{e}")))?;
                // Reject a missing store before run_search: Store::open would
                // create an empty db file, and agent curiosity should not
                // litter the filesystem.
                let store = cfg.store_path();
                if !store.exists() {
                    return Err(ToolError::Message(format!(
                        "索引不存在:{}. 请先运行 conga-rag ingest 建立索引",
                        store.display()
                    )));
                }
                let k = k_arg.unwrap_or(cfg.ask.top_k).clamp(1, MAX_K);
                info!("rag_search: query='{}' k={} source={:?}", query, k, source);

                let hits = conga_rag::search::run_search(&cfg, &query, k, source.as_deref())
                    .await
                    .map_err(|e| ToolError::Message(format!("检索失败:{e}")))?;
                if hits.is_empty() {
                    return Ok(ToolResult::text(
                        "未找到相关片段。换个说法或更具体的关键词再试。",
                    ));
                }
                Ok(ToolResult::text(format_hits(&cfg, &hits)))
            })
        }),
    });
}

/// Absolute path for `read`: `run_search` returns source-root-relative paths
/// (CLI display convention), so rejoin with the configured source root.
/// A path that is already absolute (source root unknown / not a prefix) is
/// shown as-is.
fn display_path(cfg: &conga_rag::config::RagConfig, h: &conga_rag::store::Hit) -> String {
    let p = std::path::Path::new(&h.path);
    if p.is_absolute() {
        return h.path.clone();
    }
    match cfg.sources.get(&h.source) {
        Some(src) => src.path.join(p).to_string_lossy().into_owned(),
        None => h.path.clone(),
    }
}

/// Char-safe truncation (never slices through a multi-byte char).
fn cap_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("\n…(片段过长已截断)");
    out
}

fn format_hits(cfg: &conga_rag::config::RagConfig, hits: &[conga_rag::store::Hit]) -> String {
    let model = cfg
        .resolve_embedding()
        .map(|r| r.model)
        .unwrap_or_else(|_| "?".into());
    let mut out = format!("找到 {} 条相关片段(索引模型 {}):\n\n", hits.len(), model);
    for (i, h) in hits.iter().enumerate() {
        out.push_str(&format!(
            "[{}] score={:.3} {} {} (片段 {})\n",
            i + 1,
            h.score,
            h.source,
            display_path(cfg, h),
            h.ordinal + 1
        ));
        out.push_str(&cap_chars(&h.content, MAX_HIT_CHARS));
        out.push_str("\n\n");
    }
    out.push_str("提示:完整文件可用 read 工具读取;索引基于上次 ingest,最新内容以磁盘为准。");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use conga::{ContentBlock, ExtensionApiImpl, ToolCallCtx};
    use std::sync::atomic::AtomicBool;

    /// CONGA_RAG_CONFIG is process-global; serialize the tests that touch it.
    /// tokio Mutex in a OnceLock (its `new` is not const): the guard is held
    /// across the awaited tool call.
    static ENV_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

    async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        ENV_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    fn make_ctx(args: serde_json::Value) -> ToolCallCtx {
        ToolCallCtx {
            tool_call_id: "t".into(),
            args,
            signal: Arc::new(AtomicBool::new(false)),
            ctx: conga::ToolContext {
                cwd: std::env::temp_dir(),
                env: Default::default(),
                session_id: "test".into(),
                state_dir: std::env::temp_dir(),
            },
        }
    }

    fn registered_tool() -> ToolDefinition {
        let mut api = ExtensionApiImpl::new();
        register(&mut api);
        api.tools
            .into_iter()
            .find(|t| t.name == "rag_search")
            .expect("rag_search registered")
    }

    fn text_of(res: ToolResult) -> String {
        match res.content.first() {
            Some(ContentBlock::Text { text }) => text.clone(),
            other => panic!("expected text block, got {other:?}"),
        }
    }

    #[test]
    fn register_exposes_rag_search_with_low_risk() {
        let tool = registered_tool();
        // Security invariant: the tool is read-only, so Low is correct —
        // and it must never be raised to auto-approve-worthy risk silently.
        assert!(matches!(tool.risk, RiskLevel::Low));
        assert!(tool.parameters["properties"]["query"].is_object());
        assert_eq!(tool.parameters["required"][0], "query");
    }

    #[tokio::test]
    async fn tool_searches_mock_index_end_to_end() {
        let _g = env_lock().await;
        let notes = tempfile::tempdir().unwrap();
        let dbdir = tempfile::tempdir().unwrap();
        std::fs::write(
            notes.path().join("架构.md"),
            "架构分层设计要点 match target",
        )
        .unwrap();
        std::fs::write(notes.path().join("其他.md"), "completely unrelated").unwrap();
        let (base, _req) = conga_rag::testsupport::spawn_mock_embeddings(0).await;
        let cfg_path = dbdir.path().join("rag.toml");
        std::fs::write(
            &cfg_path,
            format!(
                r#"[sources.notes]
kind = "dir"
path = {:?}
include = ["**/*.md"]
exclude = []

[embedding]
base_url = {:?}
api_key = "k"
model = "mock"
batch = 4

[store]
path = {:?}
"#,
                notes.path(),
                base,
                dbdir.path().join("t.db")
            ),
        )
        .unwrap();
        std::env::set_var("CONGA_RAG_CONFIG", &cfg_path);

        let (_p, cfg) = conga_rag::config::RagConfig::load().unwrap();
        conga_rag::pipeline::run_ingest(&cfg, None, false)
            .await
            .unwrap();

        let tool = registered_tool();
        let res = (tool.execute)(make_ctx(serde_json::json!({ "query": "match", "k": 2 })))
            .await
            .expect("tool succeeds");
        std::env::remove_var("CONGA_RAG_CONFIG");

        let text = text_of(res);
        assert!(text.contains("找到"), "{text}");
        assert!(text.contains("match target"), "{text}");
        assert!(
            text.contains("架构.md"),
            "absolute-ish source path shown: {text}"
        );
        assert!(text.contains("read 工具"), "follow-up hint present: {text}");
    }

    #[tokio::test]
    async fn missing_store_errors_before_search() {
        let _g = env_lock().await;
        let notes = tempfile::tempdir().unwrap();
        let dbdir = tempfile::tempdir().unwrap();
        let (base, _req) = conga_rag::testsupport::spawn_mock_embeddings(0).await;
        let cfg_path = dbdir.path().join("rag.toml");
        std::fs::write(
            &cfg_path,
            format!(
                r#"[sources.notes]
kind = "dir"
path = {:?}
include = ["**/*.md"]
exclude = []

[embedding]
base_url = {:?}
api_key = "k"
model = "mock"

[store]
path = {:?}
"#,
                notes.path(),
                base,
                dbdir.path().join("never-built.db")
            ),
        )
        .unwrap();
        std::env::set_var("CONGA_RAG_CONFIG", &cfg_path);

        let tool = registered_tool();
        let res = (tool.execute)(make_ctx(serde_json::json!({ "query": "x" }))).await;
        std::env::remove_var("CONGA_RAG_CONFIG");

        let err = res.expect_err("store missing must fail");
        assert!(err.to_string().contains("conga-rag ingest"), "{err}");
    }

    #[tokio::test]
    async fn missing_config_fails_loud() {
        let _g = env_lock().await;
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("CONGA_RAG_CONFIG", tmp.path().join("no-such-rag.toml"));

        let tool = registered_tool();
        let res = (tool.execute)(make_ctx(serde_json::json!({ "query": "x" }))).await;
        std::env::remove_var("CONGA_RAG_CONFIG");

        let err = res.expect_err("config missing must fail");
        assert!(err.to_string().contains("CONGA_RAG_CONFIG"), "{err}");
    }

    #[tokio::test]
    async fn empty_query_is_rejected() {
        let tool = registered_tool();
        let res = (tool.execute)(make_ctx(serde_json::json!({ "query": "  " }))).await;
        assert!(res.is_err(), "blank query rejected without touching config");
    }

    #[test]
    fn cap_chars_is_char_safe() {
        let s = "中".repeat(2000);
        let out = cap_chars(&s, MAX_HIT_CHARS);
        // head + one-line truncation suffix; char-boundary slicing must not
        // panic on multi-byte input.
        assert!(out.chars().count() <= MAX_HIT_CHARS + 20);
        assert!(out.contains("…"));
        // Short input passes through untouched.
        let short = "短".repeat(10);
        assert_eq!(cap_chars(&short, MAX_HIT_CHARS), short);
    }
}
