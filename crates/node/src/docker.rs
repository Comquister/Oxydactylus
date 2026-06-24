use async_trait::async_trait;
use futures_util::stream::BoxStream;
use crate::error::{NodeError, Result};

pub struct ContainerSpec {
    pub image:       String,
    pub name:        String,
    pub env:         Vec<String>,
    pub memory_mb:   i64,
    pub cpu_percent: i64,
}

pub struct ContainerStats {
    pub memory_bytes: u64,
    pub cpu_percent:  f64,
    pub rx_bytes:     u64,
    pub tx_bytes:     u64,
}

pub struct LogChunk {
    pub content: String,
    pub stream:  String,
}

#[async_trait]
#[cfg_attr(test, mockall::automock)]
pub trait DockerBackend: Send + Sync + 'static {
    async fn create_container(&self, spec: ContainerSpec) -> Result<String>;
    async fn start_container(&self, id: String)  -> Result<()>;
    async fn stop_container(&self, id: String, timeout: u32) -> Result<()>;
    async fn delete_container(&self, id: String) -> Result<()>;
    async fn send_command(&self, id: String, command: String) -> Result<()>;
    async fn get_stats(&self, id: String) -> Result<ContainerStats>;
    async fn log_stream(&self, id: String, follow: bool)
        -> Result<BoxStream<'static, Result<LogChunk>>>;
}

pub struct BollardDocker {
    inner: bollard::Docker,
}

impl BollardDocker {
    pub fn connect() -> Result<Self> {
        let inner = bollard::Docker::connect_with_local_defaults()
            .map_err(|e| NodeError::Docker(e.to_string()))?;
        Ok(Self { inner })
    }
}
