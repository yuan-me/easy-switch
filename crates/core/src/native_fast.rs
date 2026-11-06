use crate::{AuthMode, Protocol, Provider, Result, Settings};
use anyhow::{Context, ensure};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{net::TcpListener, time::Duration};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, protocol::WebSocketConfig},
};

const SCRIPT: &str = include_str!("../assets/native-fast-unlock.js");
const LIMIT: usize = 96 * 1024 * 1024;
const FLAGS: &str = r#"window.__codexPlusNativeFastUnlock.runtime.update({enhancementsEnabled:true,codexAppNativeFastUnlock:true,nativeFastLocalApiConfigured:true});"#;

pub fn supports_model(p: &Provider, all: &[Provider], model: &str, catalogs: &crate::catalog::Catalogs) -> bool {
    if p.protocol != Protocol::Responses { return false; }
    if p.mode == AuthMode::Aggregate {
        let members: Vec<_> = p.members.iter().filter(|m| m.enabled).collect();
        return model == p.model
            && !members.is_empty()
            && members.iter().all(|m| {
                all.iter().find(|x| x.id == m.provider_id).is_some_and(|x| {
                    matches!(x.mode, AuthMode::Api | AuthMode::Mixed)
                        && x.protocol == Protocol::Responses
                })
            });
    }
    matches!(p.mode, AuthMode::Api | AuthMode::Mixed)
        && p.protocol == Protocol::Responses
        && catalogs.providers.get(&p.id).map_or(model == p.model, |list| list.selected(&p.model).iter().any(|id| id == model))
}

pub fn model_tiers(model: &mut Value, supported: bool) {
    for (field, id, entry) in [
        (
            "service_tiers",
            "priority",
            json!({"id":"priority","name":"Fast","description":"优先处理（以供应商支持为准）"}),
        ),
        ("additional_speed_tiers", "fast", json!("fast")),
    ] {
        let mut tiers = model[field].as_array().cloned().unwrap_or_default();
        tiers.retain(|v| v.as_str().or_else(|| v["id"].as_str()) != Some(id));
        if supported {
            tiers.push(entry);
        }
        model[field] = json!(tiers);
    }
}

fn local_api_config(text: &str) -> bool {
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    if doc.get("profile").is_some()
        || doc.get("forced_login_method").and_then(|v| v.as_str()) != Some("api")
    {
        return false;
    }
    let Some(id) = doc
        .get("model_provider")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty() && *id != "openai")
    else {
        return false;
    };
    let Some(provider) = doc.get("model_providers").and_then(|v| v.get(id)) else {
        return false;
    };
    provider
        .get("requires_openai_auth")
        .and_then(|v| v.as_bool())
        == Some(false)
        && provider
            .get("experimental_bearer_token")
            .and_then(|v| v.as_str())
            .is_some_and(|key| !key.is_empty())
}

pub fn launch_port(settings: &Settings, enabled: bool) -> Result<Option<u16>> {
    if !enabled {
        return Ok(None);
    }
    let path = settings.codex_home.join("config.toml");
    if !path.is_file() {
        return Ok(None);
    }
    crate::sessions::no_links(&path)?;
    ensure!(
        std::fs::metadata(&path)?.len() <= 8 * 1024 * 1024,
        "Codex 配置过大，未开启 Fast 调试端口"
    );
    if !local_api_config(&std::fs::read_to_string(path)?) {
        return Ok(None);
    }
    // A fresh ephemeral loopback port avoids attaching to an existing debug session.
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    Ok(Some(listener.local_addr()?.port()))
}

fn local_page(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| u.scheme() == "app" && u.host_str() == Some("-"))
}

fn socket_url(url: &str, port: u16) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "ws"
            && u.host_str() == Some("127.0.0.1")
            && u.port() == Some(port)
            && u.username().is_empty()
            && u.password().is_none()
            && u.path().starts_with("/devtools/page/")
            && u.query().is_none()
            && u.fragment().is_none()
    })
}

struct Cdp {
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    id: u64,
    module_urls: Vec<String>,
}

impl Cdp {
    async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.id += 1;
        let id = self.id;
        tokio::time::timeout(Duration::from_secs(45), async {
            self.socket
                .send(Message::Text(
                    json!({"id":id,"method":method,"params":params})
                        .to_string()
                        .into(),
                ))
                .await?;
            while let Some(message) = self.socket.next().await {
                if let Message::Text(text) = message? {
                    let value: Value = serde_json::from_str(&text)?;
                    if value["method"] == "Debugger.scriptParsed" {
                        if let Some(url) = value["params"]["url"].as_str().filter(|u| u.starts_with("app://-/assets/")) {
                            self.module_urls.push(url.to_owned());
                        }
                    }
                    if value["id"] == id {
                        ensure!(value.get("error").is_none(), "Codex 调试命令失败：{method}");
                        return Ok(value["result"].clone());
                    }
                }
            }
            anyhow::bail!("Codex 调试连接已关闭")
        })
        .await
        .context("Fast 解锁超时")?
    }

    async fn evaluate(&mut self, expression: &str) -> Result<Value> {
        let result = self
            .call(
                "Runtime.evaluate",
                json!({"expression":expression,"returnByValue":true,"awaitPromise":true}),
            )
            .await?;
        ensure!(
            result.get("exceptionDetails").is_none(),
            "Codex Fast 脚本执行失败"
        );
        Ok(result["result"]["value"].clone())
    }
}

fn checked_modules(value: &Value) -> Result<&Vec<Value>> {
    ensure!(
        value["ok"] == true,
        "当前 Codex 版本不支持 Fast 解锁，已保留原界面。请关闭开关或更新 Easy Switch"
    );
    let modules = value["modules"].as_array().context("Fast 补丁缺少模块")?;
    ensure!(
        !modules.is_empty() && modules.len() <= 16,
        "Fast 补丁模块数量无效"
    );
    let mut total = 0;
    let mut urls = std::collections::HashSet::new();
    let mut entry = false;
    for module in modules {
        let url = module["url"].as_str().context("Fast 模块地址缺失")?;
        let parsed = url::Url::parse(url)?;
        ensure!(
            local_page(url)
                && parsed.path().starts_with("/assets/")
                && parsed.path().ends_with(".js")
                && parsed.query().is_none()
                && parsed.fragment().is_none()
                && urls.insert(url),
            "Fast 模块必须来自当前本地客户端"
        );
        let source = module["patchedSource"]
            .as_str()
            .context("Fast 模块内容缺失")?;
        ensure!(
            !source.is_empty() && source.len() <= 24 * 1024 * 1024,
            "Fast 模块体积无效"
        );
        total += source.len();
        entry |= source.contains("window.__codexPlusNativeFastCompiled=\"v1\";");
    }
    ensure!(
        total <= 64 * 1024 * 1024 && entry,
        "Fast 模块不完整或体积过大"
    );
    Ok(modules)
}

async fn install(port: u16) -> Result<Cdp> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
    let websocket = loop {
        if let Ok(response) = client
            .get(format!("http://127.0.0.1:{port}/json/list"))
            .send()
            .await
        {
            if response
                .content_length()
                .is_some_and(|len| len <= 1024 * 1024)
            {
                if let Ok(pages) = response.json::<Vec<Value>>().await {
                    let pages: Vec<_> = pages
                        .iter()
                        .filter(|v| {
                            v["type"] == "page" && v["url"].as_str().is_some_and(local_page)
                        })
                        .collect();
                    ensure!(
                        pages.len() <= 1,
                        "检测到多个 Codex 页面，Fast 未解锁。请关闭多余窗口后重试"
                    );
                    if let Some(url) = pages
                        .first()
                        .and_then(|v| v["webSocketDebuggerUrl"].as_str())
                    {
                        ensure!(socket_url(url, port), "拒绝连接非本机 Codex 调试地址");
                        break url.to_owned();
                    }
                }
            }
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Codex 未开放本机调试入口；原生 Fast 未解锁"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(LIMIT))
        .max_frame_size(Some(LIMIT));
    let (socket, _) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async_with_config(&websocket, Some(config), true),
    )
    .await??;
    let mut cdp = Cdp { socket, id: 0, module_urls: vec![] };
    cdp.call("Runtime.enable", json!({})).await?;
    cdp.call("Page.enable", json!({})).await?;
    ensure!(
        cdp.evaluate("location.origin").await? == "app://-",
        "调试页面不是 Codex 本机界面"
    );
    let loaded_by = tokio::time::Instant::now() + Duration::from_secs(20);
    while cdp.evaluate("document.readyState").await? != "complete" {
        ensure!(
            tokio::time::Instant::now() < loaded_by,
            "Codex 页面尚未加载完成，Fast 未解锁"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    cdp.evaluate(&format!("{SCRIPT}\n{FLAGS}")).await?;
    cdp.call("Debugger.enable", json!({})).await?;
    // 新版窗口先显示，Rspack 的 Fast 模块随后才加载。
    let modules_by = tokio::time::Instant::now() + Duration::from_secs(35);
    let prepared = loop {
        let result = cdp.evaluate(&format!("window.__codexPlusNativeFastUnlock.prepareDocumentModules({{moduleUrls:{}}}).then(r=>({{ok:r.ok,modules:r.ok?r.modules:[],diagnostics:r.diagnostics}}))", serde_json::to_string(&cdp.module_urls)?)).await?;
        if result["ok"] == true || result["diagnostics"]["reason"] != "incomplete-gates"
            || tokio::time::Instant::now() >= modules_by { break result; }
        tokio::time::sleep(Duration::from_millis(500)).await;
        cdp.evaluate("document.readyState").await?;
    };
    cdp.call("Debugger.disable", json!({})).await?;
    let modules = checked_modules(&prepared)?;
    let source = format!(
        "{SCRIPT}\n{FLAGS}\nwindow.__codexPlusNativeFastUnlock.installModules({});",
        serde_json::to_string(modules)?
    );
    let registered = cdp
        .call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({"source":source}),
        )
        .await?;
    let identifier = registered["identifier"]
        .as_str()
        .filter(|v| !v.is_empty())
        .context("Fast 启动脚本未登记")?
        .to_owned();
    let applied: Result<()> = async {
        cdp.call("Page.reload", json!({"ignoreCache":true})).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(35);
        loop {
            if let Ok(state) = cdp.evaluate("({state:window.__codexPlusNativeFastPatchState,compiled:window.__codexPlusNativeFastCompiled})").await {
                if state["state"] == "patched" && state["compiled"] == "v1" { return Ok(()); }
                ensure!(state["state"] != "failed", "Fast 模块加载失败，恢复原始界面");
            }
            ensure!(tokio::time::Instant::now() < deadline, "Fast 模块未确认生效，恢复原始界面");
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }.await;
    if let Err(error) = applied {
        let removed = cdp
            .call(
                "Page.removeScriptToEvaluateOnNewDocument",
                json!({"identifier":identifier}),
            )
            .await;
        if removed.is_ok() {
            let _ = cdp.call("Page.reload", json!({"ignoreCache":true})).await;
        }
        return Err(error);
    }
    Ok(cdp)
}

pub fn install_blocking(port: u16) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let mut cdp = runtime.block_on(install(port))?;
    // Retain the document hook for reloads; the client discards it when Codex exits.
    std::thread::spawn(move || {
        runtime.block_on(async move {
            while let Some(Ok(message)) = cdp.socket.next().await {
                if message.is_close() {
                    break;
                }
                if cdp.socket.flush().await.is_err() {
                    break;
                }
            }
        })
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_is_explicit_and_aggregate_requires_every_enabled_member() {
        assert!(!Settings::default().native_fast_unlock);
        assert!(!Provider::default().native_fast_enabled());
        let p = Provider {
            id: "api".into(),
            model: "one".into(),
            ..Default::default()
        };
        let mut catalogs = crate::catalog::Catalogs::default();
        assert!(supports_model(&p, &[], "one", &catalogs));
        assert!(!supports_model(&p, &[], "two", &catalogs));
        let list = catalogs.providers.entry(p.id.clone()).or_default();
        list.refresh(vec!["one".into(), "two".into()], 1).unwrap();
        assert!(supports_model(&p, &[], "two", &catalogs));
        catalogs.providers.get_mut(&p.id).unwrap().select(vec!["two".into()], &p.model).unwrap();
        assert!(!supports_model(&p, &[], "two", &catalogs));
        let mut group = Provider {
            mode: AuthMode::Aggregate,
            model: "group".into(),
            members: vec![crate::Member {
                provider_id: "api".into(),
                weight: 1,
                enabled: true,
            }],
            ..Default::default()
        };
        assert!(supports_model(&group, &[p.clone()], "group", &catalogs));
        group.members.push(crate::Member {
            provider_id: "missing".into(),
            weight: 1,
            enabled: true,
        });
        assert!(!supports_model(&group, &[p.clone()], "group", &catalogs));
        group.members[1].enabled = false;
        assert!(supports_model(&group, &[p], "group", &catalogs));
        group.members.clear();
        assert!(!supports_model(&group, &[], "group", &catalogs));
    }

    #[test]
    fn declarations_remove_only_fast_and_preserve_other_metadata() {
        let mut model = json!({"service_tiers":[{"id":"flex"}],"additional_speed_tiers":["slow"],"context_window":123});
        model_tiers(&mut model, true);
        model_tiers(&mut model, true);
        assert_eq!(model["service_tiers"].as_array().unwrap().len(), 2);
        model_tiers(&mut model, false);
        assert_eq!(model["service_tiers"], json!([{"id":"flex"}]));
        assert_eq!(model["additional_speed_tiers"], json!(["slow"]));
        assert_eq!(model["context_window"], 123);
    }

    #[test]
    fn debug_port_requires_fast_and_local_api_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let settings = Settings { codex_home: temp.path().to_owned(), ..Default::default() };
        assert!(launch_port(&settings, false).unwrap().is_none());
        assert!(launch_port(&settings, true).unwrap().is_none());
        std::fs::write(temp.path().join("config.toml"), "forced_login_method='chatgpt'").unwrap();
        assert!(launch_port(&settings, true).unwrap().is_none());
        std::fs::write(temp.path().join("config.toml"), "forced_login_method='api'\nmodel_provider='mine'\n[model_providers.mine]\nrequires_openai_auth=false\nexperimental_bearer_token='synthetic-test'").unwrap();
        assert!(launch_port(&settings, false).unwrap().is_none());
        assert!(launch_port(&settings, true).unwrap().is_some());
    }

    #[test]
    fn identities_and_transport_fail_closed() {
        let api = "forced_login_method='api'\nmodel_provider='mine'\n[model_providers.mine]\nrequires_openai_auth=false\nexperimental_bearer_token='test'";
        assert!(local_api_config(api));
        assert!(!local_api_config(&api.replace("'api'", "'chatgpt'")));
        assert!(!local_api_config(&format!("profile='other'\n{api}")));
        assert!(!local_api_config(
            "forced_login_method='api'\nmodel_provider='missing'"
        ));
        assert!(!local_api_config("bad syntax"));
        assert!(socket_url("ws://127.0.0.1:1234/devtools/page/test", 1234));
        for url in [
            "ws://example.com:1234/devtools/page/test",
            "ws://127.0.0.1:1235/devtools/page/test",
            "ws://user@127.0.0.1:1234/devtools/page/test",
        ] {
            assert!(!socket_url(url, 1234));
        }
        assert!(checked_modules(&json!({"ok":false})).is_err());
        assert!(checked_modules(&json!({"ok":true,"modules":[{"url":"https://example.com/assets/a.js","patchedSource":"x"}]})).is_err());
    }

    #[tokio::test]
    async fn cdp_applies_only_complete_modules_and_removes_failed_hooks() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        for mode in ["patched", "delayed", "unsupported", "failed"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut http, _) = listener.accept().await.unwrap();
                let mut request = vec![0; 4096];
                http.read(&mut request).await.unwrap();
                let body = json!([{"type":"page","url":"app://-/index.html","webSocketDebuggerUrl":format!("ws://127.0.0.1:{port}/devtools/page/test")}]).to_string();
                http.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
                drop(http);
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                let mut methods = vec![];
                let mut preparations = 0;
                while let Some(Ok(Message::Text(text))) = socket.next().await {
                    let call: Value = serde_json::from_str(&text).unwrap();
                    let method = call["method"].as_str().unwrap();
                    methods.push(method.to_owned());
                    let expression = call["params"]["expression"].as_str().unwrap_or_default();
                    let result = match method {
                        "Runtime.evaluate" => {
                            json!({"result":{"value":if expression == "location.origin" {
                            json!("app://-")
                        } else if expression == "document.readyState" {
                            json!("complete")
                        } else if expression.starts_with("window.__codexPlusNativeFastUnlock.prepareDocumentModules(") {
                            preparations += 1;
                            assert!(expression.contains("app://-/assets/async/1.abc.js"));
                            if mode == "delayed" && preparations == 1 {
                                json!({"ok":false,"modules":[],"diagnostics":{"reason":"incomplete-gates"}})
                            } else {
                                json!({"ok":mode != "unsupported","modules":[{"url":"app://-/assets/index-test.js","patchedSource":"window.__codexPlusNativeFastCompiled=\"v1\";"}]})
                            }
                        } else if expression.starts_with("({state:") {
                            json!({"state":if mode == "delayed" { "patched" } else { mode },"compiled":"v1"})
                        } else { Value::Null }}})
                        }
                        "Page.addScriptToEvaluateOnNewDocument" => {
                            assert!(
                                call["params"]["source"]
                                    .as_str()
                                    .unwrap()
                                    .contains("installModules")
                            );
                            let source = call["params"]["source"].as_str().unwrap();
                            assert!(source.contains(FLAGS));
                            assert!(!source.contains("__easySwitchLanguagePatched"));
                            json!({"identifier":"hook"})
                        }
                        _ => json!({}),
                    };
                    if method == "Debugger.enable" {
                        socket.send(Message::Text(json!({"method":"Debugger.scriptParsed","params":{"url":"app://-/assets/async/1.abc.js"}}).to_string().into())).await.unwrap();
                    }
                    socket
                        .send(Message::Text(
                            json!({"id":call["id"],"result":result}).to_string().into(),
                        ))
                        .await
                        .unwrap();
                }
                methods
            });
            let installed = install(port).await;
            assert_eq!(installed.is_ok(), matches!(mode, "patched" | "delayed"));
            drop(installed);
            let methods = server.await.unwrap();
            if mode == "unsupported" {
                assert!(!methods.iter().any(|m| m == "Page.reload"));
            }
            if mode == "failed" {
                assert!(
                    methods
                        .iter()
                        .any(|m| m == "Page.removeScriptToEvaluateOnNewDocument")
                );
                assert_eq!(methods.iter().filter(|m| *m == "Page.reload").count(), 2);
            }
        }
    }
}
