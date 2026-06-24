use tonic::Status;

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("docker error: {0}")]
    Docker(String),
    #[error("container not found: {0}")]
    NotFound(String),
    #[error("validation error: {0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, NodeError>;

impl From<bollard::errors::Error> for NodeError {
    fn from(e: bollard::errors::Error) -> Self {
        use bollard::errors::Error as BE;
        match &e {
            BE::DockerResponseServerError { status_code: 404, .. } => {
                NodeError::NotFound(e.to_string())
            }
            _ => NodeError::Docker(e.to_string()),
        }
    }
}

impl From<NodeError> for Status {
    fn from(e: NodeError) -> Self {
        match e {
            NodeError::Docker(msg)     => Status::internal(msg),
            NodeError::NotFound(msg)   => Status::not_found(msg),
            NodeError::Validation(msg) => Status::invalid_argument(msg),
        }
    }
}
