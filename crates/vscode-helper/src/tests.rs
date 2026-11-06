use super::*;
use rusqlite::{Connection, params};
mod appserver;

struct Fixture {
    _root: tempfile::TempDir,
    request: Request,
    rollout: PathBuf,
    body: String,
}
impl Fixture {
    fn new() -> Self {
        #[cfg(not(windows))]
        easy_switch_core::crypto::set_session_key([73; 32]).unwrap();
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("codex");
        let project = root.path().join("project");
        fs::create_dir_all(home.join("sessions")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            home.join("config.toml"),
            "# keep this comment\nmodel = 'official-model'\n[features]\nweb_search = true\n",
        )
        .unwrap();
        fs::write(
            home.join("auth.json"),
            "{\"tokens\":{\"access_token\":\"synthetic-do-not-send\"}}",
        )
        .unwrap();
        let rollout = home.join("sessions/rollout.jsonl");
        let snapshot = json!({"type":"event_msg","timestamp":"2026-10-29T00:00:00Z","payload":{
        "type":"thread_settings_applied","thread_id":"old-thread","thread_settings":{
            "model":"official-model","model_provider_id":"openai","approval_policy":"on-request",
            "approvals_reviewer":"user","permission_profile":{"type":"managed","file_system":{"type":"restricted","entries":[{"path":{"type":"special","value":{"kind":"root"}},"access":"read"}]},"network":"restricted"},
            "cwd":project,"collaboration_mode":{"mode":"default","settings":{"model":"official-model","reasoning_effort":null,"developer_instructions":null}},"disabled_plugin_ids":["preserved-plugin"]
        }}});
        let body = format!(
            "{snapshot}\n{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"official-model\"}}}}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"user_message\",\"message\":\"Keep my old conversation\"}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"function_call\",\"name\":\"test_tool\",\"call_id\":\"keep-call\",\"arguments\":\"{{}}\"}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"function_call_output\",\"call_id\":\"keep-call\",\"output\":\"keep tool output\"}}}}\n"
        );
        let meta = json!({"type":"session_meta","payload":{"id":"old-thread","cwd":project,"model_provider":"openai"}});
        fs::write(&rollout, format!("{meta}\n{body}")).unwrap();
        let db = Connection::open(home.join("state_5.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE threads (id TEXT PRIMARY KEY,title TEXT,cwd TEXT,model_provider TEXT,rollout_path TEXT,updated_at INTEGER,archived INTEGER,model TEXT)").unwrap();
        db.execute_batch("CREATE TABLE backfill_state(id INTEGER PRIMARY KEY,status TEXT); INSERT INTO backfill_state VALUES(1,'complete')").unwrap();
        db.execute("INSERT INTO threads VALUES ('old-thread','Old chat',?1,'openai',?2,42,0,'official-model')",params![project.to_str(),rollout.to_str()]).unwrap();
        drop(db);
        let request = Request {
            protocol_version: 1,
            action: "switch".into(),
            home,
            sqlite_home: None,
            store_root: root.path().join("plugin"),
            provider: None,
            api_key: None,
            official_model: None,
            journal_key: None,
            operation_id: None,
            expected_config_fingerprint: None,
            project_dirs: vec![project],
        };
        Self {
            _root: root,
            request,
            rollout,
            body,
        }
    }
    fn select(&mut self, id: &str, model: &str) {
        self.request.provider = Some(Provider {
            id: id.into(),
            name: format!("Provider {id}"),
            base_url: "https://example.test/v1".into(),
            model: model.into(),
        });
        self.request.api_key = Some(format!("synthetic-key-{id}"));
        self.request.expected_config_fingerprint = Some(journal::hash_bytes(
            read_config(&self.request.home).unwrap().0.as_bytes(),
        ));
    }
    fn run(&self) -> Result<Value> {
        validate_paths(&self.request)?;
        switch(&self.request, &|_| {}, &|| Ok(()))
    }
}

#[test]
fn official_api_a_api_b_official_keeps_login_and_history() {
    let mut f = Fixture::new();
    let auth = fs::read(f.request.home.join("auth.json")).unwrap();
    for (id, model, provider) in [
        ("a", "api-a-model", PROVIDER),
        ("b", "api-b-model", PROVIDER),
        ("official", "", "openai"),
    ] {
        f.select(id, model);
        assert_eq!(f.run().unwrap()["committed"], true);
        let text = fs::read_to_string(&f.rollout).unwrap();
        assert!(text.split_once('\n').unwrap().1.starts_with(&f.body));
        let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(last["payload"]["type"], "thread_settings_applied");
        assert_eq!(
            last["payload"]["thread_settings"]["model_provider_id"],
            provider
        );
        assert_eq!(
            last["payload"]["thread_settings"]["model"],
            if id == "official" {
                "official-model"
            } else {
                model
            }
        );
        assert_eq!(fs::read(f.request.home.join("auth.json")).unwrap(), auth);
        let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
        let row: (String, String, i64) = db
            .query_row(
                "SELECT model_provider,model,updated_at FROM threads WHERE id='old-thread'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                provider.into(),
                if id == "official" {
                    "official-model".into()
                } else {
                    model.into()
                },
                42
            )
        );
        let cfg = read_config(&f.request.home).unwrap().0;
        assert!(
            cfg.contains("# keep this comment") && cfg.contains("web_search = true"),
            "{id}: {cfg}"
        );
        if id == "official" {
            assert!(!cfg.contains("synthetic-key"));
        }
        assert_eq!(
            active_provider(
                &read_config(&f.request.home).unwrap().1,
                &read_state(&f.request).unwrap()
            ),
            Some(id.into())
        );
    }
}

#[test]
fn stale_configuration_and_busy_guard_leave_files_unchanged() {
    let mut f = Fixture::new();
    f.select("a", "model-a");
    let before = fs::read(&f.rollout).unwrap();
    fs::write(f.request.home.join("config.toml"), "# external change\n").unwrap();
    assert!(f.run().unwrap_err().to_string().contains("外部修改"));
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
    f.select("a", "model-a");
    assert!(switch(&f.request, &|_| {}, &|| anyhow::bail!("busy")).is_err());
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
}

#[test]
fn rejects_collision_and_project_overrides_without_stopping_backend() {
    let mut f = Fixture::new();
    f.select("a", "model-a");
    let config = f.request.home.join("config.toml");
    fs::write(
        &config,
        format!("[model_providers.{PROVIDER}]\nname='not ours'\n"),
    )
    .unwrap();
    assert!(
        inspect(&f.request)
            .unwrap_err()
            .to_string()
            .contains("冲突")
    );
    fs::write(&config, "").unwrap();
    let project_config = f.request.project_dirs[0].join(".codex");
    fs::create_dir(&project_config).unwrap();
    fs::write(
        project_config.join("config.toml"),
        "model_provider='another'\n",
    )
    .unwrap();
    assert!(
        inspect(&f.request)
            .unwrap_err()
            .to_string()
            .contains("项目配置覆盖")
    );
}

#[test]
fn missing_key_and_invalid_url_do_not_touch_configuration() {
    let mut f = Fixture::new();
    f.select("a", "model-a");
    let before = read_config(&f.request.home).unwrap().0;
    f.request.api_key = None;
    assert!(f.run().is_err());
    assert_eq!(read_config(&f.request.home).unwrap().0, before);
    f.request.api_key = Some("synthetic-key".into());
    f.request.provider.as_mut().unwrap().base_url = "https://user:password@example.test/v1".into();
    assert!(f.run().is_err());
    assert_eq!(read_config(&f.request.home).unwrap().0, before);
}

#[test]
fn incomplete_active_index_is_rejected_before_switching() {
    for damage in ["missing-db", "missing-row", "missing-model", "backfilling"] {
        let mut f = Fixture::new();
        f.select("a", "model-a");
        let path = f.request.home.join("state_5.sqlite");
        if damage == "missing-db" {
            fs::rename(&path, f._root.path().join("offline.sqlite")).unwrap();
        } else {
            let db = Connection::open(&path).unwrap();
            db.execute_batch(match damage {
                "missing-row" => "DELETE FROM threads",
                "missing-model" => "ALTER TABLE threads DROP COLUMN model",
                _ => "UPDATE backfill_state SET status='running'",
            })
            .unwrap();
        }
        let config = fs::read(f.request.home.join("config.toml")).unwrap();
        let history = fs::read(&f.rollout).unwrap();
        assert!(inspect(&f.request).is_err(), "{damage}");
        assert_eq!(
            fs::read(f.request.home.join("config.toml")).unwrap(),
            config
        );
        assert_eq!(fs::read(&f.rollout).unwrap(), history);
        assert!(!f.request.home.join(OWNER_FILE).exists());
    }
}

#[test]
fn model_history_without_owned_settings_is_rejected_in_preflight() {
    let mut f = Fixture::new();
    f.select("a", "model-a");
    let meta = fs::read_to_string(&f.rollout)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_owned();
    let original = format!("{meta}\n{}", f.body.split_once('\n').unwrap().1);
    fs::write(&f.rollout, &original).unwrap();
    assert!(inspect(&f.request).is_err());
    assert_eq!(fs::read_to_string(&f.rollout).unwrap(), original);
    assert!(!f.request.home.join(OWNER_FILE).exists());
}

#[test]
fn official_without_config_model_uses_confirmed_model_and_restores_default_config() {
    let mut f = Fixture::new();
    fs::write(
        f.request.home.join("config.toml"),
        "# default official model\n",
    )
    .unwrap();
    f.select("a", "api-only");
    let mut status = f.request.clone();
    status.provider = None;
    assert!(inspect(&status).unwrap()["officialModel"].is_null());
    assert!(f.run().unwrap_err().to_string().contains("确认切回官方"));
    f.request.official_model = Some("confirmed-official".into());
    assert_eq!(
        inspect(&f.request).unwrap()["officialModel"],
        "confirmed-official"
    );
    f.run().unwrap();
    f.request.official_model = None;
    f.select("official", "");
    f.run().unwrap();
    let db = Connection::open(f.request.home.join("state_5.sqlite")).unwrap();
    let model: Option<String> = db
        .query_row("SELECT model FROM threads", [], |r| r.get(0))
        .unwrap();
    assert_eq!(model.as_deref(), Some("confirmed-official"));
    assert!(
        !read_config(&f.request.home)
            .unwrap()
            .1
            .contains_key("model")
    );
}

#[test]
fn user_config_selects_real_database_directory_over_environment() {
    let mut f = Fixture::new();
    let dbdir = f._root.path().join("external-db");
    fs::create_dir(&dbdir).unwrap();
    fs::rename(
        f.request.home.join("state_5.sqlite"),
        dbdir.join("state_5.sqlite"),
    )
    .unwrap();
    let wrong = f._root.path().join("unused-env-db");
    fs::create_dir(&wrong).unwrap();
    let mut doc = read_config(&f.request.home).unwrap().1;
    doc["sqlite_home"] = value(dbdir.to_str().unwrap());
    fs::write(f.request.home.join("config.toml"), doc.to_string()).unwrap();
    f.request.sqlite_home = Some(wrong);
    f.select("a", "target-model");
    let resolved = resolve_request(&f.request).unwrap();
    assert_eq!(resolved.sqlite_home.as_deref(), Some(dbdir.as_path()));
    assert_eq!(
        inspect(&resolved).unwrap()["sqliteHome"],
        dbdir.to_str().unwrap()
    );
    switch(&resolved, &|_| {}, &|| Ok(())).unwrap();
    let db = Connection::open(dbdir.join("state_5.sqlite")).unwrap();
    let row: (String, String) = db
        .query_row("SELECT model_provider,model FROM threads", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(row, (PROVIDER.into(), "target-model".into()));
}

#[test]
fn interrupted_operation_remains_visible_even_if_live_config_is_invalid() {
    let mut f = Fixture::new();
    f.request.action = "inspect".into();
    let op = f.request.store_root.join("operations/test-operation");
    fs::create_dir_all(&op).unwrap();
    fs::write(
        op.join("manifest.json"),
        r#"{"state":"Applying","description":"Interrupted test","files":[]}"#,
    )
    .unwrap();
    fs::write(
        f.request.store_root.join("target.json"),
        serde_json::to_vec(&Target {
            home: f.request.home.clone(),
            sqlite_home: None,
        })
        .unwrap(),
    )
    .unwrap();
    fs::write(
        f.request.home.join("config.toml"),
        "this is not valid toml = [",
    )
    .unwrap();
    let result = execute(&f.request, &|_| {}).unwrap();
    assert_eq!(result["operations"][0]["state"], "Applying");
    assert!(result["configFingerprint"].is_null());
}

#[test]
fn another_store_cannot_inspect_switch_or_recover_an_owned_home() {
    let mut f = Fixture::new();
    f.request.action = "inspect".into();
    execute(&f.request, &|_| {}).unwrap();
    assert!(!f.request.home.join(OWNER_FILE).exists());
    fs::create_dir(&f.request.store_root).unwrap();
    claim_owner(&f.request).unwrap();
    let owner_bytes = fs::read(f.request.home.join(OWNER_FILE)).unwrap();
    let before = fs::read(&f.rollout).unwrap();
    let config = read_config(&f.request.home).unwrap().0;
    let operation = f.request.store_root.join("operations/interrupted");
    fs::create_dir_all(&operation).unwrap();
    fs::write(
        operation.join("manifest.json"),
        r#"{"state":"Applying","description":"Original store interrupted","files":[]}"#,
    )
    .unwrap();
    let mut foreign = f.request.clone();
    foreign.store_root = f._root.path().join("another-profile");
    foreign.operation_id = Some("interrupted".into());
    for action in ["inspect", "switch", "recover"] {
        foreign.action = action.into();
        assert!(
            execute(&foreign, &|_| {})
                .unwrap_err()
                .to_string()
                .contains("已绑定另一 VS Code 存储环境"),
            "{action} must reject before inspecting foreign operations or running the Linux guard"
        );
    }
    assert!(switch(&foreign, &|_| {}, &|| Ok(())).is_err());
    assert!(recover(&foreign, &|| Ok(())).is_err());
    assert!(!foreign.store_root.exists());
    assert_eq!(
        fs::read(f.request.home.join(OWNER_FILE)).unwrap(),
        owner_bytes
    );
    assert_eq!(fs::read(&f.rollout).unwrap(), before);
    assert_eq!(read_config(&f.request.home).unwrap().0, config);
    assert!(operation.join("manifest.json").exists());
}

#[test]
fn concurrent_first_claim_has_one_owner_and_never_overwrites_it() {
    let f = Fixture::new();
    let first = f.request.clone();
    let mut second = first.clone();
    second.store_root = f._root.path().join("another-profile");
    fs::create_dir(&first.store_root).unwrap();
    fs::create_dir(&second.store_root).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = [first.clone(), second.clone()].map(|request| {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            let result = claim_owner(&request);
            (request, result)
        })
    });
    let outcomes = handles.map(|handle| handle.join().unwrap());
    assert_eq!(
        outcomes.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    let (winner, _) = outcomes.iter().find(|(_, result)| result.is_ok()).unwrap();
    let (loser, _) = outcomes.iter().find(|(_, result)| result.is_err()).unwrap();
    let path = f.request.home.join(OWNER_FILE);
    let before = fs::read(&path).unwrap();
    let owner: HomeOwner = serde_json::from_slice(&before).unwrap();
    assert_eq!(owner.home, canonical_directory(&winner.home).unwrap());
    assert_eq!(
        owner.store_root,
        canonical_directory(&winner.store_root).unwrap()
    );
    claim_owner(winner).unwrap();
    assert!(claim_owner(loser).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(fs::read_dir(&f.request.home).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".easy-switch-owner-")
    }));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[cfg(unix)]
#[test]
fn sqlite_directory_symlink_is_rejected_before_migration() {
    use std::os::unix::fs::symlink;
    let mut f = Fixture::new();
    let target = f._root.path().join("real-db");
    fs::create_dir(&target).unwrap();
    let link = f._root.path().join("linked-db");
    symlink(target, &link).unwrap();
    let mut doc = read_config(&f.request.home).unwrap().1;
    doc["sqlite_home"] = value(link.to_str().unwrap());
    fs::write(f.request.home.join("config.toml"), doc.to_string()).unwrap();
    f.select("a", "model-a");
    assert!(
        resolve_request(&f.request)
            .err()
            .unwrap()
            .to_string()
            .contains("链接")
    );
}
