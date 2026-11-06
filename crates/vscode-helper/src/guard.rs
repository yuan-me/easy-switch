use anyhow::{Context, Result, ensure};
use easy_switch_core::sessions;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn ensure_stopped(home: &Path, sqlite_home: Option<&Path>) -> Result<()> {
    ensure!(cfg!(target_os = "linux"), "停写检查仅支持 Linux");
    check_proc(Path::new("/proc"), home, sqlite_home, std::process::id())
}

fn uid(status: &str) -> Option<u32> {
    status.lines().find_map(|line| {
        line.strip_prefix("Uid:")
            .and_then(|v| v.split_whitespace().next())?
            .parse()
            .ok()
    })
}

fn ssh_status(status: &str, owner: u32) -> bool {
    status.lines().any(|l| {
        l.strip_prefix("Name:")
            .is_some_and(|v| v.trim() == "sshd-session")
    }) && status.lines().any(|l| {
        l.strip_prefix("Uid:").is_some_and(|v| {
            let ids: Vec<_> = v.split_whitespace().collect();
            ids.len() == 4 && ids.iter().all(|id| id.parse::<u32>().ok() == Some(owner))
        })
    })
}

fn proc_stamp(dir: &Path) -> Result<(u32, u32, String)> {
    let text = fs::read_to_string(dir.join("stat"))?;
    let (pid, _) = text.split_once(' ').context("missing pid")?;
    let end = text.rfind(')').context("missing comm")?;
    let fields: Vec<_> = text[end + 1..].split_whitespace().collect();
    Ok((
        pid.parse()?,
        fields.get(1).context("missing ppid")?.parse()?,
        fields.get(19).context("missing starttime")?.to_string(),
    ))
}

// Only the root monitor's protected OpenSSH transport is exempt, never its shells.
fn protected_ssh_session(proc: &Path, pid: u32, owner: u32) -> bool {
    #[cfg(unix)]
    fn verify(proc: &Path, pid: u32, owner: u32) -> Result<bool> {
        use std::os::unix::fs::MetadataExt;
        let dir = proc.join(pid.to_string());
        if owner == 0 || !ssh_status(&fs::read_to_string(dir.join("status"))?, owner) {
            return Ok(false);
        }
        let child = proc_stamp(&dir)?;
        let command = fs::read_to_string(dir.join("cmdline"))?;
        let Some(rest) = command
            .trim_end_matches('\0')
            .strip_prefix("sshd-session: ")
        else {
            return Ok(false);
        };
        let Some((user, terminal)) = rest.split_once('@') else {
            return Ok(false);
        };
        if user.is_empty()
            || !user
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            || !(terminal == "notty"
                || terminal
                    .strip_prefix("pts/")
                    .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())))
            || child.0 != pid
            || child.1 <= 1
        {
            return Ok(false);
        }
        let parent = proc.join(child.1.to_string());
        let parent_stamp = proc_stamp(&parent)?;
        let parent_command = fs::read_to_string(parent.join("cmdline"))?;
        if parent_stamp.0 != child.1
            || !ssh_status(&fs::read_to_string(parent.join("status"))?, 0)
            || parent_command.trim_end_matches('\0') != format!("sshd-session: {user} [priv]")
        {
            return Ok(false);
        }
        let exe = fs::symlink_metadata(dir.join("exe"))?;
        let fd = fs::symlink_metadata(dir.join("fd"))?;
        if !exe.is_symlink()
            || exe.uid() != 0
            || !fd.is_dir()
            || fd.uid() != 0
            || fd.mode() & 0o077 != 0
        {
            return Ok(false);
        }
        Ok(proc_stamp(&dir)? == child
            && proc_stamp(&parent)? == parent_stamp
            && ssh_status(&fs::read_to_string(dir.join("status"))?, owner)
            && ssh_status(&fs::read_to_string(parent.join("status"))?, 0)
            && fs::read_to_string(dir.join("cmdline"))? == command
            && fs::read_to_string(parent.join("cmdline"))? == parent_command)
    }
    #[cfg(unix)]
    {
        verify(proc, pid, owner).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = (proc, pid, owner);
        false
    }
}

fn is_codex(exe: &Path, args: &[&str]) -> bool {
    let named = |p: &Path| {
        p.file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s == "codex" || s.starts_with("codex-") || s.starts_with("codex."))
    };
    named(exe) || args.first().is_some_and(|a| named(Path::new(a)))
}

fn process_home(bytes: &[u8]) -> Result<PathBuf> {
    let vars: Vec<_> = bytes
        .split(|b| *b == 0)
        .filter_map(|v| std::str::from_utf8(v).ok())
        .collect();
    if let Some(value) = vars
        .iter()
        .find_map(|v| v.strip_prefix("CODEX_HOME="))
        .filter(|v| !v.is_empty())
    {
        return sessions::normalize(Path::new(value));
    }
    let home = vars
        .iter()
        .find_map(|v| v.strip_prefix("HOME="))
        .context("无法识别运行中的 Codex 数据目录")?;
    sessions::normalize(&Path::new(home).join(".codex"))
}

fn check_proc(proc: &Path, home: &Path, sqlite_home: Option<&Path>, own_pid: u32) -> Result<()> {
    let own_uid =
        uid(&fs::read_to_string(proc.join("self/status"))?).context("无法确认当前 Linux 用户")?;
    for entry in fs::read_dir(proc)? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == own_pid {
            continue;
        }
        let dir = entry.path();
        let status = match fs::read_to_string(dir.join("status")) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => anyhow::bail!("无法核实进程 {pid} 的身份，停止迁移"),
        };
        if uid(&status) != Some(own_uid) {
            continue;
        }
        let bytes = match fs::read(dir.join("cmdline")) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => anyhow::bail!("无法核实进程 {pid} 的命令，停止迁移"),
        };
        if bytes.is_empty() {
            continue;
        }
        let args: Vec<_> = bytes
            .split(|b| *b == 0)
            .filter_map(|v| std::str::from_utf8(v).ok())
            .collect();
        let exe = match fs::read_link(dir.join("exe")) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e)
                if e.kind() == std::io::ErrorKind::PermissionDenied
                    && protected_ssh_session(proc, pid, own_uid) =>
            {
                continue;
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::PermissionDenied
                    && proc == Path::new("/proc") =>
            {
                audit_process(pid, home, sqlite_home)?;
                continue;
            }
            Err(_) => anyhow::bail!("无法核实进程 {pid} 的可执行文件，停止迁移"),
        };
        if is_codex(&exe, &args) {
            let env = fs::read(dir.join("environ"))
                .with_context(|| format!("无法核实 Codex 进程 {pid} 的数据目录"))?;
            ensure!(
                !sessions::same_path(&process_home(&env)?, home)?,
                "同一 Codex 目录仍有进程 {pid}，请关闭相应 CLI 或其他 VS Code 窗口后重试"
            );
        }
        let descriptors = match fs::read_dir(dir.join("fd")) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e)
                if e.kind() == std::io::ErrorKind::PermissionDenied
                    && proc == Path::new("/proc") =>
            {
                audit_process(pid, home, sqlite_home)?;
                continue;
            }
            Err(_) => anyhow::bail!("无法检查进程 {pid} 的文件占用，停止迁移"),
        };
        for fd in descriptors {
            let fd = fd?;
            let target = match fs::read_link(fd.path()) {
                Ok(v) if v.is_absolute() => v,
                _ => continue,
            };
            let relevant = sessions::within(&target, home)?
                || sqlite_home.is_some_and(|p| sessions::within(&target, p).unwrap_or(false));
            if !relevant {
                continue;
            }
            let info = match fs::read_to_string(dir.join("fdinfo").join(fd.file_name())) {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => anyhow::bail!("无法检查进程 {pid} 的文件写入状态"),
            };
            let flags = info
                .lines()
                .find_map(|line| line.strip_prefix("flags:"))
                .and_then(|v| u64::from_str_radix(v.trim(), 8).ok())
                .context("无法解析 Linux 文件占用状态")?;
            ensure!(
                flags & 3 == 0,
                "进程 {pid} 正在写入目标 Codex 目录，停止迁移"
            );
        }
    }
    Ok(())
}

fn audit_process(pid: u32, home: &Path, sqlite_home: Option<&Path>) -> Result<()> {
    use std::{
        io::Write,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let request = serde_json::to_vec(
        &serde_json::json!({"protocolVersion":1,"pid":pid,"home":home,"sqliteHome":sqlite_home}),
    )?;
    ensure!(request.len() <= 8192, "管理员检查请求超出长度限制");
    let mut child = Command::new("/usr/bin/sudo")
        .args([
            "-n",
            "/usr/bin/python3",
            "-I",
            "-B",
            "/usr/local/lib/easy-switch/proc-audit.py",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("无法启动只读管理员进程检查，请先安装检查器")?;
    let sent = child
        .stdin
        .take()
        .context("无法打开检查通道")?
        .write_all(&request);
    if sent.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        anyhow::bail!("只读管理员检查通道中断");
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("只读管理员检查超时，停止迁移");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success() && output.stdout.len() <= 16384,
        "PID {pid} 需要只读管理员检查，请先按说明安装检查器"
    );
    let reply: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("管理员检查返回无效结果")?;
    let current = proc_stamp(&Path::new("/proc").join(pid.to_string()))?;
    validate_audit_reply(pid, &reply, &current)
}

fn validate_audit_reply(
    pid: u32,
    reply: &serde_json::Value,
    current: &(u32, u32, String),
) -> Result<()> {
    ensure!(
        reply["protocolVersion"] == 1 && reply["pid"] == pid && reply["status"] == "ok",
        "PID {pid} 仍可能写入 Codex 数据或无法完成检查，停止迁移"
    );
    ensure!(
        reply["ppid"] == current.1 && reply["startTime"] == current.2 && current.0 == pid,
        "PID {pid} 在管理员检查后发生变化，停止迁移"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn privileged_reply_must_match_protocol_pid_and_start_time() {
        let valid = serde_json::json!({"protocolVersion":1,"pid":3133,"status":"ok","ppid":1,"startTime":"1234"});
        let current = (3133, 1, "1234".to_string());
        assert!(validate_audit_reply(3133, &valid, &current).is_ok());
        for (key, value) in [
            ("protocolVersion", serde_json::json!(2)),
            ("pid", serde_json::json!(1)),
            ("status", serde_json::json!("busy")),
            ("startTime", serde_json::json!("1235")),
            ("ppid", serde_json::json!(2)),
        ] {
            let mut invalid = valid.clone();
            invalid[key] = value;
            assert!(validate_audit_reply(3133, &invalid, &current).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn protected_transport_requires_root_monitor_and_matching_identity() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
        let tmp = tempfile::tempdir().unwrap();
        let proc = tmp.path();
        let child = proc.join("100");
        let parent = proc.join("90");
        fs::create_dir_all(child.join("fd")).unwrap();
        fs::set_permissions(child.join("fd"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(&parent).unwrap();
        symlink("/usr/lib/openssh/sshd-session", child.join("exe")).unwrap();
        let status =
            |owner| format!("Name:\tsshd-session\nUid:\t{owner}\t{owner}\t{owner}\t{owner}\n");
        let stat = |pid, ppid| {
            format!(
                "{pid} (sshd-session) S {ppid} {} 1234",
                vec!["0"; 17].join(" ")
            )
        };
        fs::write(child.join("status"), status(1000)).unwrap();
        fs::write(child.join("stat"), stat(100, 90)).unwrap();
        fs::write(child.join("cmdline"), b"sshd-session: guiyn@notty\0").unwrap();
        fs::write(parent.join("status"), status(0)).unwrap();
        fs::write(parent.join("stat"), stat(90, 1)).unwrap();
        fs::write(parent.join("cmdline"), b"sshd-session: guiyn [priv]\0").unwrap();
        // The offline Linux guest runs as root; ordinary-user fixtures must be rejected.
        let root_owned = fs::symlink_metadata(child.join("exe")).unwrap().uid() == 0;
        assert_eq!(protected_ssh_session(proc, 100, 1000), root_owned);
        fs::write(parent.join("status"), status(1000)).unwrap();
        assert!(!protected_ssh_session(proc, 100, 1000));
        fs::write(parent.join("status"), status(0)).unwrap();
        fs::write(parent.join("cmdline"), b"sshd-session: other [priv]\0").unwrap();
        assert!(!protected_ssh_session(proc, 100, 1000));
        fs::write(parent.join("cmdline"), b"sshd-session: guiyn [priv]\0").unwrap();
        fs::write(child.join("cmdline"), b"sshd-session: guiyn@notty\0extra").unwrap();
        assert!(!protected_ssh_session(proc, 100, 1000));
        fs::write(child.join("cmdline"), b"sshd-session: guiyn@notty\0").unwrap();
        fs::set_permissions(child.join("fd"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!protected_ssh_session(proc, 100, 1000));
    }
    #[test]
    fn recognizes_only_codex_and_uses_exact_home() {
        assert!(is_codex(Path::new("/opt/codex"), &["codex", "app-server"]));
        assert!(is_codex(
            Path::new("/opt/codex-x86_64-unknown-linux-musl"),
            &["codex-x86_64-unknown-linux-musl", "exec"]
        ));
        assert!(!is_codex(
            Path::new("/opt/easy-switch-helper"),
            &["easy-switch-helper"]
        ));
        assert_eq!(
            uid("Name: test\nUid:\t1000\t1000\t1000\t1000\n"),
            Some(1000)
        );
        #[cfg(unix)]
        {
            assert_eq!(
                process_home(b"HOME=/home/Example\0CODEX_HOME=/data/Codex\0").unwrap(),
                PathBuf::from("/data/Codex")
            );
            assert_eq!(
                process_home(b"HOME=/home/Example\0").unwrap(),
                PathBuf::from("/home/Example/.codex")
            );
            assert!(process_home(b"HOME=/home/Example\0CODEX_HOME=relative\0").is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn blocks_idle_codex_and_unrelated_process_writing_the_target() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let proc = temp.path().join("proc");
        let home = temp.path().join("Codex");
        fs::create_dir_all(proc.join("self")).unwrap();
        fs::create_dir_all(proc.join("100/fd")).unwrap();
        fs::create_dir_all(proc.join("100/fdinfo")).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::write(proc.join("self/status"), "Uid:\t1000\t1000\t1000\t1000\n").unwrap();
        fs::write(proc.join("100/status"), "Uid:\t1000\t1000\t1000\t1000\n").unwrap();
        fs::write(
            proc.join("100/cmdline"),
            b"codex-x86_64-unknown-linux-musl\0exec\0",
        )
        .unwrap();
        fs::write(
            proc.join("100/environ"),
            format!("CODEX_HOME={}\0", home.display()),
        )
        .unwrap();
        symlink("/opt/codex-x86_64-unknown-linux-musl", proc.join("100/exe")).unwrap();
        assert!(
            check_proc(&proc, &home, None, 777)
                .unwrap_err()
                .to_string()
                .contains("进程 100")
        );
        fs::write(proc.join("100/environ"), b"HOME=/another-user\0").unwrap();
        assert!(check_proc(&proc, &home, None, 777).is_ok());
        fs::remove_file(proc.join("100/exe")).unwrap();
        symlink("/usr/bin/python", proc.join("100/exe")).unwrap();
        fs::write(proc.join("100/cmdline"), b"python\0script.py\0").unwrap();
        symlink(home.join("state.db"), proc.join("100/fd/3")).unwrap();
        fs::write(proc.join("100/fdinfo/3"), "flags:\t0100002\n").unwrap();
        assert!(
            check_proc(&proc, &home, None, 777)
                .unwrap_err()
                .to_string()
                .contains("正在写入")
        );
        fs::write(proc.join("100/fdinfo/3"), "flags:\t0100000\n").unwrap();
        assert!(check_proc(&proc, &home, None, 777).is_ok());
    }
}
