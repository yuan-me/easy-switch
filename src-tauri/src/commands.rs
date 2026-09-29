use anyhow::{Context, ensure};
use easy_switch_core::{
    self as core,
    desktop::{Desktop, DesktopHost},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, State};
use tokio_util::sync::CancellationToken;

pub struct AppState {
    pub store: Store,
    pub busy: Arc<AtomicBool>,
    pub cancel: Mutex<CancellationToken>,
    scanner: Arc<Mutex<core::sessions::Scanner>>,
    migration_error: Option<String>,
}
impl AppState {
    pub fn new(store: Store, migration_error: Option<String>) -> Self {
        Self {
            store,
            busy: Arc::new(AtomicBool::new(false)),
            cancel: Mutex::new(CancellationToken::new()),
            scanner: Arc::new(Mutex::new(Default::default())),
            migration_error,
        }
    }
    pub fn begin(&self) -> Result<(Busy, CancellationToken)> {
        ensure!(
            self.migration_error.is_none(),
            "旧数据导入未完成，请先处理导入错误"
        );
        self.begin_recovery()
    }
    pub fn begin_recovery(&self) -> Result<(Busy, CancellationToken)> {
        ensure!(
            self.busy
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "另一个操作正在进行"
        );
        let ct = CancellationToken::new();
        *self.cancel.lock().unwrap() = ct.clone();
        Ok((Busy(self.busy.clone()), ct))
    }
}
pub struct Busy(Arc<AtomicBool>);
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
type Reply<T> = std::result::Result<T, String>;
pub fn err(e: anyhow::Error) -> String {
    core::runtime::safe_error(&e)
}
fn public_provider(mut p: Provider) -> Value {
    let key = p.protected_key.take().is_some();
    p.protected_headers = None;
    for value in p.headers.values_mut() {
        value.clear();
    }
    let mut v = serde_json::to_value(p).unwrap();
    v["hasKey"] = json!(key);
    v
}
fn public_settings(mut s: Settings) -> Value {
    s.runtime_key = None;
    serde_json::to_value(s).unwrap()
}
#[tauri::command]
pub fn bootstrap(app: tauri::AppHandle, state: State<AppState>) -> Reply<Value> {
    ( ||->Result<_>{let providers=state.store.providers()?.into_iter().map(public_provider).collect::<Vec<_>>();Ok(json!({"providers":providers,"settings":public_settings(state.store.settings()?),"version":app.package_info().version.to_string(),"migrationError":state.migration_error,"busy":state.busy.load(Ordering::SeqCst)}))})().map_err(err)
}
#[tauri::command]
pub fn save_provider(state: State<AppState>, provider: Provider, key: Option<String>) -> Reply<()> {
    let (_guard, _) = state.begin().map_err(err)?;
    state.store.save_provider(provider, key).map_err(err)
}
#[tauri::command]
pub fn delete_provider(state: State<AppState>, id: String) -> Reply<()> {
    let (_guard, _) = state.begin().map_err(err)?;
    state.store.delete_provider(&id).map_err(err)
}
#[tauri::command]
pub fn save_settings(state: State<AppState>, mut settings: Settings) -> Reply<()> {
    let (_guard, _) = state.begin().map_err(err)?;
    (|| -> Result<_> {
        ensure!(
            matches!(settings.theme.as_str(), "system" | "light" | "dark"),
            "主题选项无效"
        );
        ensure!(settings.runtime_port >= 1024, "代理端口须至少为 1024");
        core::sessions::normalize(&settings.codex_home)?;
        core::sessions::no_links(&settings.codex_home)?;
        if let Some(p) = &settings.sqlite_home {
            core::sessions::normalize(p)?;
            core::sessions::no_links(p)?;
        }
        let old = state.store.settings()?;
        ensure!(
            settings.codex_home == old.codex_home || settings.codex_home.is_dir(),
            "Codex 数据目录不存在"
        );
        settings.runtime_key = old.runtime_key;
        settings.active_provider_id = old.active_provider_id;
        settings.scroll_positions = old.scroll_positions;
        settings.enable_page_recovery = false;
        // Do not move an active Runtime out from under Codex.
        if settings.runtime_port != old.runtime_port {
            ensure!(Desktop::writers().is_empty(), "修改代理端口前请退出 Codex");
            core::runtime::stop_blocking(&state.store)?;
        }
        state.store.save("settings.json", &settings)
    })()
    .map_err(err)
}
#[tauri::command]
pub async fn discover() -> Reply<Vec<core::desktop::Installation>> {
    tauri::async_runtime::spawn_blocking(core::desktop::discover)
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}
#[tauri::command]
pub async fn switch_provider(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Reply<String> {
    let (guard, ct) = state.begin().map_err(err)?;
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let p = store
            .providers()?
            .into_iter()
            .find(|p| p.id == id)
            .context("供应商不存在")?;
        let host = Desktop::resolve(store.settings()?)?;
        core::service::switch(&store, &p, &host, &ct, |event| {
            let _ = app.emit("operation-progress", event);
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}
#[tauri::command]
pub fn cancel_operation(state: State<AppState>) {
    state.cancel.lock().unwrap().cancel();
}
#[tauri::command]
pub async fn scan_sessions(state: State<'_, AppState>) -> Reply<core::sessions::Scan> {
    let settings = state.store.settings().map_err(err)?;
    let scanner = state.scanner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        scanner
            .lock()
            .unwrap()
            .scan(&settings, &CancellationToken::new())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}
fn selected(
    store: &Store,
    scanner: &mut core::sessions::Scanner,
    ids: &[String],
) -> Result<Vec<core::sessions::Session>> {
    let result = scanner.scan(&store.settings()?, &CancellationToken::new())?;
    let wanted: HashSet<_> = ids.iter().collect();
    let selected: Vec<_> = result
        .sessions
        .into_iter()
        .filter(|s| wanted.contains(&s.id))
        .collect();
    ensure!(selected.len() == ids.len(), "会话已消失或选择重复，请刷新");
    Ok(selected)
}
#[tauri::command]
pub async fn session_detail(
    state: State<'_, AppState>,
    id: String,
) -> Reply<core::sessions::Detail> {
    let store = state.store.clone();
    let scanner = state.scanner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let rows = selected(&store, &mut scanner.lock().unwrap(), &[id])?;
        core::sessions::detail(&rows[0], 500)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}
#[tauri::command]
pub async fn batch_sessions(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    action: String,
    value: Option<String>,
) -> Reply<String> {
    let (guard, ct) = state.begin().map_err(err)?;
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let host = Desktop::resolve(store.settings()?)?;
        core::service::batch(
            &store,
            &ids,
            &action,
            value.as_deref(),
            &host,
            &ct,
            |event| {
                let _ = app.emit("operation-progress", event);
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}
#[tauri::command]
pub async fn export_sessions(
    state: State<'_, AppState>,
    ids: Vec<String>,
    folder: PathBuf,
) -> Reply<Value> {
    let (guard, ct) = state.begin().map_err(err)?;
    let store = state.store.clone();
    let scanner = state.scanner.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let _guard = guard;
        core::sessions::normalize(&folder)?;
        let rows = selected(&store, &mut scanner.lock().unwrap(), &ids)?;
        let mut results = vec![];
        for s in rows {
            if ct.is_cancelled() {
                break;
            }
            match core::sessions::export(&s, &folder) {
                Ok(p) => results.push(json!({"id":s.id,"path":p,"state":"passed"})),
                Err(e) => results.push(json!({"id":s.id,"state":"failed","error":e.to_string()})),
            }
        }
        Ok(json!(results))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}
#[tauri::command]
pub fn open_thread(id: String) -> Reply<()> {
    core::desktop::open_thread(&id).map_err(err)
}
#[tauri::command]
pub async fn doctor(
    state: State<'_, AppState>,
    id: String,
    network: bool,
) -> Reply<Vec<Diagnostic>> {
    let (_guard, ct) = state.begin().map_err(err)?;
    let all = state.store.providers().map_err(err)?;
    let p = all.iter().find(|p| p.id == id).ok_or("供应商不存在")?;
    core::diagnostics::doctor(p, &all, network, &ct)
        .await
        .map_err(err)
}
#[tauri::command]
pub async fn fetch_models(
    state: State<'_, AppState>,
    mut provider: Provider,
    key: Option<String>,
) -> Reply<Vec<String>> {
    let (_guard, ct) = state.begin().map_err(err)?;
    let all = state.store.providers().map_err(err)?;
    let old = all.iter().find(|p| p.id == provider.id);
    if let Some(old) = old {
        for (name, value) in &mut provider.headers {
            if value.is_empty() {
                if let Some(v) = old.headers.get(name) {
                    *value = v.clone();
                }
            }
        }
    }
    provider.protected_key = if let Some(k) = key.filter(|s| !s.is_empty()) {
        Some(core::crypto::protect(&k).map_err(err)?)
    } else {
        old.and_then(|p| p.protected_key.clone())
    };
    core::diagnostics::models(&provider, &ct).await.map_err(err)
}
#[tauri::command]
pub fn list_operations(state: State<AppState>) -> Reply<Vec<core::journal::OperationInfo>> {
    (|| -> Result<_> {
        let mut ops = core::journal::list(&state.store.root.join("operations"), false)?;
        if let Some(root) = state
            .store
            .read::<Option<PathBuf>>("legacy-store.json", None)?
        {
            ops.extend(core::journal::list(&root.join("operations"), true)?);
        }
        Ok(ops)
    })()
    .map_err(err)
}
#[tauri::command]
pub async fn restore_operation(
    state: State<'_, AppState>,
    id: String,
    legacy: bool,
) -> Reply<String> {
    let (guard, _) = state.begin_recovery().map_err(err)?;
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let _guard = guard;
        if !legacy {
            ensure!(
                !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "备份 ID 无效"
            );
            let manifest: core::journal::Manifest = serde_json::from_slice(&std::fs::read(
                store
                    .root
                    .join("operations")
                    .join(&id)
                    .join("manifest.json"),
            )?)?;
            if manifest
                .files
                .iter()
                .all(|f| core::sessions::within(&f.path, &store.root).unwrap_or(false))
            {
                core::journal::restore(&store.root, &id, &[store.root.clone()])?;
                return Ok("本地数据已恢复。请重新启动 Easy Switch 以重新检查导入状态。".into());
            }
        }
        let settings = store.settings()?;
        let host = Desktop::resolve(settings.clone())?;
        host.stop()?;
        core::runtime::stop_blocking(&store)?;
        let root = if legacy {
            store
                .read::<Option<PathBuf>>("legacy-store.json", None)?
                .context("未找到旧版备份目录")?
        } else {
            store.root.clone()
        };
        let mut allowed = vec![
            store.root.clone(),
            root.clone(),
            settings.codex_home.clone(),
        ];
        if let Some(sqlite) = settings.sqlite_home {
            allowed.push(sqlite);
        }
        core::journal::restore(&root, &id, &allowed)?;
        store.synchronize_active()?;
        let active = store.settings()?.active_provider_id;
        if store
            .providers()?
            .iter()
            .any(|p| Some(&p.id) == active.as_ref() && p.runtime())
        {
            core::runtime::ensure_started_blocking(&store)?;
        }
        host.start()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}
#[tauri::command]
pub fn save_scroll(state: State<AppState>, id: String, position: f64) -> Reply<()> {
    let Ok((_guard, _)) = state.begin() else {
        return Ok(());
    };
    if !position.is_finite() || position < 0.0 {
        return Err("位置无效".into());
    }
    let mut settings = state.store.settings().map_err(err)?;
    settings.scroll_positions.insert(id, position);
    state.store.save("settings.json", &settings).map_err(err)
}
