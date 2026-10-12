use crate::commands::{AppState, err};
use easy_switch_core::{self as core, AuthMode, catalog};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::atomic::Ordering, time::Duration};
use tauri::{Emitter, Manager};
use tokio_util::sync::CancellationToken;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub available: Vec<String>,
    pub excluded: Vec<String>,
}

fn emit(app: &tauri::AppHandle) {
    if let Ok(catalogs) = catalog::load(&app.state::<AppState>().store) {
        let _ = app.emit("models-updated", catalogs.providers);
    }
}

#[tauri::command]
pub async fn refresh_provider_models(
    app: tauri::AppHandle,
    id: String,
) -> Result<catalog::ModelList, String> {
    let state = app.state::<AppState>();
    let _network = state.begin_model_refresh().map_err(err)?;
    let p = state
        .store
        .providers()
        .map_err(err)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("供应商不存在")?;
    if !matches!(p.mode, AuthMode::Api | AuthMode::Mixed) {
        return Err("此供应商不使用 API 模型列表".into());
    }
    let response = core::diagnostics::models(&p, &CancellationToken::new()).await;
    // Network requests do not hold the write gate or block foreground operations.
    let (guard, _) = state.begin().map_err(err)?;
    let store = state.store.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || -> core::Result<catalog::ModelList> {
            let _guard = guard;
            let current = store
                .providers()?
                .into_iter()
                .find(|candidate| candidate.id == id)
                .ok_or_else(|| anyhow::anyhow!("供应商已删除"))?;
            anyhow::ensure!(
                serde_json::to_vec(&current)? == serde_json::to_vec(&p)?,
                "供应商配置已改变，请重新获取模型"
            );
            let result = (|| {
                let mut catalogs = catalog::load(&store)?;
                let models = catalogs.providers.entry(id.clone()).or_default();
                models.refresh(response?, catalog::now())?;
                let result = models.clone();
                catalog::save(&store, catalogs, &id, None)?;
                Ok(result)
            })();
            if let Err(e) = &result {
                catalog::record_error(&store, &id, core::runtime::safe_error(e))?;
            }
            result
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(err);
    emit(&app);
    result
}

#[tauri::command]
pub fn acknowledge_models(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (_guard, _) = state.begin().map_err(err)?;
    catalog::acknowledge(&state.store, &id).map_err(err)?;
    emit(&app);
    Ok(())
}

pub fn schedule(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let mut attempted: HashMap<String, i64> = HashMap::new();
        loop {
            let state = app.state::<AppState>();
            if !state.busy.load(Ordering::SeqCst) {
                if let Ok(providers) = state.store.providers() {
                    attempted.retain(|id, _| providers.iter().any(|p| &p.id == id));
                    for p in providers
                        .into_iter()
                        .filter(|p| matches!(p.mode, AuthMode::Api | AuthMode::Mixed))
                    {
                        if !catalog::refresh_due(attempted.get(&p.id).copied(), catalog::now()) {
                            continue;
                        }
                        if state.busy.load(Ordering::SeqCst) {
                            break;
                        }
                        let result = refresh_provider_models(app.clone(), p.id.clone()).await;
                        if result.is_ok() || !state.busy.load(Ordering::SeqCst) {
                            attempted.insert(p.id, catalog::now());
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}
