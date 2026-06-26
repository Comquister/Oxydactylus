use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub grpc_addr: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateNodeBody {
    pub name: String,
    pub grpc_addr: String,
}
