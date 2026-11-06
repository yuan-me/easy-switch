//! Optional local-binary proof. Only synthetic threads are resumed; no login or turn requests.
use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

const VERSION: &str = "codex-cli 0.164.0-alpha.11.1";
const THREAD: &str = "dc6b9584-cb06-4bf8-98c9-5360f3ced2da";
const TITLE: &str = "Easy Switch isolated projection proof";

fn log(path: &Path, message: &str) -> Result<()> {
    println!("{message}");
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?,
        "{message}"
    )?;
    Ok(())
}

fn command(executable: &Path, root: &Path, endpoint: &str) -> Command {
    let mut command = Command::new(executable);
    command.env_clear();
    // Preserve only OS process-launch essentials, never inherited Codex credentials or paths.
    for key in ["SystemRoot", "WINDIR", "COMSPEC", "PATHEXT"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .current_dir(root.join("project"))
        .env("CODEX_HOME", root.join("codex"))
        .env("HOME", root.join("user"))
        .env("USERPROFILE", root.join("user"))
        .env("APPDATA", root.join("appdata"))
        .env("LOCALAPPDATA", root.join("localappdata"))
        .env("TMP", root.join("tmp"))
        .env("TEMP", root.join("tmp"))
        .env("TMPDIR", root.join("tmp"))
        .env("OPENAI_BASE_URL", format!("{endpoint}/v1"))
        .env("OTEL_SDK_DISABLED", "true")
        .env("NO_PROXY", "")
        .env("no_proxy", "");
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        command.env(key, endpoint);
    }
    command
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Server {
    child: OwnedChild,
    input: Option<ChildStdin>,
    responses: Receiver<String>,
}
impl Server {
    fn start(executable: &Path, root: &Path, endpoint: &str, evidence: &Path) -> Result<Self> {
        let stderr = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(evidence)?;
        let mut child = command(executable, root, endpoint)
            .args(["app-server", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()?;
        let input = child.stdin.take();
        let stdout = child
            .stdout
            .take()
            .context("app-server stdout unavailable")?;
        let (send, responses) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let mut server = Self {
            child: OwnedChild(child),
            input,
            responses,
        };
        let initialized = server.rpc(
            1,
            "initialize",
            json!({
                "clientInfo":{"name":"easy_switch_projection_test","version":"0.1.0"},
                "capabilities":{"experimentalApi":true}
            }),
        )?;
        log(evidence, &format!("initialize: {initialized}"))?;
        server.send(json!({"method":"initialized","params":{}}))?;
        Ok(server)
    }
    fn send(&mut self, message: Value) -> Result<()> {
        let input = self.input.as_mut().context("app-server stdin closed")?;
        writeln!(input, "{message}")?;
        input.flush()?;
        Ok(())
    }
    fn rpc(&mut self, id: u64, method: &str, params: Value) -> Result<Value> {
        ensure!(
            matches!(method, "initialize" | "thread/list" | "thread/resume"),
            "test must not send account or turn requests"
        );
        self.send(json!({"id":id,"method":method,"params":params}))?;
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .context("app-server request timed out")?;
            let line = self.responses.recv_timeout(remaining)?;
            let response: Value = serde_json::from_str(&line)?;
            if response["id"] == id {
                ensure!(
                    response.get("error").is_none(),
                    "{method} failed: {response}"
                );
                return Ok(response["result"].clone());
            }
        }
    }
    fn stop(mut self) -> Result<()> {
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.child.0.try_wait()?.is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        ensure!(
            self.child.0.try_wait()?.is_some(),
            "app-server did not exit after stdin closed"
        );
        // Drop only owns this exact spawned child; it never enumerates or kills other Codex processes.
        Ok(())
    }
}

#[test]
#[ignore = "requires EASY_SWITCH_TEST_CODEX; isolated real app-server schema and search reconciliation"]
fn real_appserver_search_keeps_appended_model_projection() -> Result<()> {
    #[cfg(not(windows))]
    easy_switch_core::crypto::set_session_key([73; 32])?;
    let executable = PathBuf::from(
        std::env::var_os("EASY_SWITCH_TEST_CODEX").context("set EASY_SWITCH_TEST_CODEX")?,
    );
    ensure!(executable.is_file(), "Codex binary is missing");
    let artifacts = match std::env::var_os("EASY_SWITCH_TEST_ARTIFACTS") {
        Some(path) => PathBuf::from(path),
        None => Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../artifacts")
            .canonicalize()?,
    };
    fs::create_dir_all(&artifacts)?;
    let temporary = tempfile::Builder::new()
        .prefix("appserver-proof-")
        .tempdir_in(&artifacts)?;
    let root = sessions::normalize(&temporary.path().canonicalize()?)?;
    let home = root.join("codex");
    for folder in [
        "codex/sessions",
        "project",
        "user",
        "appdata",
        "localappdata",
        "tmp",
    ] {
        fs::create_dir_all(root.join(folder))?;
    }
    let evidence = artifacts.join("vscode-appserver-projection.log");
    fs::write(
        &evidence,
        b"Isolated app-server projection validation; synthetic data only.\n",
    )?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    drop(listener); // Connections go to this unbound loopback port, never an external service.
    let mut config = DocumentMut::new();
    config["model"] = value("official-model");
    config["cli_auth_credentials_store"] = value("file");
    config["chatgpt_base_url"] = value(&endpoint);
    config["check_for_update_on_startup"] = value(false);
    fs::write(home.join("config.toml"), config.to_string())?;
    let mut version = OwnedChild(
        command(&executable, &root, &endpoint)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while version.0.try_wait()?.is_none() {
        ensure!(Instant::now() < deadline, "Codex --version timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut version_text = String::new();
    version
        .0
        .stdout
        .take()
        .context("version stdout unavailable")?
        .read_to_string(&mut version_text)?;
    let expected_version =
        std::env::var("EASY_SWITCH_TEST_CODEX_VERSION").unwrap_or_else(|_| VERSION.into());
    ensure!(
        version_text.trim() == expected_version,
        "unvalidated Codex version: {}",
        version_text.trim()
    );
    log(&evidence, version_text.trim())?;

    let mut server = Server::start(&executable, &root, &endpoint, &evidence)?;
    server.rpc(2, "thread/list", json!({"limit":10,"searchTerm":TITLE}))?;
    server.stop()?;
    let database = home.join("state_5.sqlite");
    ensure!(
        database.is_file(),
        "official app-server did not create its real database"
    );
    let db = Connection::open(&database)?;
    let schema: String = db.query_row(
        "SELECT sql FROM sqlite_master WHERE name='threads'",
        [],
        |r| r.get(0),
    )?;
    log(
        &evidence,
        &format!("Official-created threads schema: {schema}"),
    )?;
    let columns = db
        .prepare("PRAGMA table_info(threads)")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let rollout_dir = home.join("sessions/2026/10/29");
    fs::create_dir_all(&rollout_dir)?;
    let rollout = rollout_dir.join(format!("rollout-2026-10-29T00-00-00-{THREAD}.jsonl"));
    let meta = json!({"timestamp":"2026-10-29T00:00:00Z","type":"session_meta","payload":{
        "id":THREAD,"session_id":THREAD,"timestamp":"2026-10-29T00:00:00Z","cwd":root.join("project"),
        "originator":"easy_switch_projection_test","cli_version":version_text.trim().trim_start_matches("codex-cli "),
        "source":"vscode","model_provider":"openai","history_mode":"legacy"
    }});
    let snapshot = json!({"timestamp":"2026-10-29T00:00:01Z","type":"event_msg","payload":{
        "type":"thread_settings_applied","thread_id":THREAD,"thread_settings":{
            "model":"official-model","model_provider_id":"openai","approval_policy":"on-request","approvals_reviewer":"user",
            "permission_profile":{"type":"managed","file_system":{"type":"restricted","entries":[]},"network":"restricted"},
            "cwd":root.join("project"),"collaboration_mode":{"mode":"default","settings":{"model":"official-model","reasoning_effort":null,"developer_instructions":null}},"disabled_plugin_ids":[]
    }}});
    let context = json!({"timestamp":"2026-10-29T00:00:02Z","type":"turn_context","payload":{
        "cwd":root.join("project"),"approval_policy":"on-request","sandbox_policy":{"type":"read-only"},"model":"official-model"
    }});
    let user = json!({"timestamp":"2026-10-29T00:00:03Z","type":"event_msg","payload":{"type":"user_message","message":TITLE}});
    let body = format!("{snapshot}\n{context}\n{user}\n");
    fs::write(&rollout, format!("{meta}\n{body}"))?;
    let mut names = vec![];
    let mut values = vec![];
    for (name, kind, required, default) in &columns {
        use rusqlite::types::Value as Sql;
        let known = match name.as_str() {
            "id" => Some(Sql::Text(THREAD.into())),
            "rollout_path" => Some(Sql::Text(rollout.to_string_lossy().into())),
            "cwd" => Some(Sql::Text(root.join("project").to_string_lossy().into())),
            "title" | "first_user_message" | "preview" => Some(Sql::Text(TITLE.into())),
            "has_user_event" => Some(Sql::Integer(1)),
            "source" => Some(Sql::Text("vscode".into())),
            "model_provider" => Some(Sql::Text("openai".into())),
            "model" => Some(Sql::Text("official-model".into())),
            "approval_mode" => Some(Sql::Text("on-request".into())),
            "sandbox_policy" => Some(Sql::Text("{\"type\":\"read-only\"}".into())),
            "created_at" | "updated_at" => Some(Sql::Integer(1_793_232_000)),
            _ if *required && default.is_none() => Some(match kind.to_uppercase().as_str() {
                "TEXT" => Sql::Text(String::new()),
                "INTEGER" | "INT" => Sql::Integer(0),
                _ => anyhow::bail!("unknown required official schema field: {name} {kind}"),
            }),
            _ => None,
        };
        if let Some(v) = known {
            names.push(format!("\"{}\"", name.replace('"', "\"\"")));
            values.push(v);
        }
    }
    db.execute(
        &format!(
            "INSERT INTO threads ({}) VALUES ({})",
            names.join(","),
            vec!["?"; values.len()].join(",")
        ),
        rusqlite::params_from_iter(values),
    )?;
    let backfill: String =
        db.query_row("SELECT status FROM backfill_state WHERE id=1", [], |r| {
            r.get(0)
        })?;
    ensure!(
        backfill == "complete",
        "official initial backfill is not complete: {backfill}"
    );
    drop(db);
    let mut request = Request {
        protocol_version: 1,
        action: "switch".into(),
        home: home.clone(),
        sqlite_home: None,
        store_root: root.join("plugin"),
        provider: None,
        api_key: None,
        official_model: None,
        journal_key: None,
        operation_id: None,
        expected_config_fingerprint: None,
        project_dirs: vec![root.join("project")],
    };
    for (id, model, expected_provider) in [
        ("a", "api-a-model", PROVIDER),
        ("b", "api-b-model", PROVIDER),
        ("official", "official-model", "openai"),
    ] {
        request.provider = Some(Provider {
            id: id.into(),
            name: format!("Synthetic {id}"),
            base_url: format!("{endpoint}/v1"),
            model: model.into(),
        });
        request.api_key = Some("synthetic-local-only-key".into());
        request.official_model = (id == "official").then(|| model.into());
        request.expected_config_fingerprint =
            Some(journal::hash_bytes(read_config(&home)?.0.as_bytes()));
        switch(&request, &|_| {}, &|| Ok(()))?;
        let db = Connection::open(&database)?;
        db.execute("UPDATE threads SET model='deliberately-stale-model',model_provider='deliberately-stale-provider' WHERE id=?1", [THREAD])?;
        drop(db);
        let mut server = Server::start(&executable, &root, &endpoint, &evidence)?;
        let result = server.rpc(2,"thread/list",json!({"limit":20,"searchTerm":TITLE,"sourceKinds":["vscode"],"modelProviders":[expected_provider,"deliberately-stale-provider","openai"]}))?;
        let resumed = server.rpc(3, "thread/resume", json!({"threadId": THREAD}))?;
        ensure!(
            resumed["thread"]["id"] == THREAD,
            "resume changed thread identity: {resumed}"
        );
        ensure!(
            resumed["model"] == model && resumed["modelProvider"] == expected_provider,
            "resume lost target model/provider: {resumed}"
        );
        log(
            &evidence,
            &format!(
                "PASS resume {id}: original thread, provider={expected_provider}, model={model}"
            ),
        )?;
        server.stop()?;
        let db = Connection::open(&database)?;
        let projection: (String, String) = db.query_row(
            "SELECT model_provider,model FROM threads WHERE id=?1",
            [THREAD],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        log(
            &evidence,
            &format!("Search result: {result}; database projection: {projection:?}"),
        )?;
        ensure!(
            result["data"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|r| r["id"] == THREAD)),
            "synthetic thread missing after actual search reconciliation: {result}"
        );
        ensure!(
            projection == (expected_provider.into(), model.into()),
            "actual reconciliation lost target projection: {projection:?}"
        );
        ensure!(
            fs::read_to_string(&rollout)?
                .split_once('\n')
                .context("missing metadata line")?
                .1
                .starts_with(&body),
            "original history bytes changed"
        );
        ensure!(
            !home.join("auth.json").exists(),
            "test unexpectedly created authentication state"
        );
        log(
            &evidence,
            &format!(
                "PASS {id}: real thread/list(searchTerm) rebuilt poisoned SQLite -> provider={expected_provider}, model={model}; original history preserved"
            ),
        )?;
    }
    log(
        &evidence,
        "PASS isolated real schema/parser/reconciliation; no account or turn/start requests",
    )?;
    Ok(())
}
