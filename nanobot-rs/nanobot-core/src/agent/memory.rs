//! Memory store for long-term context

use std::path::PathBuf;

use anyhow::Result;
use tracing::debug;

/// Memory store for long-term context
pub struct MemoryStore {
    memory_dir: PathBuf,
}

impl MemoryStore {
    /// Create a new memory store
    pub fn new(workspace: PathBuf) -> Self {
        let memory_dir = workspace.join("memory");
        let _ = std::fs::create_dir_all(&memory_dir);

        Self { memory_dir }
    }

    /// Get the long-term memory file path
    fn memory_path(&self) -> PathBuf {
        self.memory_dir.join("MEMORY.md")
    }

    /// Get the history file path
    fn history_path(&self) -> PathBuf {
        self.memory_dir.join("HISTORY.md")
    }

    /// Read long-term memory
    pub fn read_long_term(&self) -> Result<String> {
        let path = self.memory_path();
        if path.exists() {
            Ok(std::fs::read_to_string(path)?)
        } else {
            Ok(String::new())
        }
    }

    /// Write long-term memory
    pub fn write_long_term(&self, content: &str) -> Result<()> {
        let path = self.memory_path();
        std::fs::write(path, content)?;
        debug!("Wrote long-term memory");
        Ok(())
    }

    /// Append to history
    pub fn append_history(&self, entry: &str) -> Result<()> {
        let path = self.history_path();
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M");
        let content = format!("\n[{}] {}\n", timestamp, entry);
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(content.as_bytes())?;
        debug!("Appended to history");
        Ok(())
    }

    /// Read history
    pub fn read_history(&self) -> Result<String> {
        let path = self.history_path();
        if path.exists() {
            Ok(std::fs::read_to_string(path)?)
        } else {
            Ok(String::new())
        }
    }
}

use std::io::Write;
