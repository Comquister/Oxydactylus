use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub status: String,
    pub image: String,
    pub memory_mb: i32,
    pub cpu_percent: i32,
    pub user_id: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateServerBody {
    pub user_id: String,
    pub node_id: String,
    pub name: String,
    pub image: String,
    pub memory_mb: i32,
    pub cpu_percent: i32,
}

#[derive(Deserialize)]
pub struct ServerStats {
    pub memory_bytes: u64,
    pub cpu_percent: f32,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: i64,
    pub mode: String,
}

#[derive(Serialize)]
pub struct WriteFileRequest {
    pub content: String,
}

#[derive(Serialize)]
pub struct CreateDirectoryRequest {
    pub path: String,
}

#[derive(Serialize)]
pub struct DeleteFilesRequest {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Serialize)]
pub struct RenameFileRequest {
    pub old_path: String,
    pub new_path: String,
}

// Startup variables
#[derive(Clone, Debug, Deserialize)]
pub struct StartupVariable {
    pub env_variable: String,
    pub name: String,
    pub description: Option<String>,
    pub value: String,
    pub default_val: Option<String>,
    pub user_editable: bool,
    pub user_viewable: bool,
    pub field_type: String,
    pub rules: Option<String>,
}

#[derive(Serialize)]
pub struct UpdateStartupRequest {
    pub variables: std::collections::HashMap<String, String>,
}

#[derive(Serialize)]
pub struct UpdateDockerImageRequest {
    pub image: String,
}

// Databases
#[derive(Clone, Debug, Deserialize)]
pub struct ServerDatabase {
    pub id: String,
    pub server_id: String,
    pub host_id: String,
    pub database_name: String,
    pub username: String,
    pub remote: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateServerDatabaseRequest {
    pub host_id: String,
    pub database_name: String,
    pub username: Option<String>,
    pub remote: Option<String>,
}

// Schedules
#[derive(Clone, Debug, Deserialize)]
pub struct ScheduleInfo {
    pub id: String,
    pub server_id: String,
    pub name: String,
    pub cron_minute: String,
    pub cron_hour: String,
    pub cron_day_of_month: String,
    pub cron_month: String,
    pub cron_day_of_week: String,
    pub is_active: bool,
    pub is_processing: bool,
    pub only_when_online: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScheduleTask {
    pub id: String,
    pub schedule_id: String,
    pub sequence_id: i32,
    pub action: String,
    pub payload: String,
    pub time_offset: i32,
    pub is_queued: bool,
    pub continue_on_failure: bool,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateScheduleRequest {
    pub name: String,
    pub cron_minute: Option<String>,
    pub cron_hour: Option<String>,
    pub cron_day_of_month: Option<String>,
    pub cron_month: Option<String>,
    pub cron_day_of_week: Option<String>,
    pub only_when_online: Option<bool>,
}

#[derive(Serialize)]
pub struct CreateScheduleTaskRequest {
    pub action: String,
    pub payload: String,
    pub time_offset: Option<i32>,
    pub continue_on_failure: Option<bool>,
}

// Backups
#[derive(Clone, Debug, Deserialize)]
pub struct Backup {
    pub id: String,
    pub server_id: String,
    pub uuid: String,
    pub name: String,
    pub ignored_files: Vec<String>,
    pub driver: String,
    pub sha256_hash: Option<String>,
    pub bytes: i64,
    pub is_successful: bool,
    pub is_locked: bool,
    pub completed_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateBackupRequest {
    pub name: String,
    pub ignored_files: Option<Vec<String>>,
}

// Server operations
#[derive(Serialize)]
pub struct UpdateServerRequest {
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct ChangeEggRequest {
    pub egg_id: String,
}

#[derive(Serialize)]
pub struct DatabaseHostInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i32,
}
