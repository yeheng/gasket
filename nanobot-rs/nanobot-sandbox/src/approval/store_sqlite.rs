//! SQLite-based permission store
//!
//! Provides SQLite database persistence for approval rules.

use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::PathBuf;
use tracing::{debug, info, warn};

use super::{ApprovalRule, PermissionStore};
use crate::approval::{OperationType, PermissionLevel, RuleSource};
use crate::error::{Result, SandboxError};

// Re-export RuleSource Display implementation for tests
#[cfg(test)]
use std::str::FromStr;

/// SQLite-based permission store
pub struct SqlitePermissionStore {
    pool: SqlitePool,
}

impl SqlitePermissionStore {
    /// Create a new SQLite store at the given path
    pub async fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    SandboxError::StoreError(format!("Failed to create directory: {}", e))
                })?;
            }
        }

        let db_url = format!("sqlite:{}?mode=rwc", path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await
            .map_err(|e| {
                SandboxError::StoreError(format!("Failed to connect to database: {}", e))
            })?;

        let store = Self { pool };
        store.initialize().await?;

        info!("SQLite permission store initialized at {:?}", path);
        Ok(store)
    }

    /// Create a store in the default location
    pub async fn default_location() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".nanobot")))
            .ok_or_else(|| SandboxError::StoreError("Cannot determine config directory".into()))?;

        let path = config_dir.join("approval_rules.db");
        Self::new(path).await
    }

    /// Initialize the database schema
    async fn initialize(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS approval_rules (
                id TEXT PRIMARY KEY,
                operation_type TEXT NOT NULL,
                operation_data TEXT NOT NULL,
                permission TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT,
                conditions TEXT,
                description TEXT,
                source TEXT NOT NULL DEFAULT 'user'
            );

            CREATE INDEX IF NOT EXISTS idx_operation_type ON approval_rules(operation_type);
            CREATE INDEX IF NOT EXISTS idx_permission ON approval_rules(permission);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| SandboxError::StoreError(format!("Failed to initialize database: {}", e)))?;

        debug!("Database schema initialized");
        Ok(())
    }

    /// Serialize operation type for storage
    fn serialize_operation(op: &OperationType) -> (String, String) {
        let op_type = match op {
            OperationType::Command { .. } => "command",
            OperationType::FileRead { .. } => "file_read",
            OperationType::FileWrite { .. } => "file_write",
            OperationType::Network { .. } => "network",
            OperationType::EnvVar { .. } => "env_var",
            OperationType::Custom { .. } => "custom",
        };

        let op_data = serde_json::to_string(op).unwrap_or_else(|_| "{}".to_string());

        (op_type.to_string(), op_data)
    }

    /// Deserialize operation from storage
    fn deserialize_operation(_op_type: &str, op_data: &str) -> Result<OperationType> {
        serde_json::from_str(op_data).map_err(|e| {
            SandboxError::StoreError(format!("Failed to deserialize operation: {}", e))
        })
    }
}

#[async_trait]
impl PermissionStore for SqlitePermissionStore {
    async fn load_rules(&self) -> Result<Vec<ApprovalRule>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, Option<String>, Option<String>, Option<String>, String)>(
            r#"
            SELECT id, operation_type, operation_data, permission, created_at, expires_at, conditions, description, source
            FROM approval_rules
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SandboxError::StoreError(format!("Failed to load rules: {}", e)))?;

        let mut rules = Vec::new();

        for row in rows {
            let (
                id,
                op_type,
                op_data,
                permission,
                created_at,
                expires_at,
                conditions,
                description,
                source,
            ) = row;

            // Parse UUID
            let id = uuid::Uuid::parse_str(&id)
                .map_err(|e| SandboxError::StoreError(format!("Invalid UUID: {}", e)))?;

            // Parse operation
            let operation = Self::deserialize_operation(&op_type, &op_data)?;

            // Parse permission level
            let permission: PermissionLevel = permission.parse().map_err(|e| {
                SandboxError::StoreError(format!("Invalid permission level: {}", e))
            })?;

            // Parse timestamps
            let created_at = chrono::DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            let expires_at = expires_at
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));

            // Parse conditions
            let conditions: Vec<super::Condition> = conditions
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();

            // Parse source
            let source: RuleSource = source.parse().unwrap_or(RuleSource::User);

            let rule = ApprovalRule {
                id,
                operation,
                permission,
                created_at,
                expires_at,
                conditions,
                description,
                source,
            };

            // Filter out expired rules
            if !rule.is_expired() {
                rules.push(rule);
            }
        }

        debug!("Loaded {} active rules from SQLite", rules.len());
        Ok(rules)
    }

    async fn save_rules(&self, rules: &[ApprovalRule]) -> Result<()> {
        // Clear existing rules
        sqlx::query("DELETE FROM approval_rules")
            .execute(&self.pool)
            .await
            .map_err(|e| SandboxError::StoreError(format!("Failed to clear rules: {}", e)))?;

        // Insert all rules
        for rule in rules {
            self.add_rule(rule).await?;
        }

        debug!("Saved {} rules to SQLite", rules.len());
        Ok(())
    }

    async fn add_rule(&self, rule: &ApprovalRule) -> Result<()> {
        let (op_type, op_data) = Self::serialize_operation(&rule.operation);

        let conditions = if rule.conditions.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&rule.conditions).unwrap_or_default())
        };

        sqlx::query(
            r#"
            INSERT INTO approval_rules (id, operation_type, operation_data, permission, created_at, expires_at, conditions, description, source)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(rule.id.to_string())
        .bind(op_type)
        .bind(op_data)
        .bind(rule.permission.to_string())
        .bind(rule.created_at.to_rfc3339())
        .bind(rule.expires_at.map(|t| t.to_rfc3339()))
        .bind(conditions)
        .bind(&rule.description)
        .bind(rule.source.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| SandboxError::StoreError(format!("Failed to add rule: {}", e)))?;

        debug!("Added rule {} to SQLite", rule.id);
        Ok(())
    }

    async fn remove_rule(&self, rule_id: uuid::Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM approval_rules WHERE id = ?")
            .bind(rule_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| SandboxError::StoreError(format!("Failed to remove rule: {}", e)))?;

        if result.rows_affected() == 0 {
            warn!("Rule {} not found for removal", rule_id);
        }

        debug!("Removed rule {} from SQLite", rule_id);
        Ok(())
    }

    async fn update_rule(&self, rule: &ApprovalRule) -> Result<()> {
        // Check if rule exists
        let exists: Option<(i32,)> = sqlx::query_as("SELECT 1 FROM approval_rules WHERE id = ?")
            .bind(rule.id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| SandboxError::StoreError(format!("Failed to check rule: {}", e)))?;

        if exists.is_some() {
            // Update existing rule
            self.remove_rule(rule.id).await?;
            self.add_rule(rule).await?;
            debug!("Updated rule {} in SQLite", rule.id);
        } else {
            warn!("Rule {} not found for update, adding as new", rule.id);
            self.add_rule(rule).await?;
        }

        Ok(())
    }

    async fn get_rule(&self, rule_id: uuid::Uuid) -> Result<Option<ApprovalRule>> {
        let row = sqlx::query_as::<_, (String, String, String, String, String, Option<String>, Option<String>, Option<String>, String)>(
            r#"
            SELECT id, operation_type, operation_data, permission, created_at, expires_at, conditions, description, source
            FROM approval_rules
            WHERE id = ?
            "#,
        )
        .bind(rule_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SandboxError::StoreError(format!("Failed to get rule: {}", e)))?;

        if let Some((
            id,
            op_type,
            op_data,
            permission,
            created_at,
            expires_at,
            conditions,
            description,
            source,
        )) = row
        {
            let id = uuid::Uuid::parse_str(&id)
                .map_err(|e| SandboxError::StoreError(format!("Invalid UUID: {}", e)))?;

            let operation = Self::deserialize_operation(&op_type, &op_data)?;
            let permission: PermissionLevel = permission.parse().map_err(|e| {
                SandboxError::StoreError(format!("Invalid permission level: {}", e))
            })?;

            let created_at = chrono::DateTime::parse_from_rfc3339(&created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            let expires_at = expires_at
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));

            let conditions: Vec<super::Condition> = conditions
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();

            let source: RuleSource = source.parse().unwrap_or(RuleSource::User);

            Ok(Some(ApprovalRule {
                id,
                operation,
                permission,
                created_at,
                expires_at,
                conditions,
                description,
                source,
            }))
        } else {
            Ok(None)
        }
    }

    async fn clear(&self) -> Result<()> {
        sqlx::query("DELETE FROM approval_rules")
            .execute(&self.pool)
            .await
            .map_err(|e| SandboxError::StoreError(format!("Failed to clear rules: {}", e)))?;

        debug!("Cleared all rules from SQLite");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::OperationType;

    #[tokio::test]
    async fn test_sqlite_store_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test_rules.db");
        let store = SqlitePermissionStore::new(&path).await.unwrap();

        // Create a rule
        let rule = ApprovalRule::new(OperationType::command("ls"), PermissionLevel::Allowed);

        // Add the rule
        store.add_rule(&rule).await.unwrap();

        // Load rules
        let rules = store.load_rules().await.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].operation, OperationType::command("ls"));

        // Get specific rule
        let loaded = store.get_rule(rule.id).await.unwrap();
        assert!(loaded.is_some());

        // Remove the rule
        store.remove_rule(rule.id).await.unwrap();
        let rules = store.load_rules().await.unwrap();
        assert!(rules.is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_store_update() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test_update.db");
        let store = SqlitePermissionStore::new(&path).await.unwrap();

        // Create and add a rule
        let mut rule = ApprovalRule::new(OperationType::command("rm"), PermissionLevel::AskAlways);
        store.add_rule(&rule).await.unwrap();

        // Update the rule
        rule.permission = PermissionLevel::Denied;
        store.update_rule(&rule).await.unwrap();

        // Verify update
        let loaded = store.get_rule(rule.id).await.unwrap().unwrap();
        assert_eq!(loaded.permission, PermissionLevel::Denied);
    }

    #[tokio::test]
    async fn test_sqlite_store_clear() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test_clear.db");
        let store = SqlitePermissionStore::new(&path).await.unwrap();

        // Add multiple rules
        for i in 0..5 {
            let rule = ApprovalRule::new(
                OperationType::command(format!("cmd{}", i)),
                PermissionLevel::Allowed,
            );
            store.add_rule(&rule).await.unwrap();
        }

        let rules = store.load_rules().await.unwrap();
        assert_eq!(rules.len(), 5);

        // Clear all
        store.clear().await.unwrap();
        let rules = store.load_rules().await.unwrap();
        assert!(rules.is_empty());
    }
}
