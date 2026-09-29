//! Codex paginated histories identify immutable rollouts separately from threads.
//! Metadata growth rebases physical byte positions without changing event ordinals or bodies.
use crate::{Result, Settings, sessions};
use anyhow::{Context, ensure};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};

pub struct Header {
    pub id: String,
    pub session_id: String,
    pub paginated: bool,
    pub base: Option<String>,
    pub base_offset: Option<u64>,
}
pub fn header(path: &Path) -> Result<Header> {
    sessions::no_links(path)?;
    let mut input = BufReader::new(File::open(path)?).take(32 * 1024 * 1024 + 1);
    for _ in 0..40 {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            break;
        }
        ensure!(line.len() <= 32 * 1024 * 1024, "会话元数据过大");
        let v: Value = serde_json::from_str(line.trim_start_matches('\u{feff}'))?;
        if v["type"] == "session_meta" {
            let p = &v["payload"];
            return Ok(Header {
                id: p["id"].as_str().context("线程身份缺失")?.into(),
                session_id: p["session_id"].as_str().unwrap_or("").into(),
                paginated: p["history_mode"] == "paginated",
                base: p["history_base"]["thread_id"].as_str().map(str::to_owned),
                base_offset: p["history_base"]["end_byte_offset"].as_u64(),
            });
        }
    }
    anyhow::bail!("缺少 session_meta")
}
pub fn lines(path: &Path) -> Result<impl Iterator<Item = std::io::Result<String>>> {
    let first = header(path)?;
    let mut paths = std::collections::HashMap::new();
    if first.base.is_some() {
        let home = path
            .ancestors()
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n == "sessions" || n == "archived_sessions")
            })
            .and_then(Path::parent)
            .context("无法定位分页历史目录")?;
        for folder in ["sessions", "archived_sessions"] {
            let root = home.join(folder);
            if !root.exists() {
                continue;
            }
            for entry in walkdir::WalkDir::new(root).follow_links(false) {
                let e = entry?;
                if e.file_type().is_file() && e.path().extension().is_some_and(|n| n == "jsonl") {
                    if let Some(id) = rollout_id(e.path()) {
                        ensure!(
                            paths.insert(id, e.path().to_path_buf()).is_none(),
                            "分页历史文件标识重复"
                        );
                    }
                }
            }
        }
    }
    let mut segments = vec![];
    let mut next = path.to_path_buf();
    let mut cap = u64::MAX;
    let mut seen = HashSet::new();
    loop {
        ensure!(
            seen.insert(next.clone()) && seen.len() <= 128,
            "分页历史存在循环或层级过深"
        );
        let h = header(&next)?;
        let len = std::fs::metadata(&next)?.len();
        if cap != u64::MAX {
            ensure!(cap <= len, "分页历史引用超出文件末尾");
            if cap > 0 {
                use std::io::{Seek, SeekFrom};
                let mut f = File::open(&next)?;
                f.seek(SeekFrom::Start(cap - 1))?;
                let mut b = [0];
                f.read_exact(&mut b)?;
                ensure!(b[0] == b'\n', "分页历史引用未对齐事件边界");
            }
        }
        segments.push((next, cap));
        let Some(base) = h.base else { break };
        next = paths.get(&base).context("分页历史引用文件缺失")?.clone();
        cap = h.base_offset.context("分页历史缺少字节偏移")?;
    }
    segments.reverse();
    let readers = segments
        .into_iter()
        .map(|(p, n)| Ok(BufReader::new(File::open(p)?.take(n))))
        .collect::<Result<Vec<_>>>()?;
    Ok(readers.into_iter().flat_map(|r| r.lines()))
}
pub fn rollout_id(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let id = if let Some((_, id)) = stem.rsplit_once('_') {
        id
    } else {
        stem.get(stem.len().checked_sub(36)?..)?
    };
    uuid::Uuid::parse_str(id).ok().map(|v| v.to_string())
}
pub fn same_paginated_thread(a: &Path, b: &Path) -> Result<bool> {
    let x = header(a)?;
    let y = header(b)?;
    Ok(x.paginated
        && y.paginated
        && x.id == y.id
        && !x.session_id.is_empty()
        && x.session_id == y.session_id
        && rollout_id(a).is_some()
        && rollout_id(b).is_some()
        && rollout_id(a) != rollout_id(b))
}
pub fn ensure_unreferenced(settings: &Settings, deleting: &[PathBuf]) -> Result<()> {
    let deleting = deleting
        .iter()
        .map(|p| sessions::normalize(p))
        .collect::<Result<Vec<_>>>()?;
    let ids: HashSet<_> = deleting.iter().filter_map(|p| rollout_id(p)).collect();
    for folder in ["sessions", "archived_sessions"] {
        let root = settings.codex_home.join(folder);
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(root).follow_links(false) {
            let e = entry?;
            if !e.file_type().is_file() || e.path().extension().is_none_or(|v| v != "jsonl") {
                continue;
            }
            let path = sessions::owned(e.path(), settings)?;
            if deleting.iter().any(|p| {
                p.to_string_lossy()
                    .eq_ignore_ascii_case(&path.to_string_lossy())
            }) {
                continue;
            }
            if let Some(base) = header(&path)?.base {
                ensure!(
                    !ids.contains(&base),
                    "所选会话仍被其他线程引用，请先保留其历史或一并选择依赖线程"
                );
            }
        }
    }
    Ok(())
}

struct MetadataLine {
    path: PathBuf,
    raw: Vec<u8>,
    node: Value,
    start: u64,
    end: u64,
    json_start: usize,
    json_end: usize,
}
impl MetadataLine {
    fn read(path: PathBuf) -> Result<Self> {
        let mut input = BufReader::new(File::open(&path)?);
        let mut start = 0;
        for _ in 0..40 {
            let mut raw = vec![];
            let len = input
                .by_ref()
                .take(32 * 1024 * 1024 + 1)
                .read_until(b'\n', &mut raw)?;
            if len == 0 {
                break;
            }
            ensure!(len <= 32 * 1024 * 1024, "会话元数据过大");
            let json_start = if start == 0 && raw.starts_with(&[0xef, 0xbb, 0xbf]) {
                3
            } else {
                0
            };
            let mut json_end = raw.len();
            while json_end > json_start && matches!(raw[json_end - 1], b'\r' | b'\n') {
                json_end -= 1;
            }
            if raw[json_start..json_end]
                .iter()
                .all(u8::is_ascii_whitespace)
            {
                start += len as u64;
                continue;
            }
            let node: Value = serde_json::from_slice(&raw[json_start..json_end])?;
            if node["type"] == "session_meta" {
                return Ok(Self {
                    path,
                    raw,
                    node,
                    start,
                    end: start + len as u64,
                    json_start,
                    json_end,
                });
            }
            start += len as u64;
        }
        anyhow::bail!("缺少 session_meta")
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct OffsetShift {
    path: PathBuf,
    start: u64,
    end: u64,
    delta: u64,
}
impl OffsetShift {
    fn translate(&self, offset: u64) -> Result<u64> {
        use std::io::{Seek, SeekFrom};
        let mut file = File::open(&self.path)?;
        ensure!(offset <= file.metadata()?.len(), "分页历史偏移超出文件末尾");
        if offset > 0 {
            file.seek(SeekFrom::Start(offset - 1))?;
            let mut byte = [0];
            file.read_exact(&mut byte)?;
            ensure!(byte[0] == b'\n', "分页历史偏移未对齐事件边界");
        }
        ensure!(
            offset <= self.start || offset >= self.end,
            "分页历史偏移落在元数据内部"
        );
        if offset >= self.end {
            offset.checked_add(self.delta).context("分页历史偏移溢出")
        } else {
            Ok(offset)
        }
    }
}
fn path_key(path: &Path) -> Result<String> {
    Ok(sessions::normalize(path)?.to_string_lossy().to_lowercase())
}

// Acyclic history_base references are resolved parent first. Padding is retained when
// possible, so switching back to a shorter provider does not move offsets again.
fn resolve_metadata(
    index: usize,
    nodes: &[MetadataLine],
    by_rollout: &std::collections::HashMap<String, usize>,
    selected: &HashSet<String>,
    key: &str,
    value: &str,
    resolved: &mut std::collections::HashMap<usize, (Vec<u8>, OffsetShift)>,
    visiting: &mut HashSet<usize>,
) -> Result<()> {
    if resolved.contains_key(&index) {
        return Ok(());
    }
    ensure!(
        visiting.insert(index) && visiting.len() <= 128,
        "分页历史存在循环或层级过深"
    );
    let m = &nodes[index];
    let mut node = m.node.clone();
    if selected.contains(&path_key(&m.path)?) {
        node["payload"][key] = Value::String(value.into());
    }
    if let Some(base) = m.node["payload"]
        .get("history_base")
        .filter(|v| !v.is_null())
    {
        let id = base["thread_id"].as_str().context("分页历史引用身份缺失")?;
        let parent = *by_rollout.get(id).context("分页历史引用文件缺失")?;
        resolve_metadata(
            parent, nodes, by_rollout, selected, key, value, resolved, visiting,
        )?;
        let old = base["end_byte_offset"]
            .as_u64()
            .context("分页历史缺少字节偏移")?;
        node["payload"]["history_base"]["end_byte_offset"] =
            resolved[&parent].1.translate(old)?.into();
    }
    let bytes = if node == m.node {
        m.raw.clone()
    } else {
        let json = serde_json::to_vec(&node)?;
        let mut bytes = m.raw[..m.json_start].to_vec();
        bytes.extend_from_slice(&json);
        bytes.resize(
            m.json_start + json.len().max(m.json_end - m.json_start),
            b' ',
        );
        bytes.extend_from_slice(&m.raw[m.json_end..]);
        bytes
    };
    let delta = (bytes.len() - m.raw.len()) as u64;
    resolved.insert(
        index,
        (
            bytes,
            OffsetShift {
                path: m.path.clone(),
                start: m.start,
                end: m.end,
                delta,
            },
        ),
    );
    visiting.remove(&index);
    Ok(())
}

/// Stage selected paginated metadata and every affected descendant in the same journal.
/// Legacy edits keep their existing path. Returned shifts are keyed by physical rollout path.
pub fn stage_metadata(
    journal: &mut crate::journal::Journal,
    settings: &Settings,
    items: &[sessions::Session],
    key: &str,
    value: &str,
    ct: &tokio_util::sync::CancellationToken,
) -> Result<std::collections::HashMap<String, OffsetShift>> {
    use std::io::Write;
    ensure!(
        matches!(key, "model_provider" | "cwd"),
        "不支持的元数据字段"
    );
    let selected = items
        .iter()
        .map(|s| path_key(&s.path))
        .collect::<Result<HashSet<_>>>()?;
    let mut nodes = vec![];
    for folder in ["sessions", "archived_sessions"] {
        let root = settings.codex_home.join(folder);
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(root).follow_links(false) {
            ensure!(!ct.is_cancelled(), "操作已取消");
            let entry = entry?;
            if !entry.file_type().is_file() || entry.path().extension().is_none_or(|e| e != "jsonl")
            {
                continue;
            }
            let path = sessions::owned(entry.path(), settings)?;
            let m = MetadataLine::read(path)?;
            if m.node["payload"]["history_mode"] == "paginated" {
                nodes.push(m);
            }
        }
    }
    let mut by_rollout = std::collections::HashMap::new();
    for (i, m) in nodes.iter().enumerate() {
        let id = rollout_id(&m.path).context("无法识别分页历史文件标识")?;
        ensure!(by_rollout.insert(id, i).is_none(), "分页历史文件标识重复");
    }
    let mut resolved = std::collections::HashMap::new();
    for i in 0..nodes.len() {
        ensure!(!ct.is_cancelled(), "操作已取消");
        resolve_metadata(
            i,
            &nodes,
            &by_rollout,
            &selected,
            key,
            value,
            &mut resolved,
            &mut HashSet::new(),
        )?;
    }
    let mut shifts = std::collections::HashMap::new();
    for (i, m) in nodes.iter().enumerate() {
        ensure!(!ct.is_cancelled(), "操作已取消");
        let (bytes, shift) = resolved.remove(&i).unwrap();
        if bytes == m.raw {
            continue;
        }
        if shift.delta > 0 {
            shifts.insert(path_key(&m.path)?, shift);
        }
        let temp = journal.dir.join(format!("{}.jsonl", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let before = crate::journal::hash(&m.path)?;
            let mut input = BufReader::new(File::open(&m.path)?);
            let mut output = File::create(&temp)?;
            let mut offset = 0;
            let mut found = false;
            let mut line = vec![];
            loop {
                line.clear();
                ensure!(!ct.is_cancelled(), "操作已取消");
                let len = input
                    .by_ref()
                    .take(32 * 1024 * 1024 + 1)
                    .read_until(b'\n', &mut line)?;
                if len == 0 {
                    break;
                }
                ensure!(len <= 32 * 1024 * 1024, "单条会话事件超过 32 MiB");
                if line.iter().all(u8::is_ascii_whitespace) {
                    output.write_all(&line)?;
                    offset += len as u64;
                    continue;
                }
                let text = std::str::from_utf8(&line)?.trim_start_matches('\u{feff}');
                let row: Value = serde_json::from_str(text)?;
                if row["type"] == "session_meta" {
                    ensure!(
                        !found && offset == m.start && line == m.raw,
                        "会话元数据重复或已被外部修改"
                    );
                    found = true;
                    output.write_all(&bytes)?;
                } else {
                    output.write_all(&line)?;
                }
                offset += len as u64;
            }
            ensure!(found, "会话元数据已消失");
            output.sync_all()?;
            drop(output);
            ensure!(
                crate::journal::hash(&m.path)? == before,
                "会话文件出现并发修改"
            );
            journal.stage_file(&m.path, &temp)?;
            ensure!(
                journal
                    .manifest
                    .files
                    .last()
                    .is_some_and(|c| c.path == m.path && c.before_hash == before),
                "备份期间会话文件出现并发修改"
            );
            Ok(())
        })();
        let _ = std::fs::remove_file(temp);
        result?;
    }
    Ok(shifts)
}

/// SQLite projections refer to the canonical rollout in threads.rollout_path, not every
/// revision sharing the same thread ID. Never apply a historical revision's delta twice.
pub fn active_shifts(
    settings: &Settings,
    shifts: &std::collections::HashMap<String, OffsetShift>,
) -> Result<std::collections::HashMap<String, OffsetShift>> {
    let mut active = std::collections::HashMap::new();
    if shifts.is_empty() {
        return Ok(active);
    }
    for path in sessions::databases(settings)? {
        let db = sessions::open(&path)?;
        if !sessions::columns(&db, "threads")?.contains("rollout_path") {
            continue;
        }
        let mut query = db.prepare("SELECT id,rollout_path FROM threads")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let path = PathBuf::from(row.get::<_, String>(1)?);
            if let Some(shift) = shifts.get(&path_key(&path)?) {
                ensure!(
                    active.get(&id).is_none_or(|existing| existing == shift),
                    "线程存在冲突的定位索引"
                );
                active.insert(id, shift.clone());
            }
        }
    }
    Ok(active)
}
pub fn rebase_database(
    db: &rusqlite::Connection,
    active: &std::collections::HashMap<String, OffsetShift>,
) -> Result<()> {
    if active.is_empty() {
        return Ok(());
    }
    let known = [
        ("thread_turns", "rollout_byte_offset"),
        ("thread_turns", "rollout_end_byte_offset"),
        (
            "thread_history_projection_state",
            "next_rollout_byte_offset",
        ),
    ];
    let tables = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for table in tables {
        let columns = sessions::columns(db, &table)?;
        for column in columns.iter().filter(|c| c.ends_with("byte_offset")) {
            ensure!(
                known.contains(&(table.as_str(), column.as_str())) && columns.contains("thread_id"),
                "未知分页历史偏移结构，未提交"
            );
            for (id, shift) in active {
                let mut query = db.prepare(&format!("SELECT DISTINCT {column} FROM {table} WHERE thread_id=?1 AND {column} IS NOT NULL"))?;
                let offsets = query
                    .query_map([id], |r| r.get::<_, i64>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                for offset in offsets {
                    ensure!(offset >= 0, "分页历史偏移为负数");
                    i64::try_from(shift.translate(offset as u64)?).context("数据库偏移溢出")?;
                }
                db.execute(&format!("UPDATE {table} SET {column}={column}+?1 WHERE thread_id=?2 AND {column}>=?3"),rusqlite::params![i64::try_from(shift.delta)?,id,i64::try_from(shift.end)?])?;
            }
        }
    }
    Ok(())
}
