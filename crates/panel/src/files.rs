use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::{PanelError, Result},
    permissions::{
        FILE_CREATE, FILE_DELETE, FILE_READ, FILE_READ_CONTENT, FILE_UPDATE,
    },
    servers::{get_node_client, load_server_with_access},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: i64,
    pub mode: String,
}

impl From<oxy_core::proto::node::FileInfo> for FileInfo {
    fn from(proto: oxy_core::proto::node::FileInfo) -> Self {
        Self {
            name: proto.name,
            path: proto.path,
            is_dir: proto.is_dir,
            size_bytes: proto.size_bytes,
            mode: proto.mode,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListFilesQuery {
    directory: Option<String>,
}

async fn list_files(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Query(q): Query<ListFilesQuery>,
) -> Result<Json<Vec<FileInfo>>> {
    let server = load_server_with_access(&state, &user, server_id, Some(FILE_READ)).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    let path = q.directory.unwrap_or_else(|| "/".to_string());
    let files = client.list_files(&server.id.to_string(), &path).await?;
    Ok(Json(files.into_iter().map(FileInfo::from).collect()))
}

#[derive(Debug, Deserialize)]
struct FileContentsQuery {
    file: Option<String>,
}

async fn get_file_contents(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Query(q): Query<FileContentsQuery>,
) -> Result<impl IntoResponse> {
    let file_path = q.file.ok_or_else(|| {
        PanelError::Validation("file parameter required".to_string())
    })?;
    let server = load_server_with_access(&state, &user, server_id, Some(FILE_READ_CONTENT)).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    let contents = client.get_file_contents(&server.id.to_string(), &file_path).await?;
    Ok((StatusCode::OK, contents))
}

#[derive(Debug, Deserialize)]
struct WriteFileRequest {
    content: String,
}

async fn write_file_contents(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Query(q): Query<FileContentsQuery>,
    Json(body): Json<WriteFileRequest>,
) -> Result<StatusCode> {
    let file_path = q.file.ok_or_else(|| {
        PanelError::Validation("file parameter required".to_string())
    })?;
    let server = load_server_with_access(&state, &user, server_id, Some(FILE_UPDATE)).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    client
        .write_file_contents(&server.id.to_string(), &file_path, body.content.into_bytes())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct CreateDirectoryRequest {
    path: String,
}

async fn create_directory(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(body): Json<CreateDirectoryRequest>,
) -> Result<StatusCode> {
    let server = load_server_with_access(&state, &user, server_id, Some(FILE_CREATE)).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    client
        .create_directory(&server.id.to_string(), &body.path)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct DeleteFilesRequest {
    path: String,
    #[serde(default)]
    recursive: bool,
}

async fn delete_files(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(body): Json<DeleteFilesRequest>,
) -> Result<StatusCode> {
    let server = load_server_with_access(&state, &user, server_id, Some(FILE_DELETE)).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    client
        .delete_files(&server.id.to_string(), &body.path, body.recursive)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct RenameFileRequest {
    old_path: String,
    new_path: String,
}

async fn rename_file(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(body): Json<RenameFileRequest>,
) -> Result<StatusCode> {
    let server = load_server_with_access(&state, &user, server_id, Some(FILE_UPDATE)).await?;
    let mut client = get_node_client(&state, server.node_id).await?;
    client
        .rename_file(&server.id.to_string(), &body.old_path, &body.new_path)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn files_router() -> Router<AppState> {
    Router::new()
        .route("/:id/files", get(list_files))
        .route("/:id/files/contents", get(get_file_contents).post(write_file_contents))
        .route("/:id/files/create-directory", post(create_directory))
        .route("/:id/files/delete", post(delete_files))
        .route("/:id/files/rename", put(rename_file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn file_info_conversion_works() {
        let proto_file = oxy_core::proto::node::FileInfo {
            name: "test.txt".to_string(),
            path: "/tmp/test.txt".to_string(),
            is_dir: false,
            size_bytes: 42,
            mode: "0644".to_string(),
        };
        let file_info = FileInfo::from(proto_file);
        assert_eq!(file_info.name, "test.txt");
        assert_eq!(file_info.size_bytes, 42);
        assert!(!file_info.is_dir);
    }
}
