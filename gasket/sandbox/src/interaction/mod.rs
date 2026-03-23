//! User interaction for approval requests
//!
//! Provides the `ApprovalInteraction` trait for user confirmations.
//!
//! ## Built-in Implementations
//!
//! - `DenyAllInteraction`: Always denies approval requests
//! - `AllowAllInteraction`: Always allows approval requests
//!
//! ## Custom Implementations
//!
//! For interactive approval (e.g., CLI prompts, WebSocket UI), implement
//! this trait in your application layer and inject it into the sandbox.

use async_trait::async_trait;

use crate::approval::{ApprovalRequest, PermissionLevel};
use crate::error::Result;

/// Approval interaction trait
///
/// Implement this trait to provide custom approval interaction behavior.
/// The sandbox will call `confirm()` when it needs user approval for
/// sensitive operations.
#[async_trait]
pub trait ApprovalInteraction: Send + Sync {
    /// Request user confirmation for an operation
    ///
    /// Returns the permission level granted by the user.
    async fn confirm(&self, request: &ApprovalRequest) -> Result<PermissionLevel>;
}

/// No-op interaction handler that always denies
pub struct DenyAllInteraction;

#[async_trait]
impl ApprovalInteraction for DenyAllInteraction {
    async fn confirm(&self, _request: &ApprovalRequest) -> Result<PermissionLevel> {
        Ok(PermissionLevel::Denied)
    }
}

/// No-op interaction handler that always allows
pub struct AllowAllInteraction;

#[async_trait]
impl ApprovalInteraction for AllowAllInteraction {
    async fn confirm(&self, _request: &ApprovalRequest) -> Result<PermissionLevel> {
        Ok(PermissionLevel::Allowed)
    }
}
