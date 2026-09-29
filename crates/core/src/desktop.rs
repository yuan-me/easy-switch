use crate::{Result, Settings};
use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

pub fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub executable: String,
    pub app_id: Option<String>,
    pub version: String,
}
pub fn belongs_to_desktop(
    pid: u32,
    expected: &Path,
    processes: &std::collections::HashMap<u32, (Option<u32>, Option<std::path::PathBuf>)>,
) -> bool {
    let mut id = pid;
    let mut seen = std::collections::HashSet::new();
    while seen.insert(id) {
        let Some((parent, path)) = processes.get(&id) else {
            return false;
        };
        if path.as_ref().is_some_and(|p| {
            match (
                crate::sessions::normalize(p),
                crate::sessions::normalize(expected),
            ) {
                (Ok(a), Ok(b)) => a
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&b.to_string_lossy()),
                _ => false,
            }
        }) {
            return true;
        }
        let Some(parent) = parent else {
            return false;
        };
        id = *parent;
    }
    false
}
pub fn discover() -> Result<Vec<Installation>> {
    let mut all = vec![];
    #[cfg(windows)]
    {
        let script = r#"$ErrorActionPreference='Stop'; @((Get-AppxPackage -Name OpenAI.Codex | ForEach-Object { $p=$_; [xml]$m=Get-Content -LiteralPath (Join-Path $p.InstallLocation 'AppxManifest.xml'); foreach($a in $m.Package.Applications.Application) { $exe=Join-Path $p.InstallLocation $a.Executable; if(Test-Path -LiteralPath $exe){ @{executable=$exe;appId=($p.PackageFamilyName+'!'+$a.Id);version=$p.Version.ToString()} } } })) | ConvertTo-Json -Compress"#;
        let out = hidden(Command::new("powershell.exe").args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ]))
        .output()?;
        if out.status.success() {
            if let Ok(v) = serde_json::from_slice::<Vec<Installation>>(&out.stdout) {
                all.extend(v);
            } else if let Ok(v) = serde_json::from_slice::<Installation>(&out.stdout) {
                all.push(v);
            }
        }
        let base = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default());
        for root in [base.join("Programs/Codex"), base.join("OpenAI/Codex")] {
            if root.exists() {
                for e in walkdir::WalkDir::new(root)
                    .max_depth(4)
                    .follow_links(false)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if e.file_type().is_file()
                        && e.file_name()
                            .to_string_lossy()
                            .eq_ignore_ascii_case("codex.exe")
                        && (e.path().parent().unwrap().join("resources").exists()
                            || e.path().parent().unwrap().join("icudtl.dat").exists())
                    {
                        all.push(Installation {
                            executable: e.path().to_string_lossy().into(),
                            app_id: None,
                            version: "EXE".into(),
                        });
                    }
                }
            }
        }
    }
    all.retain(is_desktop_entry);
    all.sort_by(|a, b| b.version.cmp(&a.version));
    all.dedup_by(|a, b| a.executable.eq_ignore_ascii_case(&b.executable));
    Ok(all)
}
pub fn is_desktop_entry(item: &Installation) -> bool {
    item.executable.rsplit(['/', '\\']).next().is_some_and(|n| {
        n.eq_ignore_ascii_case("ChatGPT.exe") || n.eq_ignore_ascii_case("codex.exe")
    }) && item.app_id.as_ref().is_none_or(|id| id.ends_with("!App"))
}
pub trait DesktopHost {
    fn stop(&self) -> Result<()>;
    fn start(&self) -> Result<String>;
}
pub struct Desktop {
    pub settings: Settings,
}
impl Desktop {
    pub fn resolve(mut s: Settings) -> Result<Self> {
        if s.desktop_executable.is_empty() || !Path::new(&s.desktop_executable).is_file() {
            let installations = discover()?;
            let item = installations
                .iter()
                .find(|i| s.desktop_app_id.is_some() && i.app_id == s.desktop_app_id)
                .or_else(|| installations.first())
                .cloned()
                .context("未识别 Codex 桌面程序，请在设置中选择")?;
            s.desktop_executable = item.executable;
            s.desktop_app_id = item.app_id;
        }
        let p = Path::new(&s.desktop_executable);
        ensure!(
            p.is_absolute() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")),
            "请选择完整 Codex 桌面 EXE 路径"
        );
        ensure!(
            s.desktop_app_id.is_some()
                || p.parent()
                    .is_some_and(|r| r.join("resources").exists() || r.join("icudtl.dat").exists()),
            "所选文件不像 Codex 桌面入口，不能使用 CLI"
        );
        Ok(Self { settings: s })
    }
    pub fn writers() -> Vec<(u32, Option<std::path::PathBuf>)> {
        let system = sysinfo::System::new_all();
        system
            .processes()
            .iter()
            .filter(|(_, p)| {
                p.name().to_string_lossy().eq_ignore_ascii_case("codex.exe")
                    || p.name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("ChatGPT.exe")
            })
            .map(|(id, p)| (id.as_u32(), p.exe().map(Path::to_owned)))
            .collect()
    }
    pub fn ensure_stopped() -> Result<()> {
        ensure!(
            Self::writers().is_empty(),
            "仍有 Codex/CLI/IDE 实例运行，未写入数据"
        );
        Ok(())
    }
}
impl DesktopHost for Desktop {
    fn stop(&self) -> Result<()> {
        let writers = Self::writers();
        if writers.is_empty() {
            return Ok(());
        }
        let expected = Path::new(&self.settings.desktop_executable);
        let system = sysinfo::System::new_all();
        let processes = system
            .processes()
            .iter()
            .map(|(id, p)| {
                (
                    id.as_u32(),
                    (p.parent().map(|p| p.as_u32()), p.exe().map(Path::to_owned)),
                )
            })
            .collect();
        ensure!(
            writers
                .iter()
                .all(|(id, _)| belongs_to_desktop(*id, expected, &processes)),
            "存在其他 Codex 写入实例，请正常退出后重试"
        );
        #[cfg(windows)]
        {
            let ids: Vec<_> = writers.iter().map(|x| x.0).collect();
            let mut window = native::find(&ids);
            if window == 0 {
                self.start()?;
                for _ in 0..40 {
                    std::thread::sleep(Duration::from_millis(100));
                    window = native::find(&ids);
                    if window != 0 {
                        break;
                    }
                }
            }
            ensure!(window != 0, "后台 Codex 无法恢复窗口，请从托盘退出");
            native::quit(window)?;
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(25) {
                if Self::writers().is_empty() {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        anyhow::bail!("Codex 未完全退出，未修改数据；请按 Ctrl+Q 正常退出")
    }
    fn start(&self) -> Result<String> {
        let s = &self.settings;
        if let Some(id) = &s.desktop_app_id {
            ensure!(
                crate::sessions::normalize(&s.codex_home)?
                    == crate::sessions::normalize(&Settings::default().codex_home)?,
                "MSIX 不能保证自定义 CODEX_HOME，请选择默认目录或普通 EXE"
            );
            ensure!(
                id.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._!-".contains(c)),
                "MSIX 应用标识无效"
            );
            #[cfg(windows)]
            {
                use windows::{
                    Win32::{
                        System::Com::{
                            CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance,
                            CoInitializeEx,
                        },
                        UI::Shell::{
                            AO_NONE, ApplicationActivationManager, IApplicationActivationManager,
                        },
                    },
                    core::{HSTRING, PCWSTR},
                };
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                    let manager: IApplicationActivationManager =
                        CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)?;
                    let app = HSTRING::from(id);
                    manager.ActivateApplication(PCWSTR(app.as_ptr()), None, AO_NONE)?;
                }
            }
        } else {
            let mut cmd = Command::new(&s.desktop_executable);
            cmd.current_dir(Path::new(&s.desktop_executable).parent().unwrap())
                .env("CODEX_HOME", &s.codex_home);
            if let Some(sqlite) = &s.sqlite_home {
                cmd.env("CODEX_SQLITE_HOME", sqlite);
            } else {
                cmd.env_remove("CODEX_SQLITE_HOME");
            }
            cmd.spawn()?;
        }
        for _ in 0..50 {
            let ids: Vec<_> = Self::writers()
                .iter()
                .filter(|(_, p)| {
                    p.as_ref().is_some_and(|p| {
                        p.to_string_lossy()
                            .eq_ignore_ascii_case(&s.desktop_executable)
                    })
                })
                .map(|x| x.0)
                .collect();
            #[cfg(windows)]
            if native::find(&ids) != 0 {
                return Ok("Codex 窗口已就绪；工具能力需在新会话验证".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok("已发送启动请求；窗口尚未确认就绪".into())
    }
}
pub fn open_thread(id: &str) -> Result<()> {
    uuid::Uuid::parse_str(id)?;
    #[cfg(windows)]
    {
        hidden(Command::new("explorer.exe").arg(format!("codex://threads/{id}"))).spawn()?;
    }
    Ok(())
}

#[cfg(windows)]
mod native {
    use super::*;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(cb: unsafe extern "system" fn(isize, isize) -> i32, data: isize) -> i32;
        fn IsWindowVisible(h: isize) -> i32;
        fn GetWindowThreadProcessId(h: isize, pid: *mut u32) -> u32;
        fn SetForegroundWindow(h: isize) -> i32;
        fn GetForegroundWindow() -> isize;
        fn ShowWindowAsync(h: isize, cmd: i32) -> i32;
        fn GetAsyncKeyState(k: i32) -> i16;
        fn GetMenu(h: isize) -> isize;
        fn GetMenuItemCount(m: isize) -> i32;
        fn GetSubMenu(m: isize, pos: i32) -> isize;
        fn GetMenuStringW(m: isize, id: u32, text: *mut u16, n: i32, flags: u32) -> i32;
        fn GetMenuItemID(m: isize, pos: i32) -> u32;
        fn GetMenuState(m: isize, id: u32, flags: u32) -> u32;
        fn PostMessageW(h: isize, msg: u32, w: usize, l: isize) -> i32;
        fn SendInput(n: u32, input: *const Input, size: i32) -> u32;
    }
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct Keyboard {
        key: u16,
        scan: u16,
        flags: u32,
        time: u32,
        extra: usize,
    }
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct Mouse {
        x: i32,
        y: i32,
        data: u32,
        flags: u32,
        time: u32,
        extra: usize,
    }
    #[repr(C)]
    union Data {
        k: Keyboard,
        m: Mouse,
    }
    #[repr(C)]
    struct Input {
        kind: u32,
        data: Data,
    }
    fn key(k: u16, up: bool) -> Input {
        Input {
            kind: 1,
            data: Data {
                k: Keyboard {
                    key: k,
                    scan: 0,
                    flags: if up { 2 } else { 0 },
                    time: 0,
                    extra: 0,
                },
            },
        }
    }
    struct Search<'a> {
        ids: &'a [u32],
        found: isize,
    }
    unsafe extern "system" fn visit(h: isize, data: isize) -> i32 {
        unsafe {
            let s = &mut *(data as *mut Search);
            let mut id = 0;
            GetWindowThreadProcessId(h, &mut id);
            if s.ids.contains(&id) && IsWindowVisible(h) != 0 {
                s.found = h;
                return 0;
            }
            1
        }
    }
    pub fn find(ids: &[u32]) -> isize {
        let mut s = Search { ids, found: 0 };
        unsafe {
            EnumWindows(visit, &mut s as *mut _ as isize);
        }
        s.found
    }
    unsafe fn quit_id(m: isize) -> Option<u32> {
        unsafe {
            if m == 0 {
                return None;
            }
            for i in 0..GetMenuItemCount(m) {
                let mut text = [0u16; 512];
                let n = GetMenuStringW(m, i as u32, text.as_mut_ptr(), 512, 0x400);
                let text = String::from_utf16_lossy(&text[..n.max(0) as usize]);
                if text
                    .split('\t')
                    .next_back()
                    .unwrap_or("")
                    .replace(' ', "")
                    .eq_ignore_ascii_case("Ctrl+Q")
                {
                    let id = GetMenuItemID(m, i);
                    if id != u32::MAX && GetMenuState(m, i as u32, 0x400) & 3 == 0 {
                        return Some(id);
                    }
                }
                if let Some(id) = quit_id(GetSubMenu(m, i)) {
                    return Some(id);
                }
            }
            None
        }
    }
    pub fn quit(h: isize) -> Result<()> {
        unsafe {
            if let Some(id) = quit_id(GetMenu(h)) {
                ensure!(
                    PostMessageW(h, 0x111, id as usize, 0) != 0,
                    "退出菜单请求失败"
                );
                return Ok(());
            }
            ShowWindowAsync(h, 9);
            SetForegroundWindow(h);
            for _ in 0..10 {
                if GetForegroundWindow() == h {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            ensure!(
                GetForegroundWindow() == h,
                "无法聚焦 Codex，请在其中按 Ctrl+Q 后重试"
            );
            ensure!(
                [0x10, 0x11, 0x12, 0x5b, 0x5c]
                    .iter()
                    .all(|k| GetAsyncKeyState(*k) >= 0),
                "请松开键盘修饰键后重试"
            );
            let keys = [
                key(0x11, false),
                key(0x51, false),
                key(0x51, true),
                key(0x11, true),
            ];
            if SendInput(4, keys.as_ptr(), std::mem::size_of::<Input>() as i32) != 4 {
                let up = [key(0x51, true), key(0x11, true)];
                SendInput(2, up.as_ptr(), std::mem::size_of::<Input>() as i32);
                anyhow::bail!("Windows 阻止了退出快捷键，请手动 Ctrl+Q");
            }
            Ok(())
        }
    }
}
