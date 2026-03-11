//! Index maintenance operations.

mod backup;
mod compact;
mod expire;
mod scheduler;
mod stats;

pub use backup::{backup_index, restore_index};
pub use compact::compact_index;
pub use expire::expire_documents;
pub use scheduler::{MaintenanceConfig, MaintenanceScheduler};
pub use stats::IndexHealth;
