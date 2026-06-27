use async_trait::async_trait;
use crate::error::Result;

#[async_trait]
pub trait BackupDriver: Send + Sync {
    async fn create(&self, server_id: &str, uuid: &str, backup_path: &str) -> Result<(String, i64)>;
    async fn delete(&self, backup_path: &str) -> Result<()>;
}
