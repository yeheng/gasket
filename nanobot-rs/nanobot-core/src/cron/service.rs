//! Cron service for scheduled tasks

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// A scheduled job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    /// Unique job ID
    pub id: String,

    /// Job name
    pub name: String,

    /// Cron expression
    pub cron: String,

    /// Message to send
    pub message: String,

    /// Target channel
    #[serde(default)]
    pub channel: Option<String>,

    /// Target chat ID
    #[serde(default)]
    pub chat_id: Option<String>,

    /// Last run time
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,

    /// Next run time
    #[serde(default)]
    pub next_run: Option<DateTime<Utc>>,

    /// Enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl CronJob {
    /// Create a new cron job
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        cron: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let cron_str = cron.into();
        let next_run = Self::calculate_next_run(&cron_str);

        Self {
            id: id.into(),
            name: name.into(),
            cron: cron_str,
            message: message.into(),
            channel: None,
            chat_id: None,
            last_run: None,
            next_run,
            enabled: true,
        }
    }

    /// Calculate next run time from cron expression
    fn calculate_next_run(cron_expr: &str) -> Option<DateTime<Utc>> {
        let schedule: Schedule = cron_expr.parse().ok()?;
        let now = chrono::Utc::now();
        schedule.after(&now).next()
    }

    /// Update next run time
    pub fn update_next_run(&mut self) {
        self.last_run = Some(Utc::now());
        self.next_run = Self::calculate_next_run(&self.cron);
    }
}

/// Cron service with dirty-flag persistence.
///
/// Jobs are kept in memory and flushed to disk by a background task
/// every 5 seconds, but only when changes have actually occurred.
pub struct CronService {
    jobs: Arc<RwLock<HashMap<String, CronJob>>>,
    jobs_dir: std::path::PathBuf,
    dirty: Arc<AtomicBool>,
    _flusher_handle: tokio::task::JoinHandle<()>,
}

impl CronService {
    /// Create a new cron service
    pub fn new(workspace: std::path::PathBuf) -> Self {
        let jobs_dir = workspace.join("cron");
        let _ = std::fs::create_dir_all(&jobs_dir);

        let jobs = Arc::new(RwLock::new(HashMap::new()));
        let dirty = Arc::new(AtomicBool::new(false));

        // Load existing jobs synchronously during init
        {
            let path = jobs_dir.join("jobs.json");
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        if let Ok(loaded) = serde_json::from_str::<HashMap<String, CronJob>>(&content) {
                            let loaded: HashMap<String, CronJob> = loaded
                                .into_iter()
                                .map(|(id, mut job)| {
                                    job.next_run = CronJob::calculate_next_run(&job.cron);
                                    (id, job)
                                })
                                .collect();
                            info!("Loaded {} cron jobs", loaded.len());
                            *jobs.blocking_write() = loaded;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read cron jobs file: {}", e);
                    }
                }
            }
        }

        // Spawn background flusher (every 5 seconds)
        let flusher_handle = {
            let jobs = jobs.clone();
            let dirty = dirty.clone();
            let jobs_dir = jobs_dir.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    if dirty.swap(false, Ordering::AcqRel) {
                        let jobs_snapshot = jobs.read().await.clone();
                        let path = jobs_dir.join("jobs.json");
                        match serde_json::to_string_pretty(&jobs_snapshot) {
                            Ok(content) => {
                                if let Err(e) = std::fs::write(&path, content) {
                                    warn!("Failed to flush cron jobs: {}", e);
                                } else {
                                    debug!("Flushed {} cron jobs to disk", jobs_snapshot.len());
                                }
                            }
                            Err(e) => warn!("Failed to serialize cron jobs: {}", e),
                        }
                    }
                }
            })
        };

        Self {
            jobs,
            jobs_dir,
            dirty,
            _flusher_handle: flusher_handle,
        }
    }

    /// Mark jobs as dirty (will be flushed by background task)
    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    /// Force-flush jobs to disk immediately
    fn flush_sync(&self, jobs: &HashMap<String, CronJob>) {
        let path = self.jobs_dir.join("jobs.json");
        if let Ok(content) = serde_json::to_string_pretty(jobs) {
            let _ = std::fs::write(&path, content);
        }
    }

    /// Add a job (immediately flushed since it's a user-facing mutation)
    pub async fn add_job(&self, job: CronJob) -> anyhow::Result<()> {
        let mut jobs = self.jobs.write().await;
        jobs.insert(job.id.clone(), job.clone());
        self.flush_sync(&jobs);
        info!("Added cron job: {} ({})", job.name, job.id);
        Ok(())
    }

    /// Remove a job (immediately flushed since it's a user-facing mutation)
    pub async fn remove_job(&self, id: &str) -> anyhow::Result<bool> {
        let mut jobs = self.jobs.write().await;
        let removed = jobs.remove(id).is_some();
        if removed {
            self.flush_sync(&jobs);
            info!("Removed cron job: {}", id);
        }
        Ok(removed)
    }

    /// Get a job
    pub async fn get_job(&self, id: &str) -> Option<CronJob> {
        let jobs = self.jobs.read().await;
        jobs.get(id).cloned()
    }

    /// List all jobs
    pub async fn list_jobs(&self) -> Vec<CronJob> {
        let jobs = self.jobs.read().await;
        jobs.values().cloned().collect()
    }

    /// Get jobs that are due to run
    pub async fn get_due_jobs(&self) -> Vec<CronJob> {
        let jobs = self.jobs.read().await;
        let now = Utc::now();

        jobs.values()
            .filter(|job| job.enabled && job.next_run.is_some_and(|next| next <= now))
            .cloned()
            .collect()
    }

    /// Mark a job as run (debounced — only marks dirty, flushed by background task)
    pub async fn mark_job_run(&self, id: &str) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(id) {
            job.update_next_run();
        }
        self.mark_dirty();
        debug!("Marked job {} as run", id);
    }
}
