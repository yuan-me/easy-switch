#[cfg(not(windows))]
use easy_switch_core::crypto;
use easy_switch_core::{
    Settings,
    journal::{self, Journal},
    sessions,
};
use rusqlite::{Connection, params};
use serde_json::json;
#[cfg(not(windows))]
use std::io::Cursor;
use std::{cell::Cell, fs};
use tokio_util::sync::CancellationToken;

fn initialize_crypto() {
    #[cfg(not(windows))]
    crypto::set_session_key([0x42; 32]).unwrap();
}

fn settings_event(
    id: &str,
    cwd: &std::path::Path,
    model: &str,
    provider: &str,
) -> serde_json::Value {
    json!({"timestamp":"2026-10-29T00:00:00Z","type":"event_msg","payload":{
    "type":"thread_settings_applied","thread_id":id,"thread_settings":{
        "model":model,"model_provider_id":provider,"approval_policy":"on-request",
        "approvals_reviewer":"user","permission_profile":{"type":"managed",
            "file_system":{"type":"restricted","entries":[{"path":{"type":"special","value":{"kind":"root"}},"access":"read"}]},"network":"restricted"},
        "cwd":cwd,"collaboration_mode":{"mode":"default","settings":{"model":model,"reasoning_effort":null,"developer_instructions":"keep original instructions"}},
        "disabled_plugin_ids":["keep-plugin"],"service_tier":"priority","future_field":{"preserve":true}
    }}})
}

fn projected_model_provider(path: &std::path::Path, id: &str) -> (Option<String>, Option<String>) {
    let mut model = None;
    let mut provider = None;
    for line in easy_switch_core::history::lines(path).unwrap() {
        let row: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        if row["type"] == "session_meta" {
            provider = row["payload"]["model_provider"].as_str().map(str::to_owned);
        }
        if row["type"] == "turn_context" {
            model = row["payload"]["model"].as_str().map(str::to_owned);
        }
        if row["type"] == "event_msg"
            && row["payload"]["type"] == "thread_settings_applied"
            && row["payload"]["thread_id"] == id
        {
            model = row["payload"]["thread_settings"]["model"]
                .as_str()
                .map(str::to_owned);
            provider = row["payload"]["thread_settings"]["model_provider_id"]
                .as_str()
                .map(str::to_owned);
        }
    }
    (model, provider)
}

#[test]
fn journal_setup_failure_leaves_no_manifest_and_releases_lock() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("store");
    assert!(
        Journal::new_with_setup(&root, "setup failure", &|| {
            assert!(journal::list(&root.join("operations"), false)?.is_empty());
            anyhow::bail!("target binding failed");
        })
        .is_err()
    );
    assert!(
        journal::list(&root.join("operations"), false)
            .unwrap()
            .is_empty()
    );
    assert_eq!(fs::read_dir(root.join("operations")).unwrap().count(), 0);
    let binding = root.join("target.json");
    let transaction = Journal::new_with_setup(&root, "retry", &|| {
        assert!(journal::list(&root.join("operations"), false)?.is_empty());
        fs::write(&binding, "bound target")?;
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::read(binding).unwrap(), b"bound target");
    assert_eq!(transaction.manifest.state, "Preparing");
}

#[test]
fn journal_setup_does_not_replace_binding_for_an_unfinished_operation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("store");
    let binding = root.join("target.json");
    {
        let mut transaction = Journal::new_with_setup(&root, "interrupted", &|| {
            fs::write(&binding, "original target")?;
            Ok(())
        })
        .unwrap();
        transaction.manifest.state = "Applying".into();
        fs::write(
            transaction.dir.join("manifest.json"),
            serde_json::to_vec(&transaction.manifest).unwrap(),
        )
        .unwrap();
    }
    let called = Cell::new(false);
    assert!(
        Journal::new_with_setup(&root, "new target", &|| {
            called.set(true);
            fs::write(&binding, "replacement target")?;
            Ok(())
        })
        .is_err()
    );
    assert!(!called.get());
    assert_eq!(fs::read(binding).unwrap(), b"original target");
    let pending = journal::list(&root.join("operations"), false).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].state, "Applying");
}

#[test]
fn commit_guard_retains_interrupted_transaction_until_writers_stop() {
    initialize_crypto();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("store");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::write(&first, "before one").unwrap();
    fs::write(&second, "before two").unwrap();
    let operation;
    {
        let mut transaction = Journal::new(&root, "guard test").unwrap();
        operation = transaction
            .dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        transaction.stage(&first, Some(b"after one")).unwrap();
        transaction.stage(&second, Some(b"after two")).unwrap();
        let checks = Cell::new(0);
        let result = transaction.commit_with_guard(&|_| {}, &|| {
            checks.set(checks.get() + 1);
            anyhow::ensure!(checks.get() <= 3, "writer appeared");
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(transaction.manifest.state, "Applying");
        assert_eq!(fs::read(&first).unwrap(), b"after one");
        assert_eq!(fs::read(&second).unwrap(), b"before two");
    }
    assert!(
        journal::restore_with_guard(
            &root,
            &operation,
            &[temp.path().to_owned()],
            &|| anyhow::bail!("still active")
        )
        .is_err()
    );
    assert_eq!(fs::read(&first).unwrap(), b"after one");
    journal::restore_with_guard(&root, &operation, &[temp.path().to_owned()], &|| Ok(())).unwrap();
    assert_eq!(fs::read(&first).unwrap(), b"before one");
    assert_eq!(fs::read(&second).unwrap(), b"before two");
}

#[test]
fn guard_failure_does_not_overwrite_external_edits_during_rollback() {
    initialize_crypto();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("store");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::write(&first, "before one").unwrap();
    fs::write(&second, "before two").unwrap();
    let mut transaction = Journal::new(&root, "external edit").unwrap();
    transaction.stage(&first, Some(b"after one")).unwrap();
    transaction.stage(&second, Some(b"after two")).unwrap();
    let checks = Cell::new(0);
    assert!(
        transaction
            .commit_with_guard(&|_| {}, &|| {
                checks.set(checks.get() + 1);
                if checks.get() == 4 {
                    fs::write(&first, "external edit")?;
                    anyhow::bail!("writer appeared");
                }
                Ok(())
            })
            .is_err()
    );
    assert_eq!(transaction.manifest.state, "Applying");
    assert_eq!(fs::read(&first).unwrap(), b"external edit");
    assert_eq!(fs::read(&second).unwrap(), b"before two");
}

#[test]
fn semantic_verification_failure_rolls_back_before_backups_are_removed() {
    initialize_crypto();
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("config.toml");
    fs::write(&file, b"original config").unwrap();
    let mut transaction = Journal::new(&temp.path().join("store"), "verify").unwrap();
    transaction.stage(&file, Some(b"invalid config")).unwrap();
    assert!(
        transaction
            .commit_verified(&|_| {}, &|| Ok(()), &|| {
                assert_eq!(fs::read(&file).unwrap(), b"invalid config");
                anyhow::bail!("semantic validation failed")
            })
            .is_err()
    );
    assert_eq!(transaction.manifest.state, "Restored");
    assert_eq!(fs::read(file).unwrap(), b"original config");
}

#[test]
fn provider_switch_updates_current_model_without_rewriting_historical_model() {
    initialize_crypto();
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(home.join("sessions")).unwrap();
    let rollout = home.join("sessions/rollout.jsonl");
    let original_snapshot = settings_event("thread-one", &home, "historical-model", "openai");
    let body = format!(
        "{original_snapshot}\n{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"historical-model\"}}}}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"user_message\",\"message\":\"keep me\"}}}}\n"
    );
    fs::write(&rollout, format!("{}\n{body}", json!({"type":"session_meta","payload":{"id":"thread-one","cwd":home,"model_provider":"openai"}}))).unwrap();
    let database = home.join("state_5.sqlite");
    let db = Connection::open(&database).unwrap();
    db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,title TEXT,cwd TEXT,model_provider TEXT,rollout_path TEXT,updated_at INTEGER,archived INTEGER,model TEXT)").unwrap();
    db.execute(
        "INSERT INTO threads VALUES('thread-one','title',?1,'openai',?2,1,0,'official-model')",
        params![home.to_string_lossy(), rollout.to_string_lossy()],
    )
    .unwrap();
    drop(db);
    let settings = Settings {
        codex_home: home.clone(),
        sqlite_home: None,
        ..Settings::default()
    };
    for (provider, model) in [
        ("api_a", Some("model-a")),
        ("api_b", Some("model-b")),
        ("api_b", Some("model-c")),
        ("openai", Some("official-model")),
    ] {
        let scanned = sessions::Scanner::default()
            .scan(&settings, &CancellationToken::new())
            .unwrap();
        assert!(scanned.warnings.is_empty(), "{:?}", scanned.warnings);
        sessions::validate_provider_model_history(&scanned.sessions, model).unwrap();
        let previous = fs::read_to_string(&rollout).unwrap();
        let mut transaction = Journal::new(&temp.path().join("store"), "switch").unwrap();
        sessions::stage_provider_with_model(
            &mut transaction,
            &settings,
            &scanned.sessions,
            provider,
            model,
            &CancellationToken::new(),
            &|_| {},
        )
        .unwrap();
        transaction.commit_with_guard(&|_| {}, &|| Ok(())).unwrap();
        let db = Connection::open(&database).unwrap();
        let current: (String, Option<String>) = db
            .query_row(
                "SELECT model_provider,model FROM threads WHERE id='thread-one'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(current, (provider.into(), model.map(str::to_owned)));
        let contents = fs::read_to_string(&rollout).unwrap();
        let tail = contents.split_once('\n').unwrap().1;
        assert!(tail.starts_with(previous.split_once('\n').unwrap().1));
        assert!(tail.starts_with(&body));
        assert_eq!(
            projected_model_provider(&rollout, "thread-one"),
            (model.map(str::to_owned), Some(provider.into()))
        );
        let appended: serde_json::Value =
            serde_json::from_str(contents.lines().last().unwrap()).unwrap();
        assert!(appended.get("ordinal").is_none());
        let mut expected = original_snapshot["payload"]["thread_settings"].clone();
        expected["model"] = model.unwrap().into();
        expected["model_provider_id"] = provider.into();
        expected["collaboration_mode"]["settings"]["model"] = model.unwrap().into();
        assert_eq!(appended["payload"]["thread_settings"], expected);
    }
}

#[test]
fn model_history_without_owned_complete_snapshot_or_target_model_is_rejected() {
    initialize_crypto();
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(home.join("sessions")).unwrap();
    let path = home.join("sessions/rollout.jsonl");
    let settings = Settings {
        codex_home: home.clone(),
        ..Settings::default()
    };
    let item = sessions::Session {
        id: "thread-one".into(),
        title: "test".into(),
        cwd: home.to_string_lossy().into(),
        provider: "openai".into(),
        path: path.clone(),
        updated: 0,
        archived: false,
        database: None,
        related_databases: vec![],
        related_paths: vec![],
    };
    let complete = settings_event(&item.id, &home, "old-model", "openai");
    let mut foreign = complete.clone();
    foreign["payload"]["thread_id"] = "another-thread".into();
    let mut incomplete = complete.clone();
    incomplete["payload"]["thread_settings"]
        .as_object_mut()
        .unwrap()
        .remove("permission_profile");
    let context = json!({"type":"turn_context","payload":{"model":"old-model"}});
    for (body, model, expected_error) in [
        (format!("{context}\n"), Some("new-model"), "完整设置快照"),
        (format!("{complete}\n{context}\n"), None, "明确指定目标模型"),
        (
            format!("{foreign}\n{context}\n"),
            Some("new-model"),
            "完整设置快照",
        ),
        (
            format!("{incomplete}\n{context}\n"),
            Some("new-model"),
            "权限配置",
        ),
    ] {
        let original = format!(
            "{}\n{body}",
            json!({"type":"session_meta","payload":{"id":item.id,"cwd":home,"model_provider":"openai"}})
        );
        fs::write(&path, &original).unwrap();
        assert!(
            sessions::validate_provider_model_history(std::slice::from_ref(&item), model)
                .unwrap_err()
                .to_string()
                .contains(expected_error)
        );
        let mut transaction =
            Journal::new(&temp.path().join("store"), "refuse unsafe settings").unwrap();
        assert!(
            sessions::stage_provider_with_model(
                &mut transaction,
                &settings,
                std::slice::from_ref(&item),
                "api-a",
                model,
                &CancellationToken::new(),
                &|_| {}
            )
            .is_err()
        );
        assert!(transaction.manifest.files.is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}

#[test]
fn paginated_model_event_keeps_rebased_checkpoint_before_new_event() {
    initialize_crypto();
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let folder = home.join("sessions");
    fs::create_dir_all(&folder).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let revision = uuid::Uuid::new_v4().to_string();
    let base = folder.join(format!("rollout-{id}.jsonl"));
    let current = folder.join(format!("rollout-{id}_{revision}.jsonl"));
    let mut snapshot = settings_event(&id, &home, "old-model", "openai");
    snapshot["ordinal"] = 1.into();
    let context = json!({"ordinal":2,"type":"turn_context","payload":{"model":"old-model"}});
    let base_meta = json!({"ordinal":0,"type":"session_meta","payload":{"id":id,"session_id":id,"history_mode":"paginated","cwd":home,"model_provider":"openai"}});
    fs::write(&base, format!("{base_meta}\n{snapshot}\n{context}\n")).unwrap();
    let base_len = fs::metadata(&base).unwrap().len();
    let current_meta = json!({"ordinal":3,"type":"session_meta","payload":{"id":id,"session_id":id,"history_mode":"paginated","cwd":home,"model_provider":"openai","history_base":{"thread_id":id,"end_byte_offset":base_len,"end_ordinal_exclusive":3}}});
    let user = json!({"ordinal":4,"type":"event_msg","payload":{"type":"user_message","message":"preserve this message"}});
    fs::write(&current, format!("{current_meta}\n{user}\n")).unwrap();
    let originals = [fs::read(&base).unwrap(), fs::read(&current).unwrap()];
    let db = Connection::open(home.join("state.sqlite")).unwrap();
    db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,title TEXT,cwd TEXT,model_provider TEXT,rollout_path TEXT,updated_at INTEGER,archived INTEGER,model TEXT)").unwrap();
    db.execute(
        "INSERT INTO threads VALUES(?1,'title',?2,'openai',?3,1,0,'old-model')",
        params![id, home.to_string_lossy(), current.to_string_lossy()],
    )
    .unwrap();
    drop(db);
    let projection = home.join("thread_history_1.sqlite");
    let db = Connection::open(&projection).unwrap();
    db.execute_batch("CREATE TABLE thread_history_projection_state(thread_id TEXT PRIMARY KEY,next_rollout_byte_offset INTEGER,next_rollout_ordinal INTEGER);CREATE TABLE thread_turns(thread_id TEXT,rollout_byte_offset INTEGER,rollout_end_byte_offset INTEGER,rollout_ordinal INTEGER)").unwrap();
    for (rollout, raw, ordinal) in [(&id, &originals[0], 3), (&revision, &originals[1], 5)] {
        let start = raw.iter().position(|b| *b == b'\n').unwrap() + 1;
        db.execute(
            "INSERT INTO thread_history_projection_state VALUES(?1,?2,?3)",
            params![rollout, raw.len(), ordinal],
        )
        .unwrap();
        db.execute(
            "INSERT INTO thread_turns VALUES(?1,?2,?3,?4)",
            params![rollout, start, raw.len(), ordinal - 1],
        )
        .unwrap();
    }
    drop(db);
    let settings = Settings {
        codex_home: home,
        ..Settings::default()
    };
    let scan = sessions::Scanner::default()
        .scan(&settings, &CancellationToken::new())
        .unwrap();
    assert!(scan.warnings.is_empty(), "{:?}", scan.warnings);
    assert_eq!(scan.sessions.len(), 1);
    assert_eq!(scan.sessions[0].path, current);
    let mut transaction =
        Journal::new(&temp.path().join("store"), "paginated model projection").unwrap();
    let provider = "api_provider_with_a_longer_identifier_than_openai";
    sessions::stage_provider_with_model(
        &mut transaction,
        &settings,
        &scan.sessions,
        provider,
        Some("new-model"),
        &CancellationToken::new(),
        &|_| {},
    )
    .unwrap();
    transaction.commit().unwrap();
    let db = Connection::open(&projection).unwrap();
    for (i, (path, rollout, old_ordinal)) in [(&base, &id, 3), (&current, &revision, 5)]
        .into_iter()
        .enumerate()
    {
        let bytes = fs::read(path).unwrap();
        let old = &originals[i];
        let old_header = old.iter().position(|b| *b == b'\n').unwrap() + 1;
        let new_header = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
        let delta = new_header - old_header;
        assert!(delta > 0);
        assert!(bytes[new_header..].starts_with(&old[old_header..]));
        let checkpoint: (u64, u64) = db.query_row("SELECT next_rollout_byte_offset,next_rollout_ordinal FROM thread_history_projection_state WHERE thread_id=?1", [rollout], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(checkpoint, ((old.len() + delta) as u64, old_ordinal));
        let turn: (u64, u64) = db.query_row("SELECT rollout_byte_offset,rollout_end_byte_offset FROM thread_turns WHERE thread_id=?1", [rollout], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(turn, (new_header as u64, (old.len() + delta) as u64));
        if path == &current {
            assert!(checkpoint.0 < bytes.len() as u64);
            let appended: serde_json::Value =
                serde_json::from_slice(&bytes[checkpoint.0 as usize..]).unwrap();
            assert_eq!(appended["ordinal"], 5);
            assert_eq!(appended["payload"]["thread_id"], id);
        } else {
            assert_eq!(checkpoint.0, bytes.len() as u64);
        }
    }
    assert_eq!(
        easy_switch_core::history::header(&current)
            .unwrap()
            .base_offset,
        Some(fs::metadata(&base).unwrap().len())
    );
    assert_eq!(
        projected_model_provider(&current, &id),
        (Some("new-model".into()), Some(provider.into()))
    );
    let valid = fs::read_to_string(&current).unwrap();
    let mut rows: Vec<serde_json::Value> = valid
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for (last_ordinal, prefix, expected) in [
        (None, None, "缺少 ordinal"),
        (Some(u64::MAX), None, "溢出"),
        (Some(5), Some(10), "继承前缀"),
    ] {
        let last = rows.last_mut().unwrap().as_object_mut().unwrap();
        if let Some(n) = last_ordinal {
            last.insert("ordinal".into(), n.into());
        } else {
            last.remove("ordinal");
        }
        if let Some(n) = prefix {
            rows[0]["payload"]["subagent_history_start_ordinal"] = n.into();
        }
        fs::write(
            &current,
            rows.iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>(),
        )
        .unwrap();
        assert!(
            sessions::validate_provider_model_history(&scan.sessions, Some("another-model"))
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
}

#[test]
fn default_model_restore_refuses_nonnullable_schema_without_changing_history() {
    initialize_crypto();
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(home.join("sessions")).unwrap();
    let rollout = home.join("sessions/rollout.jsonl");
    let original = format!(
        "{}\n",
        json!({"type":"session_meta","payload":{"id":"one","cwd":home,"model_provider":"api_a"}})
    );
    fs::write(&rollout, &original).unwrap();
    let database = home.join("state.sqlite");
    let db = Connection::open(&database).unwrap();
    db.execute_batch("CREATE TABLE threads(id TEXT PRIMARY KEY,title TEXT,cwd TEXT,model_provider TEXT,rollout_path TEXT,updated_at INTEGER,archived INTEGER,model TEXT NOT NULL)").unwrap();
    db.execute(
        "INSERT INTO threads VALUES('one','title',?1,'api_a',?2,1,0,'model-a')",
        params![home.to_string_lossy(), rollout.to_string_lossy()],
    )
    .unwrap();
    drop(db);
    let settings = Settings {
        codex_home: home.clone(),
        sqlite_home: None,
        ..Settings::default()
    };
    let scan = sessions::Scanner::default()
        .scan(&settings, &CancellationToken::new())
        .unwrap();
    assert!(scan.warnings.is_empty());
    let before = fs::read(&database).unwrap();
    let mut transaction = Journal::new(&temp.path().join("store"), "unsupported schema").unwrap();
    assert!(
        sessions::stage_provider_with_model(
            &mut transaction,
            &settings,
            &scan.sessions,
            "openai",
            None,
            &CancellationToken::new(),
            &|_| {}
        )
        .is_err()
    );
    drop(transaction);
    assert_eq!(fs::read(&database).unwrap(), before);
    assert_eq!(fs::read_to_string(rollout).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn unix_paths_keep_case_and_allow_valid_posix_names() {
    let root = std::path::Path::new("/home/Alice/.codex");
    assert!(sessions::within(&root.join("sessions/file.jsonl"), root).unwrap());
    assert!(
        !sessions::within(
            std::path::Path::new("/home/alice/.codex/sessions/file.jsonl"),
            root
        )
        .unwrap()
    );
    assert!(!sessions::within(std::path::Path::new("/home/Alice/.codex-other"), root).unwrap());
    assert!(sessions::within(root, std::path::Path::new("/")).unwrap());
    assert!(sessions::normalize(&root.join("valid:posix. ")).is_ok());
    assert!(sessions::normalize(&root.join("../other")).is_err());
    assert!(!sessions::same_path(&root.join("A"), &root.join("a")).unwrap());
}

#[cfg(unix)]
#[test]
fn unix_files_are_private_and_symlinks_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    initialize_crypto();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private/store");
    let store = easy_switch_core::Store::open(root.clone()).unwrap();
    store
        .save("config.json", &json!({"key":"synthetic"}))
        .unwrap();
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(root.join("config.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let external = temp.path().join("external");
    fs::write(&external, b"preserved").unwrap();
    symlink(&external, root.join("alias")).unwrap();
    assert!(easy_switch_core::store::atomic_write(&root.join("alias"), b"replacement").is_err());
    assert!(sessions::no_links(&root.join("alias")).is_err());
    assert_eq!(fs::read(external).unwrap(), b"preserved");
    let mut journal = Journal::new(&root, "case").unwrap();
    journal.stage(&root.join("A"), Some(b"upper")).unwrap();
    journal.stage(&root.join("a"), Some(b"lower")).unwrap();
    journal.commit().unwrap();
    assert_eq!(fs::read(root.join("A")).unwrap(), b"upper");
    assert_eq!(fs::read(root.join("a")).unwrap(), b"lower");
}

#[cfg(not(windows))]
#[test]
fn encrypted_stream_rejects_tampering_truncation_and_frame_reordering() {
    initialize_crypto();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("backup");
    let data = vec![0x39; 2 * 1024 * 1024 + 17];
    crypto::encrypt_reader(Cursor::new(&data), &path).unwrap();
    let encrypted = fs::read(&path).unwrap();
    let mut plain = vec![];
    crypto::decrypt_file(&path, &mut plain).unwrap();
    assert_eq!(plain, data);
    assert!(!encrypted.windows(64).any(|part| part == &data[..64]));
    let mut cases = vec![
        encrypted[..encrypted.len() - 1].to_vec(),
        encrypted[..12].to_vec(),
    ];
    let mut tampered = encrypted.clone();
    tampered[30] ^= 1;
    cases.push(tampered);
    let frame = 4 + 1024 * 1024 + 16;
    let mut reordered = encrypted.clone();
    reordered[12..12 + frame].copy_from_slice(&encrypted[12 + frame..12 + 2 * frame]);
    reordered[12 + frame..12 + 2 * frame].copy_from_slice(&encrypted[12..12 + frame]);
    cases.push(reordered);
    for altered in cases {
        fs::write(&path, altered).unwrap();
        assert!(crypto::decrypt_file(&path, std::io::sink()).is_err());
    }
    let credential = crypto::protect("synthetic-key").unwrap();
    assert_eq!(crypto::unprotect(&credential).unwrap(), "synthetic-key");
    assert!(crypto::set_session_key([0x43; 32]).is_err());
}
