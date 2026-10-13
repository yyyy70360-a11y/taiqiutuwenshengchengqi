use crate::{
    cloud, jobs, local_ai,
    models::{
        Account, CloudStatus, CloudSyncResult, CopyFitLimits, CopyItem, JobComplete, JobFailure,
        JobProgress, PresetInfo, RenderRequest, RenderResponse, SettingsInput, StartupStatus,
        TemplateInfo, UiDiagnostics,
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

fn copy_limits_for_template(template: &str) -> CopyFitLimits {
    render::copy_limits_for_template(template)
}

static LAST_FILE_STAMP: AtomicU64 = AtomicU64::new(0);
const MAX_BATCH_SIZE: usize = 100;

#[tauri::command]
pub fn get_templates() -> Vec<TemplateInfo> {
    crate::copy_rules::catalog()
        .iter()
        .map(|spec| TemplateInfo {
            id: spec.id.clone(),
            name: spec.name.clone(),
            category: spec.category.clone(),
            copy_limits: copy_limits_for_template(&spec.id),
            theme_colors: render::default_colors_for_template(&spec.id),
        })
        .collect()
}

#[tauri::command]
pub async fn template_thumbnail(template: String) -> Result<String, String> {
    if !crate::copy_rules::catalog()
        .iter()
        .any(|item| item.id == template)
    {
        return Err("模板不存在".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let request = RenderRequest {
            template,
            title: "有没有一起参赛的球搭子？".into(),
            body: "限1–3档，免报名费，一天打完。\n想找个同城球友一起去，互相打打气。".into(),
            tags: "#同城台球 #找球搭子".into(),
            layout_mode: "auto".into(),
            ..Default::default()
        };
        render::render_png_at_width(&request, 216).map(|bytes| STANDARD.encode(bytes))
    })
    .await
    .map_err(|error| error.to_string())?
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
    let sidecar = path.with_extension("txt");
    let copy = format!(
        "标题：{}\n正文：{}\n话题：{}\n模板：{}\n",
        request.title, request.body, request.tags, request.template
    );
    write_copy_sidecar(&path, &copy)?;
    if let Err(error) =
        storage::record_history(app, &relative_path, &request.template, &root_directory)
    {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&sidecar);
        return Err(error);
    }
    response.file_name = relative_path;
    Ok(response)
}

#[tauri::command]
pub async fn render_batch(
    app: AppHandle,
    requests: Vec<RenderRequest>,
    output_dir: Option<String>,
    job_id: Option<String>,
) -> Result<crate::models::BatchRenderResult, String> {
    let total = requests.len();
    if total == 0 || total > MAX_BATCH_SIZE {
        return Err("单次需要 1–100 张图片".into());
    }
    let lease = jobs::acquire("render", job_id)?;
    let all_requests = requests.clone();
    crate::workspace::save_render_checkpoint(
        &app,
        &lease.id,
        serde_json::json!({"requests":all_requests,"completed":0,"failed":[],"outputs":[]}),
    )?;
    let mut outputs = Vec::new();
    let mut failed = Vec::new();
    let mut completed = 0;
    for (index, request) in requests.into_iter().enumerate() {
        if lease.control.is_cancelled() {
            break;
        }
        match render_save(app.clone(), request.clone(), output_dir.clone()).await {
            Ok(mut response) => {
                let _ = app.emit(
                    "job-progress",
                    JobProgress {
                        job_id: lease.id.clone(),
                        completed: index + 1,
                        total,
                        file_name: response.file_name.clone(),
                    },
                );
                response.image_base64.clear();
                outputs.push(response);
            }
            Err(error) => {
                let _ = app.emit(
                    "job-failed",
                    JobFailure {
                        job_id: lease.id.clone(),
                        completed: index + 1,
                        total,
                        error: error.clone(),
                    },
                );
                failed.push(crate::models::RenderFailure {
                    index,
                    request,
                    error,
                });
            }
        }
        completed = index + 1;
        crate::workspace::save_render_checkpoint(
            &app,
            &lease.id,
            serde_json::json!({"requests":all_requests,"completed":completed,"failed":failed,"outputs":outputs}),
        )?;
    }
    let _ = app.emit(
        "job-complete",
        JobComplete {
            job_id: lease.id.clone(),
            total,
            succeeded: outputs.len(),
            failed: failed.len(),
        },
    );
    Ok(crate::models::BatchRenderResult {
        job_id: lease.id.clone(),
        total,
        completed,
        outputs,
        failed,
        cancelled: lease.control.is_cancelled(),
    })
}

#[tauri::command]
pub fn cancel_job(job_id: String) -> Result<(), String> {
    jobs::cancel(&job_id)
}

#[tauri::command]
pub fn open_output_folder(app: AppHandle, path: Option<String>) -> Result<(), String> {
    let directory = path
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or(storage::output_dir(&app)?);
    fs::create_dir_all(&directory).map_err(|error| format!("创建目录失败: {error}"))?;
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&directory)
            .spawn()
            .map_err(|error| format!("打开目录失败: {error}"))?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(&directory)
            .spawn()
            .map_err(|error| format!("打开目录失败: {error}"))?;
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("当前平台暂不支持打开目录".into())
    }
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
        .map_err(|error| format!("检查钥匙串任务失败: {error}"))?
}

#[tauri::command]
pub async fn get_cloud_status(app: AppHandle) -> Result<CloudStatus, String> {
    cloud::status(&app).await
}

#[tauri::command]
pub async fn validate_cloud_session(app: AppHandle) -> Result<Option<bool>, String> {
    cloud::validate_session(&app).await
}

#[tauri::command]
pub async fn startup_check(app: AppHandle) -> Result<StartupStatus, String> {
    let app_for_local = app.clone();
    let local_ready = tauri::async_runtime::spawn_blocking(move || {
        storage::get_settings(&app_for_local)?;
        storage::get_accounts(&app_for_local)?;
        storage::get_library(&app_for_local)?;
        storage::get_history(&app_for_local)?;
        Ok::<_, String>(())
    })
    .await
    .map_err(|error| format!("本地数据检查任务失败: {error}"))?
    .is_ok();
    let resources_ready = render::validate_embedded_resources().is_ok();
    let cloud_status = cloud::status(&app).await?;
    let cloud_reachable = cloud::test_connection(&app).await.is_ok();
    let session_valid = cloud::validate_session(&app).await?;
    Ok(StartupStatus {
        local_ready,
        resources_ready,
        cloud_configured: cloud_status.server_configured,
        cloud_reachable,
        session_valid,
    })
}

#[tauri::command]
pub fn report_ui_ready(report: UiDiagnostics) -> Result<(), String> {
    let valid = report.template_count == 50
        && report.tone_count == 31
        && report.preview_ratio.replace(' ', "") == "9/16"
        && report.preview_fit == "contain"
        && report.center_overflow == "hidden"
        && matches!(report.left_overflow.as_str(), "auto" | "scroll")
        && matches!(report.right_overflow.as_str(), "auto" | "scroll");
    if !valid {
        return Err(format!("UI 自检未通过: {report:?}"));
    }
    Ok(())
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
pub async fn cloud_register_application(
    app: AppHandle,
    email: String,
    password: String,
    confirm_password: String,
) -> Result<String, String> {
    cloud::register_application(&app, &email, &password, &confirm_password).await
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
pub async fn test_local_ai(
    app: AppHandle,
    url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
) -> Result<String, String> {
    local_ai::test_connection(&app, url.as_deref(), model.as_deref(), api_key.as_deref()).await
}

#[tauri::command]
pub async fn clear_local_api_key(app: AppHandle) -> Result<(), String> {
    storage::clear_api_key(&app)
}

async fn validated_cloud_copy(
    app: &AppHandle,
    prompt: &str,
    template: Option<&str>,
) -> Result<CopyItem, String> {
    let cap = crate::copy_rules::template_capacity(template);
    let facts = crate::copy_rules::required_facts(prompt);
    let mut working = prompt.to_string();
    for attempt in 0..=2 {
        let item = cloud::generate_copy(app, &working, template).await?;
        match crate::copy_rules::validate(&item.title, &item.body, &item.tags, cap, &facts)
            .and_then(|_| {
                crate::copy_rules::validate_match_conditions(
                    prompt,
                    &format!("{}。{}", item.title, item.body),
                )
            }) {
            Ok(()) => return Ok(item),
            Err(reason) => {
                if attempt == 2 {
                    return Err(format!("云端文案仍不合格：{reason}，没有进行截断"));
                }
                working = crate::copy_rules::repair_prompt(
                    prompt,
                    &item.title,
                    &item.body,
                    &item.tags,
                    cap,
                    &facts,
                    &reason,
                );
            }
        }
    }
    unreachable!()
}

#[tauri::command]
pub async fn generate_copy(
    app: AppHandle,
    prompt: String,
    template: Option<String>,
) -> Result<CopyItem, String> {
    let mode = storage::get_ai_mode(&app).unwrap_or_else(|_| "local".into());
    if mode == "cloud" {
        validated_cloud_copy(&app, &prompt, template.as_deref()).await
    } else {
        let has_key = storage::get_api_key_status(&app).unwrap_or(false);
        if !has_key
            && cloud::status(&app)
                .await
                .map(|s| s.logged_in)
                .unwrap_or(false)
        {
            validated_cloud_copy(&app, &prompt, template.as_deref()).await
        } else {
            local_ai::generate_copy(&app, &prompt, template.as_deref()).await
        }
    }
}

#[tauri::command]
pub async fn generate_batch_copy(
    app: AppHandle,
    prompt: String,
    count: usize,
    template: Option<String>,
    job_id: Option<String>,
    existing: Option<Vec<CopyItem>>,
) -> Result<crate::models::CopyBatchResult, String> {
    let count = count.clamp(1, 100);
    let lease = jobs::acquire("copy", job_id)?;
    let cloud_mode = storage::get_ai_mode(&app)? == "cloud";
    let mut items: Vec<CopyItem> = existing
        .unwrap_or_default()
        .into_iter()
        .take(count)
        .collect();
    let mut warning = String::new();
    let mut rounds = 0;
    let mut errors = 0;
    while items.len() < count && rounds < count.div_ceil(5) * 3 + 3 {
        if lease.control.is_cancelled() {
            break;
        }
        rounds += 1;
        let wanted = (count - items.len()).min(5);
        let avoid: Vec<_> = items.iter().map(|item| item.title.as_str()).collect();
        let group_prompt = format!(
            "{prompt}\n【本组差异化】第 {rounds} 组，变化开头和邀请方式。已有标题不要重复：{}",
            avoid.join("；")
        );
        let request = async {
            if cloud_mode {
                cloud::generate_batch(&app, &group_prompt, wanted, template.as_deref()).await
            } else {
                local_ai::generate_batch(&app, &group_prompt, wanted, template.as_deref()).await
            }
        };
        let result = tokio::select! {
            result = request => result,
            _ = lease.control.cancelled() => { break; }
        };
        match result {
            Ok(group) => {
                errors = 0;
                let facts = crate::copy_rules::required_facts(&prompt);
                let cap = crate::copy_rules::template_capacity(template.as_deref());
                for item in group.into_iter().take(wanted) {
                    if crate::copy_rules::validate(&item.title, &item.body, &item.tags, cap, &facts)
                        .is_ok()
                        && crate::copy_rules::validate_match_conditions(
                            &prompt,
                            &format!("{}。{}", item.title, item.body),
                        )
                        .is_ok()
                        && !items.iter().any(|old| {
                            crate::copy_rules::near_duplicate(
                                &item.title,
                                &item.body,
                                &old.title,
                                &old.body,
                            )
                        })
                    {
                        items.push(item);
                    }
                }
                let _ = app.emit(
                    "copy-progress",
                    crate::models::CopyBatchResult {
                        job_id: lease.id.clone(),
                        total: count,
                        items: items.clone(),
                        cancelled: false,
                        warning: String::new(),
                    },
                );
            }
            Err(error) => {
                errors += 1;
                warning = error;
                if errors >= 2 {
                    break;
                }
            }
        }
    }
    if items.len() < count && warning.is_empty() && !lease.control.is_cancelled() {
        warning = format!(
            "去重与补齐后完成 {} / {count} 条，可补齐剩余部分",
            items.len()
        );
    }
    Ok(crate::models::CopyBatchResult {
        job_id: lease.id.clone(),
        total: count,
        items,
        cancelled: lease.control.is_cancelled(),
        warning,
    })
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

// The PNG belongs to this save call; never replace an existing text sidecar.
fn write_copy_sidecar(png: &Path, copy: &str) -> Result<(), String> {
    let sidecar = png.with_extension("txt");
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&sidecar)
    {
        Ok(file) => file,
        Err(error) => {
            let _ = fs::remove_file(png);
            return Err(format!("对应文案保存失败：{error}"));
        }
    };
    if let Err(error) = file.write_all(copy.as_bytes()) {
        drop(file);
        let _ = fs::remove_file(png);
        let _ = fs::remove_file(&sidecar);
        return Err(format!("对应文案写入失败：{error}"));
    }
    Ok(())
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
                if let Err(error) = file.write_all(bytes) {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(format!("保存图片失败: {error}"));
                }
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
    fn copy_sidecar_is_paired_and_never_overwrites_existing_text() {
        let directory = std::env::temp_dir().join(format!("copy-pair-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let image = directory.join("sample.png");
        fs::write(&image, b"png").unwrap();
        write_copy_sidecar(&image, "败方结台费").unwrap();
        assert_eq!(
            fs::read_to_string(image.with_extension("txt")).unwrap(),
            "败方结台费"
        );
        assert!(write_copy_sidecar(&image, "被替换的内容").is_err());
        assert_eq!(
            fs::read_to_string(image.with_extension("txt")).unwrap(),
            "败方结台费"
        );
        fs::remove_dir_all(directory).unwrap();
    }

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
