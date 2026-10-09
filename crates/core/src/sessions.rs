use crate::{
    Result, Settings,
    journal::{self, Journal},
};
use anyhow::{Context, ensure};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub provider: String,
    pub path: PathBuf,
    pub updated: i64,
    pub archived: bool,
    pub database: Option<PathBuf>,
    #[serde(default, skip_serializing)]
    pub related_databases: Vec<PathBuf>,
    #[serde(default)]
    pub related_paths: Vec<PathBuf>,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub sessions: Vec<Session>,
    pub warnings: Vec<String>,
}
#[derive(Default)]
pub struct Scanner {
    cache: HashMap<PathBuf, (u64, SystemTime, Session)>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: String,
    pub text: String,
    pub time: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub messages: Vec<Message>,
    pub tokens: Vec<Value>,
    pub truncated: bool,
    pub has_encrypted_content: bool,
    pub relations: Vec<Value>,
}
fn s(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").to_owned()
}
pub fn normalize(path: &Path) -> Result<PathBuf> {
    let raw = path.to_string_lossy();
    #[cfg(windows)]
    let raw = std::borrow::Cow::Owned::<str>(raw.replace('/', "\\"));
    let raw = if raw.starts_with(r"\\?\UNC\") {
        format!(r"\\{}", &raw[8..])
    } else if raw.starts_with(r"\\?\") {
        raw[4..].into()
    } else {
        raw.into_owned()
    };
    ensure!(
        !raw.starts_with(r"\\.\") && !raw.starts_with(r"\??\") && !raw.contains('\0'),
        "拒绝设备命名空间路径"
    );
    let p = PathBuf::from(raw);
    ensure!(p.is_absolute(), "路径不是绝对路径");
    for part in p.components() {
        if let std::path::Component::Normal(n) = part {
            let n = n.to_string_lossy();
            ensure!(
                !n.ends_with(['.', ' ']) && !n.contains(':'),
                "路径包含不安全片段"
            );
        }
        ensure!(
            !matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            ),
            "路径包含相对跳转"
        );
    }
    Ok(p.components().collect())
}
pub fn within(path: &Path, root: &Path) -> Result<bool> {
    let p = normalize(path)?.to_string_lossy().to_lowercase();
    let r = normalize(root)?
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase();
    Ok(p == r || p.starts_with(&(r + std::path::MAIN_SEPARATOR_STR)))
}
pub fn no_links(path: &Path) -> Result<()> {
    for part in path.ancestors() {
        if let Ok(m) = fs::symlink_metadata(part) {
            ensure!(!m.file_type().is_symlink(), "拒绝通过链接访问会话或备份");
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(m.file_attributes() & 0x400 == 0, "拒绝通过重解析点访问数据");
            }
        }
    }
    Ok(())
}
pub fn owned(path: &Path, settings: &Settings) -> Result<PathBuf> {
    let p = normalize(path)?;
    ensure!(
        within(&p, &settings.codex_home.join("sessions"))?
            || within(&p, &settings.codex_home.join("archived_sessions"))?,
        "会话路径不在选定 Codex Home 内"
    );
    no_links(&p)?;
    Ok(p)
}
pub fn databases(settings: &Settings) -> Result<Vec<PathBuf>> {
    let mut all = vec![];
    let mut seen = HashSet::new();
    for root in [
        settings.codex_home.clone(),
        settings
            .sqlite_home
            .clone()
            .unwrap_or(settings.codex_home.join("sqlite")),
    ] {
        if !root.exists() {
            continue;
        }
        no_links(&root)?;
        for file in fs::read_dir(root)? {
            let file = file?;
            let p = file.path();
            if file.file_type()?.is_file()
                && matches!(
                    p.extension().and_then(|s| s.to_str()),
                    Some("db" | "sqlite")
                )
            {
                no_links(&p)?;
                let p = normalize(&p)?;
                if seen.insert(p.to_string_lossy().to_lowercase()) {
                    all.push(p);
                }
            }
        }
    }
    all.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    Ok(all)
}
pub fn open(path: &Path) -> Result<Connection> {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    c.busy_timeout(std::time::Duration::from_secs(2))?;
    Ok(c)
}
pub fn columns(c: &Connection, table: &str) -> Result<HashSet<String>> {
    ensure!(
        table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "表名无效"
    );
    let mut q = c.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = q.query_map([], |r| r.get(1))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
fn required(cols: &HashSet<String>, names: &[&str]) -> Result<()> {
    ensure!(
        names.iter().all(|c| cols.contains(*c)),
        "未知数据库结构，停止写入"
    );
    Ok(())
}
fn cancellation(ct: &CancellationToken) -> Result<()> {
    ensure!(!ct.is_cancelled(), "操作已取消");
    Ok(())
}
impl Scanner {
    pub fn scan(&mut self, settings: &Settings, ct: &CancellationToken) -> Result<Scan> {
        self.scan_with_progress(settings, ct, &|_| {})
    }
    pub fn scan_with_progress(
        &mut self,
        settings: &Settings,
        ct: &CancellationToken,
        progress: &impl Fn(crate::Progress),
    ) -> Result<Scan> {
        progress(crate::Progress::new(
            "扫描会话",
            "检查数据库与会话目录",
            0,
            None,
        ));
        let mut out = Scan::default();
        let mut scanned = 0;
        let mut indexed = HashSet::new();
        let mut ids: HashMap<String, usize> = HashMap::new();
        for path in databases(settings)? {
            cancellation(ct)?;
            progress(crate::Progress::new(
                "扫描会话",
                "读取数据库索引",
                scanned,
                None,
            ));
            let result = (|| -> Result<Vec<Session>> {
                let c = open(&path)?;
                let cols = columns(&c, "threads")?;
                if cols.is_empty() {
                    return Ok(vec![]);
                }
                required(
                    &cols,
                    &[
                        "id",
                        "title",
                        "cwd",
                        "model_provider",
                        "rollout_path",
                        "updated_at",
                        "archived",
                    ],
                )?;
                let mut q=c.prepare("SELECT id,title,cwd,model_provider,rollout_path,updated_at,archived FROM threads")?;
                let mut rows = q.query([])?;
                let mut items = vec![];
                while let Some(r) = rows.next()? {
                    cancellation(ct)?;
                    let p = owned(&PathBuf::from(r.get::<_, String>(4)?), settings)?;
                    let t: i64 = r.get(5)?;
                    items.push(Session {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        cwd: r.get(2)?,
                        provider: r.get(3)?,
                        path: p,
                        updated: if t > 100_000_000_000 { t / 1000 } else { t },
                        archived: r.get::<_, i64>(6)? != 0,
                        database: Some(path.clone()),
                        related_databases: vec![],
                        related_paths: vec![],
                    });
                }
                Ok(items)
            })();
            match result {
                Ok(items) => {
                    for mut item in items {
                        if let Some(&position) = ids.get(&item.id) {
                            let existing = &mut out.sessions[position];
                            let same_database = existing.database == item.database
                                || existing.related_databases.contains(&path);
                            let same_file = existing
                                .path
                                .to_string_lossy()
                                .eq_ignore_ascii_case(&item.path.to_string_lossy());
                            if same_database || !same_file || existing.archived != item.archived {
                                out.warnings.push(format!(
                                    "线程索引冲突：{}（{}、{}；{}）",
                                    item.id,
                                    existing
                                        .database
                                        .as_ref()
                                        .and_then(|p| p.file_name())
                                        .unwrap_or_default()
                                        .to_string_lossy(),
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                    if same_database {
                                        "同一数据库内 ID 重复"
                                    } else if !same_file {
                                        "会话文件不同"
                                    } else {
                                        "归档状态不同"
                                    }
                                ));
                                continue;
                            }
                            // Mirrored indexes may have stale display metadata. Keep all write targets.
                            if item.updated > existing.updated {
                                std::mem::swap(existing, &mut item);
                            }
                            existing.related_databases.push(item.database.unwrap());
                            existing.related_databases.extend(item.related_databases);
                        } else {
                            ids.insert(item.id.clone(), out.sessions.len());
                            indexed.insert(item.path.to_string_lossy().to_lowercase());
                            out.sessions.push(item);
                        }
                    }
                }
                Err(e) => out.warnings.push(format!(
                    "{}：{e}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
        let mut alive = HashSet::new();
        for folder in ["sessions", "archived_sessions"] {
            let root = settings.codex_home.join(folder);
            if !root.exists() {
                continue;
            }
            for entry in walkdir::WalkDir::new(root).follow_links(false) {
                cancellation(ct)?;
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        out.warnings.push(e.to_string());
                        continue;
                    }
                };
                if !entry.file_type().is_file()
                    || entry.path().extension().and_then(|x| x.to_str()) != Some("jsonl")
                {
                    continue;
                }
                let path = normalize(entry.path())?;
                scanned += 1;
                progress(crate::Progress::new(
                    "扫描会话",
                    &format!("已发现 {scanned} 个会话文件"),
                    scanned,
                    None,
                ));
                alive.insert(path.clone());
                if indexed.contains(&path.to_string_lossy().to_lowercase()) {
                    continue;
                }
                let result = (|| -> Result<Session> {
                    owned(&path, settings)?;
                    let m = entry.metadata()?;
                    let modified = m.modified()?;
                    if let Some((n, t, s)) = self.cache.get(&path) {
                        if *n == m.len() && *t == modified {
                            return Ok(s.clone());
                        }
                    }
                    let mut item = None;
                    for line in BufReader::new(File::open(&path)?).lines().take(40) {
                        let line = line?;
                        let v: Value = serde_json::from_str(line.trim_start_matches('\u{feff}'))?;
                        let p = &v["payload"];
                        if v["type"] == "session_meta" {
                            ensure!(item.is_none(), "元数据重复");
                            let id = s(p, "id");
                            ensure!(!id.is_empty(), "线程 ID 缺失");
                            item = Some(Session {
                                id,
                                title: "未命名会话".into(),
                                cwd: s(p, "cwd"),
                                provider: s(p, "model_provider"),
                                path: path.clone(),
                                updated: modified.duration_since(SystemTime::UNIX_EPOCH)?.as_secs()
                                    as i64,
                                archived: folder == "archived_sessions",
                                database: None,
                                related_databases: vec![],
                                related_paths: vec![],
                            });
                        }
                        if v["type"] == "event_msg" && p["type"] == "user_message" {
                            if let Some(item) = item.as_mut() {
                                item.title = s(p, "message")
                                    .replace('\n', " ")
                                    .chars()
                                    .take(100)
                                    .collect();
                                break;
                            }
                        }
                    }
                    let item = item.context("缺少 session_meta")?;
                    self.cache
                        .insert(path.clone(), (m.len(), modified, item.clone()));
                    Ok(item)
                })();
                match result {
                    Ok(item) => {
                        if !ids.contains_key(&item.id) {
                            ids.insert(item.id.clone(), out.sessions.len());
                            out.sessions.push(item);
                        } else {
                            let existing = &mut out.sessions[ids[&item.id]];
                            if crate::history::same_paginated_thread(&existing.path, &item.path)
                                .unwrap_or(false)
                            {
                                existing.related_paths.push(item.path);
                            } else {
                                out.warnings.push(format!(
                                    "同一线程有多个无法确认关系的会话文件：{}",
                                    item.id
                                ));
                            }
                        }
                    }
                    Err(e) => out.warnings.push(format!(
                        "{}：{e}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )),
                }
            }
        }
        self.cache.retain(|p, _| alive.contains(p));
        out.sessions.sort_by(|a, b| b.updated.cmp(&a.updated));
        Ok(out)
    }
}
pub fn detail(session: &Session, limit: usize) -> Result<Detail> {
    let mut out = Detail {
        messages: vec![],
        tokens: vec![],
        truncated: false,
        has_encrypted_content: false,
        relations: vec![],
    };
    let mut events = vec![];
    let mut prev = Value::Null;
    let mut chars = 0usize;
    for line in crate::history::lines(&session.path)? {
        let line = line?;
        let v: Value = serde_json::from_str(line.trim_start_matches('\u{feff}'))?;
        let p = &v["payload"];
        let time = s(&v, "timestamp");
        out.has_encrypted_content |= p.get("encrypted_content").is_some();
        if v["type"] == "response_item" {
            let kind = s(p, "type");
            let text = match kind.as_str() {
                "message" => Some(
                    p["content"]
                        .as_array()
                        .unwrap_or(&vec![])
                        .iter()
                        .filter_map(|c| {
                            c["text"].as_str().map(str::to_owned).or_else(|| {
                                if c["type"] == "input_image" {
                                    Some("[图片引用]".into())
                                } else {
                                    None
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                "function_call" | "custom_tool_call" => Some(format!(
                    "工具：{}\n{}",
                    s(p, "name"),
                    p.get("arguments")
                        .or(p.get("input"))
                        .unwrap_or(&Value::Null)
                )),
                "function_call_output" | "custom_tool_call_output" => {
                    Some(format!("工具结果：\n{}", p["output"]))
                }
                _ => None,
            };
            if let Some(text) = text {
                if out.messages.len() < limit && chars < 2_000_000 {
                    let clipped: String = text.chars().take(20000).collect();
                    out.truncated |= clipped.len() < text.len();
                    chars += clipped.len();
                    out.messages.push(Message {
                        role: p["role"].as_str().unwrap_or("tool").into(),
                        text: clipped,
                        time: time.clone(),
                    });
                } else {
                    out.truncated = true;
                }
            }
        }
        if v["type"] == "event_msg" {
            let kind = s(p, "type");
            if matches!(kind.as_str(), "user_message" | "agent_message") && events.len() < limit {
                events.push(Message {
                    role: if kind == "user_message" {
                        "user"
                    } else {
                        "assistant"
                    }
                    .into(),
                    text: s(p, "message").chars().take(20000).collect(),
                    time: time.clone(),
                });
            }
            if kind == "token_count" {
                let total = &p["info"]["total_token_usage"];
                let last = &p["info"]["last_token_usage"];
                let (counts, cumulative) = if total.is_object() {
                    (total, true)
                } else {
                    (last, false)
                };
                if counts.is_object() && (!cumulative || *counts != prev) {
                    out.tokens.push(json!({"time":time,"cumulative":cumulative,"input":counts["input_tokens"],"output":counts["output_tokens"],"cached":counts["cached_input_tokens"],"reasoning":counts["reasoning_output_tokens"],"total":counts["total_tokens"]}));
                    if cumulative {
                        prev = counts.clone();
                    }
                }
            }
        }
    }
    if out.messages.is_empty() {
        out.messages = events;
    }
    let mut relations = HashSet::new();
    for path in session.database.iter().chain(&session.related_databases) {
        let c = open(path)?;
        let cols = columns(&c, "thread_spawn_edges")?;
        if cols.contains("parent_thread_id") && cols.contains("child_thread_id") {
            let mut q=c.prepare("SELECT parent_thread_id,child_thread_id FROM thread_spawn_edges WHERE parent_thread_id=?1 OR child_thread_id=?1")?;
            let mut rows = q.query([&session.id])?;
            while let Some(r) = rows.next()? {
                let relation = (r.get::<_, String>(0)?, r.get::<_, String>(1)?);
                if relations.insert(relation.clone()) {
                    out.relations
                        .push(json!({"parent":relation.0,"child":relation.1}));
                }
            }
        }
    }
    Ok(out)
}
pub fn patch_metadata(source: &Path, dest: &Path, key: &str, value: &str) -> Result<()> {
    ensure!(
        matches!(key, "model_provider" | "cwd"),
        "不支持的元数据字段"
    );
    let mut input = BufReader::new(File::open(source)?);
    let mut output = File::create(dest)?;
    let mut line = vec![];
    let mut found = false;
    let mut first = true;
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        ensure!(line.len() <= 32 * 1024 * 1024, "单条会话事件超过 32 MiB");
        let start = if first && line.starts_with(&[0xef, 0xbb, 0xbf]) {
            3
        } else {
            0
        };
        first = false;
        let mut end = line.len();
        while end > start && matches!(line[end - 1], b'\r' | b'\n') {
            end -= 1;
        }
        if end > start {
            let mut node: Value = serde_json::from_slice(&line[start..end])?;
            if node["type"] == "session_meta" {
                ensure!(
                    !found && node["payload"]["id"].is_string(),
                    "会话元数据不完整或重复"
                );
                found = true;
                if node["payload"][key] != value {
                    node["payload"][key] = json!(value);
                    output.write_all(&line[..start])?;
                    let replacement = serde_json::to_vec(&node)?;
                    if node["payload"]["history_mode"] == "paginated" {
                        // HistoryPosition and SQLite projections store byte offsets. Preserve them.
                        ensure!(
                            replacement.len() <= end - start,
                            "分页会话元数据空间不足，未修改历史；需要支持偏移重建后才能执行此项变更"
                        );
                        output.write_all(&replacement)?;
                        output.write_all(&vec![b' '; end - start - replacement.len()])?;
                    } else {
                        output.write_all(&replacement)?;
                    }
                    output.write_all(&line[end..])?;
                    continue;
                }
            }
        }
        output.write_all(&line)?;
    }
    ensure!(found, "缺少 session_meta");
    output.sync_all()?;
    Ok(())
}
pub fn stage_database(
    j: &mut Journal,
    path: &Path,
    change: impl FnOnce(&Connection) -> Result<()>,
) -> Result<()> {
    no_links(path)?;
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    source.busy_timeout(std::time::Duration::from_secs(2))?;
    let busy: i64 = source.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
    ensure!(busy == 0, "数据库仍被占用");
    let before = journal::hash(path)?;
    let tmp = j.dir.join(format!("{}.db", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut copy = Connection::open(&tmp)?;
        {
            let b = rusqlite::backup::Backup::new(&source, &mut copy)?;
            b.run_to_completion(128, std::time::Duration::from_millis(1), None)?;
        }
        copy.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")?;
        change(&copy)?;
        copy.execute_batch("COMMIT")?;
        ensure!(
            copy.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))? == "ok",
            "数据库完整性校验失败"
        );
        drop(copy);
        drop(source);
        ensure!(journal::hash(path)? == before, "数据库出现并发修改");
        let wal = PathBuf::from(format!("{}-wal", path.display()));
        ensure!(
            !wal.exists() || fs::metadata(wal)?.len() == 0,
            "数据库出现并发 WAL"
        );
        j.stage_file(path, &tmp)
    })();
    let _ = fs::remove_file(tmp);
    result
}
pub fn stage_changes(
    j: &mut Journal,
    settings: &Settings,
    items: &[Session],
    action: &str,
    value: Option<&str>,
    ct: &CancellationToken,
) -> Result<()> {
    stage_changes_with_progress(j, settings, items, action, value, ct, &|_| {})
}
pub fn stage_changes_with_progress(
    j: &mut Journal,
    settings: &Settings,
    items: &[Session],
    action: &str,
    value: Option<&str>,
    ct: &CancellationToken,
    progress: &impl Fn(crate::Progress),
) -> Result<()> {
    progress(crate::Progress::new("修复会话", "检查会话关联", 0, None));
    ensure!(
        matches!(
            action,
            "provider" | "migrate" | "archive" | "unarchive" | "delete"
        ),
        "不支持的会话操作"
    );
    let mut destinations = HashMap::new();
    let mut expanded = vec![];
    for item in items {
        expanded.push(item.clone());
        for related in &item.related_paths {
            let mut part = item.clone();
            part.path = related.clone();
            part.database = None;
            part.related_databases.clear();
            part.related_paths.clear();
            part.archived = within(related, &settings.codex_home.join("archived_sessions"))?;
            expanded.push(part);
        }
    }
    if action == "delete" {
        crate::history::ensure_unreferenced(
            settings,
            &expanded.iter().map(|s| s.path.clone()).collect::<Vec<_>>(),
        )?;
    }
    let offsets = if matches!(action, "provider" | "migrate") {
        crate::history::stage_metadata(
            j,
            settings,
            &expanded,
            if action == "provider" {
                "model_provider"
            } else {
                "cwd"
            },
            value.context("缺少修改值")?,
            ct,
            progress,
        )?
    } else {
        HashMap::new()
    };
    for (i, item) in expanded.iter().enumerate() {
        cancellation(ct)?;
        progress(crate::Progress::new(
            "修复会话",
            "备份并处理会话文件",
            i,
            Some(expanded.len()),
        ));
        let path = owned(&item.path, settings)?;
        ensure!(path.is_file(), "会话文件不存在");
        ensure!(
            crate::history::header(&path)?.id == item.id,
            "线程索引与会话身份不一致，未修改"
        );
        match action {
            "provider" | "migrate" => {
                if crate::history::header(&path)?.paginated {
                    continue;
                }
                let temp = j.dir.join(format!("{}.jsonl", uuid::Uuid::new_v4()));
                let result = (|| {
                    patch_metadata(
                        &path,
                        &temp,
                        if action == "provider" {
                            "model_provider"
                        } else {
                            "cwd"
                        },
                        value.context("缺少修改值")?,
                    )?;
                    j.stage_file(&path, &temp)
                })();
                let _ = fs::remove_file(temp);
                result?;
            }
            "archive" | "unarchive" => {
                let archive = action == "archive";
                if item.archived == archive {
                    continue;
                }
                let source_root = normalize(&settings.codex_home.join(if item.archived {
                    "archived_sessions"
                } else {
                    "sessions"
                }))?;
                ensure!(within(&path, &source_root)?, "会话归档状态与所在目录不一致");
                let relative: PathBuf = path
                    .components()
                    .skip(source_root.components().count())
                    .collect();
                let dest = settings
                    .codex_home
                    .join(if archive {
                        "archived_sessions"
                    } else {
                        "sessions"
                    })
                    .join(relative);
                owned(&dest, settings)?;
                ensure!(!dest.exists(), "目标会话文件已经存在");
                j.stage_file(&dest, &path)?;
                j.stage(&path, None)?;
                if item.database.is_some() || items.iter().any(|s| s.path == item.path) {
                    destinations.insert(item.id.clone(), dest);
                }
            }
            "delete" => j.stage(&path, None)?,
            _ => unreachable!(),
        }
    }
    progress(crate::Progress::new(
        "修复会话",
        "会话文件处理完成",
        expanded.len(),
        Some(expanded.len()),
    ));
    progress(crate::Progress::new("修复索引", "检查数据库结构", 0, None));
    let mut dbs = databases(settings)?
        .into_iter()
        .map(|path| {
            let threads = !columns(&open(&path)?, "threads")?.is_empty();
            Ok((path, threads))
        })
        .collect::<Result<Vec<_>>>()?;
    dbs.sort_by_key(|(_, threads)| !threads);
    let mut projects = HashMap::new();
    let total = dbs.len();
    for (i, (database, _)) in dbs.into_iter().enumerate() {
        cancellation(ct)?;
        progress(crate::Progress::new(
            "修复索引",
            "备份并更新数据库与历史偏移",
            i,
            Some(total),
        ));
        let c = open(&database)?;
        let thread_cols = columns(&c, "threads")?;
        let cat_cols = columns(&c, "local_thread_catalog")?;
        let has_offsets: bool = !offsets.is_empty() && c.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master m JOIN pragma_table_info(m.name) p WHERE m.type='table' AND p.name GLOB '*byte_offset')",[],|r|r.get(0))?;
        drop(c);
        let selected: Vec<_> = items
            .iter()
            .filter(|s| {
                s.database.as_ref() == Some(&database) || s.related_databases.contains(&database)
            })
            .collect();
        if selected.is_empty() && cat_cols.is_empty() && !has_offsets {
            continue;
        }
        stage_database(j, &database, |db| {
            let mut local_projects = HashMap::new();
            if has_offsets {
                crate::history::rebase_database(db, &offsets)?;
            }
            if !selected.is_empty() {
                required(
                    &thread_cols,
                    &["id", "cwd", "model_provider", "rollout_path", "archived"],
                )?;
            }
            for item in &selected {
                let (rollout, archived): (String, i64) = db.query_row(
                    "SELECT rollout_path,archived FROM threads WHERE id=?1",
                    [&item.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                ensure!(
                    owned(Path::new(&rollout), settings)?
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&item.path.to_string_lossy())
                        && (archived != 0) == item.archived,
                    "扫描后线程索引发生变化，未修改"
                );
                let affected = match action {
                    "provider" => db.execute(
                        "UPDATE threads SET model_provider=?1 WHERE id=?2",
                        params![value, &item.id],
                    )?,
                    "migrate" => {
                        let project = project_for(db, value.context("目标路径缺失")?)?;
                        local_projects.insert(item.id.clone(), project.clone());
                        if item.database.as_ref() == Some(&database) {
                            projects.insert(item.id.clone(), project.clone());
                        }
                        let n = db.execute(
                            "UPDATE threads SET cwd=?1 WHERE id=?2",
                            params![value, &item.id],
                        )?;
                        if let Some(p) = project {
                            db.execute(
                                "UPDATE threads SET project_id=?1 WHERE id=?2",
                                params![p, &item.id],
                            )?;
                        }
                        n
                    }
                    "archive" | "unarchive" => {
                        let dest = destinations.get(&item.id).unwrap_or(&item.path);
                        let n = db.execute(
                            "UPDATE threads SET archived=?1,rollout_path=?2 WHERE id=?3",
                            params![action == "archive", dest.to_string_lossy(), &item.id],
                        )?;
                        if thread_cols.contains("archived_at") {
                            db.execute(
                                "UPDATE threads SET archived_at=?1 WHERE id=?2",
                                params![
                                    if action == "archive" {
                                        Some(chrono::Utc::now().timestamp())
                                    } else {
                                        None
                                    },
                                    &item.id
                                ],
                            )?;
                        }
                        n
                    }
                    "delete" => {
                        if !columns(db, "thread_spawn_edges")?.is_empty() {
                            db.execute("DELETE FROM thread_spawn_edges WHERE parent_thread_id=?1 OR child_thread_id=?1",[&item.id])?;
                        }
                        db.execute("DELETE FROM threads WHERE id=?1", [&item.id])?
                    }
                    _ => unreachable!(),
                };
                ensure!(affected == 1, "线程索引不唯一或已消失");
            }
            if !cat_cols.is_empty() {
                required(&cat_cols, &["host_id", "thread_id"])?;
                required(
                    &columns(db, "local_thread_catalog_hosts")?,
                    &["host_id", "host_kind"],
                )?;
                for item in items {
                    const LOCAL: &str = "thread_id=?1 AND host_id IN (SELECT host_id FROM local_thread_catalog_hosts WHERE host_kind='local')";
                    match action {
                        "provider" => {
                            required(&cat_cols, &["model_provider"])?;
                            db.execute(&format!("UPDATE local_thread_catalog SET model_provider=?2 WHERE {LOCAL}"),params![item.id,value])?;
                        }
                        "migrate" => {
                            required(&cat_cols, &["cwd", "project_id"])?;
                            db.execute(&format!("UPDATE local_thread_catalog SET cwd=?2,project_id=?3 WHERE {LOCAL}"),params![item.id,value,local_projects.get(&item.id).or_else(|| projects.get(&item.id)).and_then(|p|p.as_deref())])?;
                        }
                        "archive" | "delete" => {
                            db.execute(
                                &format!("DELETE FROM local_thread_catalog WHERE {LOCAL}"),
                                [&item.id],
                            )?;
                        }
                        _ => {}
                    }
                }
                if columns(db, "local_thread_catalog_metadata")?.contains("catalog_revision") {
                    db.execute("UPDATE local_thread_catalog_metadata SET catalog_revision=catalog_revision+1",[])?;
                }
                let sync = columns(db, "local_thread_catalog_sync_state")?;
                if !sync.is_empty() {
                    required(
                        &sync,
                        &["host_id", "initial_build_complete", "watermark_updated_at"],
                    )?;
                    db.execute("UPDATE local_thread_catalog_sync_state SET initial_build_complete=0,watermark_updated_at=0 WHERE host_id IN (SELECT host_id FROM local_thread_catalog_hosts WHERE host_kind='local')",[])?;
                }
            }
            Ok(())
        })?;
    }
    progress(crate::Progress::new(
        "修复索引",
        "数据库处理完成",
        total,
        Some(total),
    ));
    Ok(())
}
fn project_for(db: &Connection, path: &str) -> Result<Option<String>> {
    if !columns(db, "threads")?.contains("project_id") {
        return Ok(None);
    }
    required(
        &columns(db, "project_roots")?,
        &["project_id", "position", "path"],
    )?;
    required(
        &columns(db, "projects")?,
        &[
            "id",
            "name",
            "metadata",
            "position",
            "created_at_ms",
            "updated_at_ms",
        ],
    )?;
    let mut q =
        db.prepare("SELECT project_id FROM project_roots WHERE path=?1 COLLATE NOCASE LIMIT 2")?;
    let found = q
        .query_map([path], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(found.len() < 2, "目标目录属于多个项目");
    if let Some(id) = found.into_iter().next() {
        return Ok(Some(id));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let name = Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    db.execute("INSERT INTO projects(id,name,metadata,position,created_at_ms,updated_at_ms) VALUES(?1,?2,'{}',(SELECT COALESCE(MAX(position),0)+1 FROM projects),?3,?3)",params![id,name,chrono::Utc::now().timestamp_millis()])?;
    db.execute(
        "INSERT INTO project_roots(project_id,position,path) VALUES(?1,0,?2)",
        params![id, path],
    )?;
    Ok(Some(id))
}
pub fn export(session: &Session, folder: &Path) -> Result<PathBuf> {
    ensure!(
        !session.id.is_empty()
            && session.id.len() <= 128
            && session
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "线程 ID 不适合作为导出文件名"
    );
    fs::create_dir_all(folder)?;
    no_links(folder)?;
    let stem: String = session
        .title
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(70)
        .collect();
    let stem = stem.trim_end_matches(['.', ' ']);
    let mut path = folder.join(format!(
        "{}-{}.md",
        if stem.is_empty() { "session" } else { stem },
        session.id
    ));
    let mut suffix = 1;
    while path.exists() {
        path = folder.join(format!("{stem}-{}-{suffix}.md", session.id));
        suffix += 1;
    }
    let mut out = File::options().create_new(true).write(true).open(&path)?;
    writeln!(
        out,
        "# {}\n\n线程：{}\n\n项目：{}\n",
        session.title, session.id, session.cwd
    )?;
    let mut has_messages = false;
    for line in crate::history::lines(&session.path)? {
        let v: Value = serde_json::from_str(line?.trim_start_matches('\u{feff}'))?;
        if v["type"] == "response_item" && v["payload"]["type"] == "message" {
            has_messages = true;
            break;
        }
    }
    for line in crate::history::lines(&session.path)? {
        let v: Value = serde_json::from_str(line?.trim_start_matches('\u{feff}'))?;
        let p = &v["payload"];
        if v["type"] == "response_item" {
            let kind = s(p, "type");
            if kind == "message" {
                writeln!(out, "## {} · {}\n", s(p, "role"), s(&v, "timestamp"))?;
                if let Some(parts) = p["content"].as_array() {
                    for part in parts {
                        if let Some(text) = part["text"].as_str() {
                            writeln!(out, "{text}\n")?;
                        } else if part["type"] == "input_image" {
                            writeln!(out, "[图片引用：{}]\n", part["image_url"])?;
                        }
                    }
                }
            } else if matches!(
                kind.as_str(),
                "function_call"
                    | "function_call_output"
                    | "custom_tool_call"
                    | "custom_tool_call_output"
            ) {
                writeln!(out, "## 工具记录\n\n{}\n", serde_json::to_string_pretty(p)?)?;
            }
        } else if !has_messages
            && v["type"] == "event_msg"
            && matches!(p["type"].as_str(), Some("user_message" | "agent_message"))
        {
            writeln!(
                out,
                "## {} · {}\n\n{}\n",
                s(p, "type"),
                s(&v, "timestamp"),
                s(p, "message")
            )?;
        }
    }
    out.sync_all()?;
    Ok(path)
}
