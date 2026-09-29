use crate::commands::{AppState, err};
use serde_json::{Value, json};
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
#[derive(Default)]
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
    download: Mutex<Option<Vec<u8>>>,
    status: Mutex<Value>,
    gate: tokio::sync::Mutex<()>,
}
fn status(app: &tauri::AppHandle, value: Value) -> Value {
    *app.state::<UpdateState>().status.lock().unwrap() = value.clone();
    let _ = app.emit("update-status", &value);
    value
}
#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> Result<Value, String> {
    let state = app.state::<UpdateState>();
    let _guard = state.gate.try_lock().map_err(|_| "更新操作正在进行")?;
    if state.download.lock().unwrap().is_some() {
        return Ok(state.status.lock().unwrap().clone());
    }
    status(&app, json!({"state":"checking"}));
    let result = async {
        let updater = app
            .updater_builder()
            .timeout(std::time::Duration::from_secs(45))
            .build()
            .map_err(|_| "更新配置不可用")?;
        updater
            .check()
            .await
            .map_err(|_| "无法访问 GitHub 更新服务，请稍后重试")
    }
    .await;
    match result {
        Ok(Some(update)) => {
            let v = json!({"state":"available","version":update.version,"notes":update.body});
            *state.pending.lock().unwrap() = Some(update);
            Ok(status(&app, v))
        }
        Ok(None) => Ok(status(&app, json!({"state":"current"}))),
        Err(e) => {
            status(&app, json!({"state":"error","message":e}));
            Err(e.into())
        }
    }
}
#[tauri::command]
pub async fn download_update(app: tauri::AppHandle) -> Result<Value, String> {
    let state = app.state::<UpdateState>();
    let _guard = state.gate.try_lock().map_err(|_| "更新操作正在进行")?;
    let mut update = state
        .pending
        .lock()
        .unwrap()
        .clone()
        .ok_or("请先检查更新")?;
    let version = update.version.clone();
    update.timeout = Some(std::time::Duration::from_secs(20 * 60));
    let mut downloaded = 0u64;
    let data=update.download(|chunk,total|{downloaded+=chunk as u64;status(&app,json!({"state":"downloading","version":version,"downloaded":downloaded,"total":total}));},||{}).await.map_err(|_|{status(&app,json!({"state":"error","message":"下载失败或签名校验不通过；当前版本未改变"}));"下载失败或签名校验不通过".to_string()})?;
    *state.download.lock().unwrap() = Some(data);
    Ok(status(
        &app,
        json!({"state":"ready","version":version,"notes":update.body}),
    ))
}
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<UpdateState>();
    let _update_guard = state.gate.try_lock().map_err(|_| "更新操作正在进行")?;
    let app_state = app.state::<AppState>();
    let (_busy, _) = app_state.begin().map_err(err)?;
    let store = app_state.store.clone();
    if !easy_switch_core::desktop::Desktop::writers().is_empty() {
        return Err("请先正常退出 Codex，避免更新期间出现新的会话写入".into());
    }
    if let Some(health) = easy_switch_core::runtime::health_check(&store)
        .await
        .map_err(err)?
    {
        if health["active"].as_u64().unwrap_or(1) > 0 {
            return Err("代理仍在处理请求，请空闲后安装".into());
        }
    }
    // Shutdown is authoritative and rejects a request racing the health check.
    tauri::async_runtime::spawn_blocking(move || easy_switch_core::runtime::stop_blocking(&store))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)?;
    let update = state
        .pending
        .lock()
        .unwrap()
        .clone()
        .ok_or("请先检查更新")?;
    let data = state
        .download
        .lock()
        .unwrap()
        .take()
        .ok_or("请先下载更新")?;
    if update.install(&data).is_err() {
        *state.download.lock().unwrap() = Some(data);
        return Err("安装未完成，请重试或安装上一版本；数据目录保留".into());
    }
    Ok(())
}
pub fn schedule(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(12)).await;
        loop {
            let s = app.state::<AppState>().store.settings();
            if let Ok(s) = s {
                if s.automatic_updates {
                    if let Ok(v) = check_update(app.clone()).await {
                        if v["state"] == "available" && s.automatic_download {
                            let _ = download_update(app.clone()).await;
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
        }
    });
}
