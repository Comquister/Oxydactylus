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

#[async_trait]
impl DockerBackend for BollardDocker {
    async fn create_container(&self, spec: ContainerSpec) -> Result<String> {
        use bollard::container::{Config, CreateContainerOptions};
        use bollard::models::HostConfig;

        if spec.memory_mb <= 0 || spec.cpu_percent <= 0 {
            return Err(NodeError::Validation(
                "memory_mb and cpu_percent must be positive".into(),
            ));
        }

        let opts = CreateContainerOptions {
            name:     spec.name.as_str(),
            platform: None,
        };
        let cfg = Config {
            image:       Some(spec.image.as_str()),
            env:         Some(spec.env.iter().map(String::as_str).collect()),
            open_stdin:  Some(true),
            stdin_once:  Some(false),
            host_config: Some(HostConfig {
                memory:    Some(spec.memory_mb * 1024 * 1024),
                nano_cpus: Some(spec.cpu_percent * 10_000_000),
                ..Default::default()
            }),
            ..Default::default()
        };
        let resp = self.inner.create_container(Some(opts), cfg).await?;
        Ok(resp.id)
    }

    async fn start_container(&self, id: String) -> Result<()> {
        self.inner
            .start_container::<String>(&id, None)
            .await
            .map_err(NodeError::from)
    }

    async fn stop_container(&self, id: String, timeout: u32) -> Result<()> {
        use bollard::container::StopContainerOptions;
        self.inner
            .stop_container(&id, Some(StopContainerOptions { t: timeout as i64 }))
            .await
            .map_err(NodeError::from)
    }

    async fn delete_container(&self, id: String) -> Result<()> {
        use bollard::container::RemoveContainerOptions;
        self.inner
            .remove_container(&id, Some(RemoveContainerOptions {
                v:     true,
                force: false,
                link:  false,
            }))
            .await
            .map_err(NodeError::from)
    }

    async fn send_command(&self, id: String, command: String) -> Result<()> {
        use bollard::container::AttachContainerOptions;
        use tokio::io::AsyncWriteExt;

        let mut attach = self.inner
            .attach_container(&id, Some(AttachContainerOptions::<String> {
                stdin:  Some(true),
                stream: Some(true),
                stdout: Some(false),
                stderr: Some(false),
                ..Default::default()
            }))
            .await
            .map_err(NodeError::from)?;

        let payload = format!("{}\n", command);
        attach.input
            .write_all(payload.as_bytes())
            .await
            .map_err(|e| NodeError::Docker(e.to_string()))
    }

    async fn get_stats(&self, _id: String) -> Result<ContainerStats> {
        unimplemented!("implemented in Task 5")
    }

    async fn log_stream(&self, _id: String, _follow: bool)
        -> Result<BoxStream<'static, Result<LogChunk>>>
    {
        unimplemented!("implemented in Task 4")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::future::FutureExt;

    #[tokio::test]
    async fn mock_create_container_returns_id() {
        let mut mock = MockDockerBackend::new();
        mock.expect_create_container()
            .once()
            .returning(|spec| {
                async move {
                    assert_eq!(spec.image, "nginx:latest");
                    assert_eq!(spec.name, "srv-1");
                    assert_eq!(spec.memory_mb, 512);
                    assert_eq!(spec.cpu_percent, 50);
                    Ok("abc123".to_string())
                }.boxed()
            });

        let id = mock.create_container(ContainerSpec {
            image:       "nginx:latest".into(),
            name:        "srv-1".into(),
            env:         vec!["PORT=25565".into()],
            memory_mb:   512,
            cpu_percent: 50,
        }).await.unwrap();

        assert_eq!(id, "abc123");
    }

    #[tokio::test]
    async fn mock_start_container_called_with_id() {
        let mut mock = MockDockerBackend::new();
        mock.expect_start_container()
            .withf(|id| id == "abc123")
            .once()
            .returning(|_| async { Ok(()) }.boxed());

        mock.start_container("abc123".into()).await.unwrap();
    }

    #[tokio::test]
    async fn mock_stop_container_passes_timeout() {
        let mut mock = MockDockerBackend::new();
        mock.expect_stop_container()
            .withf(|id, timeout| id == "abc123" && *timeout == 10)
            .once()
            .returning(|_, _| async { Ok(()) }.boxed());

        mock.stop_container("abc123".into(), 10).await.unwrap();
    }

    #[tokio::test]
    async fn mock_delete_container_called_with_id() {
        let mut mock = MockDockerBackend::new();
        mock.expect_delete_container()
            .withf(|id| id == "abc123")
            .once()
            .returning(|_| async { Ok(()) }.boxed());

        mock.delete_container("abc123".into()).await.unwrap();
    }

    #[tokio::test]
    async fn mock_send_command_receives_command_with_newline() {
        let mut mock = MockDockerBackend::new();
        mock.expect_send_command()
            .withf(|id, cmd| id == "srv-1" && cmd == "say hello\n")
            .once()
            .returning(|_, _| async { Ok(()) }.boxed());

        mock.send_command("srv-1".into(), "say hello\n".into()).await.unwrap();
    }
}
