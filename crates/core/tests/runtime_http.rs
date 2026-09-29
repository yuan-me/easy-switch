use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use easy_switch_core::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test]
async fn model_discovery_bounds_streams_and_cancels_after_headers() {
    use axum::routing::get;
    use tokio_util::sync::CancellationToken;
    let stalled = Arc::new(tokio::sync::Notify::new());
    let signal = stalled.clone();
    let app = Router::new()
        .route(
            "/ok/models",
            get(|| async { Json(json!({"data":[{"id":"b"},{"id":"a"},{"id":"a"}]})) }),
        )
        .route(
            "/failed/models",
            get(|| async { StatusCode::SERVICE_UNAVAILABLE }),
        )
        .route(
            "/large/models",
            get(|| async {
                Body::from_stream(futures_util::stream::iter(
                    (0..80).map(|_| Ok::<_, std::io::Error>(vec![b' '; 65536])),
                ))
            }),
        )
        .route(
            "/slow/models",
            get(move || {
                let signal = signal.clone();
                async move {
                    Body::from_stream(async_stream::stream! {
                        yield Ok::<_,std::io::Error>("{\"data\":[");
                        signal.notify_one();
                        std::future::pending::<()>().await;
                    })
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(axum::serve(listener, app).into_future());
    let provider = |route: &str| Provider {
        base_url: format!("http://{address}/{route}"),
        protected_key: Some(crypto::protect("synthetic-only").unwrap()),
        ..Default::default()
    };
    let ct = CancellationToken::new();
    assert_eq!(
        diagnostics::models(&provider("ok"), &ct).await.unwrap(),
        vec!["a", "b"]
    );
    assert!(
        diagnostics::models(&provider("failed"), &ct)
            .await
            .unwrap_err()
            .to_string()
            .contains("503")
    );
    assert!(
        diagnostics::models(&provider("large"), &ct)
            .await
            .unwrap_err()
            .to_string()
            .contains("过大")
    );
    let slow = provider("slow");
    let task_ct = ct.clone();
    let task = tokio::spawn(async move { diagnostics::models(&slow, &task_ct).await });
    tokio::time::timeout(Duration::from_secs(2), stalled.notified())
        .await
        .unwrap();
    ct.cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(result.unwrap_err().to_string().contains("取消"));
    server.abort();
}

#[derive(Clone, Default)]
struct Upstream {
    calls: Arc<Mutex<Vec<Value>>>,
}
async fn respond(
    State(s): State<Upstream>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> axum::response::Response {
    assert_eq!(
        headers.get("authorization").unwrap(),
        "Bearer synthetic-upstream-key"
    );
    assert_eq!(
        headers.get("x-openai-actor-authorization").unwrap(),
        "local-image-extension"
    );
    s.calls.lock().unwrap().push(input.clone());
    if input["input"] == "fail" {
        return (StatusCode::SERVICE_UNAVAILABLE, "synthetic").into_response();
    }
    if input["stream"] == true {
        let stream = async_stream::stream! {yield Ok::<_,std::io::Error>("data: {\"type\":\"response.created\"}\n\n");tokio::time::sleep(Duration::from_millis(700)).await;yield Ok("data: {\"type\":\"response.completed\"}\n\n");};
        return (
            [("content-type", "text/event-stream")],
            Body::from_stream(stream),
        )
            .into_response();
    }
    Json(json!({"id":"response-test","output":[{"type":"image_generation_call","result":"synthetic-image-base64"}],"unknown":{"keep":true}})).into_response()
}

#[tokio::test]
async fn runtime_forwards_native_tools_and_protects_inflight_requests() {
    let upstream = Upstream::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(
        axum::serve(
            listener,
            Router::new()
                .route("/v1/responses", post(respond))
                .route("/v1/responses/compact", post(respond))
                .with_state(upstream.clone()),
        )
        .into_future(),
    );
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().to_owned()).unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let key = "synthetic-local-only";
    let member = Provider {
        id: "upstream".into(),
        name: "upstream".into(),
        base_url: format!("http://{address}/v1"),
        model: "synthetic-model".into(),
        image_compatibility: true,
        protected_key: Some(crypto::protect("synthetic-upstream-key").unwrap()),
        ..Default::default()
    };
    let aggregate = Provider {
        id: "aggregate".into(),
        name: "aggregate".into(),
        mode: AuthMode::Aggregate,
        members: vec![Member {
            provider_id: member.id.clone(),
            weight: 1,
            enabled: true,
        }],
        ..Default::default()
    };
    store
        .save("providers.json", &vec![member, aggregate])
        .unwrap();
    store
        .save(
            "settings.json",
            &Settings {
                runtime_port: port,
                runtime_key: Some(crypto::protect(key).unwrap()),
                active_provider_id: Some("aggregate".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let local = store.clone();
    let runtime = tokio::spawn(async move { runtime::serve(local).await });
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}");
    for _ in 0..100 {
        if client
            .get(format!("{url}/health"))
            .bearer_auth(key)
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let request =
        json!({"input":"image","tools":[{"type":"image_generation"}],"unknown":"preserve"});
    assert_eq!(
        client
            .post(format!("{url}/v1/responses"))
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(format!("{url}/v1/responses"))
            .bearer_auth(key)
            .header("origin", "https://example.com")
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response: Value = client
        .post(format!("{url}/v1/responses"))
        .bearer_auth(key)
        .json(&request)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(response["output"][0]["result"], "synthetic-image-base64");
    assert_eq!(response["unknown"]["keep"], true);
    assert_eq!(upstream.calls.lock().unwrap()[0]["tools"], request["tools"]);
    assert_eq!(upstream.calls.lock().unwrap()[0]["unknown"], "preserve");
    let response = client
        .post(format!("{url}/v1/responses"))
        .bearer_auth(key)
        .json(&json!({"input":"stream","stream":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        client
            .post(format!("{url}/shutdown"))
            .bearer_auth(key)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    assert!(
        response
            .text()
            .await
            .unwrap()
            .contains("response.completed")
    );
    let compact = client
        .post(format!("{url}/v1/responses/compact"))
        .bearer_auth(key)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(compact.status(), 200);
    let fail = client
        .post(format!("{url}/v1/responses"))
        .bearer_auth(key)
        .json(&json!({"input":"fail","tools":[{"type":"image_generation"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(fail.status(), 503);
    assert_eq!(upstream.calls.lock().unwrap().len(), 4);
    let log = std::fs::read_to_string(store.root.join("routing.jsonl")).unwrap();
    assert!(!log.contains("synthetic-upstream-key") && !log.contains("synthetic-image"));
    assert_eq!(
        client
            .post(format!("{url}/shutdown"))
            .bearer_auth(key)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    tokio::time::timeout(Duration::from_secs(3), runtime)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    server.abort();
}
