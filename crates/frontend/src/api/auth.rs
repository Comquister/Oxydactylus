use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

const AUTH_BASE: &str = "/auth";

#[derive(Serialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub email: String,
    pub is_admin: bool,
}

pub async fn login(email: &str, password: &str) -> Result<LoginResponse, String> {
    Request::post(&format!("{}/login", AUTH_BASE))
        .header("Content-Type", "application/json")
        .body(
            serde_json::to_string(&LoginRequest {
                email: email.to_string(),
                password: password.to_string(),
            })
            .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<LoginResponse>()
        .await
        .map_err(|e| e.to_string())
}
