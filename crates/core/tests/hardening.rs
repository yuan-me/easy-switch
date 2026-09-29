use easy_switch_core::*;
use std::{collections::HashMap, path::PathBuf};
#[tokio::test]
async fn idle_runtime_refused_connection_is_not_unknown_state() {
    let t = tempfile::tempdir().unwrap();
    let store = Store::open(t.path().to_owned()).unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let settings = Settings {
        runtime_port: port,
        runtime_key: Some(crypto::protect("synthetic-runtime-key").unwrap()),
        ..Default::default()
    };
    store.save("settings.json", &settings).unwrap();
    assert!(runtime::health_check(&store).await.unwrap().is_none());
}
#[test]
fn refused_restore_keeps_completed_operation_state() {
    let t = tempfile::tempdir().unwrap();
    let target = t.path().join("value");
    std::fs::write(&target, "before").unwrap();
    let mut j = journal::Journal::new(t.path(), "test").unwrap();
    j.stage(&target, Some(b"after")).unwrap();
    j.commit().unwrap();
    let id = j.dir.file_name().unwrap().to_str().unwrap().to_owned();
    drop(j);
    std::fs::write(&target, "external update").unwrap();
    assert!(journal::restore(t.path(), &id, &[t.path().to_path_buf()]).is_err());
    assert_eq!(
        journal::list(&t.path().join("operations"), false).unwrap()[0].state,
        "Complete"
    );
    assert_eq!(std::fs::read_to_string(target).unwrap(), "external update");
}
#[test]
fn discovery_excludes_packaged_command_runner() {
    use desktop::{Installation, is_desktop_entry};
    let mut item = Installation {
        executable: r"C:\Apps\app\resources\codex-command-runner.exe".into(),
        app_id: Some("OpenAI.Codex_test!CodexCoreCommandRunner".into()),
        version: "1".into(),
    };
    assert!(!is_desktop_entry(&item));
    item.executable = r"C:\Apps\app\ChatGPT.exe".into();
    item.app_id = Some("OpenAI.Codex_test!App".into());
    assert!(is_desktop_entry(&item));
}
fn provider() -> Provider {
    Provider {
        id: "custom".into(),
        name: "Custom".into(),
        base_url: "https://example.com/v1".into(),
        model: "test".into(),
        protected_key: Some(crypto::protect("fake-key").unwrap()),
        ..Default::default()
    }
}
#[test]
fn header_credentials_encrypted_and_blank_preserves_saved_value() {
    let t = tempfile::tempdir().unwrap();
    let store = Store::open(t.path().to_owned()).unwrap();
    let mut p = provider();
    p.headers
        .insert("x-api-key".into(), "synthetic-header-secret".into());
    store.save_provider(p, Some("fake-key".into())).unwrap();
    assert!(
        !std::fs::read_to_string(t.path().join("providers.json"))
            .unwrap()
            .contains("synthetic-header-secret")
    );
    let mut p = store
        .providers()
        .unwrap()
        .into_iter()
        .find(|p| p.id == "custom")
        .unwrap();
    assert_eq!(p.headers["x-api-key"], "synthetic-header-secret");
    p.headers.insert("x-api-key".into(), String::new());
    store.save_provider(p, None).unwrap();
    assert_eq!(
        store.providers().unwrap()[1].headers["x-api-key"],
        "synthetic-header-secret"
    );
}
#[test]
fn image_compat_keeps_existing_headers_and_can_be_disabled() {
    let mut p = provider();
    p.image_compatibility = true;
    let original = "[model_providers.custom]\nname='Custom'\nhttp_headers={ 'x-custom'='keep' }\n[features]\nlegacy=true\n";
    let enabled = config::update(
        original,
        &p,
        &Settings::default(),
        Some("fake"),
        Some("custom"),
    )
    .unwrap();
    assert!(
        enabled.contains("x-custom") && enabled.contains("keep") && enabled.contains("legacy=true")
    );
    p.image_compatibility = false;
    let disabled = config::update(
        &enabled,
        &p,
        &Settings::default(),
        Some("fake"),
        Some("custom"),
    )
    .unwrap();
    assert!(!disabled.contains("local-image-extension"));
    assert!(disabled.contains("x-custom"));
}
#[test]
fn malformed_config_never_echoes_secrets() {
    let e = config::update(
        "key='private-do-not-echo",
        &provider(),
        &Settings::default(),
        Some("fake"),
        None,
    )
    .unwrap_err();
    assert!(!e.to_string().contains("private-do-not-echo"));
}
#[test]
fn official_identity_name_accepts_extended_home() {
    assert_eq!(
        config::official_backup_name(&PathBuf::from(r"C:\Users\test\.codex")),
        config::official_backup_name(&PathBuf::from(r"\\?\C:\Users\test\.codex"))
    );
}
#[test]
fn owned_process_tree_allows_app_server_but_not_unrelated_cli() {
    let expected = PathBuf::from(r"C:\App\Codex.exe");
    let processes = HashMap::from([
        (1, (None, Some(expected.clone()))),
        (
            2,
            (Some(1), Some(PathBuf::from(r"C:\App\resources\codex.exe"))),
        ),
        (3, (None, Some(PathBuf::from(r"C:\CLI\codex.exe")))),
    ]);
    assert!(desktop::belongs_to_desktop(2, &expected, &processes));
    assert!(!desktop::belongs_to_desktop(3, &expected, &processes));
}
#[test]
fn export_rejects_malicious_thread_id() {
    let t = tempfile::tempdir().unwrap();
    let s = sessions::Session {
        id: "../../outside".into(),
        title: "test".into(),
        cwd: "".into(),
        provider: "".into(),
        path: t.path().join("missing"),
        updated: 0,
        archived: false,
        database: None,
        related_paths: vec![],
    };
    assert!(sessions::export(&s, t.path()).is_err());
    assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 0);
}
#[test]
fn journal_case_alias_refuses_duplicate_target() {
    let t = tempfile::tempdir().unwrap();
    let target = t.path().join("original");
    std::fs::write(&target, "before").unwrap();
    let mut j = journal::Journal::new(t.path(), "test").unwrap();
    j.stage(&target, Some(b"after")).unwrap();
    assert!(j.stage(&t.path().join("ORIGINAL"), Some(b"after")).is_err());
}
