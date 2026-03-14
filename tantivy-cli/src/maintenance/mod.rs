//! Index maintenance operations.

mod backup;
mod compact;
mod expire;
mod jobs;
mod rebuild;
mod scheduler;
mod stats;

pub use backup::{backup_index, restore_index};
pub use compact::compact_index;
pub use expire::expire_documents;
pub use jobs::{JobId, JobInfo, JobRegistry, JobStatus, JobType};
pub use rebuild::{rebuild_index, RebuildResult};
pub use scheduler::{MaintenanceConfig, MaintenanceScheduler};
pub use stats::IndexHealth;
