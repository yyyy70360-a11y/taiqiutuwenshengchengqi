use crate::{
    cloud,
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

const TEMPLATES: &[(&str, &str)] = &[
    ("magazine", "高级杂志风"),
    ("magazine_pro", "杂志风Pro"),
    ("fresh", "清新卡片风"),
    ("minimal", "极简留白风"),
    ("poster", "深色海报风"),
    ("journal", "手账贴纸风"),
    ("neon_club", "霓虹球房风"),
    ("chalkboard", "黑板战术风"),
    ("retro_ticket", "复古票根风"),
    ("cyber_grid", "赛博网格风"),
    ("cream_note", "奶油便签风"),
    ("arena_score", "球赛记分牌风"),
    ("sunset_gradient", "日落渐变风"),
    ("ink_stamp", "墨印章报风"),
    ("glass_card", "玻璃拟态风"),
    ("tactical_blue", "战术蓝图风"),
    ("midnight_lux", "午夜奢华风"),
    ("candy_pop", "糖果撞色风"),
    ("forest_match", "森林球局风"),
    ("steel_gray", "钢灰商务风"),
    ("royal_gold", "皇家金紫风"),
    ("ocean_wave", "海浪清爽风"),
    ("lava_motion", "熔岩动感风"),
    ("pearl_lite", "珍珠浅色风"),
    ("street_snap", "街拍黄黑风"),
    ("comic_burst", "漫画爆炸风"),
    ("vaporwave", "蒸汽波风"),
    ("newspaper", "复古报纸风"),
    ("coffee_receipt", "咖啡票据风"),
    ("scoreboard_green", "绿色记分屏风"),
    ("purple_stage", "紫色舞台风"),
    ("ice_blue", "冰蓝清透风"),
    ("red_warning", "红色警示风"),
    ("kraft_label", "牛皮纸标签风"),
    ("mint_mono", "薄荷单色风"),
    ("black_gold", "黑金会员风"),
    ("gradient_ring", "渐变圆环风"),
    ("billiard_felt", "台呢绿毡风"),
    ("tournament_bracket", "赛事对阵风"),
    ("soft_shadow", "柔和阴影风"),
    ("bold_blocks", "粗块拼贴风"),
    ("pink_soda", "粉色汽水风"),
    ("desert_sand", "沙漠暖砂风"),
    ("matrix_code", "矩阵代码风"),
    ("club_vip", "球房VIP风"),
    ("clean_blue", "干净蓝白风"),
    ("orange_zine", "橙色小刊风"),
    ("silver_card", "银色卡片风"),
    ("green_laser", "绿色激光风"),
    ("classic_serif", "经典衬线风"),
];

fn copy_limits_for_template(template: &str) -> CopyFitLimits {
    render::copy_limits_for_template(template)
}

static LAST_FILE_STAMP: AtomicU64 = AtomicU64::new(0);
const MAX_BATCH_SIZE: usize = 100;

#[tauri::command]
pub fn get_templates() -> Vec<TemplateInfo> {
    TEMPLATES
        .iter()
        .map(|(id, name)| TemplateInfo {
            id: (*id).into(),
            name: (*name).into(),
            copy_limits: copy_limits_for_template(id),
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
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&directory)
            .spawn()
            .map_err(|error| format!("打开目录失败: {error}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("当前版本只实现 macOS 打开目录".into())
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
pub async fn generate_copy(
    app: AppHandle,
    prompt: String,
    template: Option<String>,
) -> Result<CopyItem, String> {
    cloud::generate_copy(&app, &prompt, template.as_deref()).await
}

#[tauri::command]
pub async fn generate_batch_copy(
    app: AppHandle,
    prompt: String,
    count: usize,
    template: Option<String>,
) -> Result<Vec<CopyItem>, String> {
    cloud::generate_batch(&app, &prompt, count, template.as_deref()).await
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
