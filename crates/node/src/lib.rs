pub mod backups;
pub mod docker;
pub mod error;
pub mod files;
pub mod interceptor;
pub mod server;
pub mod sftp;
pub mod stream;

use std::sync::Arc;
use oxy_core::{NodeConfig, OxyError};
use oxy_core::proto::node::node_service_server::NodeServiceServer;
use crate::docker::BollardDocker;
use crate::interceptor::AuthInterceptor;
use crate::server::NodeServiceImpl;

pub async fn run(config: NodeConfig) -> oxy_core::Result<()> {
    let addr = config
        .grpc_listen
        .parse()
        .map_err(|e: std::net::AddrParseError| OxyError::Config(e.to_string()))?;

    let docker = BollardDocker::connect()
        .map_err(|e| OxyError::Config(e.to_string()))?;

    let interceptor = AuthInterceptor::new(&config.token);
    let service = NodeServiceImpl::new(Arc::new(docker));

    tracing::info!(listen = %config.grpc_listen, "node starting");

    let sftp_config = config.clone();
    tokio::spawn(async move {
        let sftp_addr = format!("0.0.0.0:{}", sftp_config.sftp_port)
            .parse::<std::net::SocketAddr>()
            .unwrap();
        let sftp_server = sftp::SftpServer::new(sftp_config.panel_addr);
        if let Err(e) = sftp_server.run(sftp_addr).await {
            tracing::error!(error = %e, "sftp server error");
        }
    });

    tonic::transport::Server::builder()
        .add_service(NodeServiceServer::with_interceptor(service, interceptor))
        .serve(addr)
        .await
        .map_err(|e| OxyError::Config(e.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use futures_util::future::FutureExt;
    use oxy_core::proto::node::{
        node_service_client::NodeServiceClient,
        node_service_server::NodeServiceServer,
        ServerStartRequest,
    };
    use tonic::transport::Server;
    use tokio_stream::wrappers::TcpListenerStream;
    use crate::docker::MockDockerBackend;
    use crate::server::NodeServiceImpl;

    #[tokio::test]
    async fn integration_grpc_start_server_round_trip() {
        let mut mock = MockDockerBackend::new();
        mock.expect_start_container()
            .once()
            .returning(|_| async { Ok(()) }.boxed());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let svc = NodeServiceImpl::new(Arc::new(mock));
        tokio::spawn(async move {
            Server::builder()
                .add_service(NodeServiceServer::new(svc))
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = NodeServiceClient::connect(format!("http://{}", addr))
            .await
            .unwrap();

        let reply = client
            .start_server(ServerStartRequest { server_id: "test-srv".into() })
            .await
            .unwrap()
            .into_inner();

        assert!(reply.success);
        assert!(reply.message.contains("test-srv"));
    }
}
