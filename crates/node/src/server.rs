use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use oxy_core::proto::node::{
    node_service_server::NodeService,
    LogLine, ServerCommandRequest, ServerDeleteRequest, ServerLogsRequest,
    ServerProvisionRequest, ServerReply, ServerStartRequest, ServerStats,
    ServerStatsRequest, ServerStopRequest,
};
use crate::docker::DockerBackend;
use crate::stream::forward_logs;

pub struct NodeServiceImpl<B: DockerBackend> {
    docker: Arc<B>,
}

impl<B: DockerBackend> NodeServiceImpl<B> {
    pub fn new(docker: Arc<B>) -> Self {
        Self { docker }
    }

    fn ok(message: impl Into<String>) -> Response<ServerReply> {
        Response::new(ServerReply { success: true, message: message.into() })
    }
}

#[async_trait]
impl<B: DockerBackend> NodeService for NodeServiceImpl<B> {
    type StreamLogsStream = ReceiverStream<Result<LogLine, Status>>;

    async fn start_server(
        &self,
        req: Request<ServerStartRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        let id = req.into_inner().server_id;
        self.docker
            .start_container(id.clone())
            .await
            .map_err(Status::from)?;
        Ok(Self::ok(format!("started {}", id)))
    }

    async fn stop_server(
        &self,
        req: Request<ServerStopRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        let r = req.into_inner();
        self.docker
            .stop_container(r.server_id.clone(), r.timeout)
            .await
            .map_err(Status::from)?;
        Ok(Self::ok(format!("stopped {}", r.server_id)))
    }

    async fn delete_server(
        &self,
        req: Request<ServerDeleteRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        let id = req.into_inner().server_id;
        self.docker
            .delete_container(id.clone())
            .await
            .map_err(Status::from)?;
        Ok(Self::ok(format!("deleted {}", id)))
    }

    async fn get_stats(
        &self,
        req: Request<ServerStatsRequest>,
    ) -> Result<Response<ServerStats>, Status> {
        let id = req.into_inner().server_id;
        let s = self.docker.get_stats(id.clone()).await.map_err(Status::from)?;
        Ok(Response::new(ServerStats {
            server_id:    id,
            memory_bytes: s.memory_bytes,
            cpu_percent:  s.cpu_percent,
            rx_bytes:     s.rx_bytes,
            tx_bytes:     s.tx_bytes,
        }))
    }

    async fn stream_logs(
        &self,
        req: Request<ServerLogsRequest>,
    ) -> Result<Response<Self::StreamLogsStream>, Status> {
        let r = req.into_inner();
        let (tx, rx) = mpsc::channel(32);
        let docker = Arc::clone(&self.docker);

        tokio::spawn(async move {
            match docker.log_stream(r.server_id, r.follow).await {
                Ok(stream) => forward_logs(stream, tx).await,
                Err(e) => {
                    let _ = tx.send(Err(Status::from(e))).await;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn send_command(
        &self,
        req: Request<ServerCommandRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        let r = req.into_inner();
        self.docker
            .send_command(r.server_id.clone(), r.content)
            .await
            .map_err(Status::from)?;
        Ok(Self::ok(format!("command sent to {}", r.server_id)))
    }

    async fn provision_server(
        &self,
        req: Request<ServerProvisionRequest>,
    ) -> Result<Response<ServerReply>, Status> {
        let r = req.into_inner();
        use crate::docker::ContainerSpec;
        self.docker
            .create_container(ContainerSpec {
                name:        r.server_id.clone(),
                image:       r.image,
                memory_mb:   r.memory_mb as i64,
                cpu_percent: r.cpu_percent as i64,
                env:         r.env,
                ports:       r.ports,
            })
            .await
            .map_err(Status::from)?;
        Ok(Self::ok(format!("provisioned {}", r.server_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::{ContainerStats, LogChunk, MockDockerBackend};
    use crate::error::NodeError;
    use futures_util::{future::FutureExt, stream};

    fn svc(mock: MockDockerBackend) -> NodeServiceImpl<MockDockerBackend> {
        NodeServiceImpl::new(Arc::new(mock))
    }

    #[tokio::test]
    async fn start_server_delegates_to_docker() {
        let mut mock = MockDockerBackend::new();
        mock.expect_start_container()
            .withf(|id| id == "srv-1")
            .once()
            .returning(|_| async { Ok(()) }.boxed());

        let reply = svc(mock)
            .start_server(Request::new(ServerStartRequest { server_id: "srv-1".into() }))
            .await
            .unwrap()
            .into_inner();

        assert!(reply.success);
    }

    #[tokio::test]
    async fn stop_server_passes_timeout() {
        let mut mock = MockDockerBackend::new();
        mock.expect_stop_container()
            .withf(|id, t| id == "srv-1" && *t == 30)
            .once()
            .returning(|_, _| async { Ok(()) }.boxed());

        svc(mock)
            .stop_server(Request::new(ServerStopRequest {
                server_id: "srv-1".into(),
                timeout:   30,
            }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_server_delegates_to_docker() {
        let mut mock = MockDockerBackend::new();
        mock.expect_delete_container()
            .withf(|id| id == "srv-1")
            .once()
            .returning(|_| async { Ok(()) }.boxed());

        svc(mock)
            .delete_server(Request::new(ServerDeleteRequest { server_id: "srv-1".into() }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn get_stats_maps_to_proto() {
        let mut mock = MockDockerBackend::new();
        mock.expect_get_stats()
            .once()
            .returning(|_| async {
                Ok(ContainerStats {
                    memory_bytes: 1024,
                    cpu_percent:  5.0,
                    rx_bytes:     100,
                    tx_bytes:     200,
                })
            }.boxed());

        let stats = svc(mock)
            .get_stats(Request::new(ServerStatsRequest { server_id: "srv-1".into() }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(stats.memory_bytes, 1024);
        assert!((stats.cpu_percent - 5.0).abs() < 0.001);
        assert_eq!(stats.rx_bytes,  100);
        assert_eq!(stats.tx_bytes,  200);
    }

    #[tokio::test]
    async fn send_command_delegates_to_docker() {
        let mut mock = MockDockerBackend::new();
        mock.expect_send_command()
            .withf(|id, cmd| id == "srv-1" && cmd == "say hello")
            .once()
            .returning(|_, _| async { Ok(()) }.boxed());

        svc(mock)
            .send_command(Request::new(ServerCommandRequest {
                server_id: "srv-1".into(),
                content:   "say hello".into(),
            }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn stream_logs_forwards_chunks_to_client() {
        use tokio_stream::StreamExt;

        let mut mock = MockDockerBackend::new();
        mock.expect_log_stream()
            .once()
            .returning(|_, _| {
                let chunks = vec![
                    Ok(LogChunk { content: "hello\n".into(), stream: "stdout".into() }),
                    Ok(LogChunk { content: "world\n".into(), stream: "stdout".into() }),
                ];
                async move { Ok(Box::pin(stream::iter(chunks)) as _) }.boxed()
            });

        let mut response = svc(mock)
            .stream_logs(Request::new(ServerLogsRequest {
                server_id: "srv-1".into(),
                follow:    false,
            }))
            .await
            .unwrap()
            .into_inner();

        let line1 = response.next().await.unwrap().unwrap();
        assert_eq!(line1.content, "hello\n");

        let line2 = response.next().await.unwrap().unwrap();
        assert_eq!(line2.content, "world\n");
    }

    #[tokio::test]
    async fn start_server_returns_grpc_error_on_not_found() {
        let mut mock = MockDockerBackend::new();
        mock.expect_start_container()
            .once()
            .returning(|_| async { Err(NodeError::NotFound("srv-x".into())) }.boxed());

        let err = svc(mock)
            .start_server(Request::new(ServerStartRequest { server_id: "srv-x".into() }))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn provision_server_creates_container() {
        let mut mock = MockDockerBackend::new();
        mock.expect_create_container()
            .once()
            .returning(|spec| {
                async move {
                    assert_eq!(spec.name, "srv-new");
                    assert_eq!(spec.image, "itzg/minecraft-server");
                    assert_eq!(spec.memory_mb, 1024);
                    assert_eq!(spec.cpu_percent, 100);
                    assert_eq!(spec.env, vec!["EULA=TRUE"]);
                    assert_eq!(spec.ports, vec!["0.0.0.0:25565:25565/tcp"]);
                    Ok("container-id-xyz".to_string())
                }.boxed()
            });

        let reply = svc(mock)
            .provision_server(Request::new(
                oxy_core::proto::node::ServerProvisionRequest {
                    server_id:   "srv-new".into(),
                    image:       "itzg/minecraft-server".into(),
                    memory_mb:   1024,
                    cpu_percent: 100,
                    env:         vec!["EULA=TRUE".into()],
                    ports:       vec!["0.0.0.0:25565:25565/tcp".into()],
                },
            ))
            .await
            .unwrap()
            .into_inner();

        assert!(reply.success);
    }
}
