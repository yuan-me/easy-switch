use crate::{Result, crypto, model::*};
use anyhow::{Context, ensure};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("文件没有父目录")?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn MoveFileExW(a: *const u16, b: *const u16, flags: u32) -> i32;
            }
            let a: Vec<u16> = temp.as_os_str().encode_wide().chain([0]).collect();
            let b: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
            ensure!(
                unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 1 | 8) } != 0,
                "原子替换失败：{}",
                std::io::Error::last_os_error()
            );
        }
        #[cfg(not(windows))]
        fs::rename(&temp, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(temp);
    result
}
impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
    pub fn default_root() -> PathBuf {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("EasySwitch")
    }
    pub fn read<T: DeserializeOwned>(&self, name: &str, fallback: T) -> Result<T> {
        let p = self.root.join(name);
        if !p.exists() {
            return Ok(fallback);
        }
        Ok(serde_json::from_slice(&fs::read(p)?)?)
    }
    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        atomic_write(&self.root.join(name), &serde_json::to_vec_pretty(value)?)
    }
    pub fn providers(&self) -> Result<Vec<Provider>> {
        let mut all: Vec<Provider> = self.read("providers.json", vec![Provider::official()])?;
        for p in &mut all {
            if let Some(cipher) = &p.protected_headers {
                p.headers = serde_json::from_str(&crypto::unprotect(cipher)?)?;
            }
        }
        Ok(all)
    }
    fn save_providers(&self, all: &[Provider]) -> Result<()> {
        let mut encrypted = all.to_vec();
        for p in &mut encrypted {
            p.protected_headers = if p.headers.is_empty() {
                None
            } else {
                Some(crypto::protect(&serde_json::to_string(&p.headers)?)?)
            };
            p.headers.clear();
        }
        self.save("providers.json", &encrypted)
    }
    pub fn settings(&self) -> Result<Settings> {
        let mut s: Settings = self.read("settings.json", Settings::default())?;
        s.active_provider_id = self.read("active-provider.json", s.active_provider_id)?;
        Ok(s)
    }
    pub fn runtime_key(&self) -> Result<String> {
        let mut s = self.settings()?;
        if let Some(key) = s.runtime_key {
            return crypto::unprotect(&key);
        }
        let key = uuid::Uuid::new_v4().to_string() + &uuid::Uuid::new_v4().to_string();
        s.runtime_key = Some(crypto::protect(&key)?);
        self.save("settings.json", &s)?;
        Ok(key)
    }
    pub fn synchronize_active(&self) -> Result<()> {
        let s = self.settings()?;
        let path = s.codex_home.join("config.toml");
        let text = if path.exists() {
            fs::read_to_string(path)?
        } else {
            String::new()
        };
        let doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|_| anyhow::anyhow!("配置语法无效，无法同步当前供应商"))?;
        let id = doc
            .get("model_provider")
            .and_then(toml_edit::Item::as_str)
            .unwrap_or("openai");
        let active = self
            .providers()?
            .into_iter()
            .find(|p| p.provider_id() == id)
            .map(|p| p.id);
        self.save("active-provider.json", &active)
    }
    pub fn save_provider(&self, mut p: Provider, key: Option<String>) -> Result<()> {
        let mut all = self.providers()?;
        let old = all.iter().find(|x| x.id == p.id);
        for (name, value) in &mut p.headers {
            if value.is_empty() {
                if let Some(v) = old.and_then(|p| p.headers.get(name)) {
                    *value = v.clone();
                }
            }
        }
        p.protected_key = if let Some(k) = key.filter(|s| !s.trim().is_empty()) {
            Some(crypto::protect(k.trim())?)
        } else {
            old.and_then(|p| p.protected_key.clone())
        };
        p.validate(&all)?;
        if let Some(i) = all.iter().position(|x| x.id == p.id) {
            all[i] = p;
        } else {
            all.push(p);
        }
        self.save_providers(&all)
    }
    pub fn delete_provider(&self, id: &str) -> Result<()> {
        let mut all = self.providers()?;
        ensure!(id != "official", "不能删除官方登录入口");
        ensure!(
            self.settings()?.active_provider_id.as_deref() != Some(id),
            "请先切换到其他供应商"
        );
        ensure!(
            !all.iter()
                .any(|p| p.members.iter().any(|m| m.provider_id == id)),
            "供应商仍被聚合路由引用"
        );
        all.retain(|p| p.id != id);
        self.save_providers(&all)
    }
    pub fn migrate(&self, legacy: &Path) -> Result<bool> {
        if self.root.join("migration.json").exists() || !legacy.join("providers.json").exists() {
            return Ok(false);
        }
        ensure!(
            !self.root.join("providers.json").exists(),
            "新版已有数据，不能覆盖导入旧版"
        );
        crate::sessions::no_links(legacy)?;
        ensure!(
            crate::journal::list(&legacy.join("operations"), true)?
                .iter()
                .all(|m| matches!(m.state.as_str(), "Complete" | "Restored" | "RolledBack")),
            "旧版存在未完成操作，请先在旧版恢复后重新导入"
        );
        let old = Self {
            root: legacy.to_owned(),
        };
        let all = old.providers()?;
        for p in &all {
            if let Some(k) = &p.protected_key {
                crypto::unprotect(k).context("旧版密钥无法由当前 Windows 用户解密")?;
            }
        }
        let settings = old.settings()?;
        // Legacy journal paths are intentionally kept anchored to the original store.
        // A read-only pointer avoids breaking absolute Stage/Backup references.
        let mut journal = crate::journal::Journal::new(&self.root, "首次导入旧版数据")?;
        journal.stage(
            &self.root.join("legacy-store.json"),
            Some(&serde_json::to_vec(&legacy)?),
        )?;
        for item in fs::read_dir(legacy)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().to_string();
            if name.starts_with("official-auth-") && name.ends_with(".json") {
                crate::sessions::no_links(&item.path())?;
                journal.stage_file(&self.root.join(name), &item.path())?;
            }
        }
        journal.stage(
            &self.root.join("settings.json"),
            Some(&serde_json::to_vec_pretty(&settings)?),
        )?;
        journal.stage(
            &self.root.join("providers.json"),
            Some(&serde_json::to_vec_pretty(&all)?),
        )?;
        journal.stage(
            &self.root.join("migration.json"),
            Some(&serde_json::to_vec(
                &serde_json::json!({"version":1,"source":legacy,"completedAt":chrono::Utc::now()}),
            )?),
        )?;
        journal.commit()?;
        Ok(true)
    }
}
