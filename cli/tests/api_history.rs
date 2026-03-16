use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tempfile::tempdir;
use tokio::fs;
use tower::ServiceExt;

use pahcer_web::server::{build_app, build_state};

#[tokio::test]
async fn history_delete_removes_saved_result() {
    let dir = tempdir().unwrap();
    let frontend = dir.path().join("dist");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::write(frontend.join("index.html"), "<html></html>")
        .await
        .unwrap();
    let state = build_state(
        dir.path().to_path_buf(),
        frontend,
        Some(dir.path().join("fake-pahcer")),
    )
    .await
    .unwrap();

    let result_dir = state.storage.result_dir("123");
    fs::create_dir_all(result_dir.join("output")).await.unwrap();
    fs::write(
        state.storage.result_path("123"),
        serde_json::json!({"id":"123","datetime":"2026-03-14T00:00:00Z"}).to_string(),
    )
    .await
    .unwrap();

    let app = build_app(state.clone());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/history")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/history/123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(!state.storage.result_dir("123").exists());
}
