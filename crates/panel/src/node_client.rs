use oxy_core::proto::node::{
    node_service_client::NodeServiceClient,
    ServerCommandRequest, ServerDeleteRequest, ServerProvisionRequest,
    ServerStartRequest, ServerStats, ServerStatsRequest, ServerStopRequest,
};
use tonic::{
    metadata::MetadataValue,
    service::interceptor::InterceptedService,
    transport::Channel,
    Request,
};

use crate::error::{PanelError, Result};

struct BearerInterceptor {
    token: String,
}

impl tonic::service::Interceptor for BearerInterceptor {
    fn call(
        &mut self,
        mut req: Request<()>,
    ) -> std::result::Result<Request<()>, tonic::Status> {
        let val = MetadataValue::try_from(format!("Bearer {}", self.token))
            .map_err(|_| tonic::Status::internal("invalid token format"))?;
        req.metadata_mut().insert("authorization", val);
        Ok(req)
    }
}

pub struct NodeClient {
    inner: NodeServiceClient<InterceptedService<Channel, BearerInterceptor>>,
}

impl NodeClient {
    pub async fn connect(grpc_addr: &str, token: &str) -> Result<Self> {
        let channel = Channel::from_shared(grpc_addr.to_string())
            .map_err(|e| PanelError::Node(e.to_string()))?
            .connect()
            .await
            .map_err(|e| PanelError::Node(e.to_string()))?;
        let interceptor = BearerInterceptor { token: token.to_string() };
        Ok(Self { inner: NodeServiceClient::with_interceptor(channel, interceptor) })
    }

    pub async fn new(node: &crate::nodes::Node) -> Result<Self> {
        Self::connect(&node.grpc_addr, &node.token).await
    }

    pub async fn provision(
        &mut self,
        server_id:   &str,
        image:       &str,
        memory_mb:   u32,
        cpu_percent: u32,
        env:         Vec<String>,
    ) -> Result<()> {
        self.inner
            .provision_server(ServerProvisionRequest {
                server_id:   server_id.to_string(),
                image:       image.to_string(),
                memory_mb,
                cpu_percent,
                env,
            })
            .await
            .map(|_| ())
            .map_err(PanelError::from)
    }

    pub async fn start(&mut self, server_id: &str) -> Result<()> {
        self.inner
            .start_server(ServerStartRequest { server_id: server_id.to_string() })
            .await
            .map(|_| ())
            .map_err(PanelError::from)
    }

    pub async fn stop(&mut self, server_id: &str, timeout: u32) -> Result<()> {
        self.inner
            .stop_server(ServerStopRequest {
                server_id: server_id.to_string(),
                timeout,
            })
            .await
            .map(|_| ())
            .map_err(PanelError::from)
    }

    pub async fn delete(&mut self, server_id: &str) -> Result<()> {
        self.inner
            .delete_server(ServerDeleteRequest { server_id: server_id.to_string() })
            .await
            .map(|_| ())
            .map_err(PanelError::from)
    }

    pub async fn send_command(&mut self, server_id: &str, content: &str) -> Result<()> {
        self.inner
            .send_command(ServerCommandRequest {
                server_id: server_id.to_string(),
                content:   content.to_string(),
            })
            .await
            .map(|_| ())
            .map_err(PanelError::from)
    }

    pub async fn get_stats(&mut self, server_id: &str) -> Result<ServerStats> {
        self.inner
            .get_stats(ServerStatsRequest { server_id: server_id.to_string() })
            .await
            .map(|r| r.into_inner())
            .map_err(PanelError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxy_core::proto::node::{
        node_service_server::{NodeService, NodeServiceServer},
        LogLine, ServerCommandRequest, ServerDeleteRequest, ServerLogsRequest,
        ServerProvisionRequest, ServerReply, ServerStartRequest, ServerStopRequest,
        ServerStats, ServerStatsRequest,
    };
    use tonic::{async_trait, Request, Response, Status};
    use tokio_stream::wrappers::{ReceiverStream, TcpListenerStream};

    struct EchoNode;

    #[async_trait]
    impl NodeService for EchoNode {
        type StreamLogsStream = ReceiverStream<std::result::Result<LogLine, Status>>;

        async fn provision_server(&self, _: Request<ServerProvisionRequest>)
            -> std::result::Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "ok".into() })) }

        async fn start_server(&self, _: Request<ServerStartRequest>)
            -> std::result::Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "started".into() })) }

        async fn stop_server(&self, _: Request<ServerStopRequest>)
            -> std::result::Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "stopped".into() })) }

        async fn delete_server(&self, _: Request<ServerDeleteRequest>)
            -> std::result::Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "deleted".into() })) }

        async fn send_command(&self, _: Request<ServerCommandRequest>)
            -> std::result::Result<Response<ServerReply>, Status>
        { Ok(Response::new(ServerReply { success: true, message: "sent".into() })) }

        async fn get_stats(&self, req: Request<ServerStatsRequest>)
            -> std::result::Result<Response<ServerStats>, Status>
        {
            let id = req.into_inner().server_id;
            Ok(Response::new(ServerStats {
                server_id: id, memory_bytes: 512, cpu_percent: 5.0,
                rx_bytes: 100, tx_bytes: 200,
            }))
        }

        async fn stream_logs(&self, _: Request<ServerLogsRequest>)
            -> std::result::Result<Response<Self::StreamLogsStream>, Status>
        {
            let (_, rx) = tokio::sync::mpsc::channel(1);
            Ok(Response::new(ReceiverStream::new(rx)))
        }
    }

    async fn start_test_server(token: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let token_clone = token.to_string();

        tokio::spawn(async move {
            use oxy_node::interceptor::AuthInterceptor;
            let interceptor = AuthInterceptor::new(&token_clone);
            tonic::transport::Server::builder()
                .add_service(NodeServiceServer::with_interceptor(EchoNode, interceptor))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        format!("http://127.0.0.1:{}", addr.port())
    }

    #[tokio::test]
    async fn client_can_provision_and_start() {
        let token = "test-node-token";
        let addr = start_test_server(token).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = NodeClient::connect(&addr, token).await.unwrap();
        client.provision("srv-1", "ubuntu:latest", 512, 50, vec!["X=1".into()]).await.unwrap();
        client.start("srv-1").await.unwrap();
    }

    #[tokio::test]
    async fn client_gets_stats() {
        let token = "test-token-2";
        let addr = start_test_server(token).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = NodeClient::connect(&addr, token).await.unwrap();
        let stats = client.get_stats("srv-x").await.unwrap();
        assert_eq!(stats.server_id, "srv-x");
        assert_eq!(stats.memory_bytes, 512);
    }

    #[tokio::test]
    async fn wrong_token_returns_node_error() {
        let addr = start_test_server("correct-token").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = NodeClient::connect(&addr, "wrong-token").await.unwrap();
        let err = client.start("srv-1").await.unwrap_err();
        assert!(matches!(err, PanelError::Node(_)));
    }
}
