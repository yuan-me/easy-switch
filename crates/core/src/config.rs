use crate::{Result, Store, crypto, model::*};
use anyhow::{Context, ensure};
use sha2::{Digest, Sha256};
use toml_edit::{DocumentMut, Item, Table, value};

pub fn update(
    text: &str,
    p: &Provider,
    s: &Settings,
    key: Option<&str>,
    old_owned: Option<&str>,
) -> Result<String> {
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|_| anyhow::anyhow!("Codex TOML 配置解析失败；请在本机检查语法"))?;
    ensure!(
        doc.get("profile").is_none(),
        "存在活动 profile，先取消覆盖再切换"
    );
    ensure!(
        doc.get("cli_auth_credentials_store")
            .and_then(Item::as_str)
            .unwrap_or("file")
            == "file",
        "当前凭证后端不受支持；未修改任何数据"
    );
    // Only tables owned by this application may be replaced.
    let saved_headers = if old_owned == Some(p.id.as_str()) {
        doc.get("model_providers")
            .and_then(|v| v.get(&p.id))
            .and_then(|v| v.get("http_headers"))
            .cloned()
    } else {
        None
    };
    if let Some(id) = old_owned.filter(|id| *id != "openai") {
        if let Some(t) = doc
            .get_mut("model_providers")
            .and_then(Item::as_table_like_mut)
        {
            t.remove(id);
        }
    }
    doc["model_provider"] = value(p.provider_id());
    if p.model.is_empty() {
        doc.remove("model");
    } else {
        doc["model"] = value(&p.model);
    }
    for (name, n) in [
        ("model_context_window", p.context_window),
        ("model_auto_compact_token_limit", p.compact_limit),
    ] {
        if let Some(n) = n {
            doc[name] = value(i64::try_from(n)?);
        } else {
            doc.remove(name);
        }
    }
    doc["forced_login_method"] = value(if matches!(p.mode, AuthMode::Official | AuthMode::Mixed) {
        "chatgpt"
    } else {
        "api"
    });
    if p.mode != AuthMode::Official {
        if doc.get("model_providers").is_none() {
            doc["model_providers"] = Item::Table(Table::new());
        }
        let providers = doc["model_providers"]
            .as_table_like_mut()
            .context("model_providers 格式不支持")?;
        ensure!(
            !providers.contains_key(&p.id),
            "Provider ID 与非本软件管理的配置冲突"
        );
        let mut t = Table::new();
        t["name"] = value(&p.name);
        t["base_url"] = value(if p.runtime() {
            format!("http://127.0.0.1:{}/v1", s.runtime_port)
        } else {
            p.base_url.trim_end_matches('/').into()
        });
        t["wire_api"] = value("responses");
        t["requires_openai_auth"] = value(false);
        t["experimental_bearer_token"] = value(key.context("API 凭证缺失")?);
        let mut headers = toml_edit::InlineTable::new();
        if let Some(table) = saved_headers.as_ref().and_then(Item::as_table_like) {
            for (k, v) in table.iter() {
                if let Some(value) = v.as_str() {
                    headers.insert(k, value.into());
                }
            }
        }
        if !p.image_compatibility
            && headers
                .get("x-openai-actor-authorization")
                .and_then(toml_edit::Value::as_str)
                == Some("local-image-extension")
        {
            headers.remove("x-openai-actor-authorization");
        }
        for (k, v) in &p.headers {
            headers.insert(k, v.clone().into());
        }
        if p.image_compatibility {
            headers.insert(
                "x-openai-actor-authorization",
                "local-image-extension".into(),
            );
        }
        if !headers.is_empty() {
            t["http_headers"] = value(headers);
        }
        providers.insert(&p.id, Item::Table(t));
    }
    Ok(doc.to_string())
}
pub fn official_backup_name(home: &std::path::Path) -> String {
    let home = crate::sessions::normalize(home).unwrap_or_else(|_| home.to_owned());
    let text = home
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_uppercase();
    format!("official-auth-{:X}.json", Sha256::digest(text.as_bytes()))
        .replace(".json", "")
        .chars()
        .take(30)
        .collect::<String>()
        + ".json"
}
pub fn next_auth(
    store: &Store,
    s: &Settings,
    p: &Provider,
    current: Option<&[u8]>,
    key: Option<&str>,
) -> Result<Option<Vec<u8>>> {
    let auth: Option<serde_json::Value> = current.map(serde_json::from_slice).transpose()?;
    let name = official_backup_name(&s.codex_home);
    if auth.as_ref().and_then(|a| a.get("tokens")).is_some() {
        store.save(
            &name,
            &crypto::protect(std::str::from_utf8(current.unwrap())?)?,
        )?;
    }
    if matches!(p.mode, AuthMode::Official | AuthMode::Mixed) {
        let mut restored = if auth.as_ref().and_then(|a| a.get("tokens")).is_some() {
            auth
        } else {
            store
                .read::<Option<String>>(&name, None)?
                .map(|v| crypto::unprotect(&v))
                .transpose()?
                .map(|v| serde_json::from_str(&v))
                .transpose()?
        };
        ensure!(
            p.mode != AuthMode::Mixed || restored.is_some(),
            "混入 API 需要先在 Codex 完成官方登录"
        );
        if let Some(a) = restored.as_mut().and_then(|a| a.as_object_mut()) {
            a.remove("OPENAI_API_KEY");
        }
        Ok(restored
            .map(|a| serde_json::to_vec_pretty(&a))
            .transpose()?)
    } else {
        Ok(Some(serde_json::to_vec_pretty(
            &serde_json::json!({"OPENAI_API_KEY":key.context("API 凭证缺失")?}),
        )?))
    }
}
