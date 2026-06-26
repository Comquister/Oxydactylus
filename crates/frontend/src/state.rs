use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use gloo_storage::{LocalStorage, Storage};

const STORAGE_KEY: &str = "oxy_auth";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthState {
    pub access_token: String,
    pub refresh_token: String,
    pub email: String,
    pub is_admin: bool,
}

#[derive(Clone, Copy)]
pub struct SessionContext {
    pub auth: RwSignal<Option<AuthState>>,
}

impl SessionContext {
    pub fn new() -> Self {
        let stored: Option<AuthState> = LocalStorage::get(STORAGE_KEY).ok();
        Self {
            auth: RwSignal::new(stored),
        }
    }

    pub fn set_auth(&self, state: AuthState) {
        let _ = LocalStorage::set(STORAGE_KEY, &state);
        self.auth.set(Some(state));
    }

    pub fn clear(&self) {
        LocalStorage::delete(STORAGE_KEY);
        self.auth.set(None);
    }

    pub fn token(&self) -> String {
        self.auth
            .get_untracked()
            .map(|a| a.access_token)
            .unwrap_or_default()
    }

    pub fn is_admin(&self) -> bool {
        self.auth
            .get_untracked()
            .map(|a| a.is_admin)
            .unwrap_or(false)
    }
}
