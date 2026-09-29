use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum AuthMode {
    Official,
    Mixed,
    #[default]
    Api,
    Aggregate,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Protocol {
    #[default]
    Responses,
    ChatCompletions,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Strategy {
    #[default]
    Failover,
    Session,
    RoundRobin,
    Weighted,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    #[serde(alias = "ProviderId")]
    pub provider_id: String,
    #[serde(default = "one", alias = "Weight")]
    pub weight: u32,
    #[serde(default = "yes", alias = "Enabled")]
    pub enabled: bool,
}
fn one() -> u32 {
    1
}
fn yes() -> bool {
    true
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Provider {
    #[serde(alias = "Id")]
    pub id: String,
    #[serde(alias = "Name")]
    pub name: String,
    #[serde(alias = "Mode")]
    pub mode: AuthMode,
    #[serde(alias = "Protocol")]
    pub protocol: Protocol,
    #[serde(alias = "BaseUrl")]
    pub base_url: String,
    #[serde(alias = "Model")]
    pub model: String,
    #[serde(alias = "ContextWindow")]
    pub context_window: Option<u64>,
    #[serde(alias = "CompactLimit")]
    pub compact_limit: Option<u64>,
    #[serde(alias = "ProtectedKey", skip_serializing_if = "Option::is_none")]
    pub protected_key: Option<String>,
    #[serde(alias = "Strategy")]
    pub strategy: Strategy,
    #[serde(alias = "Members")]
    pub members: Vec<Member>,
    pub image_compatibility: bool,
    pub headers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protected_headers: Option<String>,
}
impl Default for Provider {
    fn default() -> Self {
        Self {
            id: format!("p_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
            name: String::new(),
            mode: AuthMode::Api,
            protocol: Protocol::Responses,
            base_url: String::new(),
            model: String::new(),
            context_window: None,
            compact_limit: None,
            protected_key: None,
            strategy: Strategy::Failover,
            members: vec![],
            image_compatibility: false,
            headers: BTreeMap::new(),
            protected_headers: None,
        }
    }
}
impl Provider {
    pub fn official() -> Self {
        Self {
            id: "official".into(),
            name: "OpenAI 官方".into(),
            mode: AuthMode::Official,
            ..Self::default()
        }
    }
    pub fn provider_id(&self) -> &str {
        if self.mode == AuthMode::Official {
            "openai"
        } else {
            &self.id
        }
    }
    pub fn runtime(&self) -> bool {
        self.mode == AuthMode::Aggregate || self.protocol == Protocol::ChatCompletions
    }
    pub fn validate(&self, all: &[Provider]) -> Result<()> {
        ensure!(!self.name.trim().is_empty(), "请填写供应商名称");
        ensure!(
            self.id.len() <= 64
                && self.id.starts_with(|c: char| c.is_ascii_alphabetic())
                && self
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "Provider ID 格式无效"
        );
        if self.mode != AuthMode::Official {
            ensure!(!self.model.trim().is_empty(), "请填写实际模型 ID");
        }
        if matches!(self.mode, AuthMode::Api | AuthMode::Mixed) {
            validate_url(&self.base_url)?;
            ensure!(self.protected_key.is_some(), "请填写 API Key");
        }
        ensure!(
            self.context_window != Some(0) && self.compact_limit != Some(0),
            "上下文窗口和压缩阈值须为正数"
        );
        if let (Some(c), Some(l)) = (self.context_window, self.compact_limit) {
            ensure!(l < c, "压缩阈值须小于上下文窗口");
        }
        ensure!(
            !self.image_compatibility || self.protocol == Protocol::Responses,
            "图片兼容要求 Responses 协议"
        );
        for (k, v) in &self.headers {
            ensure!(
                !k.eq_ignore_ascii_case("authorization")
                    && !k.eq_ignore_ascii_case("host")
                    && !k.eq_ignore_ascii_case("content-length"),
                "请使用密钥字段设置认证；不能覆盖保留请求头"
            );
            reqwest::header::HeaderName::from_bytes(k.as_bytes())?;
            reqwest::header::HeaderValue::from_str(v)?;
        }
        if self.mode == AuthMode::Aggregate {
            ensure!(
                self.members.iter().any(|m| m.enabled),
                "至少启用一个路由成员"
            );
            let mut seen = std::collections::HashSet::new();
            for m in &self.members {
                ensure!(seen.insert(&m.provider_id), "路由成员重复");
                ensure!((1..=100).contains(&m.weight), "权重须在 1–100 之间");
                ensure!(
                    all.iter().any(|p| p.id == m.provider_id
                        && matches!(p.mode, AuthMode::Api | AuthMode::Mixed)),
                    "成员必须是普通 API 供应商"
                );
            }
        }
        Ok(())
    }
}
pub fn validate_url(s: &str) -> Result<url::Url> {
    let u = url::Url::parse(s)?;
    ensure!(
        matches!(u.scheme(), "http" | "https")
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none(),
        "API 地址必须为 HTTP/HTTPS，不含账号、查询或片段"
    );
    Ok(u)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    #[serde(alias = "CodexHome")]
    pub codex_home: PathBuf,
    #[serde(alias = "SqliteHome")]
    pub sqlite_home: Option<PathBuf>,
    #[serde(alias = "DesktopExecutable")]
    pub desktop_executable: String,
    #[serde(alias = "DesktopAppId")]
    pub desktop_app_id: Option<String>,
    #[serde(alias = "ActiveProviderId")]
    pub active_provider_id: Option<String>,
    #[serde(alias = "RuntimePort")]
    pub runtime_port: u16,
    #[serde(alias = "RuntimeKey")]
    pub runtime_key: Option<String>,
    #[serde(alias = "ScrollPositions")]
    pub scroll_positions: BTreeMap<String, f64>,
    #[serde(alias = "EnablePageRecovery")]
    pub enable_page_recovery: bool,
    pub theme: String,
    pub automatic_updates: bool,
    pub automatic_download: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            codex_home: std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default())
                        .join(".codex")
                }),
            sqlite_home: std::env::var_os("CODEX_SQLITE_HOME").map(PathBuf::from),
            desktop_executable: String::new(),
            desktop_app_id: None,
            active_provider_id: None,
            runtime_port: 47831,
            runtime_key: None,
            scroll_positions: BTreeMap::new(),
            enable_page_recovery: false,
            theme: "system".into(),
            automatic_updates: true,
            automatic_download: true,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub name: String,
    pub state: String,
    pub detail: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: String,
    pub detail: String,
}
