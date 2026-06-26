use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Serialize};

const API_BASE: &str = "/api";

pub struct ApiClient {
    token: String,
}

impl ApiClient {
    pub fn new(token: String) -> Self {
        Self { token }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        Request::get(&format!("{}{}", API_BASE, path))
            .header("Authorization", &format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<T>()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn post<I: Serialize, O: DeserializeOwned>(
        &self,
        path: &str,
        body: &I,
    ) -> Result<O, String> {
        Request::post(&format!("{}{}", API_BASE, path))
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(body).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<O>()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn patch<I: Serialize, O: DeserializeOwned>(
        &self,
        path: &str,
        body: &I,
    ) -> Result<O, String> {
        Request::patch(&format!("{}{}", API_BASE, path))
            .header("Authorization", &format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(body).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<O>()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn delete(&self, path: &str) -> Result<(), String> {
        let resp = Request::delete(&format!("{}{}", API_BASE, path))
            .header("Authorization", &format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if resp.ok() {
            Ok(())
        } else {
            Err(format!("HTTP {}", resp.status()))
        }
    }
}
