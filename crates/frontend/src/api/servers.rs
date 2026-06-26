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
