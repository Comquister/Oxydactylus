use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub is_admin: bool,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreateUserBody {
    pub email: String,
    pub password: String,
    pub is_admin: bool,
}
