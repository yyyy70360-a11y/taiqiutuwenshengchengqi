use crate::storage;
use reqwest::Client;
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tokio::time::sleep;

const LOCAL_CLOUD_URL: &str = "http://127.0.0.1:38123";
const REMOTE_FORWARD: &str = "38123:127.0.0.1:38123";
const REMOTE_HOST: &str = "billiards-tunnel@115.191.33.129";
const EMBEDDED_KEY_FILE_NAME: &str = "billiards_tunnel_ed25519";
const EMBEDDED_SSH_FILE_NAME: &str = "ssh.exe";
const FALLBACK_KEY_FILE_NAMES: &[&str] = &["taiqiutuwen.pem", "tuwen.pem"];

static TUNNEL_CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

pub async fn ensure_startup_tunnel_now(app: &AppHandle) -> Result<(), String> {
    ensure_startup_tunnel_inner(app).await
}

pub fn shutdown() {
    let Some(lock) = TUNNEL_CHILD.get() else {
        return;
    };
    let Ok(mut guard) = lock.lock() else {
        return;
    };
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

async fn ensure_startup_tunnel_inner(app: &AppHandle) -> Result<(), String> {
    let app_for_url = app.clone();
    let server_url =
        tauri::async_runtime::spawn_blocking(move || storage::cloud_server_url(&app_for_url))
            .await
            .map_err(|error| format!("读取云服务地址任务失败: {error}"))??;

    if !should_use_local_tunnel(&server_url) {
        return Ok(());
    }

    if cloud_health_ok().await {
        return Ok(());
    }

    let key_path = find_ssh_key(app)?;
    let ssh_path = find_ssh_binary(app);
    tauri::async_runtime::spawn_blocking(move || spawn_tunnel(&ssh_path, &key_path))
        .await
        .map_err(|error| format!("启动云服务隧道任务失败: {error}"))??;

    for _ in 0..16 {
        sleep(Duration::from_millis(500)).await;
        if cloud_health_ok().await {
            return Ok(());
        }
    }

    shutdown();
    Err("云服务隧道启动后健康检查未通过".into())
}

fn should_use_local_tunnel(server_url: &str) -> bool {
    let value = server_url.trim().trim_end_matches('/');
    value.is_empty() || value == LOCAL_CLOUD_URL || value == "http://localhost:38123"
}

async fn cloud_health_ok() -> bool {
    let client = match Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };

    let response = match client.get(format!("{LOCAL_CLOUD_URL}/health")).send().await {
        Ok(response) if response.status().is_success() => response,
        _ => return false,
    };

    response
        .json::<Value>()
        .await
        .ok()
        .and_then(|value| {
            value
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .as_deref()
        == Some("ok")
}

fn find_ssh_key(app: &AppHandle) -> Result<PathBuf, String> {
    for path in candidate_embedded_key_paths(app) {
        if path.is_file() {
            return prepare_embedded_key(app, &path);
        }
    }

    for path in candidate_fallback_key_paths(app) {
        if path.is_file() {
            return Ok(path);
        }
    }

    Err("未找到云服务 SSH 私钥".into())
}

fn candidate_embedded_key_paths(app: &AppHandle) -> Vec<PathBuf> {
    candidate_resource_paths(app, EMBEDDED_KEY_FILE_NAME)
}

fn candidate_resource_paths(app: &AppHandle, file_name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for directory in [
        app.path().resource_dir().ok(),
        app.path()
            .resource_dir()
            .ok()
            .map(|path| path.join("resources").join("tunnel")),
        app.path()
            .resource_dir()
            .ok()
            .map(|path| path.join("tunnel")),
    ]
    .into_iter()
    .flatten()
    {
        paths.push(directory.join(file_name));
    }
    paths
}

fn find_ssh_binary(app: &AppHandle) -> PathBuf {
    candidate_resource_paths(app, EMBEDDED_SSH_FILE_NAME)
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("ssh.exe"))
}

fn candidate_fallback_key_paths(app: &AppHandle) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(value) = std::env::var("BILLIARDS_SSH_KEY_PATH") {
        if !value.trim().is_empty() {
            paths.push(PathBuf::from(value));
        }
    }

    for directory in [
        app.path().download_dir().ok(),
        app.path().document_dir().ok(),
        app.path().app_data_dir().ok(),
        app.path().home_dir().ok().map(|path| path.join(".ssh")),
        app.path()
            .home_dir()
            .ok()
            .map(|path| path.join("Downloads")),
        std::env::current_dir().ok(),
    ]
    .into_iter()
    .flatten()
    {
        for file_name in FALLBACK_KEY_FILE_NAMES {
            paths.push(directory.join(file_name));
        }
    }

    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path.clone()));
    paths
}

fn prepare_embedded_key(app: &AppHandle, source: &Path) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("获取应用数据目录失败: {error}"))?
        .join("tunnel");
    fs::create_dir_all(&directory).map_err(|error| format!("创建隧道目录失败: {error}"))?;
    let target = directory.join(EMBEDDED_KEY_FILE_NAME);

    let should_copy = match (fs::read(source), fs::read(&target)) {
        (Ok(source_bytes), Ok(target_bytes)) => source_bytes != target_bytes,
        (Ok(_), Err(_)) => true,
        _ => true,
    };

    if should_copy {
        fs::copy(source, &target).map_err(|error| format!("复制内置隧道私钥失败: {error}"))?;
    }

    harden_key_permissions(&target)?;
    Ok(target)
}

fn spawn_tunnel(ssh_path: &Path, key_path: &Path) -> Result<(), String> {
    let mut command = Command::new(ssh_path);
    command
        .arg("-N")
        .arg("-L")
        .arg(REMOTE_FORWARD)
        .arg("-o")
        .arg("ExitOnForwardFailure=yes")
        .arg("-o")
        .arg("ServerAliveInterval=30")
        .arg("-o")
        .arg("ServerAliveCountMax=3")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new")
        .arg("-o")
        .arg("IdentitiesOnly=yes")
        .arg("-i")
        .arg(key_path)
        .arg(REMOTE_HOST)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    hide_window(&mut command);

    let child = command
        .spawn()
        .map_err(|error| format!("启动云服务 SSH 隧道失败: {error}"))?;
    let lock = TUNNEL_CHILD.get_or_init(|| Mutex::new(None));
    let Ok(mut guard) = lock.lock() else {
        let mut child = child;
        let _ = child.kill();
        let _ = child.wait();
        return Err("保存云服务 SSH 隧道状态失败".into());
    };
    if let Some(mut previous) = guard.replace(child) {
        let _ = previous.kill();
        let _ = previous.wait();
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn harden_key_permissions(path: &Path) -> Result<(), String> {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "Users".into());

    let mut first = Command::new("icacls.exe");
    first
        .arg(path)
        .arg("/inheritance:r")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_window(&mut first);
    let _ = first.status();

    let mut second = Command::new("icacls.exe");
    second
        .arg(path)
        .arg("/grant:r")
        .arg(format!("{user}:R"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_window(&mut second);
    let status = second
        .status()
        .map_err(|error| format!("收紧隧道私钥权限失败: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("收紧隧道私钥权限失败，icacls 退出码: {status}"))
    }
}

#[cfg(unix)]
fn harden_key_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("收紧隧道私钥权限失败: {error}"))
}

#[cfg(not(any(target_os = "windows", unix)))]
fn harden_key_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn hide_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_tunnel_only_for_empty_or_loopback_config() {
        assert!(should_use_local_tunnel(""));
        assert!(should_use_local_tunnel("http://127.0.0.1:38123"));
        assert!(should_use_local_tunnel("http://localhost:38123/"));
        assert!(!should_use_local_tunnel("https://api.example.com"));
    }
}
