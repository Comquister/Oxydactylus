use async_trait::async_trait;
use futures_util::stream::BoxStream;
use crate::error::{NodeError, Result};

pub struct ContainerSpec {
    pub image:       String,
    pub name:        String,
    pub env:         Vec<String>,
    pub memory_mb:   i64,
    pub cpu_percent: i64,
    pub ports:       Vec<String>,
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
        use bollard::models::{HostConfig, PortBinding};
        use std::collections::HashMap;

        if spec.memory_mb <= 0 || spec.cpu_percent <= 0 {
            return Err(NodeError::Validation(
                "memory_mb and cpu_percent must be positive".into(),
            ));
        }

        let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
        for entry in &spec.ports {
            let parts: Vec<&str> = entry.splitn(3, ':').collect();
            let (host_ip, host_port, container_part) = match parts.as_slice() {
                [ip, hp, cp] => (ip.to_string(), hp.to_string(), cp.to_string()),
                [hp, cp]     => ("0.0.0.0".to_string(), hp.to_string(), cp.to_string()),
                _            => continue,
            };
            let key = if container_part.contains('/') {
                container_part
            } else {
                format!("{}/tcp", container_part)
            };
            port_bindings.insert(key, Some(vec![PortBinding {
                host_ip:   Some(host_ip),
                host_port: Some(host_port),
            }]));
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
                memory:         Some(spec.memory_mb * 1024 * 1024),
                nano_cpus:      Some(spec.cpu_percent * 10_000_000),
                port_bindings:  Some(port_bindings),
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
            .map_err(|e| NodeError::Docker(e.to_string()))?;
        attach.input
            .flush()
            .await
            .map_err(|e| NodeError::Docker(e.to_string()))
    }

    async fn get_stats(&self, id: String) -> Result<ContainerStats> {
        use bollard::container::StatsOptions;
        use futures_util::StreamExt;

        let mut stream = self.inner.stats(&id, Some(StatsOptions {
            stream:   false,
            one_shot: true,
        }));

        let stats = stream
            .next()
            .await
            .ok_or_else(|| NodeError::Docker("no stats returned".into()))??;

        let cpu_delta = stats.cpu_stats.cpu_usage.total_usage
            .saturating_sub(stats.precpu_stats.cpu_usage.total_usage);
        let system_delta = stats.cpu_stats.system_cpu_usage.unwrap_or(0)
            .saturating_sub(stats.precpu_stats.system_cpu_usage.unwrap_or(0));
        let num_cpus = stats.cpu_stats.online_cpus.unwrap_or(1) as f64;
        let cpu_percent = if system_delta > 0 {
            (cpu_delta as f64 / system_delta as f64) * num_cpus * 100.0
        } else {
            0.0
        };

        let memory_bytes = stats.memory_stats.usage.unwrap_or(0);

        let (rx_bytes, tx_bytes) = stats
            .networks
            .as_ref()
            .map(|nets| {
                nets.values().fold((0u64, 0u64), |(rx, tx), net| {
                    (rx + net.rx_bytes as u64, tx + net.tx_bytes as u64)
                })
            })
            .unwrap_or((0, 0));

        Ok(ContainerStats { memory_bytes, cpu_percent, rx_bytes, tx_bytes })
    }

    async fn log_stream(&self, id: String, follow: bool)
        -> Result<BoxStream<'static, Result<LogChunk>>>
    {
        use bollard::container::{LogOutput, LogsOptions};
        use futures_util::StreamExt;

        let opts = LogsOptions::<String> {
            stdout: true,
            stderr: true,
            follow,
            ..Default::default()
        };

        let stream = self.inner
            .logs(&id, Some(opts))
            .map(|result| {
                result.map_err(NodeError::from).map(|output| {
                    let stream_name = match &output {
                        LogOutput::StdOut { .. } => "stdout",
                        LogOutput::StdErr { .. } => "stderr",
                        _                        => "stdout",
                    }
                    .to_string();
                    LogChunk {
                        content: output.to_string(),
                        stream:  stream_name,
                    }
                })
            });

        Ok(Box::pin(stream))
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
            ports:       vec![],
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
            .withf(|id, cmd| id == "srv-1" && cmd == "say hello")
            .once()
            .returning(|_, _| async { Ok(()) }.boxed());

        mock.send_command("srv-1".into(), "say hello".into()).await.unwrap();
    }

    #[tokio::test]
    async fn mock_get_stats_returns_container_stats() {
        let mut mock = MockDockerBackend::new();
        mock.expect_get_stats()
            .withf(|id| id == "srv-1")
            .once()
            .returning(|_| {
                async {
                    Ok(ContainerStats {
                        memory_bytes: 256 * 1024 * 1024,
                        cpu_percent:  12.5,
                        rx_bytes:     1024,
                        tx_bytes:     2048,
                    })
                }.boxed()
            });

        let stats = mock.get_stats("srv-1".into()).await.unwrap();
        assert_eq!(stats.memory_bytes, 256 * 1024 * 1024);
        assert!((stats.cpu_percent - 12.5).abs() < 0.001);
        assert_eq!(stats.rx_bytes, 1024);
        assert_eq!(stats.tx_bytes, 2048);
    }
}
