use easy_switch_core::{
    Settings, Store,
    journal::{self, Journal},
};
use std::{fs, path::Path};

fn operation(root: &Path, id: &str, state: &str, target: &Path) -> std::path::PathBuf {
    let dir = root.join("operations").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({
            "State":state,"Description":"old operation","Files":[{
                "Path":target,"BeforeHash":null,"AfterHash":null,"Backup":null,"Stage":null
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    for name in [
        "0.before.dpapi",
        "0.after.dpapi",
        "old.before",
        "old.after",
        "snapshot.db",
        "rewrite.jsonl",
        ".old.tmp",
    ] {
        fs::write(dir.join(name), b"operation copy").unwrap();
    }
    dir
}

#[test]
fn upgrade_cleans_current_and_legacy_copies_but_preserves_user_data_and_pending_recovery() {
    let t = tempfile::tempdir().unwrap();
    let store = Store::open(t.path().join("EasySwitch")).unwrap();
    let legacy = t.path().join("CodexSwitch");
    let home = t.path().join(".codex");
    fs::create_dir_all(home.join("sessions")).unwrap();
    let target = home.join("sessions/rollout.jsonl");
    let outside = t.path().join("outside");
    fs::create_dir(&outside).unwrap();
    for p in [
        &target,
        &home.join("auth.json"),
        &home.join("config.toml"),
        &home.join("thread_history.sqlite"),
        &store.root.join("providers.json"),
    ] {
        fs::write(p, b"untouched user data").unwrap();
    }
    store
        .save(
            "settings.json",
            &Settings {
                codex_home: home.clone(),
                ..Default::default()
            },
        )
        .unwrap();
    store.save("legacy-store.json", &legacy).unwrap();
    let stale = target
        .parent()
        .unwrap()
        .join(format!(".easy-switch-{}.restore", uuid::Uuid::new_v4()));
    fs::write(&stale, "temporary").unwrap();
    let unrelated = target.parent().unwrap().join(".easy-switch-user.restore");
    fs::write(&unrelated, "keep").unwrap();
    let outside_temp = outside.join(format!(".easy-switch-{}.restore", uuid::Uuid::new_v4()));
    fs::write(&outside_temp, "keep").unwrap();
    for root in [&store.root, &legacy] {
        for state in ["Complete", "Restored", "RolledBack", "Preparing"] {
            operation(root, state, state, &target);
        }
        for state in ["Applying", "Restoring", "Unknown"] {
            operation(root, state, state, &target);
        }
        // A manifest cannot make cleanup delete files outside the managed roots.
        operation(root, "outside", "Complete", &outside.join("target"));
    }
    store.cleanup_operations().unwrap();
    store.cleanup_operations().unwrap();
    for root in [&store.root, &legacy] {
        let ops = journal::list(&root.join("operations"), false).unwrap();
        assert_eq!(ops.len(), 3);
        assert!(
            ops.iter()
                .all(|o| ["Applying", "Restoring", "Unknown"].contains(&o.state.as_str()))
        );
        for state in ["Applying", "Restoring", "Unknown"] {
            assert!(
                root.join("operations")
                    .join(state)
                    .join("old.before")
                    .exists()
            );
        }
    }
    assert!(!stale.exists());
    assert!(unrelated.exists() && outside_temp.exists());
    for p in [
        &target,
        &home.join("auth.json"),
        &home.join("config.toml"),
        &home.join("thread_history.sqlite"),
        &store.root.join("providers.json"),
    ] {
        assert_eq!(fs::read(p).unwrap(), b"untouched user data");
    }
}

#[test]
fn repeated_success_cancellation_and_rollback_leave_no_copies() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("store");
    let target = t.path().join("session");
    fs::write(&target, b"before").unwrap();
    for _ in 0..8 {
        for outcome in ["success", "cancel", "rollback"] {
            let mut j = Journal::new(&root, outcome).unwrap();
            j.stage(&target, Some(b"after")).unwrap();
            if outcome == "success" {
                j.commit().unwrap();
            }
            if outcome == "rollback" {
                assert!(j.commit_with_fault(Some(0)).is_err());
            }
            assert!(
                journal::cleanup(&root, &[]).is_err(),
                "cleanup must respect the active lock"
            );
            drop(j);
            assert_eq!(fs::read_dir(root.join("operations")).unwrap().count(), 0);
            assert_eq!(
                fs::read(&target).unwrap(),
                if outcome == "success" {
                    b"after".as_slice()
                } else {
                    b"before"
                }
            );
            fs::write(&target, b"before").unwrap();
        }
    }
}

#[test]
fn malformed_or_unknown_files_are_reported_without_blocking_other_cleanup() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    let bad = operation(root, "bad", "Complete", &root.join("value"));
    fs::write(bad.join("manifest.json"), "broken").unwrap();
    let orphan = root.join("operations/orphan");
    fs::create_dir(&orphan).unwrap();
    fs::write(orphan.join("unknown"), "keep").unwrap();
    let good = operation(root, "good", "Complete", &root.join("value"));
    assert!(journal::cleanup(root, &[root.to_owned()]).is_err());
    assert!(bad.join("old.before").exists() && orphan.join("unknown").exists());
    assert!(!good.exists());
}

#[cfg(windows)]
#[test]
fn junctions_cannot_redirect_cleanup() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("store");
    let outside = t.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("keep"), "user file").unwrap();
    let dir = operation(&root, "linked", "Complete", &root.join("value"));
    let link = dir.join("escape");
    let output = std::process::Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command",
        "New-Item -ItemType Junction -Path $env:EASY_SWITCH_TEST_LINK -Target $env:EASY_SWITCH_TEST_TARGET | Out-Null"])
        .env("EASY_SWITCH_TEST_LINK", &link).env("EASY_SWITCH_TEST_TARGET", &outside).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(journal::cleanup(&root, &[root.clone()]).is_err());
    assert_eq!(fs::read(outside.join("keep")).unwrap(), b"user file");
    assert!(dir.join("manifest.json").exists());
    fs::remove_dir(link).unwrap();
}
