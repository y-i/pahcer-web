use std::{fs as stdfs, os::unix::fs::PermissionsExt};

use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tempfile::tempdir;
use tokio::fs;
use tower::ServiceExt;

use pahcer_web::server::{build_app, build_state};

#[tokio::test]
async fn run_api_streams_and_persists_result() {
    let dir = tempdir().unwrap();
    let frontend = dir.path().join("dist");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::write(frontend.join("index.html"), "<html></html>")
        .await
        .unwrap();
    fs::create_dir_all(dir.path().join("tools/out"))
        .await
        .unwrap();
    fs::write(dir.path().join("tools/out/0000.txt"), "out0")
        .await
        .unwrap();

    let script_path = dir.path().join("fake-pahcer.sh");
    fs::write(
        &script_path,
        r#"#!/usr/bin/env bash
set -e
cmd="$1"
shift
if [[ "$cmd" == "run" ]]; then
  echo '{"seed":0,"score":100,"execution_time":0.12}'
  echo '{"seed":1,"score":50,"execution_time":0.34,"relative_score":90}'
  echo 'warning message' >&2
elif [[ "$cmd" == "list" ]]; then
  echo 'seed score'
  echo '0 100'
fi
"#,
    )
    .await
    .unwrap();
    stdfs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let state = build_state(dir.path().to_path_buf(), frontend, Some(script_path))
        .await
        .unwrap();
    let app = build_app(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/run")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"args":["-c","memo","--tag","nightly"]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("\"type\":\"stdout\""));
    assert!(text.contains("\"type\":\"stderr\""));
    assert!(text.contains("\"type\":\"exit\""));

    let history = state.storage.read_history().await.unwrap();
    assert_eq!(history.len(), 1);
    let result = &history[0];
    assert_eq!(result["comment"], "memo");
    assert_eq!(result["tag"], "nightly");
    assert_eq!(result["cases"], 2);
    assert!(result["avgScore"].as_f64().unwrap() > 70.0);

    let result_id = result["id"].as_str().unwrap();
    let output = fs::read_to_string(state.storage.result_dir(result_id).join("output/0000.txt"))
        .await
        .unwrap();
    assert_eq!(output, "out0");
}
