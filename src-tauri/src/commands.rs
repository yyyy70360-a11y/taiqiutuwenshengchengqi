use crate::{
    cloud,
    models::{
        Account, CloudStatus, CloudSyncResult, CopyItem, JobComplete, JobFailure, JobProgress,
        PresetInfo, RenderRequest, RenderResponse, SettingsInput, TemplateInfo,
    },
    render, storage,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};

const TEMPLATES: &[(&str, &str)] = &[
    ("magazine", "高级杂志风"),
    ("magazine_pro", "杂志风Pro"),
    ("fresh", "清新卡片风"),
    ("minimal", "极简留白风"),
    ("poster", "深色海报风"),
    ("journal", "手账贴纸风"),
];

static LAST_FILE_STAMP: AtomicU64 = AtomicU64::new(0);
const MAX_BATCH_SIZE: usize = 100;

#[tauri::command]
pub fn get_templates() -> Vec<TemplateInfo> {
    TEMPLATES
        .iter()
        .map(|(id, name)| TemplateInfo {
            id: (*id).into(),
            name: (*name).into(),
        })
        .collect()
}

#[tauri::command]
pub fn get_presets(app: AppHandle) -> Result<Vec<PresetInfo>, String> {
    storage::get_presets(&app)
}

#[tauri::command]
pub async fn render_preview(request: RenderRequest) -> Result<RenderResponse, String> {
    tauri::async_runtime::spawn_blocking(move || response_for(&request))
        .await
        .map_err(|error| format!("预览渲染任务失败: {error}"))?
}

#[tauri::command]
pub async fn render_save(
    app: AppHandle,
    request: RenderRequest,
    output_dir: Option<String>,
) -> Result<RenderResponse, String> {
    tauri::async_runtime::spawn_blocking(move || render_save_blocking(&app, request, output_dir))
        .await
        .map_err(|error| format!("保存渲染任务失败: {error}"))?
}

fn render_save_blocking(
    app: &AppHandle,
    request: RenderRequest,
    output_dir: Option<String>,
) -> Result<RenderResponse, String> {
    let mut response = response_for(&request)?;
    let root_directory = output_dir
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or(default_output_dir(app)?);
    let directory = safe_subfolder(root_directory.clone(), &request.subfolder)?;
    fs::create_dir_all(&directory).map_err(|error| format!("创建输出目录失败: {error}"))?;
    let bytes = STANDARD
        .decode(&response.image_base64)
        .map_err(|error| format!("图像编码失败: {error}"))?;
    let path = write_unique_png(&directory, &request.num, &bytes)?;
    let relative_path = path
        .strip_prefix(&root_directory)
        .map_err(|_| "保存路径超出输出目录".to_string())?
        .to_string_lossy()
        .to_string();
    storage::record_history(app, &relative_path, &request.template, &root_directory)?;
    response.file_name = relative_path;
    Ok(response)
}

#[tauri::command]
pub async fn render_batch(
    app: AppHandle,
    requests: Vec<RenderRequest>,
    output_dir: Option<String>,
) -> Result<Vec<RenderResponse>, String> {
    let total = requests.len();
    if total == 0 {
        return Err("批量任务不能为空".into());
    }
    if total > MAX_BATCH_SIZE {
        return Err(format!("单次最多渲染 {MAX_BATCH_SIZE} 张图片"));
    }
    let mut results = Vec::with_capacity(total);
    let mut failed = 0;
    for (index, request) in requests.into_iter().enumerate() {
        match render_save(app.clone(), request, output_dir.clone()).await {
            Ok(mut response) => {
                let _ = app.emit(
                    "job-progress",
                    JobProgress {
                        completed: index + 1,
                        total,
                        file_name: response.file_name.clone(),
                    },
                );
                // A 100-image batch must not retain every full-resolution PNG in memory.
                response.image_base64.clear();
                results.push(response);
            }
            Err(error) => {
                failed += 1;
                let _ = app.emit(
                    "job-failed",
                    JobFailure {
                        completed: index + 1,
                        total,
                        error,
                    },
                );
            }
        }
    }
    let _ = app.emit(
        "job-complete",
        JobComplete {
            total,
            succeeded: results.len(),
            failed,
        },
    );
    Ok(results)
}

#[tauri::command]
pub fn open_output_folder(app: AppHandle, path: Option<String>) -> Result<(), String> {
    let directory = path
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or(storage::output_dir(&app)?);
    fs::create_dir_all(&directory).map_err(|error| format!("创建目录失败: {error}"))?;
    open_directory(&directory)
}

#[cfg(target_os = "windows")]
fn open_directory(directory: &Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(directory)
        .spawn()
        .map_err(|error| format!("打开目录失败: {error}"))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_directory(directory: &Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(directory)
        .spawn()
        .map_err(|error| format!("打开目录失败: {error}"))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn open_directory(directory: &Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(directory)
        .spawn()
        .map_err(|error| format!("打开目录失败: {error}"))?;
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn open_directory(_directory: &Path) -> Result<(), String> {
    Err("当前平台不支持自动打开输出目录".into())
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<serde_json::Value, String> {
    storage::get_settings(&app)
}

#[tauri::command]
pub async fn set_settings(app: AppHandle, settings: SettingsInput) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || storage::set_settings(&app, settings))
        .await
        .map_err(|error| format!("保存设置任务失败: {error}"))?
}

#[tauri::command]
pub async fn get_api_key_status(app: AppHandle) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || storage::get_api_key_status(&app))
        .await
        .map_err(|error| format!("检查系统凭据任务失败: {error}"))?
}

#[tauri::command]
pub async fn get_cloud_status(app: AppHandle) -> Result<CloudStatus, String> {
    cloud::status(&app).await
}

#[tauri::command]
pub async fn set_cloud_server_url(
    app: AppHandle,
    server_url: String,
) -> Result<CloudStatus, String> {
    cloud::set_server_url(&app, &server_url).await
}

#[tauri::command]
pub async fn test_cloud_connection(app: AppHandle) -> Result<String, String> {
    cloud::test_connection(&app).await
}

#[tauri::command]
pub async fn cloud_register(
    app: AppHandle,
    email: String,
    password: String,
) -> Result<CloudStatus, String> {
    cloud::register(&app, &email, &password).await
}

#[tauri::command]
pub async fn cloud_login(
    app: AppHandle,
    email: String,
    password: String,
) -> Result<CloudStatus, String> {
    cloud::login(&app, &email, &password).await
}

#[tauri::command]
pub async fn cloud_logout(app: AppHandle) -> Result<CloudStatus, String> {
    cloud::logout(&app).await
}

#[tauri::command]
pub async fn cloud_sync_upload(app: AppHandle) -> Result<CloudSyncResult, String> {
    cloud::sync_upload(&app).await
}

#[tauri::command]
pub async fn cloud_sync_download(app: AppHandle) -> Result<CloudSyncResult, String> {
    cloud::sync_download(&app).await
}

#[tauri::command]
pub fn get_accounts(app: AppHandle) -> Result<Vec<Account>, String> {
    storage::get_accounts(&app)
}

#[tauri::command]
pub fn set_accounts(app: AppHandle, accounts: Vec<Account>) -> Result<(), String> {
    storage::save_accounts(&app, accounts)
}

#[tauri::command]
pub fn get_copy_library(app: AppHandle) -> Result<Vec<CopyItem>, String> {
    storage::get_library(&app)
}

#[tauri::command]
pub fn get_render_history(app: AppHandle) -> Result<Vec<crate::models::HistoryEntry>, String> {
    storage::get_history(&app)
}

#[tauri::command]
pub fn read_history_image(app: AppHandle, history_id: i64) -> Result<String, String> {
    let (directory, file_name) = storage::history_location(&app, history_id)?;
    let relative = safe_relative_path(&file_name)?;
    let path = directory.join(relative);
    let metadata = fs::metadata(&path).map_err(|_| "历史图片不存在或已被移动".to_string())?;
    if !metadata.is_file() || metadata.len() > 25 * 1024 * 1024 {
        return Err("历史图片无效或文件过大".into());
    }
    let bytes = fs::read(path).map_err(|error| format!("读取历史图片失败: {error}"))?;
    Ok(STANDARD.encode(bytes))
}

#[tauri::command]
pub fn clear_render_history(app: AppHandle) -> Result<(), String> {
    storage::clear_history(&app)
}

#[tauri::command]
pub fn migrate_legacy(app: AppHandle) -> Result<serde_json::Value, String> {
    storage::migrate_legacy(&app)
}

#[tauri::command]
pub fn save_copy_library(app: AppHandle, item: CopyItem) -> Result<(), String> {
    storage::save_library_item(&app, item)
}

#[tauri::command]
pub fn migrate_copy_library(app: AppHandle, items: Vec<CopyItem>) -> Result<usize, String> {
    storage::import_legacy_library(&app, items)
}

#[tauri::command]
pub async fn generate_copy(app: AppHandle, prompt: String) -> Result<CopyItem, String> {
    cloud::generate_copy(&app, &prompt).await
}

#[tauri::command]
pub async fn generate_batch_copy(
    app: AppHandle,
    prompt: String,
    count: usize,
) -> Result<Vec<CopyItem>, String> {
    cloud::generate_batch(&app, &prompt, count).await
}

fn response_for(request: &RenderRequest) -> Result<RenderResponse, String> {
    let png = render::render_png(request)?;
    Ok(RenderResponse {
        image_base64: STANDARD.encode(png),
        file_name: file_name(request),
        width: render::WIDTH,
        height: render::HEIGHT,
    })
}

fn file_name(request: &RenderRequest) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default() as u64;
    let mut previous = LAST_FILE_STAMP.load(Ordering::Relaxed);
    let stamp = loop {
        let candidate = now.max(previous.saturating_add(1));
        match LAST_FILE_STAMP.compare_exchange_weak(
            previous,
            candidate,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break candidate,
            Err(actual) => previous = actual,
        }
    };
    let num = request
        .num
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    format!(
        "post_{}_{}.png",
        if num.is_empty() { "01" } else { &num },
        stamp
    )
}

fn write_unique_png(directory: &Path, num: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    for _ in 0..1000 {
        let request = RenderRequest {
            num: num.to_string(),
            ..Default::default()
        };
        let path = directory.join(file_name(&request));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(bytes)
                    .map_err(|error| format!("保存图片失败: {error}"))?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("保存图片失败: {error}")),
        }
    }
    Err("无法生成唯一文件名".into())
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("历史文件路径不合法".into());
    }
    Ok(path.to_path_buf())
}

fn default_output_dir(app: &AppHandle) -> Result<PathBuf, String> {
    storage::output_dir(app)
}

fn safe_subfolder(base: PathBuf, subfolder: &str) -> Result<PathBuf, String> {
    let clean = subfolder.trim().replace('\\', "/");
    if clean.is_empty() {
        return Ok(base);
    }
    let mut output = base.clone();
    for part in clean.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return Err("子文件夹路径不合法".into());
        }
        output.push(part);
    }
    if !Path::new(&output).starts_with(&base) {
        return Err("子文件夹超出输出目录".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn writes_hundred_unique_files_without_overwrite() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "billiards-unique-files-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create test output directory");

        let mut names = HashSet::new();
        for index in 0..MAX_BATCH_SIZE {
            let path = write_unique_png(&directory, &format!("{:02}", index + 1), b"png")
                .expect("write unique file");
            assert!(names.insert(path.file_name().unwrap().to_owned()));
        }

        assert_eq!(names.len(), MAX_BATCH_SIZE);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), MAX_BATCH_SIZE);
        fs::remove_dir_all(directory).expect("clean test output directory");
    }

    #[test]
    fn rejects_parent_and_absolute_subfolders() {
        let base = PathBuf::from("/tmp/billiards-output");
        assert!(safe_subfolder(base.clone(), "../escape").is_err());
        assert!(safe_subfolder(base.clone(), "/absolute").is_err());
        assert!(safe_subfolder(base, "batch/0815").is_ok());
    }
}
