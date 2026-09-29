//! Codex paginated histories identify immutable rollouts separately from threads.
//! Offset-preserving metadata edits deliberately leave all history references intact.
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
