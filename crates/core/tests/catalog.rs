use easy_switch_core::{
    catalog::{self, ModelList},
    journal::{self, Journal},
    *,
};
use serde_json::{Value, json};
use std::fs;

fn names(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).into()).collect()
}

#[test]
fn refresh_preserves_exclusions_detects_additions_and_removals_and_keeps_failed_lists() {
    let mut state = ModelList::default();
    state
        .refresh(names(&["default", "old", "excluded", "old"]), 10)
        .unwrap();
    state.select(names(&["excluded"]), "default").unwrap();
    state.added.clear();
    state
        .refresh(names(&["new", "default", "excluded"]), 20)
        .unwrap();
    assert_eq!(state.added, names(&["new"]));
    assert_eq!(state.removed, names(&["old"]));
    assert_eq!(state.selected("default"), names(&["default", "new"]));
    state.refresh(names(&["default", "new"]), 30).unwrap();
    state
        .refresh(names(&["default", "new", "excluded"]), 40)
        .unwrap();
    assert!(!state.selected("default").contains(&"excluded".into()));
    assert_eq!(state.added, names(&["excluded", "new"]));
    assert!(state.select(names(&["default"]), "default").is_err());
    let before = serde_json::to_value(&state).unwrap();
    assert!(state.refresh(vec![], 50).is_err());
    assert!(state.refresh(names(&["bad\nmodel"]), 50).is_err());
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    assert_eq!(
        state.selected("missing-default"),
        names(&["default", "missing-default", "new"])
    );
    assert!(catalog::refresh_due(None, 0));
    assert!(!catalog::refresh_due(Some(10), 3609));
    assert!(catalog::refresh_due(Some(10), 3610));
    assert!(catalog::refresh_due(Some(10), 9));
}

#[test]
fn known_metadata_is_preserved_and_unknown_models_do_not_inherit_capabilities() {
    let template = json!({"slug":"known", "display_name":"Known", "base_instructions":"keep", "input_modalities":["text","image"], "context_window":99999,"tool_mode":"code_mode", "unknown":{"keep":true},"visibility":"hide"});
    let result = catalog::model_document(&names(&["known", "unknown"]), &[template]);
    assert_eq!(result["models"][0]["base_instructions"], "keep");
    assert_eq!(result["models"][0]["unknown"]["keep"], true);
    assert_eq!(result["models"][0]["visibility"], "list");
    assert_eq!(result["models"][1]["input_modalities"], json!(["text"]));
    assert!(result["models"][1].get("context_window").is_none());
    assert!(result["models"][1].get("tool_mode").is_none());
}

fn fixture() -> (tempfile::TempDir, Store, Settings, Provider) {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("store")).unwrap();
    let settings = Settings {
        codex_home: temp.path().join("home"),
        active_provider_id: Some("api".into()),
        ..Default::default()
    };
    fs::create_dir(&settings.codex_home).unwrap();
    store.save("settings.json", &settings).unwrap();
    let p = Provider {
        id: "api".into(),
        name: "Synthetic".into(),
        base_url: "https://example.invalid/v1".into(),
        model: "default".into(),
        ..Default::default()
    };
    store
        .save_provider(p.clone(), Some("synthetic-key".into()))
        .unwrap();
    fs::write(
        settings.codex_home.join("config.toml"),
        "model_provider = 'api'\nmodel = 'default'\n# keep\nunknown = 42\n",
    )
    .unwrap();
    (temp, store, settings, p)
}

#[test]
fn active_catalog_applies_and_refreshes_do_not_retain_backups() {
    let (_temp, store, settings, _p) = fixture();
    let cfg = settings.codex_home.join("config.toml");
    let mut catalogs = catalog::load(&store).unwrap();
    catalogs
        .providers
        .entry("api".into())
        .or_default()
        .refresh(names(&["default", "new"]), 1)
        .unwrap();
    catalog::save(&store, catalogs, "api", None).unwrap();
    let document: Value = serde_json::from_slice(
        &fs::read(settings.codex_home.join("easy-switch-models.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(document["models"].as_array().unwrap().len(), 2);
    let config = fs::read_to_string(&cfg).unwrap();
    assert!(config.contains("# keep"));
    assert!(config.contains("unknown = 42"));
    assert!(config.contains("model_catalog_json"));
    let ops = journal::list(&store.root.join("operations"), false).unwrap();
    assert!(ops.is_empty());
    let mut catalogs = catalog::load(&store).unwrap();
    catalogs
        .providers
        .get_mut("api")
        .unwrap()
        .refresh(names(&["new", "default"]), 3601)
        .unwrap();
    catalog::save(&store, catalogs, "api", None).unwrap();
    assert_eq!(
        journal::list(&store.root.join("operations"), false)
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn inactive_lists_stay_local_and_official_switch_restores_existing_custom_catalog() {
    let (_temp, store, settings, p) = fixture();
    let cfg = settings.codex_home.join("config.toml");
    fs::write(
        &cfg,
        "model_provider = 'api'\nmodel_catalog_json = 'custom.json'\n",
    )
    .unwrap();
    fs::write(settings.codex_home.join("custom.json"), br#"{"models":[]}"#).unwrap();
    let mut catalogs = catalog::load(&store).unwrap();
    catalogs
        .providers
        .entry("other".into())
        .or_default()
        .refresh(names(&["other-model"]), 1)
        .unwrap();
    catalog::save(&store, catalogs, "other", None).unwrap();
    assert!(!settings.codex_home.join("easy-switch-models.json").exists());
    let mut catalogs = catalog::load(&store).unwrap();
    catalogs
        .providers
        .entry(p.id.clone())
        .or_default()
        .refresh(names(&["default", "new"]), 1)
        .unwrap();
    catalog::save(&store, catalogs, "api", None).unwrap();
    let mut config = fs::read_to_string(&cfg).unwrap();
    let mut relative = config.parse::<toml_edit::DocumentMut>().unwrap();
    relative["model_catalog_json"] = toml_edit::value("easy-switch-models.json");
    config = relative.to_string();
    let mut j = Journal::new(&store.root, "official").unwrap();
    catalog::stage_for_provider(
        &mut j,
        &store,
        &Provider::official(),
        &settings,
        &mut config,
    )
    .unwrap();
    assert_eq!(
        config.parse::<toml_edit::DocumentMut>().unwrap()["model_catalog_json"].as_str(),
        Some("custom.json")
    );
    assert_eq!(
        fs::read_to_string(settings.codex_home.join("custom.json")).unwrap(),
        r#"{"models":[]}"#
    );
}

#[test]
fn stale_provider_and_profile_are_rejected_without_changing_config_or_catalog() {
    let (_temp, store, settings, _) = fixture();
    for config in [
        "model_provider='external'\n",
        "model_provider='api'\nprofile='work'\n",
    ] {
        fs::write(settings.codex_home.join("config.toml"), config).unwrap();
        let mut catalogs = catalog::load(&store).unwrap();
        catalogs
            .providers
            .entry("api".into())
            .or_default()
            .refresh(names(&["new"]), 1)
            .unwrap();
        assert!(catalog::save(&store, catalogs, "api", None).is_err());
        assert_eq!(
            fs::read_to_string(settings.codex_home.join("config.toml")).unwrap(),
            config
        );
        assert!(!settings.codex_home.join("easy-switch-models.json").exists());
    }
}

#[test]
fn saving_a_new_default_allows_removing_the_previous_default() {
    let (_temp, store, settings, mut provider) = fixture();
    provider.model = "new-default".into();
    store.save_provider(provider, None).unwrap();
    let mut catalogs = catalog::load(&store).unwrap();
    let models = catalogs.providers.entry("api".into()).or_default();
    models
        .refresh(names(&["default", "new-default"]), 1)
        .unwrap();
    models.select(names(&["default"]), "new-default").unwrap();
    catalog::save(&store, catalogs, "api", Some("new-default")).unwrap();
    let config = fs::read_to_string(settings.codex_home.join("config.toml"))
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    assert_eq!(config["model"].as_str(), Some("new-default"));
    let document: Value = serde_json::from_slice(
        &fs::read(settings.codex_home.join("easy-switch-models.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(document["models"].as_array().unwrap().len(), 1);
    assert_eq!(document["models"][0]["slug"], "new-default");
}

#[test]
#[ignore = "requires CODEX_TEST_EXECUTABLE; starts an isolated app-server with synthetic models"]
fn generated_catalog_is_accepted_by_codex_model_list() {
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Command, Stdio},
        sync::mpsc,
        time::Duration,
    };
    let executable = std::env::var_os("CODEX_TEST_EXECUTABLE").expect("set CODEX_TEST_EXECUTABLE");
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("models.json");
    fs::write(
        &path,
        serde_json::to_vec(&catalog::model_document(
            &names(&["synthetic-model-one", "synthetic-model-two"]),
            &[],
        ))
        .unwrap(),
    )
    .unwrap();
    let mut config = toml_edit::DocumentMut::new();
    config["model_catalog_json"] = toml_edit::value(path.to_string_lossy().as_ref());
    fs::write(temp.path().join("config.toml"), config.to_string()).unwrap();
    let mut child = Command::new(executable)
        .args(["app-server", "--stdio"])
        .env("CODEX_HOME", temp.path())
        .current_dir(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok)
        {
            let _ = send.send(line);
        }
    });
    let result = (|| -> Result<()> {
        let input = child.stdin.as_mut().unwrap();
        writeln!(
            input,
            "{}",
            json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"easy_switch_catalog_test","version":"1.0"}}})
        )?;
        input.flush()?;
        loop {
            let line: Value =
                serde_json::from_str(&receive.recv_timeout(Duration::from_secs(20))?)?;
            if line["id"] == 1 {
                anyhow::ensure!(line.get("error").is_none(), "initialize: {line}");
                break;
            }
        }
        writeln!(input, "{}", json!({"method":"initialized","params":{}}))?;
        writeln!(
            input,
            "{}",
            json!({"id":2,"method":"model/list","params":{"includeHidden":true}})
        )?;
        input.flush()?;
        loop {
            let line: Value =
                serde_json::from_str(&receive.recv_timeout(Duration::from_secs(20))?)?;
            if line["id"] == 2 {
                anyhow::ensure!(line.get("error").is_none(), "model/list: {line}");
                let list = line["result"]["data"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("missing model data: {line}"))?;
                let ids: Vec<_> = list.iter().filter_map(|v| v["model"].as_str()).collect();
                anyhow::ensure!(
                    ids == ["synthetic-model-one", "synthetic-model-two"],
                    "unexpected models: {ids:?}"
                );
                break;
            }
        }
        Ok(())
    })();
    let _ = child.kill();
    let _ = child.wait();
    result.unwrap();
}
