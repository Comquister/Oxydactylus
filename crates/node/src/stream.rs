use futures_util::{stream::BoxStream, StreamExt};
use tokio::sync::mpsc;
use tonic::Status;
use oxy_core::proto::node::LogLine;
use crate::docker::LogChunk;
use crate::error::Result as NodeResult;

pub async fn forward_logs(
    mut stream: BoxStream<'static, NodeResult<LogChunk>>,
    tx: mpsc::Sender<Result<LogLine, Status>>,
) {
    while let Some(chunk) = stream.next().await {
        let line = match chunk {
            Ok(c) => LogLine {
                content:   c.content,
                stream:    c.stream,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            },
            Err(e) => {
                let _ = tx.send(Err(Status::internal(e.to_string()))).await;
                return;
            }
        };
        if tx.send(Ok(line)).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NodeError;
    use futures_util::stream;

    #[tokio::test]
    async fn forward_logs_sends_all_chunks() {
        let chunks: Vec<NodeResult<LogChunk>> = vec![
            Ok(LogChunk { content: "line1\n".into(), stream: "stdout".into() }),
            Ok(LogChunk { content: "line2\n".into(), stream: "stderr".into() }),
        ];
        let s = Box::pin(stream::iter(chunks));
        let (tx, mut rx) = mpsc::channel(10);

        forward_logs(s, tx).await;

        let msg1 = rx.recv().await.unwrap().unwrap();
        assert_eq!(msg1.content, "line1\n");
        assert_eq!(msg1.stream,  "stdout");

        let msg2 = rx.recv().await.unwrap().unwrap();
        assert_eq!(msg2.content, "line2\n");
        assert_eq!(msg2.stream,  "stderr");

        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn forward_logs_stops_when_receiver_dropped() {
        // Stream com muitos itens; receiver dropado imediatamente
        let chunks: Vec<NodeResult<LogChunk>> = (0..100)
            .map(|i| Ok(LogChunk { content: format!("line{}\n", i), stream: "stdout".into() }))
            .collect();
        let s = Box::pin(stream::iter(chunks));
        let (tx, rx) = mpsc::channel(1);
        drop(rx); // receiver dropado: send vai falhar imediatamente

        // Deve retornar sem bloquear nem vazar
        forward_logs(s, tx).await;
    }

    #[tokio::test]
    async fn forward_logs_sends_error_on_stream_error() {
        let chunks: Vec<NodeResult<LogChunk>> = vec![
            Err(NodeError::Docker("boom".into())),
        ];
        let s = Box::pin(stream::iter(chunks));
        let (tx, mut rx) = mpsc::channel(10);

        forward_logs(s, tx).await;

        let result = rx.recv().await.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("boom"));
    }
}
