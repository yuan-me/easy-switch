use easy_switch_core::{desktop::DesktopHost, journal::*, protocol::*, sessions::*, *};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
struct Fixture {
    temp: TempDir,
    store: Store,
    settings: Settings,
    file: PathBuf,
    id: String,
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let store = Store::open(root.join("store")).unwrap();
    let home = root.join("home");
    fs::create_dir_all(home.join("sessions/2026/09")).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let file = home.join("sessions/2026/09/rollout.jsonl");
    fs::write(&file,format!("{}\r\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"old model_provider=a 中文\"}}]}}}}\r\n",json!({"type":"session_meta","payload":{"id":id,"cwd":"D:/old","model_provider":"a","unknown":17}}))).unwrap();
    let settings = Settings {
        codex_home: home,
        ..Default::default()
    };
    store.save("settings.json", &settings).unwrap();
    Fixture {
        temp,
        store,
        settings,
        file,
        id,
    }
}
fn api(id: &str) -> Provider {
    Provider {
        id: id.into(),
        name: id.into(),
        model: "test-model".into(),
        base_url: "https://example.com/v1".into(),
        protected_key: Some(crypto::protect("synthetic-invalid-key").unwrap()),
        ..Default::default()
    }
}
fn scan(f: &Fixture) -> Vec<Session> {
    let result = Scanner::default()
        .scan(&f.settings, &CancellationToken::new())
        .unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    result.sessions
}

#[test]
fn current_project_id_and_local_catalog_migrate_together() {
    let f = fixture();
    let path = db(&f);
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("ALTER TABLE threads ADD COLUMN project_id TEXT; CREATE TABLE projects(id TEXT PRIMARY KEY,name TEXT,metadata TEXT,position INTEGER,created_at_ms INTEGER,updated_at_ms INTEGER); CREATE TABLE project_roots(project_id TEXT,position INTEGER,path TEXT);").unwrap();
    drop(c);
    let catalog = f.settings.codex_home.join("catalog.db");
    let c = rusqlite::Connection::open(&catalog).unwrap();
    c.execute_batch("CREATE TABLE local_thread_catalog(host_id TEXT,thread_id TEXT,cwd TEXT,project_id TEXT,model_provider TEXT,unknown TEXT); CREATE TABLE local_thread_catalog_hosts(host_id TEXT,host_kind TEXT); INSERT INTO local_thread_catalog_hosts VALUES('local','local'),('remote','remote');").unwrap();
    for host in ["local", "remote"] {
        c.execute(
            "INSERT INTO local_thread_catalog VALUES(?1,?2,'D:/old',NULL,'a','keep')",
            rusqlite::params![host, f.id],
        )
        .unwrap();
    }
    drop(c);
    let rows = scan(&f);
    let mut j = Journal::new(&f.store.root, "migrate").unwrap();
    stage_changes(
        &mut j,
        &f.settings,
        &rows,
        "migrate",
        Some("D:/new"),
        &CancellationToken::new(),
    )
    .unwrap();
    j.commit().unwrap();
    drop(j);
    let c = rusqlite::Connection::open(&path).unwrap();
    let project: String = c
        .query_row("SELECT project_id FROM threads", [], |r| r.get(0))
        .unwrap();
    let root: String = c
        .query_row(
            "SELECT path FROM project_roots WHERE project_id=?1",
            [&project],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(root, "D:/new");
    drop(c);
    let c = rusqlite::Connection::open(&catalog).unwrap();
    let tuple: (String, String, String) = c
        .query_row(
            "SELECT cwd,project_id,unknown FROM local_thread_catalog WHERE host_id='local'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(tuple, ("D:/new".into(), project, "keep".into()));
    assert_eq!(
        c.query_row(
            "SELECT cwd FROM local_thread_catalog WHERE host_id='remote'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "D:/old"
    );
}

#[test]
fn ten_thousand_indexed_sessions_scan_without_reading_bodies() {
    let f = fixture();
    let path = db(&f);
    let mut c = rusqlite::Connection::open(path).unwrap();
    let tx = c.transaction().unwrap();
    for i in 0..9999 {
        tx.execute(
            "INSERT INTO threads VALUES(?1,'Benchmark','D:/old','a',?2,1000,0,NULL,'keep')",
            rusqlite::params![
                format!("synthetic-{i}"),
                f.settings
                    .codex_home
                    .join("sessions")
                    .join(format!("synthetic-{i}.jsonl"))
                    .to_string_lossy()
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    drop(c);
    let start = std::time::Instant::now();
    let rows = scan(&f);
    assert_eq!(rows.len(), 10000);
    assert!(
        start.elapsed().as_secs() < 15,
        "10k scan exceeded acceptance budget"
    );
    println!(
        "10k indexed session scan: {} ms",
        start.elapsed().as_millis()
    );
}

#[test]
fn restore_rejects_unmerged_wal_before_writing_other_files() {
    let f = fixture();
    let path = db(&f);
    let cfg = f.settings.codex_home.join("config.toml");
    fs::write(&cfg, "old").unwrap();
    let id;
    {
        let mut j = Journal::new(&f.store.root, "wal guard").unwrap();
        j.stage(&cfg, Some(b"new")).unwrap();
        stage_database(&mut j, &path, |c| {
            c.execute("UPDATE threads SET title='new'", [])?;
            Ok(())
        })
        .unwrap();
        j.commit().unwrap();
        id = j.dir.file_name().unwrap().to_str().unwrap().to_owned();
    }
    fs::write(PathBuf::from(format!("{}-wal", path.display())), "pending").unwrap();
    assert!(restore(&f.store.root, &id, &[f.temp.path().to_owned()]).is_err());
    assert_eq!(fs::read_to_string(cfg).unwrap(), "new");
}
fn db(f: &Fixture) -> PathBuf {
    let path = f.settings.codex_home.join("state_5.sqlite");
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,title TEXT,cwd TEXT,model_provider TEXT,rollout_path TEXT,updated_at INTEGER,archived INTEGER,archived_at INTEGER,unknown TEXT); CREATE TABLE thread_spawn_edges(parent_thread_id TEXT,child_thread_id TEXT);").unwrap();
    c.execute(
        "INSERT INTO threads VALUES(?1,'Example','D:/old','a',?2,1000,0,NULL,'preserve')",
        rusqlite::params![f.id, f.file.to_string_lossy()],
    )
    .unwrap();
    path
}
#[test]
fn config_preserves_unrelated_fields() {
    let p = api("b");
    let text =
        "# keep me\nmodel_provider='a'\n[features]\nfoo=true\n[mcp_servers.test]\ncommand='demo'\n";
    let out = config::update(text, &p, &Settings::default(), Some("key"), None).unwrap();
    assert!(
        out.contains("# keep me") && out.contains("foo=true") && out.contains("command='demo'")
    );
}
#[test]
fn image_compat_sets_actor_header_only_when_enabled() {
    let mut p = api("b");
    p.image_compatibility = true;
    p.headers.insert("x-test".into(), "retain".into());
    let out = config::update(
        "[features]\nold=true\n",
        &p,
        &Settings::default(),
        Some("key"),
        None,
    )
    .unwrap();
    assert!(
        out.contains("local-image-extension")
            && out.contains("x-test")
            && out.contains("requires_openai_auth = false")
    );
    assert!(!out.contains("image_generation"));
}
#[test]
fn ordinary_provider_does_not_inject_image_header() {
    let out = config::update("", &api("b"), &Settings::default(), Some("key"), None).unwrap();
    assert!(!out.contains("actor"));
}
#[test]
fn managed_provider_can_be_replaced() {
    let p = api("a");
    let first = config::update("", &p, &Settings::default(), Some("key"), None).unwrap();
    let second = config::update(
        &first,
        &api("b"),
        &Settings::default(),
        Some("next"),
        Some("a"),
    )
    .unwrap();
    assert!(!second.contains("model_providers.a"));
    assert!(second.contains("model_providers.b"));
}
#[test]
fn unmanaged_provider_collision_is_rejected() {
    assert!(
        config::update(
            "[model_providers.a]\nname='existing'",
            &api("a"),
            &Settings::default(),
            Some("key"),
            None
        )
        .is_err()
    );
}
#[test]
fn active_profile_is_rejected() {
    assert!(
        config::update(
            "profile='work'",
            &api("a"),
            &Settings::default(),
            Some("key"),
            None
        )
        .is_err()
    );
}
#[test]
fn unsupported_credential_store_is_rejected() {
    for mode in ["keyring", "auto", "ephemeral"] {
        assert!(
            config::update(
                &format!("cli_auth_credentials_store='{mode}'"),
                &api("a"),
                &Settings::default(),
                Some("key"),
                None
            )
            .is_err()
        );
    }
}
#[test]
fn official_mode_removes_managed_provider() {
    let text = config::update("", &api("a"), &Settings::default(), Some("key"), None).unwrap();
    let out = config::update(
        &text,
        &Provider::official(),
        &Settings::default(),
        None,
        Some("a"),
    )
    .unwrap();
    assert!(!out.contains("experimental_bearer_token"));
    assert!(out.contains("chatgpt"));
}
#[test]
fn auth_official_identity_survives_api_roundtrip() {
    let f = fixture();
    let current = br#"{"tokens":{"access_token":"synthetic"},"unknown":7}"#;
    let pure = config::next_auth(
        &f.store,
        &f.settings,
        &api("a"),
        Some(current),
        Some("invalid-api"),
    )
    .unwrap()
    .unwrap();
    assert!(!String::from_utf8_lossy(&pure).contains("tokens"));
    let restored = config::next_auth(
        &f.store,
        &f.settings,
        &Provider::official(),
        Some(&pure),
        None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&restored).unwrap(),
        serde_json::from_slice::<Value>(current).unwrap()
    );
}
#[test]
fn mixed_requires_official_identity() {
    let f = fixture();
    let mut p = api("a");
    p.mode = AuthMode::Mixed;
    assert!(config::next_auth(&f.store, &f.settings, &p, None, Some("key")).is_err());
}
#[test]
fn official_removes_api_key_but_keeps_tokens() {
    let f = fixture();
    let result = config::next_auth(
        &f.store,
        &f.settings,
        &Provider::official(),
        Some(br#"{"tokens":{},"OPENAI_API_KEY":"key"}"#),
        None,
    )
    .unwrap()
    .unwrap();
    let v: Value = serde_json::from_slice(&result).unwrap();
    assert!(v.get("OPENAI_API_KEY").is_none());
    assert!(v.get("tokens").is_some());
}
#[test]
fn dpapi_roundtrip_and_plaintext_not_stored() {
    let secret = "synthetic-not-real";
    let protected = crypto::protect(secret).unwrap();
    assert!(!protected.contains(secret));
    assert_eq!(crypto::unprotect(&protected).unwrap(), secret);
}
#[test]
fn large_chunked_backup_roundtrip() {
    let f = fixture();
    let data = vec![73u8; 16 * 1024 * 1024 + 7];
    let path = f.temp.path().join("backup");
    crypto::encrypt_reader(std::io::Cursor::new(&data), &path).unwrap();
    let mut restored = vec![];
    crypto::decrypt_file(&path, &mut restored).unwrap();
    assert_eq!(data, restored);
}
#[test]
fn corrupt_backup_is_rejected() {
    let f = fixture();
    let p = f.temp.path().join("backup");
    crypto::encrypt_reader(std::io::Cursor::new(b"example"), &p).unwrap();
    let mut bytes = fs::read(&p).unwrap();
    bytes[10] ^= 0xff;
    fs::write(&p, bytes).unwrap();
    assert!(crypto::decrypt_file(&p, &mut Vec::new()).is_err());
}
#[test]
fn backup_trailing_bytes_are_rejected() {
    let f = fixture();
    let p = f.temp.path().join("backup");
    crypto::encrypt_reader(std::io::Cursor::new(b"example"), &p).unwrap();
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(b"garbage")
        .unwrap();
    assert!(crypto::decrypt_file(&p, &mut Vec::new()).is_err());
}
#[test]
fn repair_preserves_non_metadata_bytes() {
    let f = fixture();
    let out = f.temp.path().join("out");
    let before = fs::read(&f.file).unwrap();
    patch_metadata(&f.file, &out, "model_provider", "b").unwrap();
    let after = fs::read(out).unwrap();
    assert_eq!(
        &before[before.iter().position(|b| *b == b'\n').unwrap() + 1..],
        &after[after.iter().position(|b| *b == b'\n').unwrap() + 1..]
    );
    assert!(String::from_utf8_lossy(&after).contains("\"unknown\":17"));
}
#[test]
fn repair_keeps_bom_and_line_endings() {
    let f = fixture();
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend(fs::read(&f.file).unwrap());
    fs::write(&f.file, bytes).unwrap();
    let out = f.temp.path().join("out");
    patch_metadata(&f.file, &out, "cwd", "D:/new").unwrap();
    let b = fs::read(out).unwrap();
    assert!(b.starts_with(&[0xef, 0xbb, 0xbf]));
    assert!(b.windows(2).any(|b| b == b"\r\n"));
}
#[test]
fn invalid_json_rejected() {
    let f = fixture();
    use std::io::Write;
    fs::OpenOptions::new()
        .append(true)
        .open(&f.file)
        .unwrap()
        .write_all(b"{bad}")
        .unwrap();
    assert!(patch_metadata(&f.file, &f.temp.path().join("out"), "cwd", "D:/new").is_err());
}
#[test]
fn duplicate_metadata_rejected() {
    let f = fixture();
    let b = fs::read(&f.file).unwrap();
    fs::write(&f.file, [b.clone(), b].concat()).unwrap();
    assert!(patch_metadata(&f.file, &f.temp.path().join("out"), "cwd", "D:/new").is_err());
}
#[test]
fn missing_metadata_rejected() {
    let f = fixture();
    fs::write(&f.file, "{\"type\":\"event_msg\"}").unwrap();
    assert!(patch_metadata(&f.file, &f.temp.path().join("out"), "cwd", "D:/new").is_err());
}
#[test]
fn extended_path_matches_normal_path() {
    let f = fixture();
    let normal = normalize(&f.file).unwrap();
    let extended = PathBuf::from(format!(r"\\?\{}", normal.display()));
    assert_eq!(owned(&extended, &f.settings).unwrap(), normal);
}
#[test]
fn paths_outside_home_are_rejected() {
    let f = fixture();
    assert!(owned(&f.temp.path().join("outside.jsonl"), &f.settings).is_err());
    assert!(
        owned(
            &f.settings.codex_home.join("sessions/../../outside.jsonl"),
            &f.settings
        )
        .is_err()
    );
}
#[test]
fn device_paths_rejected() {
    for p in [
        r"\\.\C:\data",
        r"\??\C:\data",
        r"\\?\GLOBALROOT\Device\x",
        r"relative\file",
        r"C:\bad.\file",
    ] {
        assert!(normalize(&PathBuf::from(p)).is_err(), "{p}");
    }
}
#[test]
fn unc_normalization_preserves_identity() {
    assert_eq!(
        normalize(&PathBuf::from(r"\\?\UNC\server\share\file")).unwrap(),
        PathBuf::from(r"\\server\share\file")
    );
}
#[test]
fn commit_and_restore() {
    let f = fixture();
    let before = fs::read(&f.file).unwrap();
    let id;
    {
        let mut j = Journal::new(&f.store.root, "test").unwrap();
        id = j.dir.file_name().unwrap().to_string_lossy().to_string();
        j.stage(&f.file, Some(b"changed")).unwrap();
        j.commit().unwrap();
    }
    restore(&f.store.root, &id, &[f.settings.codex_home.clone()]).unwrap();
    assert_eq!(fs::read(&f.file).unwrap(), before);
}
#[test]
fn partial_commit_rolls_back() {
    let f = fixture();
    let before = fs::read(&f.file).unwrap();
    let another = f.settings.codex_home.join("new");
    let mut j = Journal::new(&f.store.root, "test").unwrap();
    j.stage(&f.file, Some(b"changed")).unwrap();
    j.stage(&another, Some(b"new")).unwrap();
    assert!(j.commit_with_fault(Some(0)).is_err());
    assert_eq!(fs::read(&f.file).unwrap(), before);
    assert!(!another.exists());
}
#[test]
fn external_modification_prevents_commit() {
    let f = fixture();
    let mut j = Journal::new(&f.store.root, "test").unwrap();
    j.stage(&f.file, Some(b"changed")).unwrap();
    fs::write(&f.file, b"external").unwrap();
    assert!(j.commit().is_err());
    assert_eq!(fs::read(&f.file).unwrap(), b"external");
}
#[test]
fn external_modification_prevents_restore() {
    let f = fixture();
    let id;
    {
        let mut j = Journal::new(&f.store.root, "test").unwrap();
        id = j.dir.file_name().unwrap().to_string_lossy().to_string();
        j.stage(&f.file, Some(b"changed")).unwrap();
        j.commit().unwrap();
    }
    fs::write(&f.file, b"external").unwrap();
    assert!(restore(&f.store.root, &id, &[f.settings.codex_home.clone()]).is_err());
}
#[test]
fn unfinished_operation_blocks_next_write() {
    let f = fixture();
    let op = f.store.root.join("operations/interrupted");
    fs::create_dir_all(&op).unwrap();
    fs::write(
        op.join("manifest.json"),
        br#"{"State":"Applying","Description":"old","Files":[]}"#,
    )
    .unwrap();
    assert!(Journal::new(&f.store.root, "next").is_err());
}
#[test]
fn operation_lock_prevents_concurrent_mutation() {
    let f = fixture();
    let _j = Journal::new(&f.store.root, "first").unwrap();
    assert!(Journal::new(&f.store.root, "second").is_err());
}
#[test]
fn corrupt_backup_prevents_all_restore_writes() {
    let f = fixture();
    let before = fs::read(&f.file).unwrap();
    let id;
    {
        let mut j = Journal::new(&f.store.root, "test").unwrap();
        j.stage(&f.file, Some(b"new")).unwrap();
        j.commit().unwrap();
        id = j.dir.file_name().unwrap().to_string_lossy().to_string();
        fs::write(j.manifest.files[0].backup.as_ref().unwrap(), b"bad").unwrap();
    }
    assert!(restore(&f.store.root, &id, &[f.settings.codex_home.clone()]).is_err());
    assert_eq!(fs::read(&f.file).unwrap(), b"new");
    assert_ne!(before, b"new");
}
#[test]
fn sqlite_provider_repair_keeps_unknown_fields() {
    let f = fixture();
    let path = db(&f);
    let items = scan(&f);
    let mut j = Journal::new(&f.store.root, "repair").unwrap();
    stage_changes(
        &mut j,
        &f.settings,
        &items,
        "provider",
        Some("b"),
        &CancellationToken::new(),
    )
    .unwrap();
    j.commit().unwrap();
    let c = open(&path).unwrap();
    let row: (String, String) = c
        .query_row("SELECT model_provider,unknown FROM threads", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(row, ("b".into(), "preserve".into()));
}
#[test]
fn unknown_thread_schema_is_reported() {
    let f = fixture();
    let c = rusqlite::Connection::open(f.settings.codex_home.join("state.db")).unwrap();
    c.execute_batch("CREATE TABLE threads(id TEXT)").unwrap();
    let s = Scanner::default()
        .scan(&f.settings, &CancellationToken::new())
        .unwrap();
    assert_eq!(s.warnings.len(), 1);
}
#[test]
fn wal_content_survives_snapshot() {
    let f = fixture();
    let path = db(&f);
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("PRAGMA journal_mode=WAL;UPDATE threads SET title='from WAL'")
        .unwrap();
    let mut j = Journal::new(&f.store.root, "db").unwrap();
    stage_database(&mut j, &path, |db| {
        db.execute("UPDATE threads SET model_provider='b'", [])?;
        Ok(())
    })
    .unwrap();
    drop(c);
    j.commit().unwrap();
    assert_eq!(
        open(&path)
            .unwrap()
            .query_row("SELECT title FROM threads", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "from WAL"
    );
}
#[test]
fn archive_unarchive_syncs_file_and_index() {
    let f = fixture();
    db(&f);
    for action in ["archive", "unarchive"] {
        let items = scan(&f);
        let mut j = Journal::new(&f.store.root, action).unwrap();
        stage_changes(
            &mut j,
            &f.settings,
            &items,
            action,
            None,
            &CancellationToken::new(),
        )
        .unwrap();
        j.commit().unwrap();
        drop(j);
        let new = scan(&f);
        assert_eq!(new[0].archived, action == "archive");
        assert!(new[0].path.exists());
    }
}
#[test]
fn migration_preserves_historical_paths() {
    let f = fixture();
    db(&f);
    let mut j = Journal::new(&f.store.root, "migrate").unwrap();
    stage_changes(
        &mut j,
        &f.settings,
        &scan(&f),
        "migrate",
        Some("D:/new"),
        &CancellationToken::new(),
    )
    .unwrap();
    j.commit().unwrap();
    assert_eq!(scan(&f)[0].cwd, "D:/new");
    assert!(
        fs::read_to_string(&f.file)
            .unwrap()
            .contains("old model_provider=a")
    );
}
#[test]
fn delete_is_recoverable() {
    let f = fixture();
    db(&f);
    let id;
    {
        let mut j = Journal::new(&f.store.root, "delete").unwrap();
        stage_changes(
            &mut j,
            &f.settings,
            &scan(&f),
            "delete",
            None,
            &CancellationToken::new(),
        )
        .unwrap();
        j.commit().unwrap();
        id = j.dir.file_name().unwrap().to_string_lossy().to_string();
    }
    assert!(!f.file.exists());
    restore(&f.store.root, &id, &[f.settings.codex_home.clone()]).unwrap();
    assert_eq!(scan(&f).len(), 1);
}
#[test]
fn token_cumulative_dedup_and_missing_values() {
    let f = fixture();
    use std::io::Write;
    let v = json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":20,"total_tokens":120}}}});
    let mut file = fs::OpenOptions::new().append(true).open(&f.file).unwrap();
    writeln!(file, "{v}\n{v}").unwrap();
    let d = detail(&scan(&f)[0], 500).unwrap();
    assert_eq!(d.tokens.len(), 1);
    assert!(d.tokens[0]["cached"].is_null());
}
#[test]
fn export_full_text_and_unique_filenames() {
    let f = fixture();
    let s = scan(&f).remove(0);
    let folder = f.temp.path().join("export");
    let a = export(&s, &folder).unwrap();
    let b = export(&s, &folder).unwrap();
    assert_ne!(a, b);
    assert!(
        fs::read_to_string(a)
            .unwrap()
            .contains("old model_provider=a 中文")
    );
}
#[test]
fn event_only_conversation_export() {
    let f = fixture();
    use std::io::Write;
    let meta = fs::read_to_string(&f.file)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_owned();
    fs::write(&f.file, meta + "\n").unwrap();
    writeln!(
        fs::OpenOptions::new().append(true).open(&f.file).unwrap(),
        "{}",
        json!({"type":"event_msg","payload":{"type":"user_message","message":"event-only content"}})
    )
    .unwrap();
    let s = scan(&f).remove(0);
    assert!(
        fs::read_to_string(export(&s, &f.temp.path().join("out")).unwrap())
            .unwrap()
            .contains("event-only content")
    );
}
#[test]
fn chat_pairs_tool_calls_and_outputs() {
    let v = json!({"input":[{"type":"function_call","call_id":"a","name":"demo","arguments":"{}"},{"type":"function_call_output","call_id":"a","output":"OK"}]});
    let out = to_chat(&v, "test").unwrap();
    assert_eq!(
        out["messages"][0]["tool_calls"][0]["id"],
        out["messages"][1]["tool_call_id"]
    );
}
#[test]
fn chat_rejects_native_image_tools() {
    assert!(to_chat(&json!({"tools":[{"type":"image_generation"}]}), "test").is_err());
}
#[test]
fn chat_rejects_encrypted_history() {
    assert!(
        to_chat(
            &json!({"input":[{"type":"reasoning","encrypted_content":"secret"}]}),
            "test"
        )
        .is_err()
    );
}
#[test]
fn chat_rejects_previous_response_id() {
    assert!(to_chat(&json!({"previous_response_id":"resp_1"}), "test").is_err());
}
#[test]
fn chat_stream_orders_complete_events() {
    let mut stream = ChatStream::new("model");
    let mut events = stream.start();
    events.extend(
        stream
            .ingest(&json!({"choices":[{"delta":{"content":"hello"},"finish_reason":null}]}))
            .unwrap(),
    );
    events.extend(
        stream
            .ingest(&json!({"choices":[{"delta":{},"finish_reason":"stop"}]}))
            .unwrap(),
    );
    events.extend(stream.finish().unwrap());
    assert_eq!(events.last().unwrap()["type"], "response.completed");
    for (i, e) in events.iter().enumerate() {
        assert_eq!(e["sequence_number"], i as u64);
    }
}
#[test]
fn chat_stream_interruption_is_not_success() {
    let mut s = ChatStream::new("model");
    s.ingest(&json!({"choices":[{"delta":{"content":"partial"}}]}))
        .unwrap();
    assert!(s.finish().is_err());
}
#[test]
fn chat_stream_assembles_tool_arguments() {
    let mut s = ChatStream::new("m");
    for (args, finish) in [("{", Value::Null), ("}", json!("tool_calls"))] {
        s.ingest(&json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"demo","arguments":args}}]},"finish_reason":finish}]})).unwrap();
    }
    let e = s.finish().unwrap();
    assert_eq!(
        e.last().unwrap()["response"]["output"][0]["arguments"],
        "{}"
    );
}
fn aggregate(strategy: Strategy) -> (Provider, Vec<Provider>) {
    let all = vec![api("a"), api("b")];
    let p = Provider {
        id: "group".into(),
        name: "group".into(),
        mode: AuthMode::Aggregate,
        strategy,
        members: all
            .iter()
            .map(|p| Member {
                provider_id: p.id.clone(),
                weight: 1,
                enabled: true,
            })
            .collect(),
        ..Default::default()
    };
    (p, all)
}
#[test]
fn route_round_robin_alternates() {
    let (p, all) = aggregate(Strategy::RoundRobin);
    let mut r = runtime::Selector::default();
    assert_eq!(r.select(&p, &all, None, false).unwrap()[0].id, "a");
    assert_eq!(r.select(&p, &all, None, false).unwrap()[0].id, "b");
}
#[test]
fn route_session_keeps_binding() {
    let (p, all) = aggregate(Strategy::Session);
    let mut r = runtime::Selector::default();
    r.succeeded("group", Some("thread"), "b");
    assert_eq!(
        r.select(&p, &all, Some("thread"), false).unwrap()[0].id,
        "b"
    );
}
#[test]
fn route_stateful_without_binding_fails() {
    let (p, all) = aggregate(Strategy::Failover);
    assert!(
        runtime::Selector::default()
            .select(&p, &all, Some("t"), true)
            .is_err()
    );
}
#[test]
fn route_cooling_original_member_not_replayed() {
    let (p, all) = aggregate(Strategy::Session);
    let mut r = runtime::Selector::default();
    r.succeeded("group", Some("t"), "a");
    r.failed("a");
    assert!(r.select(&p, &all, Some("t"), true).is_err());
}
#[test]
fn route_failover_retains_order() {
    let (p, all) = aggregate(Strategy::Failover);
    assert_eq!(
        runtime::Selector::default()
            .select(&p, &all, None, false)
            .unwrap()
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
}
#[test]
fn route_weighted_only_selects_enabled_members() {
    let (mut p, all) = aggregate(Strategy::Weighted);
    p.members[1].enabled = false;
    for _ in 0..20 {
        assert_eq!(
            runtime::Selector::default()
                .select(&p, &all, None, false)
                .unwrap()[0]
                .id,
            "a"
        );
    }
}
#[test]
fn migrated_legacy_json_and_dpapi_stay_readable() {
    let f = fixture();
    let old = f.temp.path().join("legacy");
    fs::create_dir_all(&old).unwrap();
    let p = api("a");
    fs::write(old.join("providers.json"),serde_json::to_vec(&json!([{"Id":p.id,"Name":"Old","Mode":"Api","Protocol":"Responses","BaseUrl":p.base_url,"Model":p.model,"ProtectedKey":p.protected_key}])).unwrap()).unwrap();
    assert!(f.store.migrate(&old).unwrap());
    assert_eq!(f.store.providers().unwrap()[0].name, "Old");
    assert!(!f.store.migrate(&old).unwrap());
    assert!(old.join("providers.json").exists());
}
#[test]
fn legacy_manifest_and_chunked_backup_restore() {
    let f = fixture();
    let original = fs::read(&f.file).unwrap();
    let dir = f.store.root.join("operations/legacy-123");
    fs::create_dir_all(&dir).unwrap();
    let backup = dir.join("0.before.dpapi");
    crypto::encrypt_file(&f.file, &backup).unwrap();
    fs::write(&f.file, b"after").unwrap();
    let manifest = json!({"State":"Complete","Description":"legacy","Files":[{"Path":f.file,"BeforeHash":hash_bytes(&original),"AfterHash":hash_bytes(b"after"),"Backup":backup,"Stage":null}]});
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    restore(
        &f.store.root,
        "legacy-123",
        &[f.settings.codex_home.clone()],
    )
    .unwrap();
    assert_eq!(fs::read(&f.file).unwrap(), original);
}
struct FakeHost {
    stops: AtomicUsize,
    starts: AtomicUsize,
    fail: bool,
}
impl DesktopHost for FakeHost {
    fn stop(&self) -> Result<()> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        anyhow::ensure!(!self.fail, "shutdown failed");
        Ok(())
    }
    fn start(&self) -> Result<String> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok("ready".into())
    }
}
#[test]
fn switch_a_b_a_preserves_body_and_restarts() {
    let f = fixture();
    db(&f);
    let all = vec![api("a"), api("b")];
    f.store.save("providers.json", &all).unwrap();
    let h = FakeHost {
        stops: AtomicUsize::new(0),
        starts: AtomicUsize::new(0),
        fail: false,
    };
    for p in [&all[1], &all[0]] {
        service::switch(&f.store, p, &h, &CancellationToken::new(), |_| {}).unwrap();
        assert_eq!(scan(&f)[0].provider, p.id);
    }
    assert_eq!(h.starts.load(Ordering::SeqCst), 2);
    assert!(
        fs::read_to_string(&f.file)
            .unwrap()
            .contains("old model_provider=a")
    );
}
#[test]
fn failed_shutdown_never_writes() {
    let f = fixture();
    let before = fs::read(&f.file).unwrap();
    let h = FakeHost {
        stops: AtomicUsize::new(0),
        starts: AtomicUsize::new(0),
        fail: true,
    };
    assert!(service::switch(&f.store, &api("a"), &h, &CancellationToken::new(), |_| {}).is_err());
    assert_eq!(fs::read(&f.file).unwrap(), before);
    assert!(!f.settings.codex_home.join("config.toml").exists());
}
#[test]
fn cancellation_before_commit_keeps_original() {
    let f = fixture();
    let before = fs::read(&f.file).unwrap();
    let ct = CancellationToken::new();
    ct.cancel();
    let mut j = Journal::new(&f.store.root, "cancel").unwrap();
    assert!(stage_changes(&mut j, &f.settings, &scan(&f), "provider", Some("b"), &ct).is_err());
    assert_eq!(fs::read(&f.file).unwrap(), before);
}
