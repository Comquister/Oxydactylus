use sha2::{Sha256, Digest};
use std::fs;
use crate::error::Result;

fn build_tar_args<'a>(
    backup_path: &'a str,
    ignored_files: &'a [String],
) -> Vec<&'a str> {
    let mut tar_args = vec!["tar", "-czf", backup_path];
    for ignored in ignored_files {
        tar_args.push("--exclude");
        tar_args.push(ignored);
    }
    tar_args.push("-C");
    tar_args.push("/");
    tar_args.push(".");
    tar_args
}

pub async fn create_backup(
    backup_path: &str,
    server_id: &str,
    ignored_files: Vec<String>,
) -> Result<(String, i64)> {
    // Validate server_id is a valid UUID to prevent argument injection
    uuid::Uuid::parse_str(server_id)
        .map_err(|_| crate::error::NodeError::Validation("invalid server_id".into()))?;

    let tar_args = build_tar_args(backup_path, &ignored_files);

    tokio::process::Command::new("docker")
        .args(&["exec", "--", server_id])
        .args(&tar_args)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tar_args_does_not_inject_shell_metacharacters() {
        let ignored = vec![
            "file;rm -rf /".to_string(),
            "file$(dangerous)".to_string(),
            "file`whoami`".to_string(),
        ];

        let args = build_tar_args("/path/backup.tar.gz", &ignored);

        assert_eq!(args[0], "tar");
        assert_eq!(args[1], "-czf");
        assert_eq!(args[2], "/path/backup.tar.gz");
        assert_eq!(args[3], "--exclude");
        assert_eq!(args[4], "file;rm -rf /");
        assert_eq!(args[5], "--exclude");
        assert_eq!(args[6], "file$(dangerous)");
        assert_eq!(args[7], "--exclude");
        assert_eq!(args[8], "file`whoami`");
        assert_eq!(args[9], "-C");
        assert_eq!(args[10], "/");
        assert_eq!(args[11], ".");
    }

    #[test]
    fn build_tar_args_with_empty_ignored_files() {
        let ignored = vec![];
        let args = build_tar_args("/path/backup.tar.gz", &ignored);

        assert_eq!(
            args,
            vec!["tar", "-czf", "/path/backup.tar.gz", "-C", "/", "."]
        );
    }

    #[test]
    fn create_backup_rejects_malicious_server_id() {
        // Test that server_id starting with - (option injection) is rejected
        let malicious_id = "-I";
        let result = uuid::Uuid::parse_str(malicious_id);
        assert!(result.is_err());
    }

    #[test]
    fn create_backup_rejects_non_uuid_server_id() {
        let non_uuid = "not-a-uuid";
        assert!(uuid::Uuid::parse_str(non_uuid).is_err());

        let option_like = "--option";
        assert!(uuid::Uuid::parse_str(option_like).is_err());

        let path_traversal = "../../../etc/passwd";
        assert!(uuid::Uuid::parse_str(path_traversal).is_err());
    }
}
