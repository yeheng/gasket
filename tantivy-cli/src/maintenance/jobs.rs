//! Background job status tracking.
//!
//! Provides a thread-safe job registry for tracking long-running background operations
//! like index rebuilds, compaction, document operations, etc.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

/// Unique job identifier.
pub type JobId = String;

/// Status of a background job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Job is queued but not started.
    Pending,
    /// Job is currently running.
    Running,
    /// Job completed successfully.
    Completed,
    /// Job failed with an error.
    Failed,
}

/// Type of background job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    /// Index rebuild operation.
    IndexRebuild,
    /// Index compaction operation.
    IndexCompact,
    /// Bulk document import.
    BulkImport,
    /// Custom operation.
    Custom(String),
}

/// Information about a background job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    /// Unique job identifier.
    pub id: JobId,
    /// Type of job.
    pub job_type: JobType,
    /// Index name this job operates on (if applicable).
    pub index_name: Option<String>,
    /// Current status.
    pub status: JobStatus,
    /// Progress percentage (0-100), if applicable.
    pub progress: Option<u8>,
    /// Human-readable status message.
    pub message: String,
    /// Error message if failed.
    pub error: Option<String>,
    /// When the job was created.
    pub created_at: DateTime<Utc>,
    /// When the job was started (if started).
    pub started_at: Option<DateTime<Utc>>,
    /// When the job was completed (if completed).
    pub completed_at: Option<DateTime<Utc>>,
}

/// Global job registry for tracking background jobs.
#[derive(Debug, Clone)]
pub struct JobRegistry {
    jobs: Arc<DashMap<JobId, JobInfo>>,
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl JobRegistry {
    /// Create a new empty job registry.
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(DashMap::new()),
        }
    }

    /// Create a new job and return its ID.
    pub fn create_job(&self, job_type: JobType, index_name: Option<String>) -> JobId {
        let job_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();

        let job_info = JobInfo {
            id: job_id.clone(),
            job_type,
            index_name,
            status: JobStatus::Pending,
            progress: None,
            message: "Job queued".to_string(),
            error: None,
            created_at: now,
            started_at: None,
            completed_at: None,
        };

        self.jobs.insert(job_id.clone(), job_info);
        info!("Created job: {}", job_id);
        job_id
    }

    /// Start a job (mark as running).
    pub fn start_job(&self, job_id: &str) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.status = JobStatus::Running;
            job.started_at = Some(Utc::now());
            job.message = "Job started".to_string();
            info!("Started job: {}", job_id);
        }
    }

    /// Update job progress.
    pub fn update_progress(&self, job_id: &str, progress: u8, message: String) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.progress = Some(progress.min(100));
            job.message = message;
        }
    }

    /// Mark a job as completed successfully.
    pub fn complete_job(&self, job_id: &str, message: String) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.status = JobStatus::Completed;
            job.progress = Some(100);
            job.message = message;
            job.completed_at = Some(Utc::now());
            info!("Completed job: {}", job_id);
        }
    }

    /// Mark a job as failed.
    pub fn fail_job(&self, job_id: &str, error: String) {
        if let Some(mut job) = self.jobs.get_mut(job_id) {
            job.status = JobStatus::Failed;
            job.error = Some(error.clone());
            job.message = format!("Job failed: {}", error);
            job.completed_at = Some(Utc::now());
            tracing::error!("Job {} failed: {}", job_id, error);
        }
    }

    /// Get job information by ID.
    pub fn get_job(&self, job_id: &str) -> Option<JobInfo> {
        self.jobs.get(job_id).map(|j| j.clone())
    }

    /// List all jobs, optionally filtered by status or index.
    pub fn list_jobs(&self, status: Option<JobStatus>, index_name: Option<&str>) -> Vec<JobInfo> {
        self.jobs
            .iter()
            .filter(|entry| {
                let job = entry.value();
                let status_match = status.as_ref().is_none_or(|s| &job.status == s);
                let index_match =
                    index_name.is_none_or(|idx| job.index_name.as_ref().is_some_and(|n| n == idx));
                status_match && index_match
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Remove old completed/failed jobs (cleanup).
    pub fn cleanup_old_jobs(&self, max_age: chrono::Duration) {
        let now = Utc::now();
        let job_ids_to_remove: Vec<JobId> = self
            .jobs
            .iter()
            .filter(|entry| {
                let job = entry.value();
                if job.status == JobStatus::Completed || job.status == JobStatus::Failed {
                    if let Some(completed_at) = job.completed_at {
                        let age = now.signed_duration_since(completed_at);
                        return age > max_age;
                    }
                }
                false
            })
            .map(|entry| entry.key().clone())
            .collect();

        for job_id in job_ids_to_remove {
            self.jobs.remove(&job_id);
            info!("Cleaned up old job: {}", job_id);
        }
    }
}
