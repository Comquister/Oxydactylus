use thiserror::Error;

#[derive(Debug, Error)]
pub enum OxyError {
    #[error("config error: {0}")]
    Config(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("grpc error: {0}")]
    Grpc(#[from] tonic::Status),
}

pub type Result<T> = std::result::Result<T, OxyError>;
