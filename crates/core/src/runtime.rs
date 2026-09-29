use crate::{
    desktop::{Desktop, hidden},
    protocol, *,
};
use anyhow::{Context, ensure};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use fs2::FileExt;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct Selector {
    counter: usize,
    pinned: HashMap<String, String>,
    cooling: HashMap<String, Instant>,
}
impl Selector {
    pub fn select(
        &mut self,
        p: &Provider,
        all: &[Provider],
        session: Option<&str>,
        stateful: bool,
    ) -> Result<Vec<Provider>> {
        if p.mode != AuthMode::Aggregate {
            return Ok(vec![p.clone()]);
        }
        let members: Vec<_> = p
            .members
            .iter()
            .filter(|m| m.enabled)
            .filter_map(|m| all.iter().find(|p| p.id == m.provider_id))
            .filter(|m| {
                self.cooling
                    .get(&m.id)
                    .is_none_or(|t| t.elapsed() > Duration::from_secs(20))
            })
            .cloned()
            .collect();
        ensure!(!members.is_empty(), "所有成员已停用或正在冷却");
        let pin = format!("{}:{}", p.id, session.unwrap_or(""));
        if stateful {
            let saved = self
                .pinned
                .get(&pin)
                .filter(|_| session.is_some())
                .context("有状态请求没有原成员绑定，不能轮转")?;
            return Ok(vec![
                members
                    .iter()
                    .find(|m| &m.id == saved)
                    .context("原成员不可用，不能重放有状态请求")?
                    .clone(),
            ]);
        }
        if p.strategy == Strategy::Session {
            ensure!(session.is_some(), "按会话路由需要线程标识");
            if let Some(saved) = self.pinned.get(&pin) {
                return Ok(vec![
                    members
                        .iter()
                        .find(|m| &m.id == saved)
                        .context("会话原成员不可用")?
                        .clone(),
                ]);
            }
        }
        let index = match p.strategy {
            Strategy::Failover => 0,
            Strategy::Weighted => {
                use rand::Rng;
                let weights: Vec<u32> = members
                    .iter()
                    .map(|m| {
                        p.members
                            .iter()
                            .find(|x| x.provider_id == m.id)
                            .unwrap()
                            .weight
                    })
                    .collect();
                let mut n = rand::thread_rng().gen_range(0..weights.iter().sum());
                let mut idx = 0;
                for (i, w) in weights.iter().enumerate() {
                    if n < *w {
                        idx = i;
                        break;
                    }
                    n -= w;
                }
                idx
            }
            _ => {
                let n = self.counter % members.len();
                self.counter = self.counter.wrapping_add(1);
                n
            }
        };
        if p.strategy == Strategy::Failover {
            Ok(members)
        } else {
            Ok(vec![members[index].clone()])
        }
    }
    pub fn succeeded(&mut self, profile: &str, session: Option<&str>, member: &str) {
        if let Some(s) = session {
            if self.pinned.len() >= 10000 {
                if let Some(k) = self.pinned.keys().next().cloned() {
                    self.pinned.remove(&k);
                }
            }
            self.pinned.insert(format!("{profile}:{s}"), member.into());
        }
    }
    pub fn failed(&mut self, member: &str) {
        self.cooling.insert(member.into(), Instant::now());
    }
}
struct Runtime {
    store: Store,
    key: String,
    client: reqwest::Client,
    selector: Mutex<Selector>,
    slots: Arc<Semaphore>,
    closing: AtomicBool,
    stop: CancellationToken,
}
fn error(status: StatusCode, text: &str) -> Response {
    (
        status,
        Json(json!({"error":{"code":"easy_switch_error","message":text}})),
    )
        .into_response()
}
fn allowed(headers: &HeaderMap, key: &str) -> bool {
    !headers.contains_key("origin")
        && headers.get("authorization").and_then(|h| h.to_str().ok())
            == Some(&format!("Bearer {key}"))
}
async fn health(State(r): State<Arc<Runtime>>, h: HeaderMap) -> Response {
    if !allowed(&h, &r.key) {
        return error(StatusCode::UNAUTHORIZED, "未授权");
    }
    Json(json!({"app":"EasySwitch.Runtime","version":1,"active":8-r.slots.available_permits(),"closing":r.closing.load(Ordering::SeqCst)})).into_response()
}
async fn shutdown(State(r): State<Arc<Runtime>>, h: HeaderMap) -> Response {
    if !allowed(&h, &r.key) {
        return error(StatusCode::UNAUTHORIZED, "未授权");
    }
    r.closing.store(true, Ordering::SeqCst);
    if r.slots.available_permits() != 8 {
        r.closing.store(false, Ordering::SeqCst);
        return error(StatusCode::CONFLICT, "代理请求尚未排空");
    }
    r.stop.cancel();
    StatusCode::OK.into_response()
}
pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(12))
        .timeout(Duration::from_secs(600))
        .build()?)
}
pub fn request(
    c: &reqwest::Client,
    p: &Provider,
    endpoint: &str,
) -> Result<reqwest::RequestBuilder> {
    let mut req = c
        .post(format!("{}/{}", p.base_url.trim_end_matches('/'), endpoint))
        .bearer_auth(crypto::unprotect(
            p.protected_key.as_deref().context("API Key 未配置")?,
        )?);
    for (k, v) in &p.headers {
        req = req.header(k, v);
    }
    if p.image_compatibility {
        req = req.header("x-openai-actor-authorization", "local-image-extension");
    }
    Ok(req)
}
async fn proxy(
    State(r): State<Arc<Runtime>>,
    Path(endpoint): Path<String>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    if !allowed(&headers, &r.key) {
        return error(StatusCode::UNAUTHORIZED, "未授权");
    }
    if !matches!(endpoint.as_str(), "responses" | "responses/compact") {
        return error(StatusCode::NOT_FOUND, "未知代理端点");
    }
    let permit = match r.slots.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return error(StatusCode::TOO_MANY_REQUESTS, "代理已达到并发上限"),
    };
    if r.closing.load(Ordering::SeqCst) {
        return error(StatusCode::SERVICE_UNAVAILABLE, "代理正在退出");
    }
    let result=async {
        let profiles=r.store.providers()?;let settings=r.store.settings()?;let profile=profiles.iter().find(|p|Some(&p.id)==settings.active_provider_id.as_ref()).context("没有活动代理供应商")?;ensure!(profile.runtime(),"当前模式不需要代理");
        let session=["session_id","x-codex-thread-id","conversation_id"].iter().find_map(|k|headers.get(*k).and_then(|v|v.to_str().ok())).filter(|s|s.len()<=256);
        let stateful=input.get("previous_response_id").is_some_and(|v|!v.is_null())||input.to_string().contains("\"encrypted_content\"");
        let candidates=r.selector.lock().unwrap().select(profile,&profiles,session,stateful)?;
        for (index,member) in candidates.iter().enumerate(){ensure!(member.protocol!=Protocol::ChatCompletions||endpoint!="responses/compact","Chat Completions 不支持原生 compact");let mut payload=if member.protocol==Protocol::ChatCompletions{protocol::to_chat(&input,&member.model)?}else{input.clone()};payload["model"]=json!(member.model);let endpoint=if member.protocol==Protocol::ChatCompletions{"chat/completions"}else{endpoint.as_str()};let mut req=request(&r.client,member,endpoint)?.json(&payload);for key in ["session_id","x-codex-thread-id","conversation_id"]{if let Some(v)=headers.get(key){req=req.header(key,v);}}
            let start=Instant::now();let response=req.send().await;let status=response.as_ref().map(|r|r.status().as_u16()).unwrap_or(0);route_log(&r.store,&member.id,status,start.elapsed().as_millis());
            // Transport errors can follow an accepted request: never blindly replay them.
            let response=response.context("上游连接失败；未自动重放请求")?;
            if !response.status().is_success(){r.selector.lock().unwrap().failed(&member.id);let can_retry=matches!(status,429|503)&&!stateful&&input["tools"].as_array().is_none_or(Vec::is_empty)&&index+1<candidates.len();if can_retry{continue;}return Ok(error(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY),&format!("上游 HTTP {status}；请检查供应商诊断")));}
            r.selector.lock().unwrap().succeeded(&profile.id,session,&member.id);
            if member.protocol==Protocol::Responses {let content=response.headers().get("content-type").cloned();let stream=async_stream::try_stream!{let _permit=permit;let mut upstream=response.bytes_stream();while let Some(chunk)=upstream.next().await{yield chunk.map_err(std::io::Error::other)?;}};let body=Body::from_stream(stream.map(|item: std::io::Result<_>| item));let mut out=Response::new(body);if let Some(v)=content{out.headers_mut().insert("content-type",v);}return Ok(out);}
            if input["stream"]==true {let model=member.model.clone();let stream=async_stream::try_stream!{
                let _permit=permit;let mut state=protocol::ChatStream::new(&model);for e in state.start(){yield sse(&e);}
                let mut bytes=response.bytes_stream();let mut buffer=Vec::new();let mut done=false;
                while let Some(chunk)=bytes.next().await {buffer.extend_from_slice(&chunk.map_err(std::io::Error::other)?);if buffer.len()>4_000_000{Err(std::io::Error::other("SSE 事件超过限制"))?;}
                    while let Some(pos)=buffer.iter().position(|b|*b==b'\n'){let line:Vec<u8>=buffer.drain(..=pos).collect();let line=std::str::from_utf8(&line).map_err(std::io::Error::other)?.trim();if let Some(data)=line.strip_prefix("data:"){let data=data.trim();if data=="[DONE]"{done=true;break;}let v:Value=serde_json::from_str(data).map_err(std::io::Error::other)?;for e in state.ingest(&v).map_err(std::io::Error::other)?{yield sse(&e);}}}if done{break;}
                }
                for e in state.finish().map_err(std::io::Error::other)?{yield sse(&e);}
            };let mut out=Response::new(Body::from_stream(stream.map(|item: std::io::Result<_>| item)));out.headers_mut().insert("content-type","text/event-stream".parse().unwrap());out.headers_mut().insert("cache-control","no-cache".parse().unwrap());return Ok(out);}
            let result=response.json::<Value>().await.context("上游返回无效 JSON")?;return Ok(Json(protocol::from_chat(&result,&member.model)?).into_response());
        }anyhow::bail!("没有可用路由")
    }.await;
    match result {
        Ok(r) => r,
        Err(e) => error(StatusCode::BAD_GATEWAY, &safe_error(&e)),
    }
}
pub fn safe_error(e: &anyhow::Error) -> String {
    if e.chain().any(|e| e.is::<reqwest::Error>()) {
        return "网络请求失败；请检查地址、网络与供应商诊断".into();
    }
    e.to_string()
}
fn sse(v: &Value) -> String {
    format!(
        "event: {}\ndata: {}\n\n",
        v["type"].as_str().unwrap_or("message"),
        v
    )
}
fn route_log(store: &Store, member: &str, status: u16, ms: u128) {
    let path = store.root.join("routing.jsonl");
    if fs::metadata(&path).is_ok_and(|m| m.len() > 2_000_000) {
        let _ = fs::rename(&path, store.root.join("routing.previous.jsonl"));
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = writeln!(
            file,
            "{}",
            json!({"time":chrono::Utc::now(),"member":member,"status":status,"ms":ms})
        );
    }
}
pub async fn serve(store: Store) -> Result<()> {
    let lock = fs::File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(store.root.join("runtime.lock"))?;
    lock.try_lock_exclusive().context("Runtime 已在运行")?;
    let settings = store.settings()?;
    let key = crypto::unprotect(
        settings
            .runtime_key
            .as_deref()
            .context("Runtime 凭证缺失")?,
    )?;
    let stop = CancellationToken::new();
    let state = Arc::new(Runtime {
        store,
        key,
        client: client()?,
        selector: Mutex::new(Selector::default()),
        slots: Arc::new(Semaphore::new(8)),
        closing: AtomicBool::new(false),
        stop: stop.clone(),
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/shutdown", post(shutdown))
        .route("/v1/{*endpoint}", post(proxy))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .with_state(state.clone());
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, settings.runtime_port))
            .await?;
    let watch = state.clone();
    tokio::spawn(async move {
        let started = Instant::now();
        let mut seen = false;
        loop {
            tokio::select! {_=watch.stop.cancelled()=>break,_=tokio::time::sleep(Duration::from_secs(5))=>{let running=!Desktop::writers().is_empty();seen|=running;if !running&&watch.slots.available_permits()==8&&(seen||started.elapsed()>Duration::from_secs(120)){watch.stop.cancel();break;}}}
        }
    });
    axum::serve(listener, app)
        .with_graceful_shutdown(stop.cancelled_owned())
        .await?;
    drop(lock);
    Ok(())
}
pub async fn health_check(store: &Store) -> Result<Option<Value>> {
    let s = store.settings()?;
    let Some(key) = s.runtime_key.as_deref() else {
        return Ok(None);
    };
    let req = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(4))
        .build()?
        .get(format!("http://127.0.0.1:{}/health", s.runtime_port))
        .bearer_auth(crypto::unprotect(key)?)
        .timeout(Duration::from_secs(6))
        .send()
        .await;
    match req {
        Ok(r) => {
            ensure!(r.status().is_success(), "Runtime 端口被其他服务占用");
            let v = r.json::<Value>().await?;
            ensure!(v["app"] == "EasySwitch.Runtime", "Runtime 身份不符");
            Ok(Some(v))
        }
        Err(e) if e.is_connect() => Ok(None),
        Err(_) => anyhow::bail!("Runtime 状态无法确认，请重试"),
    }
}
pub fn ensure_started_blocking(store: &Store) -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        if health_check(store).await?.is_some() {
            return Ok(());
        }
        let exe = std::env::current_exe()?;
        hidden(
            std::process::Command::new(exe)
                .arg("--runtime")
                .arg("--store")
                .arg(&store.root),
        )
        .spawn()?;
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            if health_check(store).await?.is_some() {
                return Ok(());
            }
        }
        anyhow::bail!("Runtime 未就绪；可从备份恢复回滚")
    })
}
pub fn stop_blocking(store: &Store) -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        if health_check(store).await?.is_none() {
            return Ok(());
        }
        let s = store.settings()?;
        let r = reqwest::Client::builder()
            .no_proxy()
            .build()?
            .post(format!("http://127.0.0.1:{}/shutdown", s.runtime_port))
            .bearer_auth(crypto::unprotect(s.runtime_key.as_deref().unwrap())?)
            .timeout(Duration::from_secs(3))
            .send()
            .await?;
        ensure!(r.status().is_success(), "代理仍有请求，未修改数据");
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if health_check(store).await?.is_none() {
                return Ok(());
            }
        }
        anyhow::bail!("Runtime 未退出")
    })
}
