use sha2::{Sha256, Digest};
use std::fs;
use crate::error::Result;

pub async fn create_backup(
    backup_path: &str,
    server_id: &str,
    ignored_files: Vec<String>,
) -> Result<(String, i64)> {
    let exclude_args = build_exclude_args(&ignored_files);
    let cmd = format!("cd / && tar -czf {} {} .", backup_path, exclude_args);

    tokio::process::Command::new("docker")
        .args(&["exec", server_id, "sh", "-c", &cmd])
        .output()
        .await?;

    let mut file = fs::File::open(backup_path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    let hash = format!("{:x}", hasher.finalize());

    let metadata = fs::metadata(backup_path)?;
    let bytes = metadata.len() as i64;

    Ok((hash, bytes))
}

pub async fn delete_backup(backup_path: &str) -> Result<()> {
    fs::remove_file(backup_path)?;
    Ok(())
}

fn build_exclude_args(ignored_files: &[String]) -> String {
    ignored_files
        .iter()
        .map(|f| format!("--exclude='{}'", f))
        .collect::<Vec<_>>()
        .join(" ")
}
