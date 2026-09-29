use crate::{Result, crypto, store::atomic_write};
use anyhow::{Context, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
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
pub fn list(root: &Path, legacy: bool) -> Result<Vec<OperationInfo>> {
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut all = vec![];
    for d in fs::read_dir(root)? {
        let d = d?;
        if !d.file_type()?.is_dir() {
            continue;
        }
        let p = d.path().join("manifest.json");
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
        fs::create_dir_all(root)?;
        let lock = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(root.join("operation.lock"))?;
        lock.try_lock_exclusive()
            .context("另一个写入操作正在进行")?;
        let ops = root.join("operations");
        fs::create_dir_all(&ops)?;
        ensure!(
            list(&ops, false)?
                .iter()
                .all(|m| matches!(m.state.as_str(), "Complete" | "Restored" | "RolledBack")),
            "存在未完成操作，请先在备份恢复中处理"
        );
        let dir = ops.join(format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&dir)?;
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
        )
    }
    pub fn stage_file(&mut self, path: &Path, source: &Path) -> Result<()> {
        self.stage_reader(path, Some(Box::new(File::open(source)?)), hash(source)?)
    }
    fn stage_reader(
        &mut self,
        path: &Path,
        data: Option<Box<dyn Read>>,
        after: Option<String>,
    ) -> Result<()> {
        crate::sessions::no_links(path)?;
        let normalized = crate::sessions::normalize(path)?
            .to_string_lossy()
            .to_lowercase();
        ensure!(
            !self
                .manifest
                .files
                .iter()
                .any(|c| crate::sessions::normalize(&c.path)
                    .is_ok_and(|p| p.to_string_lossy().to_lowercase() == normalized)),
            "重复写入同一个文件"
        );
        let before = hash(path)?;
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
        });
        self.save()
    }
    pub fn commit(&mut self) -> Result<()> {
        self.commit_with_fault(None)
    }
    pub fn commit_with_fault(&mut self, fail_after: Option<usize>) -> Result<()> {
        for c in &self.manifest.files {
            ensure!(hash(&c.path)? == c.before_hash, "文件已被外部修改，未提交");
            check_wal(&c.path)?;
            verify_payload(c.stage.as_deref(), &c.after_hash, &self.dir)?;
        }
        self.manifest.state = "Applying".into();
        self.save()?;
        let result = (|| {
            for (i, c) in self.manifest.files.iter().enumerate() {
                ensure!(hash(&c.path)? == c.before_hash, "提交期间发现并发修改");
                check_wal(&c.path)?;
                replace(c.stage.as_deref(), &c.path)?;
                ensure!(hash(&c.path)? == c.after_hash, "写入后校验失败");
                if fail_after == Some(i) {
                    anyhow::bail!("模拟提交失败");
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            match self.rollback() {
                Ok(()) => return Err(e),
                Err(r) => return Err(anyhow::anyhow!("操作失败；回滚尚未完成：{r}")),
            }
        }
        self.manifest.state = "Complete".into();
        self.save()
    }
    pub fn rollback(&mut self) -> Result<()> {
        rollback_manifest(&self.dir, &mut self.manifest)?;
        self.save()
    }
}
impl Drop for Journal {
    fn drop(&mut self) {
        if self.manifest.state == "Preparing" {
            self.manifest.state = "RolledBack".into();
            let _ = self.save();
        }
    }
}
fn safe_payload(path: &Path, dir: &Path) -> Result<()> {
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
fn replace(payload: Option<&Path>, dest: &Path) -> Result<()> {
    if let Some(p) = payload {
        let parent = dest.parent().context("目标路径没有父目录")?;
        fs::create_dir_all(parent)?;
        let temp = parent.join(format!(".easy-switch-{}.restore", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut out = File::options().create_new(true).write(true).open(&temp)?;
            crypto::decrypt_file(p, &mut out)?;
            out.sync_all()?;
            drop(out);
            replace_file(&temp, dest)
        })();
        let _ = fs::remove_file(temp);
        result
    } else {
        if dest.exists() {
            fs::remove_file(dest)?;
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
fn rollback_manifest(dir: &Path, m: &mut Manifest) -> Result<()> {
    verify_restore(dir, m)?;
    for c in m.files.iter().rev() {
        if hash(&c.path)? != c.before_hash {
            replace(c.backup.as_deref(), &c.path)?;
            ensure!(hash(&c.path)? == c.before_hash, "恢复校验失败");
        }
    }
    m.state = "Restored".into();
    Ok(())
}
pub fn restore(root: &Path, id: &str, allowed: &[PathBuf]) -> Result<()> {
    ensure!(
        !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "备份 ID 无效"
    );
    let lock = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(root.join("operation.lock"))?;
    lock.try_lock_exclusive()?;
    let dir = root.join("operations").join(id);
    let path = dir.join("manifest.json");
    let mut m: Manifest = serde_json::from_slice(&fs::read(&path)?)?;
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
    rollback_manifest(&dir, &mut m)?;
    atomic_write(&path, &serde_json::to_vec_pretty(&m)?)
}
