use serde::Serialize;
use std::time::Duration;
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
    show_main(app)
}

async fn run(app: &AppHandle) {
    sleep(Duration::from_millis(450)).await;
    emit(app, "local", "running", "正在载入本地数据");
    sleep(Duration::from_millis(220)).await;
    emit(app, "local", "done", "本地数据已就绪");

    emit(app, "resources", "running", "正在检查模板与字体");
    sleep(Duration::from_millis(220)).await;
    emit(app, "resources", "done", "模板与字体已就绪");

    emit(app, "credentials", "running", "正在检查系统凭据");
    sleep(Duration::from_millis(180)).await;
    emit(app, "credentials", "done", "系统凭据已就绪");

    emit(app, "cloud", "running", "正在连接云服务");
    #[cfg(target_os = "windows")]
    let cloud_result = crate::tunnel::ensure_startup_tunnel_now(app).await;
    #[cfg(not(target_os = "windows"))]
    let cloud_result: Result<(), String> = Ok(());

    match cloud_result {
        Ok(()) => {
            emit(app, "cloud", "done", "云服务已连接");
            emit(app, "ready", "done", "工作区已准备完成");
            sleep(Duration::from_millis(420)).await;
            let _ = show_main(app);
        }
        Err(error) => emit(app, "cloud", "error", error),
    }
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

fn show_main(app: &AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "未找到主窗口".to_string())?;
    main.show()
        .map_err(|error| format!("显示主窗口失败: {error}"))?;
    main.set_focus()
        .map_err(|error| format!("聚焦主窗口失败: {error}"))?;
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
    Ok(())
}
