use crate::{AuthMode, Provider, Result, Settings, Store, journal::Journal, sessions};
use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf};
use toml_edit::{DocumentMut, Item, value};

const STATE: &str = "model-catalogs.json";
const CATALOG: &str = "easy-switch-models.json";
pub const REFRESH_SECONDS: i64 = 3600;

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn refresh_due(last: Option<i64>, now: i64) -> bool {
    last.is_none_or(|last| now < last || now.saturating_sub(last) >= REFRESH_SECONDS)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelList {
    pub available: Vec<String>,
    pub excluded: Vec<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub checked_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Catalogs {
    pub providers: BTreeMap<String, ModelList>,
    original_catalogs: BTreeMap<String, Option<String>>,
}

pub fn load(store: &Store) -> Result<Catalogs> {
    store.read(STATE, Catalogs::default())
}

pub fn validate_ids(ids: &[String]) -> Result<()> {
    ensure!(ids.len() <= 10_000, "模型列表过大");
    ensure!(
        ids.iter().all(|id| !id.trim().is_empty()
            && id.len() <= 256
            && !id.chars().any(char::is_control)),
        "模型 ID 格式无效"
    );
    Ok(())
}

impl ModelList {
    pub fn refresh(&mut self, mut names: Vec<String>, now: i64) -> Result<()> {
        validate_ids(&names)?;
        ensure!(!names.is_empty(), "供应商返回空模型列表，已保留原列表");
        names.sort();
        names.dedup();
        self.added.retain(|id| names.contains(id));
        self.removed.retain(|id| !names.contains(id));
        self.added.extend(
            names
                .iter()
                .filter(|id| !self.available.contains(id))
                .cloned(),
        );
        self.removed.extend(
            self.available
                .iter()
                .filter(|id| !names.contains(id))
                .cloned(),
        );
        self.added.sort();
        self.added.dedup();
        self.removed.sort();
        self.removed.dedup();
        self.available = names;
        self.checked_at = Some(now);
        self.error = None;
        Ok(())
    }

    pub fn select(&mut self, mut excluded: Vec<String>, default_model: &str) -> Result<()> {
        validate_ids(&excluded)?;
        ensure!(
            !excluded.iter().any(|id| id == default_model),
            "请先在供应商配置中更换默认模型，再移除此模型"
        );
        excluded.sort();
        excluded.dedup();
        self.excluded = excluded;
        Ok(())
    }

    pub fn selected(&self, default_model: &str) -> Vec<String> {
        let mut selected: Vec<_> = self
            .available
            .iter()
            .filter(|id| !self.excluded.contains(id))
            .cloned()
            .collect();
        if !default_model.is_empty() && !selected.iter().any(|id| id == default_model) {
            selected.push(default_model.to_owned());
        }
        selected.sort();
        selected
    }
}

fn read_models(path: &std::path::Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    sessions::no_links(path)?;
    ensure!(
        fs::metadata(path)?.len() <= 64 * 1024 * 1024,
        "本地模型目录过大"
    );
    let v: Value = serde_json::from_slice(&fs::read(path)?).context("本地模型目录 JSON 无效")?;
    Ok(v.get("models")
        .and_then(Value::as_array)
        .context("本地模型目录缺少 models")?
        .clone())
}

pub fn model_document(ids: &[String], templates: &[Value]) -> Value {
    let models: Vec<_> = ids.iter().enumerate().map(|(priority, id)| {
        let mut model = templates.iter().find(|m| m["slug"].as_str() == Some(id)).cloned().unwrap_or_else(|| json!({
            "slug": id, "display_name": id, "description": "供应商模型",
            "base_instructions": "You are a coding assistant. Use the available tools to help the user.",
            "supported_reasoning_levels": [], "shell_type": "unified_exec",
            "support_verbosity": false, "default_verbosity": null,
            "apply_patch_tool_type": "freeform", "default_reasoning_summary": "none",
            "truncation_policy": {"mode": "tokens", "limit": 10000},
            "experimental_supported_tools": [], "input_modalities": ["text"]
        }));
        model["visibility"] = json!("list");
        model["supported_in_api"] = json!(true);
        model["priority"] = json!(priority);
        model
    }).collect();
    json!({"models": models})
}

// Called inside the same journal as provider switching; official mode restores the prior catalog.
pub fn stage_for_provider(
    j: &mut Journal,
    store: &Store,
    p: &Provider,
    settings: &Settings,
    config: &mut String,
) -> Result<()> {
    let mut catalogs = load(store)?;
    let old_state = serde_json::to_vec(&catalogs)?;
    if let Some((path, bytes)) = prepare_catalog(p, settings, config, &mut catalogs)? {
        j.stage(&path, Some(&bytes))?;
    }
    if serde_json::to_vec(&catalogs)? != old_state {
        // Discovery state changes hourly; keeping it in file backups would block later recovery.
        store.save(STATE, &catalogs)?;
    }
    Ok(())
}

fn prepare_catalog(
    p: &Provider,
    settings: &Settings,
    config: &mut String,
    catalogs: &mut Catalogs,
) -> Result<Option<(PathBuf, Vec<u8>)>> {
    let mut doc = config
        .parse::<DocumentMut>()
        .context("Codex 配置语法无效")?;
    ensure!(
        doc.get("profile").is_none(),
        "活动 profile 覆盖了模型配置，请先取消覆盖"
    );
    let path = settings.codex_home.join(CATALOG);
    let managed = path.to_string_lossy().into_owned();
    let home = sessions::normalize(&settings.codex_home)?
        .to_string_lossy()
        .to_lowercase();
    let current = doc
        .get("model_catalog_json")
        .and_then(Item::as_str)
        .map(str::to_owned);
    let uses_managed = if let Some(current) = &current {
        let current = PathBuf::from(current);
        let current = if current.is_absolute() {
            current
        } else {
            settings.codex_home.join(current)
        };
        sessions::normalize(&current)?
            .to_string_lossy()
            .eq_ignore_ascii_case(&sessions::normalize(&path)?.to_string_lossy())
    } else {
        false
    };
    let state = catalogs
        .providers
        .get(&p.id)
        .filter(|s| p.mode != AuthMode::Official && s.checked_at.is_some());
    let mut payload = None;
    if let Some(state) = state {
        if !uses_managed {
            catalogs
                .original_catalogs
                .insert(home.clone(), current.clone());
        }
        let mut templates = Vec::new();
        if let Some(Some(original)) = catalogs.original_catalogs.get(&home) {
            let original = PathBuf::from(original);
            let original = if original.is_absolute() {
                original
            } else {
                settings.codex_home.join(original)
            };
            templates.extend(read_models(&original)?);
        }
        templates.extend(read_models(&settings.codex_home.join("models_cache.json"))?);
        let mut ids = state.selected(&p.model);
        // A model picked directly in Codex may differ from the provider's saved default.
        if let Some(current_model) = doc
            .get("model")
            .and_then(Item::as_str)
            .filter(|s| !s.is_empty())
        {
            if !ids.iter().any(|id| id == current_model) {
                ids.push(current_model.to_owned());
                ids.sort();
            }
        }
        validate_ids(&ids)?;
        ensure!(!ids.is_empty(), "至少保留一个模型");
        payload = Some((
            path,
            serde_json::to_vec_pretty(&model_document(&ids, &templates))?,
        ));
        doc["model_catalog_json"] = value(managed);
    } else if uses_managed {
        match catalogs
            .original_catalogs
            .get(&home)
            .and_then(|p| p.as_ref())
        {
            Some(original) => doc["model_catalog_json"] = value(original),
            None => {
                doc.remove("model_catalog_json");
            }
        }
    }
    *config = doc.to_string();
    Ok(payload)
}

pub fn save(
    store: &Store,
    mut catalogs: Catalogs,
    provider_id: &str,
    default_model: Option<&str>,
) -> Result<()> {
    let settings = store.settings()?;
    if settings.active_provider_id.as_deref() != Some(provider_id) {
        return store.save(STATE, &catalogs);
    }
    let active = store
        .providers()?
        .into_iter()
        .find(|p| Some(&p.id) == settings.active_provider_id.as_ref());
    let cfg = settings.codex_home.join("config.toml");
    sessions::no_links(&cfg)?;
    let before = if cfg.exists() {
        fs::read_to_string(&cfg)?
    } else {
        String::new()
    };
    let mut next = before.clone();
    if let Some(model) = default_model {
        validate_ids(&[model.to_owned()])?;
        let mut doc = next.parse::<DocumentMut>().context("Codex 配置语法无效")?;
        doc["model"] = value(model);
        next = doc.to_string();
    }
    let mut payload = None;
    if let Some(p) = active {
        let doc = before
            .parse::<DocumentMut>()
            .context("Codex 配置语法无效")?;
        ensure!(
            doc.get("model_provider")
                .and_then(Item::as_str)
                .unwrap_or("openai")
                == p.provider_id(),
            "当前供应商已在 Codex 中改变，请先同步配置"
        );
        payload = prepare_catalog(&p, &settings, &mut next, &mut catalogs)?;
        ensure!(
            if cfg.exists() {
                fs::read_to_string(&cfg)?
            } else {
                String::new()
            } == before,
            "模型刷新期间配置被修改"
        );
    }
    let catalog_changed = if let Some((path, bytes)) = &payload {
        sessions::no_links(path)?;
        crate::journal::hash(path)? != Some(crate::journal::hash_bytes(bytes))
    } else {
        false
    };
    if !catalog_changed && next == before {
        store.save(STATE, &catalogs)?;
    } else {
        let mut j = Journal::new(&store.root, "更新 Codex 模型列表")?;
        if let Some((path, bytes)) = payload {
            j.stage(&path, Some(&bytes))?;
        }
        j.stage(&cfg, Some(next.as_bytes()))?;
        if let Some(change) = j.manifest.files.iter().find(|c| c.path == cfg) {
            ensure!(
                change.before_hash == Some(crate::journal::hash_bytes(before.as_bytes())),
                "模型刷新期间配置被修改"
            );
        }
        store.save(STATE, &catalogs)?;
        j.commit()?;
    }
    Ok(())
}

pub fn record_error(store: &Store, id: &str, message: String) -> Result<()> {
    let mut catalogs = load(store)?;
    catalogs.providers.entry(id.into()).or_default().error = Some(message);
    store.save(STATE, &catalogs)
}

pub fn acknowledge(store: &Store, id: &str) -> Result<()> {
    let mut catalogs = load(store)?;
    if let Some(state) = catalogs.providers.get_mut(id) {
        state.added.clear();
        state.removed.clear();
    }
    store.save(STATE, &catalogs)
}
