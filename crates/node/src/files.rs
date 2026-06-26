use crate::docker::DockerBackend;
use crate::error::Result;
use std::sync::Arc;

pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: i64,
    pub mode: String,
}

pub async fn list_files(
    docker: Arc<impl DockerBackend>,
    server_id: String,
    path: String,
) -> Result<Vec<FileEntry>> {
    let cmd = format!("ls -la --time-style=long-iso {}", shell_escape(&path));
    let output = docker.send_command_with_output(server_id, cmd).await?;

    let mut entries = Vec::new();
    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }

        let name = if parts.len() > 8 {
            parts[8..].join(" ")
        } else {
            parts[7].to_string()
        };
        let is_dir = line.starts_with('d');
        let size_bytes = parts[4].parse::<i64>().unwrap_or(0);
        let mode = parts[0].to_string();
        let file_path = format!("{}/{}", path.trim_end_matches('/'), &name);

        entries.push(FileEntry { name, path: file_path, is_dir, size_bytes, mode });
    }

    Ok(entries)
}

pub async fn get_file_contents(
    docker: Arc<impl DockerBackend>,
    server_id: String,
    path: String,
) -> Result<Vec<u8>> {
    let cmd = format!("cat {}", shell_escape(&path));
    let output = docker.send_command_with_output(server_id, cmd).await?;
    Ok(output.into_bytes())
}

pub async fn write_file_contents(
    docker: Arc<impl DockerBackend>,
    server_id: String,
    path: String,
    content: Vec<u8>,
) -> Result<()> {
    use base64::engine::general_purpose::STANDARD;
    use base64::engine::Engine;
    let encoded = STANDARD.encode(&content);
    let cmd = format!("echo {} | base64 -d > {}", encoded, shell_escape(&path));
    docker.send_command(server_id, cmd).await
}

pub async fn create_directory(
    docker: Arc<impl DockerBackend>,
    server_id: String,
    path: String,
) -> Result<()> {
    let cmd = format!("mkdir -p {}", shell_escape(&path));
    docker.send_command(server_id, cmd).await
}

pub async fn delete_files(
    docker: Arc<impl DockerBackend>,
    server_id: String,
    path: String,
    recursive: bool,
) -> Result<()> {
    let flag = if recursive { "-rf" } else { "-f" };
    let cmd = format!("rm {} {}", flag, shell_escape(&path));
    docker.send_command(server_id, cmd).await
}

pub async fn rename_file(
    docker: Arc<impl DockerBackend>,
    server_id: String,
    old_path: String,
    new_path: String,
) -> Result<()> {
    let cmd = format!("mv {} {}", shell_escape(&old_path), shell_escape(&new_path));
    docker.send_command(server_id, cmd).await
}

pub async fn download_file_chunk(
    docker: Arc<impl DockerBackend>,
    server_id: String,
    path: String,
    offset: u64,
    chunk_size: u64,
) -> Result<Vec<u8>> {
    let cmd = format!("dd if={} bs=1 skip={} count={} 2>/dev/null", shell_escape(&path), offset, chunk_size);
    let output = docker.send_command_with_output(server_id, cmd).await?;
    Ok(output.into_bytes())
}

pub async fn upload_file_chunk(
    docker: Arc<impl DockerBackend>,
    server_id: String,
    path: String,
    chunk: Vec<u8>,
    append: bool,
) -> Result<()> {
    use base64::engine::general_purpose::STANDARD;
    use base64::engine::Engine;
    let encoded = STANDARD.encode(&chunk);
    let redirect = if append { ">>" } else { ">" };
    let cmd = format!("echo {} | base64 -d {} {}", encoded, redirect, shell_escape(&path));
    docker.send_command(server_id, cmd).await
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::MockDockerBackend;
    use futures_util::future::FutureExt;

    #[tokio::test]
    async fn test_list_files_returns_correct_structure() {
        let mut mock = MockDockerBackend::new();

        mock.expect_send_command_with_output()
            .once()
            .returning(|_, _| {
                let output = "total 8\ndrwxr-xr-x 2 root root 4096 2026-06-26 10:00 .\ndrwxr-xr-x 2 root root 4096 2026-06-26 10:00 ..\n-rw-r--r-- 1 root root 1234 2026-06-26 10:00 test.txt";
                async { Ok(output.to_string()) }.boxed()
            });

        let entries = list_files(Arc::new(mock), "srv-1".into(), "/root".into()).await.unwrap();
        assert!(entries.len() >= 2);
        assert!(entries.iter().any(|e| e.name == "test.txt"));
    }

    #[tokio::test]
    async fn test_get_file_contents_reads_from_docker() {
        let mut mock = MockDockerBackend::new();
        mock.expect_send_command_with_output()
            .withf(|_, cmd| cmd.contains("cat"))
            .once()
            .returning(|_, _| async { Ok("file contents\n".to_string()) }.boxed());

        let content = get_file_contents(Arc::new(mock), "srv-1".into(), "/root/file.txt".into())
            .await
            .unwrap();
        assert_eq!(content, b"file contents\n");
    }

    #[tokio::test]
    async fn test_write_file_contents_executes_write_command() {
        let mut mock = MockDockerBackend::new();
        mock.expect_send_command()
            .withf(|_, cmd| cmd.contains("base64") && cmd.contains(">"))
            .once()
            .returning(|_, _| async { Ok(()) }.boxed());

        write_file_contents(Arc::new(mock), "srv-1".into(), "/root/test.txt".into(), b"hello".to_vec())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_create_directory_uses_mkdir() {
        let mut mock = MockDockerBackend::new();
        mock.expect_send_command()
            .withf(|_, cmd| cmd.contains("mkdir -p"))
            .once()
            .returning(|_, _| async { Ok(()) }.boxed());

        create_directory(Arc::new(mock), "srv-1".into(), "/root/newdir".into())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_delete_files_recursive() {
        let mut mock = MockDockerBackend::new();
        mock.expect_send_command()
            .withf(|_, cmd| cmd.contains("rm -rf"))
            .once()
            .returning(|_, _| async { Ok(()) }.boxed());

        delete_files(Arc::new(mock), "srv-1".into(), "/root/somedir".into(), true)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_delete_files_non_recursive() {
        let mut mock = MockDockerBackend::new();
        mock.expect_send_command()
            .withf(|_, cmd| cmd.contains("rm -f") && !cmd.contains("-rf"))
            .once()
            .returning(|_, _| async { Ok(()) }.boxed());

        delete_files(Arc::new(mock), "srv-1".into(), "/root/file.txt".into(), false)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_rename_file_uses_mv() {
        let mut mock = MockDockerBackend::new();
        mock.expect_send_command()
            .withf(|_, cmd| cmd.contains("mv"))
            .once()
            .returning(|_, _| async { Ok(()) }.boxed());

        rename_file(Arc::new(mock), "srv-1".into(), "/root/old.txt".into(), "/root/new.txt".into())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_shell_escape_quotes_path() {
        let escaped = shell_escape("/root/file with spaces.txt");
        assert!(escaped.starts_with('\''));
        assert!(escaped.ends_with('\''));
    }
}
