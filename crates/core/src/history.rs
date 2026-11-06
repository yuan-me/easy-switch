//! Codex paginated histories identify immutable rollouts separately from threads.
//! Metadata growth rebases physical byte positions without changing event ordinals or bodies.
use crate::{Result, Settings, sessions};
use anyhow::{Context, ensure};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Read},
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
    Ok(segment_readers(path)?.into_iter().flat_map(|r| r.lines()))
}

fn segment_readers(path: &Path) -> Result<Vec<BufReader<std::io::Take<File>>>> {
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
    segments
        .into_iter()
        .map(|(p, n)| Ok(BufReader::new(File::open(p)?.take(n))))
        .collect::<Result<Vec<_>>>()
}

fn validate_settings_snapshot(settings: &Value) -> Result<()> {
    let nonempty = |v: &Value| v.as_str().is_some_and(|s| !s.is_empty());
    ensure!(
        settings.is_object()
            && nonempty(&settings["model"])
            && nonempty(&settings["model_provider_id"])
            && settings["cwd"]
                .as_str()
                .is_some_and(|p| Path::new(p).is_absolute()),
        "线程设置快照缺少模型、服务商或绝对工作目录"
    );
    let approval = &settings["approval_policy"];
    let approval_valid = matches!(
        approval.as_str(),
        Some("untrusted" | "on-request" | "on-failure" | "never")
    ) || approval.get("granular").is_some_and(|v| {
        ["sandbox_approval", "rules", "mcp_elicitations"]
            .iter()
            .all(|k| v[*k].is_boolean())
            && ["skill_approval", "request_permissions"]
                .iter()
                .all(|k| v.get(*k).is_none_or(Value::is_boolean))
    });
    ensure!(
        approval_valid
            && matches!(
                settings["approvals_reviewer"].as_str(),
                Some("user" | "auto_review" | "guardian_subagent")
            ),
        "线程设置快照缺少有效的审批配置"
    );
    let permissions = &settings["permission_profile"];
    let entries_valid = |v: &Value| {
        v.as_array().is_some_and(|items| {
            items.iter().all(|item| {
                matches!(
                    item["access"].as_str(),
                    Some("read" | "write" | "deny" | "none")
                ) && match item["path"]["type"].as_str() {
                    Some("path") => nonempty(&item["path"]["path"]),
                    Some("glob_pattern") => nonempty(&item["path"]["pattern"]),
                    Some("special") => nonempty(&item["path"]["value"]["kind"]),
                    _ => false,
                }
            })
        })
    };
    let network_valid = || {
        matches!(
            permissions["network"].as_str(),
            Some("restricted" | "enabled")
        )
    };
    let permissions_valid = match permissions["type"].as_str() {
        Some("disabled") => true,
        Some("external") => network_valid(),
        Some("managed") => {
            network_valid()
                && match permissions["file_system"]["type"].as_str() {
                    Some("unrestricted") => true,
                    Some("restricted") => entries_valid(&permissions["file_system"]["entries"]),
                    _ => false,
                }
        }
        None => permissions.as_object().is_some_and(|p| {
            p.keys()
                .all(|k| matches!(k.as_str(), "network" | "file_system"))
                && p.get("network").is_none_or(|v| {
                    v.is_null()
                        || (v.is_object()
                            && v.get("enabled")
                                .is_none_or(|v| v.is_null() || v.is_boolean()))
                })
                && p.get("file_system")
                    .is_none_or(|v| v.is_null() || entries_valid(&v["entries"]))
        }),
        _ => false,
    };
    ensure!(
        permissions_valid,
        "线程设置快照缺少完整且受支持的权限配置，未生成新权限"
    );
    let collaboration = &settings["collaboration_mode"];
    ensure!(
        matches!(
            collaboration["mode"].as_str(),
            Some("default" | "plan" | "code" | "pair_programming" | "execute" | "custom")
        ) && nonempty(&collaboration["settings"]["model"]),
        "线程设置快照缺少完整的协作模式"
    );
    ensure!(
        settings
            .get("disabled_plugin_ids")
            .is_none_or(|v| v.as_array().is_some_and(|v| v.iter().all(Value::is_string))),
        "线程设置快照的插件列表无效"
    );
    Ok(())
}

/// Read bounded lines from the effective history, retaining only the latest owned snapshot.
/// The event shape is pinned to Codex 0.164.0-alpha.11.1; permissions are copied, never invented.
pub(crate) fn prepare_model_events(
    items: &[sessions::Session],
    provider: Option<&str>,
    model: Option<&str>,
    ct: &tokio_util::sync::CancellationToken,
) -> Result<std::collections::HashMap<PathBuf, Vec<u8>>> {
    let mut events = std::collections::HashMap::new();
    for item in items {
        let current = header(&item.path)?;
        ensure!(current.id == item.id, "线程索引与会话身份不一致，未修改");
        let readers = segment_readers(&item.path)?;
        let current_segment = readers.len() - 1;
        let mut latest = None;
        let mut model_history = false;
        let mut last_ordinal = None;
        let mut current_metadata = None;
        for (segment, mut reader) in readers.into_iter().enumerate() {
            let mut line = vec![];
            loop {
                ensure!(!ct.is_cancelled(), "操作已取消");
                line.clear();
                let length = reader
                    .by_ref()
                    .take(32 * 1024 * 1024 + 1)
                    .read_until(b'\n', &mut line)?;
                if length == 0 {
                    break;
                }
                ensure!(length <= 32 * 1024 * 1024, "单条会话事件超过 32 MiB");
                let text = std::str::from_utf8(&line)?
                    .trim_start_matches('\u{feff}')
                    .trim();
                if text.is_empty() {
                    continue;
                }
                let row: Value = serde_json::from_str(text)?;
                if segment == current_segment {
                    last_ordinal = row["ordinal"].as_u64();
                    if current_metadata.is_none() && row["type"] == "session_meta" {
                        current_metadata = Some(row.clone());
                    }
                }
                if row["type"] == "turn_context" && !row["payload"]["model"].is_null() {
                    model_history = true;
                }
                if row["type"] == "event_msg" && row["payload"]["type"] == "thread_settings_applied"
                {
                    model_history = true;
                    if row["payload"]["thread_id"].as_str() == Some(item.id.as_str()) {
                        latest = Some(row);
                    }
                }
            }
        }
        if !model_history {
            continue;
        }
        let model = model
            .filter(|m| !m.trim().is_empty())
            .context("旧聊天存在模型历史，请明确指定目标模型后再切换")?;
        let mut event =
            latest.context("旧聊天缺少归属于当前线程的完整设置快照，无法安全切换模型")?;
        validate_settings_snapshot(&event["payload"]["thread_settings"])?;
        if current.paginated {
            let last = last_ordinal.context("分页历史最后一条事件缺少 ordinal，未追加设置事件")?;
            if let Some(prefix) = current_metadata
                .as_ref()
                .and_then(|m| m["payload"]["subagent_history_start_ordinal"].as_u64())
                .and_then(|n| n.checked_sub(1))
            {
                ensure!(
                    last >= prefix,
                    "分页子线程继承前缀未完整保存，未追加设置事件"
                );
            }
            event["ordinal"] = last
                .checked_add(1)
                .context("分页历史 ordinal 已溢出")?
                .into();
        } else {
            event
                .as_object_mut()
                .context("线程设置事件结构无效")?
                .remove("ordinal");
        }
        if let Some(provider) = provider {
            event["timestamp"] = chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
                .into();
            let snapshot = &mut event["payload"]["thread_settings"];
            snapshot["model"] = model.into();
            snapshot["model_provider_id"] = provider.into();
            snapshot["collaboration_mode"]["settings"]["model"] = model.into();
            let mut bytes = serde_json::to_vec(&event)?;
            bytes.push(b'\n');
            ensure!(
                events.insert(path_key(&item.path)?, bytes).is_none(),
                "当前会话路径重复"
            );
        }
    }
    Ok(events)
}

pub(crate) fn append_projection_event(path: &Path, event: &[u8]) -> Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    let mut file = File::options().read(true).append(true).open(path)?;
    if file.metadata()?.len() > 0 {
        file.seek(SeekFrom::End(-1))?;
        let mut last = [0];
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            file.write_all(b"\n")?;
        }
    }
    file.write_all(event)?;
    file.sync_all()?;
    Ok(())
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

pub(crate) fn inherited_metadata(owner: &Value, row: &Value, ordinal: u64) -> bool {
    let owner = &owner["payload"];
    let row = &row["payload"];
    let Some(id) = row["id"].as_str().filter(|id| !id.is_empty()) else {
        return false;
    };
    // Subagents can copy the parent's metadata inside their inherited history prefix.
    owner["id"].as_str().is_some_and(|own| own != id)
        && owner["forked_from_id"].as_str() == Some(id)
        && owner["session_id"].as_str().is_some_and(|session| {
            !session.is_empty() && row["session_id"].as_str() == Some(session)
        })
        && owner["subagent_history_start_ordinal"]
            .as_u64()
            .is_some_and(|end| ordinal > 0 && ordinal < end)
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
            if deleting
                .iter()
                .any(|p| sessions::same_path(p, &path).unwrap_or(false))
            {
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
pub struct OffsetShift {
    path: PathBuf,
    start: u64,
    end: u64,
    delta: u64,
    source_boundaries: Option<(u64, Vec<u64>)>,
}
impl OffsetShift {
    fn translate(&self, offset: u64) -> Result<u64> {
        use std::io::{Seek, SeekFrom};
        if let Some((len, boundaries)) = &self.source_boundaries {
            ensure!(offset <= *len, "分页历史偏移超出文件末尾");
            ensure!(
                boundaries.binary_search(&offset).is_ok(),
                "分页历史偏移未对齐事件边界"
            );
        } else {
            let mut file = File::open(&self.path)?;
            ensure!(offset <= file.metadata()?.len(), "分页历史偏移超出文件末尾");
            if offset > 0 {
                file.seek(SeekFrom::Start(offset - 1))?;
                let mut byte = [0];
                file.read_exact(&mut byte)?;
                ensure!(byte[0] == b'\n', "分页历史偏移未对齐事件边界");
            }
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
fn path_key(path: &Path) -> Result<PathBuf> {
    sessions::path_key(path)
}

// Acyclic history_base references are resolved parent first. Padding is retained when
// possible, so switching back to a shorter provider does not move offsets again.
fn resolve_metadata(
    index: usize,
    nodes: &[MetadataLine],
    by_rollout: &std::collections::HashMap<String, usize>,
    selected: &HashSet<PathBuf>,
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
                source_boundaries: None,
            },
        ),
    );
    visiting.remove(&index);
    Ok(())
}

/// Stage selected paginated metadata and every affected descendant in the same journal.
/// Legacy edits keep their existing path. SQLite projection thread_id keys are physical
/// rollout IDs, not logical thread IDs or the current threads.rollout_path.
pub fn stage_metadata(
    journal: &mut crate::journal::Journal,
    settings: &Settings,
    items: &[sessions::Session],
    key: &str,
    value: &str,
    ct: &tokio_util::sync::CancellationToken,
    progress: &impl Fn(crate::Progress),
) -> Result<std::collections::HashMap<String, OffsetShift>> {
    stage_metadata_with_events(
        journal,
        settings,
        items,
        key,
        value,
        &std::collections::HashMap::new(),
        ct,
        progress,
    )
}

pub(crate) fn stage_metadata_with_events(
    journal: &mut crate::journal::Journal,
    settings: &Settings,
    items: &[sessions::Session],
    key: &str,
    value: &str,
    events: &std::collections::HashMap<PathBuf, Vec<u8>>,
    ct: &tokio_util::sync::CancellationToken,
    progress: &impl Fn(crate::Progress),
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
    let mut scanned = 0;
    progress(crate::Progress::new(
        "修复会话",
        "扫描分页历史关联",
        0,
        None,
    ));
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
            scanned += 1;
            progress(crate::Progress::new(
                "修复会话",
                &format!("检查分页历史：已扫描 {scanned} 个文件"),
                scanned,
                None,
            ));
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
        progress(crate::Progress::new(
            "修复会话",
            "校验分页历史引用与偏移",
            i,
            Some(nodes.len()),
        ));
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
        progress(crate::Progress::new(
            "修复会话",
            "备份并修复分页历史文件",
            i,
            Some(nodes.len()),
        ));
        let (bytes, mut shift) = resolved.remove(&i).unwrap();
        let event = events.get(&path_key(&m.path)?);
        if bytes == m.raw && event.is_none() {
            continue;
        }
        let temp = journal.dir.join(format!("{}.jsonl", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let before = crate::journal::hash(&m.path)?;
            let mut input = BufReader::new(File::open(&m.path)?);
            let mut output =
                BufWriter::with_capacity(64 * 1024, crate::store::private_file(&temp, true)?);
            let mut offset = 0;
            let mut ordinal = 0;
            let mut found = false;
            let mut line = vec![];
            let mut boundaries = vec![0];
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
                if shift.delta > 0 && line.last() == Some(&b'\n') {
                    boundaries.push(offset + len as u64);
                }
                if line.iter().all(u8::is_ascii_whitespace) {
                    output.write_all(&line)?;
                    offset += len as u64;
                    continue;
                }
                let text = std::str::from_utf8(&line)?.trim_start_matches('\u{feff}');
                let row: Value = serde_json::from_str(text)?;
                if row["type"] == "session_meta" {
                    if !found {
                        ensure!(
                            offset == m.start && line == m.raw,
                            "会话元数据已被外部修改，请关闭 Codex 后重试"
                        );
                        found = true;
                        output.write_all(&bytes)?;
                    } else {
                        ensure!(
                            inherited_metadata(&m.node, &row, ordinal),
                            "会话元数据重复且无法确认继承关系，未修改"
                        );
                        output.write_all(&line)?;
                    }
                } else {
                    output.write_all(&line)?;
                }
                ordinal += 1;
                offset += len as u64;
            }
            ensure!(found, "会话元数据已消失");
            output.flush()?;
            output.get_ref().sync_all()?;
            drop(output);
            if let Some(event) = event {
                append_projection_event(&temp, event)?;
            }
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
            // Reuse the validated source boundaries; commit still checks its full hash.
            if shift.delta > 0 {
                shift.source_boundaries = Some((offset, boundaries));
            }
            Ok(())
        })();
        let _ = std::fs::remove_file(temp);
        result.map_err(|e| {
            anyhow::anyhow!(
                "修复会话失败（{}）：{e}",
                m.path.file_name().unwrap_or_default().to_string_lossy()
            )
        })?;
        if shift.delta > 0 {
            shifts.insert(
                rollout_id(&m.path).context("无法识别分页历史文件标识")?,
                shift,
            );
        }
    }
    progress(crate::Progress::new(
        "修复会话",
        "分页历史处理完成",
        nodes.len(),
        Some(nodes.len()),
    ));
    Ok(shifts)
}

pub fn rebase_database(
    db: &rusqlite::Connection,
    shifts: &std::collections::HashMap<String, OffsetShift>,
) -> Result<()> {
    if shifts.is_empty() {
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
            for (id, shift) in shifts {
                let mut query = db.prepare(&format!("SELECT DISTINCT {column} FROM {table} WHERE thread_id=?1 AND {column} IS NOT NULL"))?;
                let offsets = query
                    .query_map([id], |r| r.get::<_, i64>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                for offset in offsets {
                    ensure!(offset >= 0, "分页历史偏移为负数");
                    let translated = shift.translate(offset as u64).with_context(|| {
                        format!("分页索引异常：{table}.{column}，历史文件 {id}，偏移 {offset}")
                    })?;
                    i64::try_from(translated).context("数据库偏移溢出")?;
                }
                db.execute(&format!("UPDATE {table} SET {column}={column}+?1 WHERE thread_id=?2 AND {column}>=?3"),rusqlite::params![i64::try_from(shift.delta)?,id,i64::try_from(shift.end)?])?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_boundaries_match_file_validation_including_unterminated_tail() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("history.jsonl");
        let bytes = b"prefix\nmeta\none\nlast";
        std::fs::write(&path, bytes).unwrap();
        let mut shift = OffsetShift {
            path,
            start: 7,
            end: 12,
            delta: 9,
            source_boundaries: None,
        };
        let expected: Vec<_> = (0..=bytes.len() as u64 + 1)
            .map(|offset| shift.translate(offset).map_err(|e| e.to_string()))
            .collect();
        shift.source_boundaries = Some((bytes.len() as u64, vec![0, 7, 12, 16]));
        for (offset, expected) in expected.into_iter().enumerate() {
            assert_eq!(
                shift.translate(offset as u64).map_err(|e| e.to_string()),
                expected
            );
        }
    }
}
