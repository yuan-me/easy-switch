use anyhow::{Context, Result, ensure};
use easy_switch_core::{Progress, Settings, history, journal, sessions};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;
use toml_edit::{DocumentMut, Item, Table, value};

pub mod guard;

pub const PROTOCOL_VERSION: u32 = 1;
const PROVIDER: &str = "easy_switch_vscode";
const OWNER_FILE: &str = ".easy-switch-vscode-owner.json";
const OFFICIAL_KEYS: &[&str] = &[
    "model",
    "model_context_window",
    "model_auto_compact_token_limit",
    "model_catalog_json",
];

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol_version: u32,
    pub action: String,
    pub home: PathBuf,
    pub sqlite_home: Option<PathBuf>,
    pub store_root: PathBuf,
    pub provider: Option<Provider>,
    pub api_key: Option<String>,
    pub official_model: Option<String>,
    pub journal_key: Option<String>,
    pub operation_id: Option<String>,
    pub expected_config_fingerprint: Option<String>,
    #[serde(default)]
    pub project_dirs: Vec<PathBuf>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provider {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct State {
    home: PathBuf,
    sqlite_home: Option<PathBuf>,
    active_provider_id: Option<String>,
    routing_fingerprint: Option<String>,
    official_config: Option<String>,
    official_model: Option<String>,
    owns_provider: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Target {
    home: PathBuf,
    sqlite_home: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HomeOwner {
    version: u32,
    home: PathBuf,
    store_root: PathBuf,
}

fn canonical_directory(path: &Path) -> Result<PathBuf> {
    sessions::no_links(path)?;
    let canonical = sessions::normalize(
        &path
            .canonicalize()
            .context("无法确认切换归属记录中的目录")?,
    )?;
    ensure!(canonical.is_dir(), "切换归属记录中的路径不是目录");
    sessions::no_links(&canonical)?;
    Ok(canonical)
}

fn check_owner(req: &Request) -> Result<bool> {
    let home = canonical_directory(&req.home)?;
    let path = home.join(OWNER_FILE);
    sessions::no_links(&path)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) => ensure!(metadata.is_file(), "Codex 目录切换归属记录不是普通文件"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => anyhow::bail!("无法核实 Codex 目录切换归属记录，请保留现有数据并检查权限"),
    }
    let owner: HomeOwner =
        serde_json::from_slice(&fs::read(&path).context("无法读取 Codex 目录切换归属记录")?)
            .map_err(|_| anyhow::anyhow!("Codex 目录切换归属记录损坏，请保留现有数据并检查"))?;
    ensure!(owner.version == 1, "不支持此版本的 Codex 目录切换归属记录");
    ensure!(
        sessions::same_path(&owner.home, &home)?
            && sessions::same_path(&owner.store_root, &req.store_root)?,
        "此 Codex 目录已绑定另一 VS Code 存储环境，请返回原配置档及服务器数据目录执行切换或恢复；未转移恢复密钥"
    );
    ensure!(
        sessions::same_path(&canonical_directory(&req.store_root)?, &owner.store_root)?,
        "插件存储目录与 Codex 目录切换归属记录不一致"
    );
    Ok(true)
}

fn claim_owner(req: &Request) -> Result<()> {
    if check_owner(req)? {
        return Ok(());
    }
    let home = canonical_directory(&req.home)?;
    let owner = HomeOwner {
        version: 1,
        home: home.clone(),
        store_root: canonical_directory(&req.store_root)?,
    };
    static TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temp = home.join(format!(
        ".easy-switch-owner-{}-{nonce}-{}.tmp",
        std::process::id(),
        TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let path = home.join(OWNER_FILE);
    let result = (|| -> Result<()> {
        easy_switch_core::store::atomic_write(&temp, &serde_json::to_vec(&owner)?)?;
        sessions::no_links(&path)?;
        // Linking the complete private file claims the name without replacing another owner.
        match fs::hard_link(&temp, &path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e).context("无法原子绑定 Codex 目录；文件系统必须支持硬链接"),
        }
        #[cfg(unix)]
        fs::File::open(&home)?.sync_all()?;
        ensure!(check_owner(req)?, "Codex 目录切换归属记录未保存");
        Ok(())
    })();
    let cleanup = fs::remove_file(&temp);
    result?;
    cleanup.context("无法清理切换归属临时文件")?;
    Ok(())
}

fn resolve_request(req: &Request) -> Result<Request> {
    let mut initial = req.clone();
    initial.sqlite_home = None;
    validate_paths(&initial)?;
    check_owner(&initial)?;
    let mut resolved = req.clone();
    let pending = journal::list(&req.store_root.join("operations"), false)?;
    if pending.iter().any(|op| !journal::finished(&op.state)) {
        let path = req.store_root.join("target.json");
        sessions::no_links(&path)?;
        let target: Target = serde_json::from_slice(
            &fs::read(path).context("中断操作缺少目录绑定，请保留备份并检查")?,
        )
        .map_err(|_| anyhow::anyhow!("中断操作目录绑定损坏"))?;
        ensure!(
            target.home == req.home,
            "当前环境与中断操作的数据目录不同，请恢复原环境后重试"
        );
        resolved.sqlite_home = target.sqlite_home;
    } else {
        let (_, doc) = read_config(&req.home)?;
        let system = system_config()?;
        let selected = doc
            .get("sqlite_home")
            .map(|v| (v, req.home.as_path()))
            .or_else(|| {
                system
                    .get("sqlite_home")
                    .map(|v| (v, Path::new("/etc/codex")))
            });
        if let Some((item, base)) = selected {
            let configured = Path::new(item.as_str().context("sqlite_home 必须是路径字符串")?);
            let path = if configured.is_absolute() {
                configured.to_owned()
            } else {
                base.join(configured)
            };
            sessions::no_links(&path)?;
            resolved.sqlite_home = Some(sessions::normalize(
                &path
                    .canonicalize()
                    .context("配置中的 SQLite 目录不存在或不可访问")?,
            )?);
        }
    }
    validate_paths(&resolved)?;
    Ok(resolved)
}

fn system_config() -> Result<DocumentMut> {
    if !cfg!(target_os = "linux") {
        return Ok(DocumentMut::new());
    }
    let read = |path: &str| -> Result<DocumentMut> {
        match fs::read_to_string(path) {
            Ok(text) => text
                .parse()
                .map_err(|_| anyhow::anyhow!("系统 Codex 配置格式无效：{path}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
            Err(_) => anyhow::bail!("无法核实系统 Codex 配置：{path}"),
        }
    };
    for path in [
        "/etc/codex/requirements.toml",
        "/etc/codex/managed_config.toml",
    ] {
        let doc = read(path)?;
        ensure!(
            ![
                "model_provider",
                "model_providers",
                "model",
                "models",
                "sqlite_home",
                "model_catalog_json",
                "forced_login_method",
                "forced_chatgpt_workspace_id",
                "allowed_login_methods",
                "allowed_chatgpt_workspaces"
            ]
            .iter()
            .any(|k| doc.contains_key(k)),
            "管理员配置限制服务商、模型或数据目录，暂不支持自动切换：{path}"
        );
    }
    let doc = read("/etc/codex/config.toml")?;
    ensure!(
        !doc.contains_key("profile")
            && doc.get("forced_login_method").and_then(Item::as_str) != Some("api")
            && doc
                .get("model_providers")
                .and_then(|v| v.get(PROVIDER))
                .is_none(),
        "系统级 Codex 配置影响官方登录或插件服务商，请先检查 /etc/codex/config.toml"
    );
    Ok(doc)
}

fn read_config(home: &Path) -> Result<(String, DocumentMut)> {
    let path = home.join("config.toml");
    sessions::no_links(&path)?;
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => anyhow::bail!("无法读取 Codex 配置，请检查服务器文件权限"),
    };
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|_| anyhow::anyhow!("Codex TOML 配置语法无效；未修改数据"))?;
    Ok((text, doc))
}

fn fingerprint(doc: &DocumentMut) -> String {
    let mut route = DocumentMut::new();
    for key in ["model_provider", "model", "forced_login_method"] {
        if let Some(v) = doc.get(key) {
            route[key] = v.clone();
        }
    }
    if let Some(v) = doc.get("model_providers").and_then(|v| v.get(PROVIDER)) {
        route["provider"] = v.clone();
    }
    journal::hash_bytes(route.to_string().as_bytes())
}

fn read_state(req: &Request) -> Result<State> {
    let path = req.store_root.join("switch-state.json");
    sessions::no_links(&path)?;
    if !path.exists() {
        return Ok(State {
            home: req.home.clone(),
            sqlite_home: req.sqlite_home.clone(),
            ..State::default()
        });
    }
    let state: State = serde_json::from_slice(&fs::read(path)?)
        .map_err(|_| anyhow::anyhow!("切换状态文件无效，请先恢复；未修改数据"))?;
    ensure!(
        state.home == req.home && state.sqlite_home == req.sqlite_home,
        "切换状态与目标 Codex 目录不一致"
    );
    Ok(state)
}

fn active_provider(doc: &DocumentMut, state: &State) -> Option<String> {
    match doc
        .get("model_provider")
        .and_then(Item::as_str)
        .unwrap_or("openai")
    {
        "openai" => Some("official".into()),
        PROVIDER if state.routing_fingerprint.as_deref() == Some(fingerprint(doc).as_str()) => {
            state.active_provider_id.clone()
        }
        _ => None,
    }
}

fn official_model<'a>(doc: &'a DocumentMut, state: &'a State) -> Option<&'a str> {
    if doc
        .get("model_provider")
        .and_then(Item::as_str)
        .unwrap_or("openai")
        == "openai"
    {
        doc.get("model")
            .and_then(Item::as_str)
            .or(state.official_model.as_deref())
    } else {
        state.official_model.as_deref()
    }
}

fn validate_paths(req: &Request) -> Result<()> {
    ensure!(
        req.protocol_version == PROTOCOL_VERSION,
        "扩展与切换工具协议版本不匹配"
    );
    ensure!(
        matches!(req.action.as_str(), "inspect" | "switch" | "recover"),
        "不支持的操作"
    );
    for path in [&req.home, &req.store_root]
        .into_iter()
        .chain(req.sqlite_home.iter())
    {
        ensure!(
            sessions::normalize(path)? == *path,
            "目标目录必须是规范绝对路径"
        );
        sessions::no_links(path)?;
        ensure!(path.parent().is_some(), "不能使用文件系统根目录");
    }
    ensure!(
        req.home.is_dir(),
        "Codex 数据目录不存在；请先在服务器上的官方插件中完成初始化"
    );
    ensure!(
        !sessions::within(&req.store_root, &req.home)?
            && !sessions::within(&req.home, &req.store_root)?,
        "插件操作目录必须与 Codex 数据目录分离"
    );
    if let Some(sqlite) = &req.sqlite_home {
        ensure!(sqlite.is_dir(), "指定的数据库目录不存在");
        ensure!(
            !sessions::within(&req.store_root, sqlite)?
                && !sessions::within(sqlite, &req.store_root)?,
            "插件操作目录必须与数据库目录分离"
        );
    }
    Ok(())
}

fn check_projects(home: &Path, dirs: impl IntoIterator<Item = PathBuf>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for dir in dirs {
        if !dir.is_absolute() {
            continue;
        }
        for parent in dir.ancestors() {
            let path = parent.join(".codex/config.toml");
            if path == home.join("config.toml") || !seen.insert(path.clone()) {
                continue;
            }
            sessions::no_links(&path)?;
            let text = match fs::read_to_string(&path) {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => anyhow::bail!("无法检查项目配置：{}", path.display()),
            };
            let doc = text
                .parse::<DocumentMut>()
                .map_err(|_| anyhow::anyhow!("项目 TOML 配置语法无效：{}", path.display()))?;
            ensure!(
                ![
                    "model_provider",
                    "model",
                    "profile",
                    "model_providers",
                    "forced_login_method",
                    "model_catalog_json",
                    "sqlite_home"
                ]
                .iter()
                .any(|key| doc.contains_key(key)),
                "项目配置覆盖服务商、模型或数据库，请先处理：{}",
                path.display()
            );
        }
    }
    Ok(())
}

fn build_config(req: &Request, mut doc: DocumentMut, state: &mut State) -> Result<DocumentMut> {
    let provider = req.provider.as_ref().context("缺少目标服务商")?;
    state.official_model = official_model(&doc, state)
        .map(str::to_owned)
        .or_else(|| req.official_model.as_deref().map(|s| s.trim().to_owned()));
    ensure!(
        !doc.contains_key("profile"),
        "存在活动 profile，请先取消服务商覆盖"
    );
    let current = doc
        .get("model_provider")
        .and_then(Item::as_str)
        .unwrap_or("openai");
    if current == "openai" {
        let mut official = DocumentMut::new();
        for key in OFFICIAL_KEYS {
            if let Some(v) = doc.get(key) {
                official.insert_formatted(doc.key(key).unwrap(), v.clone());
            }
        }
        state.official_config = Some(official.to_string());
    }
    let exists = doc
        .get("model_providers")
        .and_then(|v| v.get(PROVIDER))
        .is_some();
    ensure!(
        !exists || state.owns_provider,
        "插件服务商标识与已有配置冲突，未覆盖"
    );
    if provider.id == "official" {
        ensure!(
            doc.get("forced_login_method").and_then(Item::as_str) != Some("api"),
            "配置限定仅 API 登录；请先在官方 Codex 设置中解除限制并登录 ChatGPT"
        );
        doc["model_provider"] = value("openai");
        let official = state
            .official_config
            .as_deref()
            .unwrap_or("")
            .parse::<DocumentMut>()
            .map_err(|_| anyhow::anyhow!("保存的官方模型配置损坏"))?;
        for key in OFFICIAL_KEYS {
            doc.remove(key);
            if let Some(v) = official.get(key) {
                doc.insert_formatted(official.key(key).unwrap(), v.clone());
            }
        }
        if state.owns_provider {
            if let Some(table) = doc
                .get_mut("model_providers")
                .and_then(Item::as_table_like_mut)
            {
                table.remove(PROVIDER);
            }
        }
    } else {
        ensure!(
            !provider.id.is_empty()
                && provider.id.len() <= 100
                && provider
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "服务商 ID 无效"
        );
        ensure!(
            !provider.name.trim().is_empty() && provider.name.len() <= 200,
            "服务商名称无效"
        );
        ensure!(
            !provider.model.trim().is_empty()
                && provider.model.len() <= 200
                && !provider.model.chars().any(char::is_control),
            "请填写有效的模型 ID"
        );
        let url = easy_switch_core::model::validate_url(&provider.base_url)?;
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "API 地址不能包含用户名、密码、查询参数或片段"
        );
        let key = req
            .api_key
            .as_deref()
            .context("请重新输入此服务器上的 API 密钥")?;
        ensure!(
            !key.trim().is_empty() && key.len() <= 16_384 && !key.chars().any(char::is_control),
            "API 密钥为空或格式无效"
        );
        for name in OFFICIAL_KEYS {
            if *name != "model" {
                doc.remove(name);
            }
        }
        doc["model_provider"] = value(PROVIDER);
        let model_decor = doc
            .get("model")
            .and_then(Item::as_value)
            .map(|v| v.decor().clone());
        doc["model"] = value(provider.model.trim());
        if let Some(decor) = model_decor {
            *doc["model"].as_value_mut().unwrap().decor_mut() = decor;
        }
        if doc.get("model_providers").is_none() {
            doc["model_providers"] = Item::Table(Table::new());
        }
        let tables = doc["model_providers"]
            .as_table_like_mut()
            .context("model_providers 格式无效")?;
        let mut table = Table::new();
        table["name"] = value(provider.name.trim());
        table["base_url"] = value(provider.base_url.trim().trim_end_matches('/'));
        table["wire_api"] = value("responses");
        table["requires_openai_auth"] = value(false);
        table["experimental_bearer_token"] = value(key);
        tables.insert(PROVIDER, Item::Table(table));
        state.owns_provider = true;
    }
    let official = state
        .official_model
        .as_deref()
        .context("请先确认切回官方时使用的模型 ID")?;
    ensure!(
        !official.trim().is_empty()
            && official.len() <= 200
            && !official.chars().any(char::is_control),
        "官方模型 ID 无效"
    );
    state.active_provider_id = Some(provider.id.clone());
    state.routing_fingerprint = Some(fingerprint(&doc));
    Ok(doc)
}

fn settings(req: &Request) -> Settings {
    Settings {
        codex_home: req.home.clone(),
        sqlite_home: req.sqlite_home.clone(),
        ..Settings::default()
    }
}

fn check_current_index(req: &Request, items: &[sessions::Session]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    // The supported Codex build reads this database and can rebuild projections from JSONL.
    let path = req
        .sqlite_home
        .as_ref()
        .unwrap_or(&req.home)
        .join("state_5.sqlite");
    sessions::no_links(&path)?;
    let db =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .context("旧聊天缺少当前版本索引，请先在官方 Codex 中完成索引初始化")?;
    let status: String = db
        .query_row("SELECT status FROM backfill_state WHERE id=1", [], |r| {
            r.get(0)
        })
        .context("无法确认当前版本的索引状态，请先在官方 Codex 中完成初始化")?;
    ensure!(
        status == "complete",
        "Codex 历史索引尚未完成，请等待官方 Codex 完成初始化后重试"
    );
    let mut count = db.prepare("SELECT COUNT(*) FROM threads WHERE id=?1")?;
    let mut row = db
        .prepare("SELECT rollout_path,archived,model,model_provider FROM threads WHERE id=?1")
        .context("当前索引缺少模型字段，请先使用受支持的官方 Codex 更新索引")?;
    for item in items {
        ensure!(
            count.query_row([&item.id], |r| r.get::<_, i64>(0))? == 1,
            "旧聊天 {} 缺少唯一的当前索引，请先在官方 Codex 中打开后重试",
            item.id
        );
        let (rollout, archived, _, _): (String, i64, Option<String>, Option<String>) = row
            .query_row([&item.id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?;
        ensure!(
            sessions::same_path(Path::new(&rollout), &item.path)?
                && (archived != 0) == item.archived,
            "当前版本的线程索引与历史文件不一致：{}",
            item.id
        );
    }
    Ok(())
}

pub fn execute(req: &Request, progress: &impl Fn(Progress)) -> Result<Value> {
    let req = resolve_request(req)?;
    match req.action.as_str() {
        "inspect" => inspect(&req),
        "switch" => switch(&req, progress, &|| {
            guard::ensure_stopped(&req.home, req.sqlite_home.as_deref())
        }),
        "recover" => recover(&req, &|| {
            guard::ensure_stopped(&req.home, req.sqlite_home.as_deref())
        }),
        _ => unreachable!(),
    }
}

fn inspect(req: &Request) -> Result<Value> {
    let operations = journal::list(&req.store_root.join("operations"), false)?;
    if operations.iter().any(|op| !journal::finished(&op.state)) {
        // A partial commit may contain mismatched indexes or TOML; recovery must remain reachable.
        return Ok(
            json!({"protocolVersion":PROTOCOL_VERSION,"home":req.home,"sqliteHome":req.sqlite_home,
            "activeProviderId":null,"operations":operations,"sessionCount":null,"configFingerprint":null}),
        );
    }
    let (text, doc) = read_config(&req.home)?;
    let state = read_state(req)?;
    let mut planned_state = read_state(req)?;
    check_projects(&req.home, req.project_dirs.clone())?;
    let planned = if req.provider.is_some() {
        Some(build_config(req, doc.clone(), &mut planned_state)?)
    } else {
        None
    };
    let scan = sessions::Scanner::default().scan(&settings(req), &CancellationToken::new())?;
    ensure!(
        scan.warnings.is_empty(),
        "会话扫描存在异常：{}",
        scan.warnings.join("；")
    );
    if req.provider.is_some() {
        check_current_index(req, &scan.sessions)?;
        sessions::validate_provider_model_history(
            &scan.sessions,
            planned
                .as_ref()
                .and_then(|d| d.get("model"))
                .and_then(Item::as_str)
                .or(planned_state.official_model.as_deref()),
        )?;
        check_projects(
            &req.home,
            scan.sessions.iter().map(|s| PathBuf::from(&s.cwd)),
        )?;
    }
    Ok(
        json!({"protocolVersion":PROTOCOL_VERSION,"home":req.home,"sqliteHome":req.sqlite_home,
        "activeProviderId":active_provider(&doc,&state),"operations":operations,
        "sessionCount":scan.sessions.len(),"officialModel":official_model(&doc,&planned_state),
        "configFingerprint":journal::hash_bytes(text.as_bytes())}),
    )
}

fn switch(
    req: &Request,
    progress: &impl Fn(Progress),
    guard: &impl Fn() -> Result<()>,
) -> Result<Value> {
    check_owner(req)?;
    guard()?;
    let (text, doc) = read_config(&req.home)?;
    ensure!(
        req.expected_config_fingerprint.as_deref()
            == Some(journal::hash_bytes(text.as_bytes()).as_str()),
        "检查后配置被外部修改，请刷新后重试"
    );
    let mut state = read_state(req)?;
    let next = build_config(req, doc, &mut state)?;
    let ct = CancellationToken::new();
    let settings = settings(req);
    let mut j =
        journal::Journal::new_with_setup(&req.store_root, "VS Code 服务商切换", &|| {
            claim_owner(req)?;
            easy_switch_core::store::atomic_write(
                &req.store_root.join("target.json"),
                &serde_json::to_vec(&Target {
                    home: req.home.clone(),
                    sqlite_home: req.sqlite_home.clone(),
                })?,
            )
        })?;
    let scan = sessions::Scanner::default().scan_with_progress(&settings, &ct, progress)?;
    ensure!(
        scan.warnings.is_empty(),
        "会话扫描存在异常：{}",
        scan.warnings.join("；")
    );
    check_current_index(req, &scan.sessions)?;
    check_projects(
        &req.home,
        req.project_dirs
            .iter()
            .cloned()
            .chain(scan.sessions.iter().map(|s| PathBuf::from(&s.cwd))),
    )?;
    guard()?;
    ensure!(read_config(&req.home)?.0 == text, "扫描期间配置被外部修改");
    j.stage(
        &req.home.join("config.toml"),
        Some(next.to_string().as_bytes()),
    )?;
    sessions::stage_provider_with_model(
        &mut j,
        &settings,
        &scan.sessions,
        next.get("model_provider")
            .and_then(Item::as_str)
            .context("缺少目标 provider")?,
        next.get("model")
            .and_then(Item::as_str)
            .or(state.official_model.as_deref()),
        &ct,
        progress,
    )?;
    j.stage(
        &req.store_root.join("switch-state.json"),
        Some(&serde_json::to_vec(&state)?),
    )?;
    let expected = next["model_provider"].as_str().unwrap();
    j.commit_verified(progress, guard, &|| {
        let verified = sessions::Scanner::default().scan(&settings, &ct)?;
        let identities = |items: &[sessions::Session]| {
            items
                .iter()
                .map(|s| (s.id.clone(), s.path.clone(), s.cwd.clone(), s.archived))
                .collect::<BTreeSet<_>>()
        };
        ensure!(
            verified.warnings.is_empty()
                && identities(&verified.sessions) == identities(&scan.sessions)
                && verified.sessions.iter().all(|s| s.provider == expected),
            "提交后会话校验失败，正在恢复"
        );
        for s in &verified.sessions {
            ensure!(
                history::header(&s.path)?.id == s.id,
                "提交后线程身份校验失败"
            );
        }
        Ok(())
    })?;
    Ok(
        json!({"protocolVersion":PROTOCOL_VERSION,"committed":true,"activeProviderId":state.active_provider_id,"sessionCount":scan.sessions.len()}),
    )
}

fn recover(req: &Request, guard: &impl Fn() -> Result<()>) -> Result<Value> {
    check_owner(req)?;
    guard()?;
    claim_owner(req)?;
    let mut allowed = vec![req.home.clone(), req.store_root.clone()];
    allowed.extend(req.sqlite_home.clone());
    journal::restore_with_guard(
        &req.store_root,
        req.operation_id.as_deref().context("缺少恢复操作 ID")?,
        &allowed,
        guard,
    )?;
    let (_, doc) = read_config(&req.home)?;
    Ok(
        json!({"protocolVersion":PROTOCOL_VERSION,"committed":true,"activeProviderId":active_provider(&doc,&read_state(req)?)}),
    )
}

#[cfg(test)]
mod tests;
