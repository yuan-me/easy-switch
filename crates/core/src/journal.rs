use crate::{Result, crypto, store::atomic_write};
use anyhow::{Context, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    #[serde(alias = "Path")]
    pub path: PathBuf,
    #[serde(alias = "BeforeHash")]
    pub before_hash: Option<String>,
    #[serde(alias = "AfterHash")]
    pub after_hash: Option<String>,
    #[serde(alias = "Backup")]
    pub backup: Option<PathBuf>,
    #[serde(alias = "Stage")]
    pub stage: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    patch: Option<FilePatch>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
struct FilePatch {
    start: u64,
    end: u64,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(alias = "State")]
    pub state: String,
    #[serde(alias = "Description")]
    pub description: String,
    #[serde(alias = "Files")]
    pub files: Vec<Change>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationInfo {
    pub id: String,
    pub description: String,
    pub state: String,
    pub file_count: usize,
    pub legacy: bool,
}
pub fn finished(state: &str) -> bool {
    matches!(state, "Complete" | "Restored" | "RolledBack")
}

fn lock(root: &Path) -> Result<File> {
    crate::sessions::no_links(root)?;
    let path = root.join("operation.lock");
    crate::sessions::no_links(&path)?;
    let mut options = File::options();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(path)?;
    lock.try_lock_exclusive()
        .context("另一个写入操作正在进行")?;
    Ok(lock)
}

// Delete the manifest last so a failed cleanup can be retried without losing its state.
fn remove_operation(dir: &Path) -> Result<()> {
    crate::sessions::no_links(dir)?;
    let parent = dir.parent().context("操作目录无效")?;
    ensure!(
        parent.file_name().is_some_and(|n| n == "operations")
            && dir.canonicalize()?.parent() == Some(parent.canonicalize()?.as_path()),
        "操作目录越界"
    );
    let files = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    for file in &files {
        crate::sessions::no_links(&file.path())?;
        ensure!(
            file.file_type()?.is_file(),
            "操作目录包含未知子目录，未清理"
        );
    }
    for file in files.iter().filter(|f| f.file_name() != "manifest.json") {
        fs::remove_file(file.path())?;
    }
    let manifest = dir.join("manifest.json");
    if manifest.exists() {
        fs::remove_file(manifest)?;
    }
    fs::remove_dir(dir)?;
    Ok(())
}

fn cleanup_temps(m: &Manifest, allowed: &[PathBuf]) -> Result<()> {
    let mut parents = std::collections::HashSet::new();
    for change in &m.files {
        if let Some(parent) = change.path.parent().filter(|p| {
            allowed
                .iter()
                .any(|r| crate::sessions::within(p, r).unwrap_or(false))
        }) {
            parents.insert(parent.to_path_buf());
        }
    }
    for parent in parents {
        crate::sessions::no_links(&parent)?;
        if !parent.exists() {
            continue;
        }
        for temp in fs::read_dir(&parent)? {
            let temp = temp?;
            let name = temp.file_name();
            let name = name.to_string_lossy();
            if name
                .strip_prefix(".easy-switch-")
                .and_then(|n| n.strip_suffix(".restore"))
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
            {
                crate::sessions::no_links(&temp.path())?;
                ensure!(temp.file_type()?.is_file(), "暂存路径不是普通文件");
                fs::remove_file(temp.path())?;
            }
        }
    }
    Ok(())
}

fn cleanup_locked(root: &Path, allowed: &[PathBuf]) -> Result<()> {
    let ops = root.join("operations");
    crate::sessions::no_links(&ops)?;
    if !ops.exists() {
        return Ok(());
    }
    let mut errors = vec![];
    for entry in fs::read_dir(&ops)? {
        let entry = entry?;
        let result = (|| -> Result<()> {
            let dir = entry.path();
            crate::sessions::no_links(&dir)?;
            if !entry.file_type()?.is_dir() {
                return Ok(());
            }
            let path = dir.join("manifest.json");
            crate::sessions::no_links(&path)?;
            if !path.exists() {
                if fs::read_dir(&dir)?.next().is_none() {
                    fs::remove_dir(&dir)?;
                } else {
                    anyhow::bail!("缺少操作清单，已保留文件");
                }
                return Ok(());
            }
            let m: Manifest = serde_json::from_slice(&fs::read(path)?)?;
            // Preparing is persisted before staging; no target is written before Applying.
            if finished(&m.state) || m.state == "Preparing" {
                cleanup_temps(&m, allowed)?;
                remove_operation(&dir)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            errors.push(format!("{}：{e}", entry.file_name().to_string_lossy()));
        }
    }
    ensure!(
        errors.is_empty(),
        "部分操作副本未清理：{}",
        errors.join("；")
    );
    Ok(())
}

pub fn cleanup(root: &Path, allowed: &[PathBuf]) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let _lock = lock(root)?;
    cleanup_locked(root, allowed)
}
pub fn hash(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(Some(format!("{:X}", h.finalize())))
}
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}
fn matching_edges(before: &mut File, after: &mut File) -> Result<(u64, u64)> {
    let old_len = before.metadata()?.len();
    let new_len = after.metadata()?.len();
    let limit = old_len.min(new_len);
    let mut a = [0u8; 64 * 1024];
    let mut b = [0u8; 64 * 1024];
    let mut start = 0;
    while start < limit {
        let n = (limit - start).min(a.len() as u64) as usize;
        before.read_exact(&mut a[..n])?;
        after.read_exact(&mut b[..n])?;
        if let Some(different) = a[..n].iter().zip(&b[..n]).position(|(a, b)| a != b) {
            start += different as u64;
            break;
        }
        start += n as u64;
    }
    let mut suffix = 0;
    while suffix < limit - start {
        let n = (limit - start - suffix).min(a.len() as u64) as usize;
        before.seek(SeekFrom::Start(old_len - suffix - n as u64))?;
        after.seek(SeekFrom::Start(new_len - suffix - n as u64))?;
        before.read_exact(&mut a[..n])?;
        after.read_exact(&mut b[..n])?;
        if let Some(different) = a[..n]
            .iter()
            .rev()
            .zip(b[..n].iter().rev())
            .position(|(a, b)| a != b)
        {
            suffix += different as u64;
            break;
        }
        suffix += n as u64;
    }
    Ok((start, suffix))
}
pub fn list(root: &Path, legacy: bool) -> Result<Vec<OperationInfo>> {
    crate::sessions::no_links(root)?;
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut all = vec![];
    for d in fs::read_dir(root)? {
        let d = d?;
        crate::sessions::no_links(&d.path())?;
        if !d.file_type()?.is_dir() {
            continue;
        }
        let p = d.path().join("manifest.json");
        crate::sessions::no_links(&p)?;
        if !p.exists() {
            continue;
        }
        let m: Manifest = serde_json::from_slice(&fs::read(p)?)?;
        all.push(OperationInfo {
            id: d.file_name().to_string_lossy().into(),
            description: m.description,
            state: m.state,
            file_count: m.files.len(),
            legacy,
        });
    }
    all.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(all)
}
pub struct Journal {
    pub dir: PathBuf,
    pub manifest: Manifest,
    _lock: File,
}
impl Journal {
    pub fn new(root: &Path, description: &str) -> Result<Self> {
        Self::new_with_setup(root, description, &|| Ok(()))
    }
    pub fn new_with_setup(
        root: &Path,
        description: &str,
        setup: &impl Fn() -> Result<()>,
    ) -> Result<Self> {
        crate::store::private_dir(root)?;
        let lock = lock(root)?;
        cleanup_locked(root, &[])?;
        let ops = root.join("operations");
        crate::store::private_dir(&ops)?;
        ensure!(
            list(&ops, false)?.iter().all(|m| finished(&m.state)),
            "存在未完成操作，请先使用首页的中断恢复提示处理"
        );
        setup()?;
        let dir = ops.join(format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            uuid::Uuid::new_v4()
        ));
        crate::store::private_dir(&dir)?;
        let j = Self {
            dir,
            manifest: Manifest {
                state: "Preparing".into(),
                description: description.into(),
                files: vec![],
            },
            _lock: lock,
        };
        j.save()?;
        Ok(j)
    }
    fn save(&self) -> Result<()> {
        atomic_write(
            &self.dir.join("manifest.json"),
            &serde_json::to_vec_pretty(&self.manifest)?,
        )
    }
    pub fn stage(&mut self, path: &Path, data: Option<&[u8]>) -> Result<()> {
        self.stage_reader(
            path,
            data.map(|d| Box::new(std::io::Cursor::new(d.to_vec())) as Box<dyn Read>),
            data.map(hash_bytes),
            None,
            None,
        )
    }
    pub fn stage_file(&mut self, path: &Path, source: &Path) -> Result<()> {
        crate::sessions::no_links(path)?;
        let before = hash(path)?;
        let after = hash(source)?;
        let mut reader = File::open(source)?;
        if before.is_some() && before != after {
            let (start, suffix) = matching_edges(&mut File::open(path)?, &mut reader)?;
            // Retain full encrypted backups; avoid encrypting unchanged bodies a second time.
            if start + suffix >= 64 * 1024 {
                let end = fs::metadata(path)?
                    .len()
                    .checked_sub(suffix)
                    .context("暂存期间源文件被截断")?;
                let replacement_len = reader
                    .metadata()?
                    .len()
                    .checked_sub(start)
                    .and_then(|n| n.checked_sub(suffix))
                    .context("暂存文件被截断")?;
                reader.seek(SeekFrom::Start(start))?;
                return self.stage_reader(
                    path,
                    Some(Box::new(reader.take(replacement_len))),
                    after,
                    Some(FilePatch { start, end }),
                    Some(&before),
                );
            }
        }
        reader.rewind()?;
        self.stage_reader(path, Some(Box::new(reader)), after, None, Some(&before))
    }
    fn stage_reader(
        &mut self,
        path: &Path,
        data: Option<Box<dyn Read>>,
        after: Option<String>,
        patch: Option<FilePatch>,
        expected_before: Option<&Option<String>>,
    ) -> Result<()> {
        crate::sessions::no_links(path)?;
        let normalized = crate::sessions::path_key(path)?;
        ensure!(
            !self
                .manifest
                .files
                .iter()
                .any(|c| crate::sessions::path_key(&c.path).is_ok_and(|p| p == normalized)),
            "重复写入同一个文件"
        );
        let before = hash(path)?;
        ensure!(
            expected_before.is_none_or(|expected| expected == &before),
            "暂存期间源文件被修改"
        );
        if before == after {
            return Ok(());
        }
        let id = uuid::Uuid::new_v4();
        let backup = if before.is_some() {
            let p = self.dir.join(format!("{id}.before"));
            crypto::encrypt_file(path, &p)?;
            ensure!(hash(path)? == before, "备份期间源文件被修改");
            Some(p)
        } else {
            None
        };
        verify_payload(backup.as_deref(), &before, &self.dir)?;
        let stage = if let Some(reader) = data {
            let p = self.dir.join(format!("{id}.after"));
            crypto::encrypt_reader(reader, &p)?;
            Some(p)
        } else {
            None
        };
        self.manifest.files.push(Change {
            path: path.to_owned(),
            before_hash: before,
            after_hash: after,
            backup,
            stage,
            patch,
        });
        self.save()
    }
    pub fn commit(&mut self) -> Result<()> {
        self.commit_with_fault(None)
    }
    pub fn commit_with_fault(&mut self, fail_after: Option<usize>) -> Result<()> {
        self.commit_observed(fail_after, &|_| {}, &|| Ok(()), &|| Ok(()))
    }
    pub fn commit_with_progress(&mut self, progress: &impl Fn(crate::Progress)) -> Result<()> {
        self.commit_observed(None, progress, &|| Ok(()), &|| Ok(()))
    }
    pub fn commit_with_guard(
        &mut self,
        progress: &impl Fn(crate::Progress),
        guard: &impl Fn() -> Result<()>,
    ) -> Result<()> {
        self.commit_observed(None, progress, guard, &|| Ok(()))
    }
    pub fn commit_verified(
        &mut self,
        progress: &impl Fn(crate::Progress),
        guard: &impl Fn() -> Result<()>,
        verify: &impl Fn() -> Result<()>,
    ) -> Result<()> {
        self.commit_observed(None, progress, guard, verify)
    }
    fn commit_observed(
        &mut self,
        fail_after: Option<usize>,
        progress: &impl Fn(crate::Progress),
        guard: &impl Fn() -> Result<()>,
        verify: &impl Fn() -> Result<()>,
    ) -> Result<()> {
        guard()?;
        let total = self.manifest.files.len() * 2;
        let report = |detail: &str, completed, total| {
            progress(crate::Progress {
                cancellable: false,
                ..crate::Progress::new("提交与校验", detail, completed, total)
            })
        };
        for (i, c) in self.manifest.files.iter().enumerate() {
            report("校验源文件与备份摘要（此阶段不可取消）", i, Some(total));
            ensure!(hash(&c.path)? == c.before_hash, "文件已被外部修改，未提交");
            check_wal(&c.path)?;
            if let Some(patch) = &c.patch {
                let stage = c.stage.as_deref().context("暂存记录缺少文件")?;
                safe_payload(stage, &self.dir)?;
                let mut sink = HashWriter(Sha256::new());
                write_payload(stage, &c.path, Some(patch), &mut sink)?;
                ensure!(
                    Some(format!("{:X}", sink.0.finalize())) == c.after_hash,
                    "备份或暂存摘要不符"
                );
            } else {
                verify_payload(c.stage.as_deref(), &c.after_hash, &self.dir)?;
            }
        }
        guard()?;
        self.manifest.state = "Applying".into();
        self.save()?;
        let result = (|| {
            for (i, c) in self.manifest.files.iter().enumerate() {
                report(
                    "写入并校验文件（此阶段不可取消）",
                    self.manifest.files.len() + i,
                    Some(total),
                );
                guard()?;
                ensure!(hash(&c.path)? == c.before_hash, "提交期间发现并发修改");
                check_wal(&c.path)?;
                replace(c.stage.as_deref(), &c.path, c.patch.as_ref(), &c.after_hash)?;
                ensure!(hash(&c.path)? == c.after_hash, "写入后校验失败");
                if fail_after == Some(i) {
                    anyhow::bail!("模拟提交失败");
                }
            }
            guard()?;
            verify()
        })();
        if let Err(e) = result {
            report("提交失败，正在自动回滚，请勿关闭程序", 0, None);
            match self.rollback_with_guard(guard) {
                Ok(()) => return Err(e),
                Err(r) => return Err(anyhow::anyhow!("操作失败；回滚尚未完成：{r}")),
            }
        }
        self.manifest.state = "Complete".into();
        self.save()?;
        report("提交与校验完成", total, Some(total));
        Ok(())
    }
    pub fn rollback(&mut self) -> Result<()> {
        self.rollback_with_guard(&|| Ok(()))
    }
    fn rollback_with_guard(&mut self, guard: &impl Fn() -> Result<()>) -> Result<()> {
        rollback_manifest(&self.dir, &mut self.manifest, guard)?;
        self.save()
    }
}
impl Drop for Journal {
    fn drop(&mut self) {
        if self.manifest.state == "Preparing" {
            self.manifest.state = "RolledBack".into();
            if self.save().is_err() {
                return;
            }
        }
        if finished(&self.manifest.state) {
            if let Err(e) = remove_operation(&self.dir) {
                eprintln!("操作副本清理未完成，下次启动重试：{e}");
            }
        }
    }
}
fn safe_payload(path: &Path, dir: &Path) -> Result<()> {
    crate::sessions::no_links(path)?;
    crate::sessions::no_links(dir)?;
    let full = path.canonicalize()?;
    ensure!(full.starts_with(dir.canonicalize()?), "备份文件越界");
    Ok(())
}
fn verify_payload(path: Option<&Path>, expected: &Option<String>, dir: &Path) -> Result<()> {
    if let Some(p) = path {
        safe_payload(p, dir)?;
        let mut sink = HashWriter(Sha256::new());
        crypto::decrypt_file(p, &mut sink)?;
        ensure!(
            Some(format!("{:X}", sink.0.finalize())) == *expected,
            "备份或暂存摘要不符"
        );
    } else {
        ensure!(expected.is_none(), "备份记录缺少文件");
    }
    Ok(())
}
struct HashWriter(Sha256);
impl Write for HashWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn check_wal(path: &Path) -> Result<()> {
    crate::sessions::no_links(path)?;
    if matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("db" | "sqlite")
    ) {
        for suffix in ["-wal", "-journal"] {
            let wal = PathBuf::from(format!("{}{suffix}", path.display()));
            ensure!(
                !wal.exists() || fs::metadata(wal)?.len() == 0,
                "数据库存在未完成写入，停止覆盖"
            );
        }
    }
    Ok(())
}
fn write_payload(
    payload: &Path,
    source: &Path,
    patch: Option<&FilePatch>,
    mut out: impl Write,
) -> Result<()> {
    if let Some(patch) = patch {
        let mut input = File::open(source)?;
        ensure!(
            patch.start <= patch.end && patch.end <= input.metadata()?.len(),
            "暂存差量范围无效"
        );
        ensure!(
            std::io::copy(&mut Read::by_ref(&mut input).take(patch.start), &mut out)?
                == patch.start,
            "暂存期间源文件被截断"
        );
        crypto::decrypt_file(payload, &mut out)?;
        input.seek(SeekFrom::Start(patch.end))?;
        std::io::copy(&mut input, &mut out)?;
        Ok(())
    } else {
        crypto::decrypt_file(payload, out)
    }
}
fn replace(
    payload: Option<&Path>,
    dest: &Path,
    patch: Option<&FilePatch>,
    expected: &Option<String>,
) -> Result<()> {
    if let Some(p) = payload {
        let parent = dest.parent().context("目标路径没有父目录")?;
        crate::store::private_dir(parent)?;
        let temp = parent.join(format!(".easy-switch-{}.restore", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut out = crate::store::private_file(&temp, true)?;
            write_payload(p, dest, patch, &mut out)?;
            out.sync_all()?;
            drop(out);
            ensure!(&hash(&temp)? == expected, "暂存写入摘要不符");
            replace_file(&temp, dest)
        })();
        let _ = fs::remove_file(temp);
        result
    } else {
        if dest.exists() {
            fs::remove_file(dest)?;
            crate::store::sync_parent(dest)?;
        }
        Ok(())
    }
}
pub fn replace_file(source: &Path, dest: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(a: *const u16, b: *const u16, flags: u32) -> i32;
        }
        let a: Vec<u16> = source.as_os_str().encode_wide().chain([0]).collect();
        let b: Vec<u16> = dest.as_os_str().encode_wide().chain([0]).collect();
        ensure!(
            unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 9) } != 0,
            "替换文件失败：{}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(not(windows))]
    fs::rename(source, dest)?;
    crate::store::sync_parent(dest)?;
    Ok(())
}
fn verify_restore(dir: &Path, m: &Manifest) -> Result<()> {
    for c in &m.files {
        let h = hash(&c.path)?;
        ensure!(
            h == c.before_hash || h == c.after_hash,
            "检测到外部改动，拒绝覆盖：{}",
            c.path.display()
        );
        check_wal(&c.path)?;
        verify_payload(c.backup.as_deref(), &c.before_hash, dir)?;
    }
    Ok(())
}
fn rollback_manifest(dir: &Path, m: &mut Manifest, guard: &impl Fn() -> Result<()>) -> Result<()> {
    guard()?;
    verify_restore(dir, m)?;
    for c in m.files.iter().rev() {
        guard()?;
        let current = hash(&c.path)?;
        ensure!(
            current == c.before_hash || current == c.after_hash,
            "恢复期间发现外部改动，拒绝覆盖"
        );
        check_wal(&c.path)?;
        if current != c.before_hash {
            replace(c.backup.as_deref(), &c.path, None, &c.before_hash)?;
            ensure!(hash(&c.path)? == c.before_hash, "恢复校验失败");
        }
    }
    m.state = "Restored".into();
    Ok(())
}
pub fn restore(root: &Path, id: &str, allowed: &[PathBuf]) -> Result<()> {
    restore_with_guard(root, id, allowed, &|| Ok(()))
}
pub fn restore_with_guard(
    root: &Path,
    id: &str,
    allowed: &[PathBuf],
    guard: &impl Fn() -> Result<()>,
) -> Result<()> {
    guard()?;
    ensure!(
        !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "备份 ID 无效"
    );
    let _lock = lock(root)?;
    let dir = root.join("operations").join(id);
    let path = dir.join("manifest.json");
    crate::sessions::no_links(&path)?;
    let mut m: Manifest = serde_json::from_slice(&fs::read(&path)?)?;
    ensure!(!finished(&m.state), "操作已经结束，不再提供历史备份恢复");
    for c in &m.files {
        ensure!(
            allowed
                .iter()
                .any(|r| crate::sessions::within(&c.path, r).unwrap_or(false)),
            "恢复目标不属于受管理目录"
        );
        crate::sessions::no_links(&c.path)?;
    }
    verify_restore(&dir, &m)?;
    m.state = "Restoring".into();
    atomic_write(&path, &serde_json::to_vec_pretty(&m)?)?;
    rollback_manifest(&dir, &mut m, guard)?;
    atomic_write(&path, &serde_json::to_vec_pretty(&m)?)?;
    cleanup_temps(&m, allowed)?;
    remove_operation(&dir)
}
