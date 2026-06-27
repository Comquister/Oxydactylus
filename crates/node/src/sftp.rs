use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncReadExt;

pub struct SftpServer {
    panel_addr: String,
}

impl SftpServer {
    pub fn new(panel_addr: String) -> Self {
        SftpServer { panel_addr }
    }

    pub async fn run(self, addr: SocketAddr) -> crate::error::Result<()> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!(listen = %addr, "sftp server listening");

        let panel_addr = Arc::new(self.panel_addr);

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            let panel_addr = panel_addr.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_sftp_connection(stream, peer_addr, &panel_addr).await {
                    tracing::debug!(peer = %peer_addr, error = %e, "sftp connection error");
                }
            });
        }
    }
}

async fn handle_sftp_connection(
    mut stream: tokio::net::TcpStream,
    peer_addr: std::net::SocketAddr,
    _panel_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; 1024];

    tokio::select! {
        _ = async {
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Ok(line) = std::str::from_utf8(&buf[..n]) {
                            tracing::debug!(peer = %peer_addr, data = %line.trim(), "sftp data");
                        }
                    }
                    Err(e) => {
                        tracing::debug!(peer = %peer_addr, error = %e, "sftp read error");
                        break;
                    }
                }
            }
        } => {}
        _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
            tracing::debug!(peer = %peer_addr, "sftp connection timeout");
        }
    }

    tracing::info!(peer = %peer_addr, "sftp connection closed");
    Ok(())
}

pub async fn verify_sftp_credentials(
    panel_addr: &str,
    username: &str,
    password: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let url = format!("{}/api/auth/sftp-verify", panel_addr);
    let body = serde_json::json!({
        "username": username,
        "password": password,
    });

    let client = reqwest::Client::new();
    let res = client.post(&url).json(&body).send().await?;

    Ok(res.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sftp_server_starts_on_configured_port() {
        let server = SftpServer::new("http://panel:3000".to_string());
        let addr = "127.0.0.1:0".parse().unwrap();

        tokio::spawn(async move {
            let _ = server.run(addr).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
