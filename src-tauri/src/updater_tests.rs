use axum::{Router, response::IntoResponse, routing::get};
use serde_json::json;
use std::time::Duration;
use tauri_plugin_updater::UpdaterExt;
async fn endpoint(
    path: &str,
    version: &str,
) -> (
    tauri::App<tauri::test::MockRuntime>,
    String,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let manifest = json!({"version":version,"notes":"synthetic signed fixture","platforms":{"windows-x86_64":{"url":format!("{base}/{path}"),"signature":include_str!("../../tests/fixtures/update.payload.sig").trim()}}});
    let router = Router::new()
        .route(
            "/manifest",
            get(move || {
                let m = manifest.clone();
                async move { axum::Json(m) }
            }),
        )
        .route(
            "/good",
            get(|| async { include_bytes!("../../tests/fixtures/update.payload").as_slice() }),
        )
        .route("/tampered", get(|| async { "tampered bytes" }))
        .route(
            "/failed",
            get(|| async {
                (axum::http::StatusCode::SERVICE_UNAVAILABLE, "offline").into_response()
            }),
        );
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().plugins.0.insert("updater".into(),json!({"pubkey":include_str!("../../tests/fixtures/update.pub").trim(),"dangerousInsecureTransportProtocol":true,"requireSignedVersion":true}));
    let app = tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .unwrap();
    (app, base, server)
}
#[tokio::test]
async fn valid_signed_update_downloads() {
    let (app, base, server) = endpoint("good", "2.0.0").await;
    let update = app
        .updater_builder()
        .target("windows-x86_64")
        .endpoints(vec![format!("{base}/manifest").parse().unwrap()])
        .unwrap()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
        .check()
        .await
        .unwrap()
        .unwrap();
    let bytes = update.download(|_, _| {}, || {}).await.unwrap();
    assert_eq!(bytes, include_bytes!("../../tests/fixtures/update.payload"));
    server.abort();
}
#[tokio::test]
async fn tampered_update_rejected() {
    let (app, base, server) = endpoint("tampered", "2.0.0").await;
    let update = app
        .updater_builder()
        .target("windows-x86_64")
        .endpoints(vec![format!("{base}/manifest").parse().unwrap()])
        .unwrap()
        .build()
        .unwrap()
        .check()
        .await
        .unwrap()
        .unwrap();
    assert!(update.download(|_, _| {}, || {}).await.is_err());
    server.abort();
}
#[tokio::test]
async fn signed_version_mismatch_rejected() {
    let (app, base, server) = endpoint("good", "3.0.0").await;
    let update = app
        .updater_builder()
        .target("windows-x86_64")
        .endpoints(vec![format!("{base}/manifest").parse().unwrap()])
        .unwrap()
        .build()
        .unwrap()
        .check()
        .await
        .unwrap()
        .unwrap();
    assert!(update.download(|_, _| {}, || {}).await.is_err());
    server.abort();
}
#[tokio::test]
async fn failed_download_is_not_ready() {
    let (app, base, server) = endpoint("failed", "2.0.0").await;
    let update = app
        .updater_builder()
        .target("windows-x86_64")
        .endpoints(vec![format!("{base}/manifest").parse().unwrap()])
        .unwrap()
        .build()
        .unwrap()
        .check()
        .await
        .unwrap()
        .unwrap();
    assert!(update.download(|_, _| {}, || {}).await.is_err());
    server.abort();
}
#[test]
fn mutations_and_update_install_share_busy_guard() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::commands::AppState::new(
        easy_switch_core::Store::open(dir.path().to_owned()).unwrap(),
        None,
    );
    let (guard, _) = state.begin().unwrap();
    assert!(state.begin().is_err());
    drop(guard);
    assert!(state.begin().is_ok());
}
