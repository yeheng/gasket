//! Config: TOML file discovery + `CONGA_RAG_*` env overrides.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

fn envvar(k: &str) -> Result<String, std::env::VarError> {
    std::env::var(k)
}

/// Full user-facing config, deserialized from `rag.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct RagConfig {
    pub sources: BTreeMap<String, SourceConfig>,
    pub embedding: EmbeddingConfig,
    pub chunking: ChunkingConfig,
    pub store: StoreConfig,
    pub ask: AskConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SourceConfig {
    #[serde(rename = "type")]
    pub kind: String,
    pub path: PathBuf,
    /// Glob patterns matched against the path relative to `path` ('/'-separated).
    /// Empty = match every file.
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            kind: "dir".into(),
            path: PathBuf::new(),
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub batch: usize,
    /// Minimum pause between consecutive embedding requests (0 = off).
    /// Guards per-minute quotas (e.g. Ark 429) when a large ingest fires
    /// many back-to-back batches.
    pub min_interval_ms: u64,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            api_key: None,
            model: None,
            batch: 64,
            min_interval_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ChunkingConfig {
    pub target_chars: usize,
    pub overlap_chars: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            target_chars: 1200,
            overlap_chars: 200,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct StoreConfig {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AskConfig {
    pub top_k: usize,
}

impl Default for AskConfig {
    fn default() -> Self {
        Self { top_k: 6 }
    }
}

/// Embedding connection fully resolved (config + fallback + env).
#[derive(Debug, Clone)]
pub struct ResolvedEmbedding {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub batch: usize,
    pub min_interval_ms: u64,
}

impl RagConfig {
    /// Load: dotenv + discovery + env overrides. Returns the file path used.
    pub fn load() -> anyhow::Result<(PathBuf, Self)> {
        let _ = dotenvy::dotenv();
        Self::load_with(&envvar)
    }

    pub fn load_with(
        env: &dyn Fn(&str) -> Result<String, std::env::VarError>,
    ) -> anyhow::Result<(PathBuf, Self)> {
        let path = Self::discover(env)?.ok_or_else(|| {
            anyhow::anyhow!(
                "未找到配置:设置 CONGA_RAG_CONFIG,或创建 ./rag.toml / ~/.conga/rag.toml"
            )
        })?;
        let raw = std::fs::read_to_string(&path)?;
        let mut cfg: RagConfig = toml::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("配置解析失败 {}: {e}", path.display()))?;
        cfg.apply_env(env)?;
        cfg.expand_tilde();
        cfg.inject_builtins();
        cfg.validate()?;
        Ok((path, cfg))
    }

    /// `CONGA_RAG_CONFIG` → `./rag.toml` → `~/.conga/rag.toml`,取第一个存在者。
    /// `CONGA_RAG_CONFIG` 已设置但文件不存在 → 报错(fail-loud,不静默回落)。
    pub fn discover(
        env: &dyn Fn(&str) -> Result<String, std::env::VarError>,
    ) -> anyhow::Result<Option<PathBuf>> {
        if let Ok(p) = env("CONGA_RAG_CONFIG") {
            let p = PathBuf::from(&p);
            if p.exists() {
                return Ok(Some(p));
            }
            anyhow::bail!("CONGA_RAG_CONFIG 已设置但文件不存在: {}", p.display());
        }
        let cwd = PathBuf::from("rag.toml");
        if cwd.exists() {
            return Ok(Some(cwd));
        }
        Ok(dirs::home_dir()
            .map(|h| h.join(".conga/rag.toml"))
            .filter(|p| p.exists()))
    }

    /// `CONGA_RAG_*` 单值覆盖。不可解析的值直接报错(fail-loud)。
    pub fn apply_env(
        &mut self,
        env: &dyn Fn(&str) -> Result<String, std::env::VarError>,
    ) -> anyhow::Result<()> {
        let s = |k: &str| env(k).ok();
        if let Some(v) = s("CONGA_RAG_EMBED_BASE_URL") {
            self.embedding.base_url = Some(v);
        }
        if let Some(v) = s("CONGA_RAG_EMBED_KEY") {
            self.embedding.api_key = Some(v);
        }
        if let Some(v) = s("CONGA_RAG_EMBED_MODEL") {
            self.embedding.model = Some(v);
        }
        if let Some(v) = s("CONGA_RAG_EMBED_BATCH") {
            self.embedding.batch = v
                .parse()
                .map_err(|_| anyhow::anyhow!("CONGA_RAG_EMBED_BATCH 值无法解析为 usize: {v:?}"))?;
        }
        if let Some(v) = s("CONGA_RAG_EMBED_MIN_INTERVAL_MS") {
            self.embedding.min_interval_ms = v.parse().map_err(|_| {
                anyhow::anyhow!("CONGA_RAG_EMBED_MIN_INTERVAL_MS 值无法解析为 u64: {v:?}")
            })?;
        }
        if let Some(v) = s("CONGA_RAG_STORE_PATH") {
            self.store.path = Some(PathBuf::from(v));
        }
        Ok(())
    }

    /// `~/` 前缀展开到家目录(源路径与库路径)。
    pub fn expand_tilde(&mut self) {
        let expand = |p: &PathBuf| -> PathBuf {
            let s = p.to_string_lossy();
            if let Some(rest) = s.strip_prefix("~/") {
                dirs::home_dir()
                    .map(|h| h.join(rest))
                    .unwrap_or_else(|| p.clone())
            } else {
                p.clone()
            }
        };
        for src in self.sources.values_mut() {
            src.path = expand(&src.path);
        }
        if let Some(p) = &self.store.path {
            self.store.path = Some(expand(p));
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.sources.is_empty(),
            "配置缺少 [sources.*]:至少一个输入源"
        );
        for (name, src) in &self.sources {
            anyhow::ensure!(src.kind == "dir", "源 {name}: 首期仅支持 type = \"dir\"");
            anyhow::ensure!(!src.path.as_os_str().is_empty(), "源 {name}: 缺少 path");
        }
        anyhow::ensure!(self.embedding.batch > 0, "embedding.batch 必须 > 0");
        anyhow::ensure!(
            self.chunking.overlap_chars < self.chunking.target_chars,
            "chunking.overlap_chars({}) 必须小于 target_chars({})",
            self.chunking.overlap_chars,
            self.chunking.target_chars
        );
        Ok(())
    }

    /// Embedding connection after config → env → CONGA_LLM_* fallback.
    pub fn resolve_embedding(&self) -> anyhow::Result<ResolvedEmbedding> {
        self.resolve_embedding_with(&envvar)
    }

    pub fn resolve_embedding_with(
        &self,
        env: &dyn Fn(&str) -> Result<String, std::env::VarError>,
    ) -> anyhow::Result<ResolvedEmbedding> {
        let base_url = self
            .embedding
            .base_url
            .clone()
            .or_else(|| env("CONGA_LLM_BASE_URL").ok())
            .ok_or_else(|| {
                anyhow::anyhow!("embedding.base_url 未配置且 CONGA_LLM_BASE_URL 未设置")
            })?;
        let api_key = self
            .embedding
            .api_key
            .clone()
            .or_else(|| env("CONGA_LLM_KEY").ok())
            .ok_or_else(|| anyhow::anyhow!("embedding.api_key 未配置且 CONGA_LLM_KEY 未设置"))?;
        let model = self
            .embedding
            .model
            .clone()
            .ok_or_else(|| anyhow::anyhow!("embedding.model 未配置"))?;
        Ok(ResolvedEmbedding {
            base_url,
            api_key,
            model,
            batch: self.embedding.batch,
            min_interval_ms: self.embedding.min_interval_ms,
        })
    }

    pub fn store_path(&self) -> PathBuf {
        self.store.path.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".conga/rag/index.db")
        })
    }

    /// Builder: override the store path (test injection).
    pub fn with_store_path(mut self, path: PathBuf) -> Self {
        self.store.path = Some(path);
        self
    }

    /// Built-in memory sources: `memory` (rag_remember output + evolve
    /// lessons, one shared library) and `notes` (legacy rag_remember
    /// output, kept indexed so pre-move files stay searchable). Injected
    /// when the dir exists and the user's rag.toml has not claimed the
    /// name. Called by `load_with` and again by `rag_remember` after it
    /// creates the memory dir (idempotent).
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
                    SourceConfig {
                        path: dir,
                        ..Default::default()
                    },
                );
            }
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = r#"
[sources.notes]
type = "dir"
path = "/tmp/notes"
include = ["**/*.md"]
exclude = ["**/drafts/**"]

[embedding]
base_url = "https://api.example.com/v1"
api_key = "k1"
model = "emb-1"
batch = 8

[chunking]
target_chars = 500
overlap_chars = 50
"#;

    fn no_env(_: &str) -> Result<String, std::env::VarError> {
        Err(std::env::VarError::NotPresent)
    }

    #[test]
    fn parses_all_sections() {
        let cfg: RagConfig = toml::from_str(TOML).unwrap();
        let src = &cfg.sources["notes"];
        assert_eq!(src.kind, "dir");
        assert_eq!(src.include, vec!["**/*.md"]);
        assert_eq!(cfg.embedding.batch, 8);
        assert_eq!(cfg.chunking.target_chars, 500);
        assert_eq!(cfg.ask.top_k, 6); // default when absent
    }

    #[test]
    fn resolve_embedding_uses_config_values() {
        let cfg: RagConfig = toml::from_str(TOML).unwrap();
        let r = cfg.resolve_embedding_with(&no_env).unwrap();
        assert_eq!(r.base_url, "https://api.example.com/v1");
        assert_eq!(r.batch, 8);
    }

    #[test]
    fn embedding_falls_back_to_conga_llm_env() {
        let cfg: RagConfig = toml::from_str("[embedding]\nmodel = \"emb-1\"\n").unwrap();
        let env = |k: &str| match k {
            "CONGA_LLM_BASE_URL" => Ok("https://fb.example.com/v1".into()),
            "CONGA_LLM_KEY" => Ok("fb-key".into()),
            _ => Err(std::env::VarError::NotPresent),
        };
        let r = cfg.resolve_embedding_with(&env).unwrap();
        assert_eq!(r.base_url, "https://fb.example.com/v1");
        assert_eq!(r.api_key, "fb-key");
    }

    #[test]
    fn env_overrides_file() {
        let env = |k: &str| match k {
            "CONGA_RAG_EMBED_MODEL" => Ok("env-model".into()),
            "CONGA_RAG_EMBED_BATCH" => Ok("3".into()),
            "CONGA_RAG_EMBED_MIN_INTERVAL_MS" => Ok("1500".into()),
            "CONGA_LLM_BASE_URL" => Ok("https://fb.example.com/v1".into()),
            "CONGA_LLM_KEY" => Ok("fb-key".into()),
            _ => Err(std::env::VarError::NotPresent),
        };
        let mut cfg: RagConfig = toml::from_str(TOML).unwrap();
        cfg.apply_env(&env).unwrap();
        let r = cfg.resolve_embedding_with(&env).unwrap();
        assert_eq!(r.model, "env-model");
        assert_eq!(r.batch, 3);
        assert_eq!(r.min_interval_ms, 1500);
    }

    #[test]
    fn discover_fails_loud_when_env_config_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("no-such-rag.toml");
        let env = |_k: &str| -> Result<String, std::env::VarError> {
            Ok(missing.to_string_lossy().into_owned())
        };
        let err = RagConfig::discover(&env).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("CONGA_RAG_CONFIG"), "must name the var: {msg}");
        assert!(
            msg.contains("no-such-rag.toml"),
            "must name the path: {msg}"
        );
    }

    #[test]
    fn apply_env_rejects_unparsable_batch() {
        let env = |k: &str| match k {
            "CONGA_RAG_EMBED_BATCH" => Ok("not-a-number".into()),
            _ => Err(std::env::VarError::NotPresent),
        };
        let mut cfg: RagConfig = toml::from_str(TOML).unwrap();
        let err = cfg.apply_env(&env).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("CONGA_RAG_EMBED_BATCH"),
            "must name the var: {msg}"
        );
        assert!(
            msg.contains("not-a-number"),
            "must include the value: {msg}"
        );
    }

    #[test]
    fn missing_model_and_url_is_error() {
        let cfg: RagConfig = toml::from_str("").unwrap();
        assert!(cfg.resolve_embedding_with(&no_env).is_err());
    }

    #[test]
    fn validation_rejects_bad_chunking_and_empty_sources() {
        let cfg: RagConfig = toml::from_str("").unwrap();
        assert!(cfg.validate().is_err(), "empty sources");
        let cfg2: RagConfig = toml::from_str(TOML).unwrap();
        assert!(cfg2.validate().is_ok());
        let mut cfg3: RagConfig = toml::from_str(TOML).unwrap();
        cfg3.chunking.overlap_chars = 600;
        assert!(cfg3.validate().is_err(), "overlap must be < target");
    }

    #[test]
    fn store_path_defaults_into_conga_home() {
        let cfg: RagConfig = toml::from_str("").unwrap();
        let p = cfg.store_path();
        assert!(p.to_string_lossy().contains("rag"));
        assert!(p.to_string_lossy().ends_with("index.db"));
    }

    #[test]
    fn shipped_example_file_parses_and_validates() {
        let raw =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../rag.example.toml"))
                .expect("rag.example.toml must exist next to the workspace");
        let mut cfg: RagConfig = toml::from_str(&raw).expect("example must parse");
        cfg.expand_tilde();
        cfg.validate().expect("example must be a valid config");
        assert!(cfg.sources.contains_key("docs"));
        assert!(cfg.sources.contains_key("code"));
    }

    // --- built-in source injection ---

    /// Hermetic core: explicit base, no env. Mirrors append_memory_in pattern.
    fn mk_cfg_with_source(name: &str, path: &std::path::Path) -> RagConfig {
        let mut cfg = RagConfig::default();
        cfg.sources.insert(
            name.into(),
            SourceConfig {
                path: path.into(),
                ..Default::default()
            },
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
}
