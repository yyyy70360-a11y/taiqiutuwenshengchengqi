use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::sleep;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupProgress<'a> {
    step: &'a str,
    state: &'a str,
    message: String,
}

pub fn begin(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        run(&app).await;
    });
}

pub async fn retry(app: &AppHandle) {
    run(app).await;
}

pub fn continue_local(app: &AppHandle) -> Result<(), String> {
    show_main(app, false)
}

async fn run(app: &AppHandle) {
    let started = Instant::now();
    sleep(Duration::from_millis(320)).await;
    emit(app, "reset", "running", "正在初始化");
    emit(app, "local", "running", "正在载入本地数据");
    let app_for_local = app.clone();
    let local_result = tauri::async_runtime::spawn_blocking(move || {
        crate::storage::get_settings(&app_for_local)?;
        crate::storage::get_accounts(&app_for_local)?;
        crate::storage::get_library(&app_for_local)?;
        crate::storage::get_history(&app_for_local)?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("载入本地数据任务失败: {error}"))
    .and_then(|result| result);
    if let Err(error) = local_result {
        emit(app, "local", "error", error);
        return;
    }
    emit(app, "local", "done", "本地数据已就绪");

    emit(app, "resources", "running", "正在检查模板与字体");
    let app_for_resources = app.clone();
    let resource_result = tauri::async_runtime::spawn_blocking(move || {
        crate::render::validate_embedded_resources()?;
        if crate::commands::get_templates().len() != 50 {
            return Err("模板清单不完整".into());
        }
        if crate::storage::get_presets(&app_for_resources)?.is_empty() {
            return Err("内容预设为空".into());
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("检查资源任务失败: {error}"))
    .and_then(|result| result);
    if let Err(error) = resource_result {
        emit(app, "resources", "error", error);
        return;
    }
    emit(app, "resources", "done", "模板与字体已就绪");

    emit(app, "credentials", "running", "正在检查系统凭据");
    let cloud_status = match crate::cloud::status(app).await {
        Ok(value) => value,
        Err(error) => {
            emit(app, "credentials", "error", error);
            return;
        }
    };
    emit(app, "credentials", "done", "系统凭据已就绪");

    emit(app, "cloud", "running", "正在检查云服务");
    let initially_connected = crate::cloud::test_connection(app).await.is_ok();
    if initially_connected {
        emit(app, "cloud", "done", "云服务已响应");
    } else {
        emit(app, "cloud", "done", "云服务需要安全通道");
    }

    emit(app, "tunnel", "running", "正在建立安全通道");
    #[cfg(target_os = "windows")]
    let tunnel_result = crate::tunnel::ensure_startup_tunnel_now(app).await;
    #[cfg(not(target_os = "windows"))]
    let tunnel_result: Result<(), String> = Ok(());
    if let Err(error) = tunnel_result {
        emit(app, "tunnel", "error", error);
        return;
    }
    emit(
        app,
        "tunnel",
        "done",
        if initially_connected {
            "安全通道已可用"
        } else {
            "安全通道已建立"
        },
    );
    if !initially_connected {
        emit(app, "cloud", "running", "正在验证云服务");
        if let Err(error) = crate::cloud::test_connection(app).await {
            emit(app, "cloud", "error", error);
            return;
        }
        emit(app, "cloud", "done", "云服务已连接");
    }

    emit(app, "session", "running", "正在验证账号会话");
    let mut needs_login = false;
    if cloud_status.logged_in {
        match crate::cloud::validate_session(app).await {
            Ok(Some(true)) => emit(app, "session", "done", "账号会话有效"),
            Ok(Some(false)) => {
                needs_login = true;
                emit(app, "session", "attention", "登录已失效，请重新登录");
            }
            Ok(None) => emit(app, "session", "done", "等待账号登录"),
            Err(error) => {
                emit(app, "session", "error", error);
                return;
            }
        }
    } else {
        emit(app, "session", "done", "等待账号登录");
    }

    emit(app, "ready", "running", "正在准备工作区");
    let minimum = Duration::from_millis(1200);
    if let Some(remaining) = minimum.checked_sub(started.elapsed()) {
        sleep(remaining).await;
    }
    emit(app, "ready", "done", "工作区已准备完成");
    sleep(Duration::from_millis(120)).await;
    let _ = show_main(app, needs_login);
}

fn emit(app: &AppHandle, step: &'static str, state: &'static str, message: impl Into<String>) {
    let _ = app.emit_to(
        "splash",
        "startup-progress",
        StartupProgress {
            step,
            state,
            message: message.into(),
        },
    );
}

fn show_main(app: &AppHandle, needs_login: bool) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "未找到主窗口".to_string())?;
    if needs_login {
        let _ = main.eval(
            "window.__STARTUP_LOGIN_REQUIRED__=true;window.dispatchEvent(new Event('startup-login-required'));",
        );
    }
    main.show()
        .map_err(|error| format!("显示主窗口失败: {error}"))?;
    main.set_focus()
        .map_err(|error| format!("聚焦主窗口失败: {error}"))?;
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
    Ok(())
}
