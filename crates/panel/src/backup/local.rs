use async_trait::async_trait;
use super::driver::BackupDriver;
use crate::error::Result;

pub struct LocalBackupDriver;

impl LocalBackupDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl BackupDriver for LocalBackupDriver {
    async fn create(&self, _server_id: &str, _uuid: &str, _backup_path: &str) -> Result<(String, i64)> {
        Err(crate::error::PanelError::Internal(
            "backup creation not yet implemented".into(),
        ))
    }

    async fn delete(&self, _backup_path: &str) -> Result<()> {
        Ok(())
    }
}
