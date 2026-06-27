use crate::error::{PanelError, Result};
use sqlx::{any::AnyPoolOptions, AnyPool};
use std::borrow::Cow;
use std::sync::OnceLock;
use regex::Regex;
use aes_gcm::{Aes256Gcm, Key, Nonce, KeyInit};
use aes_gcm::aead::{Aead, Payload};
use rand::Rng;

static PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();

pub fn port_sql<'a>(sql: &'a str, backend: &str) -> Cow<'a, str> {
    if backend == "MySQL" {
        let re = PLACEHOLDER_RE.get_or_init(|| Regex::new(r"\$\d+").unwrap());
        Cow::Owned(re.replace_all(sql, "?").into_owned())
    } else {
        Cow::Borrowed(sql)
    }
}

pub async fn create_pool(database_url: &str) -> Result<AnyPool> {
    AnyPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .map_err(|e| PanelError::Internal(e.to_string()))
}

pub async fn run_migrations(pool: &AnyPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| PanelError::Internal(e.to_string()))
}

pub fn encrypt_password(password: &str, app_key: &str) -> Result<String> {
    let key_bytes = app_key.as_bytes();
    if key_bytes.len() != 32 {
        return Err(PanelError::Internal("app_key must be 32 bytes".to_string()));
    }

    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);

    let mut rng = rand::thread_rng();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, Payload::from(password.as_bytes()))
        .map_err(|_| PanelError::Internal("encryption failed".to_string()))?;

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&ciphertext);
    Ok(format!("enc:{}", hex::encode(result)))
}

pub fn decrypt_password(encrypted: &str, app_key: &str) -> Result<String> {
    if !encrypted.starts_with("enc:") {
        return Err(PanelError::Internal("invalid encrypted format".to_string()));
    }

    let key_bytes = app_key.as_bytes();
    if key_bytes.len() != 32 {
        return Err(PanelError::Internal("app_key must be 32 bytes".to_string()));
    }

    let hex_data = hex::decode(&encrypted[4..])
        .map_err(|_| PanelError::Internal("invalid hex encoding".to_string()))?;

    if hex_data.len() < 12 {
        return Err(PanelError::Internal("invalid encrypted data length".to_string()));
    }

    let (nonce_bytes, ciphertext) = hex_data.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);

    let plaintext = cipher
        .decrypt(nonce, Payload::from(ciphertext))
        .map_err(|_| PanelError::Internal("decryption failed".to_string()))?;

    String::from_utf8(plaintext)
        .map_err(|_| PanelError::Internal("invalid utf8 in decrypted data".to_string()))
}
