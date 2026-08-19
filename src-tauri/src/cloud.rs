use crate::{
    models::{Account, CloudStatus, CloudSyncResult, CopyItem, SettingsInput},
    storage,
};
use reqwest::{Client, Method, Response, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    net::IpAddr,
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;
use tokio::sync::Mutex;
use uuid::Uuid;

const CREDENTIAL_SERVICE: &str = "com.billiards.matrix";
const ACCESS_TOKEN_ACCOUNT: &str = "cloud_access_token";
const REFRESH_TOKEN_ACCOUNT: &str = "cloud_refresh_token";

static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();
static REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Serialize)]
struct Credentials<'a> {
    email: &'a str,
    password: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RegisterApplicationRequest<'a> {
    email: &'a str,
    password: &'a str,
    confirm_password: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RefreshRequest<'a> {
    refresh_token: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct RegisterApplicationResponse {
    message: String,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: Option<String>,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: String,
    service: String,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RemoteSettings {
    api_url: String,
    api_model: String,
    output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteAccount {
    id: Option<String>,
    name: String,
    region: String,
    level: String,
    persona: String,
    tone: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCopyItem {
    id: Option<String>,
    title: String,
    body: String,
    tags: String,
}

pub async fn set_server_url(app: &AppHandle, server_url: &str) -> Result<CloudStatus, String> {
    let normalized = normalize_server_url(server_url)?;
    let app_for_read = app.clone();
    let old =
        tauri::async_runtime::spawn_blocking(move || storage::cloud_server_url(&app_for_read))
            .await
            .map_err(|error| format!("读取云服务配置任务失败: {error}"))??;
    if !old.is_empty() && old != normalized {
        clear_tokens().await?;
        let app_for_settings = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            storage::set_cloud_email(&app_for_settings, "")?;
            storage::set_cloud_owner_email(&app_for_settings, "")?;
            storage::clear_cloud_record_ids(&app_for_settings)
        })
        .await
        .map_err(|error| format!("清理云账号任务失败: {error}"))??;
    }
    let app_for_write = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage::set_cloud_server_url(&app_for_write, &normalized)
    })
    .await
    .map_err(|error| format!("保存云服务地址任务失败: {error}"))??;
    status(app).await
}

pub async fn status(app: &AppHandle) -> Result<CloudStatus, String> {
    let app = app.clone();
    let (server_url, email, last_sync_at) = tauri::async_runtime::spawn_blocking(move || {
        Ok::<_, String>((
            storage::cloud_server_url(&app)?,
            storage::cloud_email(&app)?,
            storage::last_cloud_sync_at(&app)?,
        ))
    })
    .await
    .map_err(|error| format!("读取云服务状态任务失败: {error}"))??;
    let logged_in = read_secret(REFRESH_TOKEN_ACCOUNT).await?.is_some();
    Ok(CloudStatus {
        server_configured: !server_url.is_empty(),
        server_url,
        logged_in,
        email,
        last_sync_at,
    })
}

pub async fn test_connection(app: &AppHandle) -> Result<String, String> {
    let base = configured_base_url(app).await?;
    let response = client()
        .get(endpoint(&base, "/health"))
        .send()
        .await
        .map_err(network_error)?;
    let health: HealthResponse = parse_response(response).await?;
    if health.status != "ok" {
        return Err("云服务健康检查未通过".into());
    }
    Ok(format!("{} {} 连接正常", health.service, health.version))
}

pub async fn register(app: &AppHandle, email: &str, password: &str) -> Result<CloudStatus, String> {
    let _ = register_application(app, email, password, password).await?;
    status(app).await
}

pub async fn validate_session(app: &AppHandle) -> Result<Option<bool>, String> {
    if read_secret(REFRESH_TOKEN_ACCOUNT).await?.is_none() {
        return Ok(None);
    }
    if read_secret(ACCESS_TOKEN_ACCOUNT).await?.is_none() {
        let base = configured_base_url(app).await?;
        if let Err(error) = refresh_session(&base).await {
            if error.contains("无法连接云服务") {
                return Err(error);
            }
            clear_tokens().await?;
            return Ok(Some(false));
        }
    }
    match authenticated_json::<RemoteSettings>(app, Method::GET, "/api/v1/me/settings", None).await
    {
        Ok(_) => Ok(Some(true)),
        Err(error) if error.contains("无法连接云服务") => Err(error),
        Err(_) => {
            clear_tokens().await?;
            Ok(Some(false))
        }
    }
}

pub async fn register_application(
    app: &AppHandle,
    email: &str,
    password: &str,
    confirm_password: &str,
) -> Result<String, String> {
    let email = email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err("请输入有效邮箱".into());
    }
    if password.chars().count() < 8 {
        return Err("密码至少需要 8 个字符".into());
    }
    if !password.chars().any(char::is_alphabetic) || !password.chars().any(char::is_numeric) {
        return Err("密码需同时包含字母和数字".into());
    }
    if password != confirm_password {
        return Err("两次输入的密码不一致".into());
    }
    let base = configured_base_url(app).await?;
    let response = client()
        .post(endpoint(&base, "/api/v1/auth/register-application"))
        .json(&RegisterApplicationRequest {
            email: &email,
            password,
            confirm_password,
        })
        .send()
        .await
        .map_err(network_error)?;
    let result: RegisterApplicationResponse = parse_response(response).await?;
    Ok(result.message)
}

pub async fn login(app: &AppHandle, email: &str, password: &str) -> Result<CloudStatus, String> {
    authenticate(app, "/api/v1/auth/login", email, password).await
}

async fn authenticate(
    app: &AppHandle,
    path: &str,
    email: &str,
    password: &str,
) -> Result<CloudStatus, String> {
    let email = email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err("请输入有效邮箱".into());
    }
    if password.chars().count() < 8 {
        return Err("密码至少需要 8 个字符".into());
    }
    let base = configured_base_url(app).await?;
    let response = client()
        .post(endpoint(&base, path))
        .json(&Credentials {
            email: &email,
            password,
        })
        .send()
        .await
        .map_err(network_error)?;
    let session: AuthResponse = parse_response(response).await?;
    store_tokens(&session).await?;
    let app_for_identity = app.clone();
    let identity_email = email.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let previous = storage::cloud_owner_email(&app_for_identity)?;
        if !previous.is_empty() && previous != identity_email {
            storage::clear_cloud_record_ids(&app_for_identity)?;
        }
        storage::set_cloud_owner_email(&app_for_identity, &identity_email)?;
        storage::set_cloud_email(&app_for_identity, &identity_email)
    })
    .await
    .map_err(|error| format!("保存云账号任务失败: {error}"))??;
    status(app).await
}

pub async fn logout(app: &AppHandle) -> Result<CloudStatus, String> {
    let base = configured_base_url(app).await.ok();
    let refresh = read_secret(REFRESH_TOKEN_ACCOUNT).await?;
    let remote_response = if let (Some(base), Some(refresh)) = (base, refresh) {
        Some(
            client()
                .post(endpoint(&base, "/api/v1/auth/logout"))
                .json(&RefreshRequest {
                    refresh_token: &refresh,
                })
                .send()
                .await
                .ok(),
        )
    } else {
        None
    };
    clear_tokens().await?;
    let app_for_email = app.clone();
    tauri::async_runtime::spawn_blocking(move || storage::set_cloud_email(&app_for_email, ""))
        .await
        .map_err(|error| format!("清理云账号任务失败: {error}"))??;
    let _ = remote_response;
    status(app).await
}

pub async fn generate_copy(
    app: &AppHandle,
    prompt: &str,
    template: Option<&str>,
) -> Result<CopyItem, String> {
    if prompt.trim().is_empty() {
        return Err("提示词为空".into());
    }
    let remote: RemoteCopyItem = authenticated_json(
        app,
        Method::POST,
        "/api/v1/ai/generate-copy",
        Some(json!({ "prompt": prompt, "template": template })),
    )
    .await?;
    Ok(remote.into())
}

pub async fn generate_batch(
    app: &AppHandle,
    prompt: &str,
    count: usize,
    template: Option<&str>,
) -> Result<Vec<CopyItem>, String> {
    if prompt.trim().is_empty() {
        return Err("提示词为空".into());
    }
    let remote: Vec<RemoteCopyItem> = authenticated_json(
        app,
        Method::POST,
        "/api/v1/ai/generate-batch-copy",
        Some(json!({ "prompt": prompt, "count": count.clamp(1, 100), "template": template })),
    )
    .await?;
    Ok(remote.into_iter().map(Into::into).collect())
}

pub async fn sync_upload(app: &AppHandle) -> Result<CloudSyncResult, String> {
    ensure_logged_in().await?;
    let app_for_read = app.clone();
    let (settings, accounts, library) = tauri::async_runtime::spawn_blocking(move || {
        Ok::<_, String>((
            storage::get_settings(&app_for_read)?,
            storage::get_accounts(&app_for_read)?,
            storage::get_library_records(&app_for_read)?,
        ))
    })
    .await
    .map_err(|error| format!("读取待上传数据任务失败: {error}"))??;

    let remote_settings = RemoteSettings {
        api_url: setting_string(&settings, "api_url"),
        api_model: setting_string(&settings, "api_model"),
        // Output paths are device-local and must never overwrite another platform's path.
        output_dir: String::new(),
    };
    let _: RemoteSettings = authenticated_json(
        app,
        Method::PUT,
        "/api/v1/me/settings",
        Some(serde_json::to_value(remote_settings).map_err(json_error)?),
    )
    .await?;

    let remote_accounts = accounts
        .into_iter()
        .map(|account| RemoteAccount {
            id: account
                .cloud_id
                .or_else(|| Some(Uuid::new_v4().to_string())),
            name: account.name,
            region: account.region,
            level: account.level,
            persona: account.persona,
            tone: account.tone,
            status: account.status,
        })
        .collect::<Vec<_>>();
    let saved_accounts: Vec<RemoteAccount> = authenticated_json(
        app,
        Method::PUT,
        "/api/v1/me/accounts",
        Some(serde_json::to_value(remote_accounts).map_err(json_error)?),
    )
    .await?;
    let account_count = saved_accounts.len();
    let local_accounts = saved_accounts.into_iter().map(Into::into).collect();
    let app_for_accounts = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage::save_accounts(&app_for_accounts, local_accounts)
    })
    .await
    .map_err(|error| format!("保存账号同步状态任务失败: {error}"))??;

    let mut copy_count = 0;
    for (row_id, item) in library {
        let id = item.cloud_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let app_for_id = app.clone();
        let id_for_storage = id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            storage::set_library_cloud_id(&app_for_id, row_id, &id_for_storage)
        })
        .await
        .map_err(|error| format!("保存文案同步标识任务失败: {error}"))??;
        let remote = RemoteCopyItem {
            id: Some(id),
            title: item.title,
            body: item.body,
            tags: item.tags,
        };
        let _: RemoteCopyItem = authenticated_json(
            app,
            Method::POST,
            "/api/v1/me/copy-library",
            Some(serde_json::to_value(remote).map_err(json_error)?),
        )
        .await?;
        copy_count += 1;
    }
    sync_result(app, "upload", account_count, copy_count).await
}

pub async fn sync_download(app: &AppHandle) -> Result<CloudSyncResult, String> {
    ensure_logged_in().await?;
    let remote_settings: RemoteSettings =
        authenticated_json(app, Method::GET, "/api/v1/me/settings", None).await?;
    let remote_accounts: Vec<RemoteAccount> =
        authenticated_json(app, Method::GET, "/api/v1/me/accounts", None).await?;
    let remote_library: Vec<RemoteCopyItem> =
        authenticated_json(app, Method::GET, "/api/v1/me/copy-library", None).await?;
    let account_count = remote_accounts.len();
    let copy_count = remote_library.len();
    let accounts = remote_accounts.into_iter().map(Into::into).collect();
    let library = remote_library.into_iter().map(Into::into).collect();
    let app_for_write = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage::replace_cloud_data(
            &app_for_write,
            SettingsInput {
                api_url: nonempty(remote_settings.api_url),
                api_model: nonempty(remote_settings.api_model),
                api_key: None,
                output_dir: None,
            },
            accounts,
            library,
        )
    })
    .await
    .map_err(|error| format!("写入云端数据任务失败: {error}"))??;
    sync_result(app, "download", account_count, copy_count).await
}

async fn sync_result(
    app: &AppHandle,
    direction: &str,
    accounts: usize,
    copy_items: usize,
) -> Result<CloudSyncResult, String> {
    let synced_at = now_timestamp();
    let app_for_sync = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        storage::set_last_cloud_sync_at(&app_for_sync, synced_at)
    })
    .await
    .map_err(|error| format!("保存同步时间任务失败: {error}"))??;
    Ok(CloudSyncResult {
        direction: direction.into(),
        accounts,
        copy_items,
        synced_at,
    })
}

async fn authenticated_json<T: DeserializeOwned>(
    app: &AppHandle,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<T, String> {
    let base = configured_base_url(app).await?;
    let original_token = read_secret(ACCESS_TOKEN_ACCOUNT)
        .await?
        .ok_or_else(|| "请先登录云服务".to_string())?;
    let first =
        send_authenticated(&base, &original_token, method.clone(), path, body.clone()).await?;
    if first.status() != StatusCode::UNAUTHORIZED {
        return parse_response(first).await;
    }

    let _refresh_guard = REFRESH_LOCK.get_or_init(|| Mutex::new(())).lock().await;
    let latest_token = read_secret(ACCESS_TOKEN_ACCOUNT)
        .await?
        .ok_or_else(|| "登录状态已失效，请重新登录".to_string())?;
    let token = if latest_token != original_token {
        latest_token
    } else {
        match refresh_session(&base).await {
            Ok(token) => token,
            Err(error) => {
                clear_tokens().await?;
                return Err(error);
            }
        }
    };
    let retry = send_authenticated(&base, &token, method, path, body).await?;
    if retry.status() == StatusCode::UNAUTHORIZED {
        clear_tokens().await?;
        return Err("登录状态已失效，请重新登录".into());
    }
    parse_response(retry).await
}

async fn refresh_session(base: &str) -> Result<String, String> {
    let refresh_token = read_secret(REFRESH_TOKEN_ACCOUNT)
        .await?
        .ok_or_else(|| "登录状态已失效，请重新登录".to_string())?;
    let response = client()
        .post(endpoint(base, "/api/v1/auth/refresh"))
        .json(&RefreshRequest {
            refresh_token: &refresh_token,
        })
        .send()
        .await
        .map_err(network_error)?;
    let session: AuthResponse = parse_response(response).await?;
    let access_token = session.access_token.clone();
    store_tokens(&session).await?;
    Ok(access_token)
}

async fn send_authenticated(
    base: &str,
    token: &str,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Response, String> {
    let mut request = client()
        .request(method, endpoint(base, path))
        .bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    request.send().await.map_err(network_error)
}

async fn parse_response<T: DeserializeOwned>(response: Response) -> Result<T, String> {
    if !response.status().is_success() {
        return Err(response_error(response).await);
    }
    response
        .json::<T>()
        .await
        .map_err(|_| "云服务返回的数据格式无效".to_string())
}

async fn response_error(response: Response) -> String {
    let status = response.status();
    response
        .json::<ErrorResponse>()
        .await
        .ok()
        .map(|error| format_error_code(error.error.as_deref(), &error.message))
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| format!("云服务请求失败（HTTP {}）", status.as_u16()))
}

fn format_error_code(code: Option<&str>, message: &str) -> String {
    match code {
        Some("application_pending") => "注册申请正在审核，请等待管理员批准后登录".into(),
        Some("application_rejected") => "注册申请未通过，请重新提交申请".into(),
        Some("account_exists") => "该邮箱已注册，请直接登录".into(),
        Some("too_many_requests") => "申请提交过于频繁，请稍后再试".into(),
        Some("account_disabled") => "账号已被停用，请联系管理员".into(),
        Some("invalid_credentials") => "邮箱或密码不正确".into(),
        _ => message.to_string(),
    }
}

fn client() -> &'static Client {
    HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(45))
            .build()
            .expect("cloud HTTP client configuration is valid")
    })
}

async fn configured_base_url(app: &AppHandle) -> Result<String, String> {
    let app = app.clone();
    let value = tauri::async_runtime::spawn_blocking(move || storage::cloud_server_url(&app))
        .await
        .map_err(|error| format!("读取云服务地址任务失败: {error}"))??;
    if value.is_empty() {
        return Err("请先配置云服务地址".into());
    }
    normalize_server_url(&value)
}

fn normalize_server_url(value: &str) -> Result<String, String> {
    let mut url = Url::parse(value.trim()).map_err(|_| "云服务地址格式无效".to_string())?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("云服务地址不能包含账号、查询参数或片段".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "云服务地址缺少主机名".to_string())?;
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false);
    match url.scheme() {
        "https" => {}
        "http" if loopback => {}
        "http" => return Err("远程云服务必须使用 HTTPS".into()),
        _ => return Err("云服务地址仅支持 HTTPS；本机隧道可使用 HTTP".into()),
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err("云服务地址只填写域名和端口，不要附加接口路径".into());
    }
    url.set_path("");
    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn endpoint(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

async fn ensure_logged_in() -> Result<(), String> {
    if read_secret(REFRESH_TOKEN_ACCOUNT).await?.is_none() {
        return Err("请先登录云服务".into());
    }
    Ok(())
}

async fn store_tokens(session: &AuthResponse) -> Result<(), String> {
    let access = session.access_token.clone();
    let refresh = session.refresh_token.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let access_entry = secret_entry(ACCESS_TOKEN_ACCOUNT)?;
        access_entry
            .set_password(&access)
            .map_err(|error| format!("保存登录状态失败: {error}"))?;
        let refresh_entry = secret_entry(REFRESH_TOKEN_ACCOUNT)?;
        if let Err(error) = refresh_entry.set_password(&refresh) {
            let _ = access_entry.delete_credential();
            return Err(format!("保存登录状态失败: {error}"));
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("保存登录状态任务失败: {error}"))?
}

async fn read_secret(account: &'static str) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || match secret_entry(account)?.get_password() {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("读取登录状态失败: {error}")),
    })
    .await
    .map_err(|error| format!("读取登录状态任务失败: {error}"))?
}

async fn clear_tokens() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        for account in [ACCESS_TOKEN_ACCOUNT, REFRESH_TOKEN_ACCOUNT] {
            match secret_entry(account)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(error) => return Err(format!("清理登录状态失败: {error}")),
            }
        }
        Ok(())
    })
    .await
    .map_err(|error| format!("清理登录状态任务失败: {error}"))?
}

fn secret_entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(CREDENTIAL_SERVICE, account)
        .map_err(|error| format!("访问系统凭据存储失败: {error}"))
}

fn setting_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn network_error(_: reqwest::Error) -> String {
    "无法连接云服务，请检查网络和服务器地址".into()
}

fn json_error(_: serde_json::Error) -> String {
    "准备云同步数据失败".into()
}

impl From<RemoteAccount> for Account {
    fn from(value: RemoteAccount) -> Self {
        Self {
            cloud_id: value.id,
            name: value.name,
            region: value.region,
            level: value.level,
            persona: value.persona,
            tone: value.tone,
            status: value.status,
        }
    }
}

impl From<RemoteCopyItem> for CopyItem {
    fn from(value: RemoteCopyItem) -> Self {
        Self {
            cloud_id: value.id,
            title: value.title,
            body: value.body,
            tags: value.tags,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_and_loopback_http() {
        assert_eq!(
            normalize_server_url("https://example.com/").unwrap(),
            "https://example.com"
        );
        assert_eq!(
            normalize_server_url("http://127.0.0.1:38123").unwrap(),
            "http://127.0.0.1:38123"
        );
        assert!(normalize_server_url("http://115.191.33.129").is_err());
    }

    #[test]
    fn rejects_unsafe_or_ambiguous_server_urls() {
        assert!(normalize_server_url("https://user:pass@example.com").is_err());
        assert!(normalize_server_url("https://example.com/api/v1").is_err());
        assert!(normalize_server_url("https://example.com?token=secret").is_err());
    }

    #[test]
    fn endpoint_has_one_separator() {
        assert_eq!(
            endpoint("https://example.com/", "/health"),
            "https://example.com/health"
        );
    }

    #[test]
    fn maps_registration_and_login_error_codes() {
        assert_eq!(
            format_error_code(Some("application_pending"), "fallback"),
            "注册申请正在审核，请等待管理员批准后登录"
        );
        assert_eq!(
            format_error_code(Some("application_rejected"), "fallback"),
            "注册申请未通过，请重新提交申请"
        );
        assert_eq!(format_error_code(Some("unknown"), "fallback"), "fallback");
    }
}
