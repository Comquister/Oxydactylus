use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum PanelError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden")]
    Forbidden,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("database error: {0}")]
    Db(String),
    #[error("node error: {0}")]
    Node(String),
    #[error("internal: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, PanelError>;

impl From<sqlx::Error> for PanelError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => PanelError::NotFound("record not found".to_string()),
            sqlx::Error::Database(db) if db.constraint().is_some() => {
                PanelError::Conflict(db.constraint().unwrap_or("").to_string())
            }
            _ => PanelError::Db(e.to_string()),
        }
    }
}

impl From<tonic::Status> for PanelError {
    fn from(s: tonic::Status) -> Self {
        PanelError::Node(s.message().to_string())
    }
}

impl IntoResponse for PanelError {
    fn into_response(self) -> Response {
        let status = match &self {
            PanelError::NotFound(_)     => StatusCode::NOT_FOUND,
            PanelError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            PanelError::Forbidden       => StatusCode::FORBIDDEN,
            PanelError::Conflict(_)     => StatusCode::CONFLICT,
            PanelError::Validation(_)   => StatusCode::UNPROCESSABLE_ENTITY,
            PanelError::Db(_)
            | PanelError::Node(_)
            | PanelError::Internal(_)   => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn status_of(err: PanelError) -> StatusCode {
        err.into_response().status()
    }

    #[test]
    fn not_found_maps_to_404() {
        assert_eq!(status_of(PanelError::NotFound("x".into())), StatusCode::NOT_FOUND);
    }

    #[test]
    fn unauthorized_maps_to_401() {
        assert_eq!(status_of(PanelError::Unauthorized("x".into())), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn forbidden_maps_to_403() {
        assert_eq!(status_of(PanelError::Forbidden), StatusCode::FORBIDDEN);
    }

    #[test]
    fn conflict_maps_to_409() {
        assert_eq!(status_of(PanelError::Conflict("x".into())), StatusCode::CONFLICT);
    }

    #[test]
    fn validation_maps_to_422() {
        assert_eq!(status_of(PanelError::Validation("x".into())), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn db_maps_to_500() {
        assert_eq!(status_of(PanelError::Db("x".into())), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
