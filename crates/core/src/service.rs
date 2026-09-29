use crate::{desktop::DesktopHost, journal::Journal, *};
use anyhow::{Context, ensure};
use std::{fs, path::PathBuf};
use tokio_util::sync::CancellationToken;

fn report(f: &impl Fn(Progress), stage: &str, detail: &str) {
    f(Progress {
        stage: stage.into(),
        detail: detail.into(),
    });
}
pub fn switch(
    store: &Store,
    p: &Provider,
    host: &impl DesktopHost,
    ct: &CancellationToken,
    progress: impl Fn(Progress),
) -> Result<String> {
    let mut settings = store.settings()?;
    p.validate(&store.providers()?)?;
    if p.runtime() {
        store.runtime_key()?;
        settings = store.settings()?;
    }
    let cfg = settings.codex_home.join("config.toml");
    let before = if cfg.exists() {
        fs::read_to_string(&cfg)?
    } else {
        String::new()
    };
    let secret = if p.runtime() {
        settings.runtime_key.as_deref()
    } else {
        p.protected_key.as_deref()
    }
    .map(crypto::unprotect)
    .transpose()?;
    let old = store
        .providers()?
        .into_iter()
        .find(|p| Some(&p.id) == settings.active_provider_id.as_ref());
    let next = config::update(
        &before,
        p,
        &settings,
        secret.as_deref(),
        old.as_ref().map(|p| p.provider_id()),
    )?;
    ensure!(!ct.is_cancelled(), "操作已取消");
    let mut j = Journal::new(&store.root, &format!("切换供应商：{}", p.name))?;
    report(&progress, "退出 Codex", "等待目标桌面实例正常退出");
    host.stop()?;
    runtime::stop_blocking(store)?;
    report(&progress, "备份与扫描", "验证活动、归档会话与数据库结构");
    let scan = sessions::Scanner::default().scan(&settings, ct)?;
    ensure!(
        scan.warnings.is_empty(),
        "扫描存在异常：{}",
        scan.warnings.join("；")
    );
    ensure!(
        if cfg.exists() {
            fs::read_to_string(&cfg)?
        } else {
            String::new()
        } == before,
        "退出期间配置被修改，请重试"
    );
    let auth_path = settings.codex_home.join("auth.json");
    let auth = if auth_path.exists() {
        Some(fs::read(&auth_path)?)
    } else {
        None
    };
    let new_auth = config::next_auth(store, &settings, p, auth.as_deref(), secret.as_deref())?;
    j.stage(&cfg, Some(next.as_bytes()))?;
    j.stage(&auth_path, new_auth.as_deref())?;
    report(&progress, "修复会话", "保留对话正文与未知元数据");
    sessions::stage_changes(
        &mut j,
        &settings,
        &scan.sessions,
        "provider",
        Some(p.provider_id()),
        ct,
    )?;
    j.stage(
        &store.root.join("active-provider.json"),
        Some(&serde_json::to_vec(&p.id)?),
    )?;
    ensure!(!ct.is_cancelled(), "操作已取消");
    report(&progress, "提交与校验", "提交期间不可取消；失败自动回滚");
    j.commit()?;
    report(&progress, "重新启动", "检查代理与 Codex 窗口");
    if p.runtime() {
        runtime::ensure_started_blocking(store)?;
    }
    host.start()
        .context("配置已经提交，但 Codex 启动失败；可在备份恢复中回滚")
}
pub fn batch(
    store: &Store,
    ids: &[String],
    action: &str,
    value: Option<&str>,
    host: &impl DesktopHost,
    ct: &CancellationToken,
    progress: impl Fn(Progress),
) -> Result<String> {
    ensure!(!ids.is_empty(), "请选择会话");
    let settings = store.settings()?;
    if action == "migrate" {
        let destination = PathBuf::from(value.context("请选择目标项目目录")?);
        ensure!(
            destination.is_absolute() && destination.is_dir(),
            "目标项目目录不存在"
        );
        sessions::no_links(&destination)?;
    }
    let mut j = Journal::new(&store.root, &format!("会话操作：{action}"))?;
    report(&progress, "退出 Codex", "确保会话没有其他写入者");
    host.stop()?;
    runtime::stop_blocking(store)?;
    let scan = sessions::Scanner::default().scan(&settings, ct)?;
    ensure!(scan.warnings.is_empty(), "扫描存在异常，未修改会话");
    let selected: Vec<_> = scan
        .sessions
        .into_iter()
        .filter(|s| ids.contains(&s.id))
        .collect();
    ensure!(selected.len() == ids.len(), "选择中存在重复或已消失会话");
    sessions::stage_changes(&mut j, &settings, &selected, action, value, ct)?;
    ensure!(!ct.is_cancelled(), "操作已取消");
    report(&progress, "提交与校验", "保留完整批次备份");
    j.commit()?;
    let active = store
        .providers()?
        .into_iter()
        .find(|p| Some(&p.id) == settings.active_provider_id.as_ref());
    if active.is_some_and(|p| p.runtime()) {
        runtime::ensure_started_blocking(store)?;
    }
    let message = host.start()?;
    Ok(format!("已处理 {} 个会话。{message}", selected.len()))
}
