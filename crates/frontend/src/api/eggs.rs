use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct Egg {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateEggBody {
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub start_cmd: String,
    pub stop_cmd: String,
    pub startup_done: String,
    pub docker_images: serde_json::Value,
}
