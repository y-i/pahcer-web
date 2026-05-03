use std::fs as stdfs;

use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tempfile::tempdir;
use tokio::fs;
use tower::ServiceExt;

use pahcer_web::models::ResultJsonMode;
use pahcer_web::server::{build_app, build_state};

#[tokio::test]
async fn history_reads_materialized_result_and_delete_keeps_pahcer_json() {
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
    fs::create_dir_all(state.storage.pahcer_json_dir())
        .await
        .unwrap();
    fs::write(
        state.storage.additional_path("123"),
        serde_json::json!({
            "id":"123",
            "args":["--comment","memo"],
            "resultFileName":"result_20260314_151401.json",
            "avgScore":50,
            "avgLogScore":1.5,
            "avgRelativeScore":75
        })
        .to_string(),
    )
    .await
    .unwrap();
    fs::write(
        state
            .storage
            .pahcer_result_path("result_20260314_151401.json"),
        serde_json::json!({
            "start_time":"2026-03-14T15:14:01+09:00",
            "case_count":2,
            "total_score":100,
            "total_score_log10":3,
            "total_relative_score":150,
            "max_execution_time":0.2,
            "comment":"memo",
            "tag_name":"nightly",
            "cases":[
                {"seed":0,"score":40,"execution_time":0.1,"error_message":""},
                {"seed":1,"score":60,"execution_time":0.2,"error_message":""}
            ]
        })
        .to_string(),
    )
    .await
    .unwrap();
    state
        .storage
        .materialize_result_json(
            "123",
            "result_20260314_151401.json",
            ResultJsonMode::Symlink,
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
    let history = json.as_array().unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["id"], "123");
    assert_eq!(history[0]["tag"], "nightly");
    assert_eq!(history[0]["comment"], "memo");

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
    assert!(
        state
            .storage
            .pahcer_result_path("result_20260314_151401.json")
            .exists()
    );
    assert!(
        stdfs::metadata(
            state
                .storage
                .pahcer_result_path("result_20260314_151401.json")
        )
        .unwrap()
        .is_file()
    );
}
