use std::{fs as stdfs, os::unix::fs::PermissionsExt};

use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tempfile::tempdir;
use tokio::fs;
use tower::ServiceExt;

use pahcer_web::server::{build_app, build_state};

#[tokio::test]
async fn run_api_streams_and_materializes_result_as_symlink_by_default() {
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
    mkdir -p pahcer/json
        stamp="$(date --iso-8601=seconds)"
        file_name="result_$(date +%Y%m%d_%H%M%S).json"
        printf '{"start_time":"%s","case_count":2,"total_score":150,"total_score_log10":3.6989700043360187,"total_relative_score":90,"max_execution_time":0.34,"comment":"memo","tag_name":"nightly","cases":[{"seed":0,"score":100,"execution_time":0.12,"error_message":""},{"seed":1,"score":50,"execution_time":0.34,"relative_score":90,"error_message":""}]}' "$stamp" > "pahcer/json/$file_name"
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
    assert_eq!(result.comment, "memo");
    assert_eq!(result.tag, "nightly");
    assert_eq!(result.cases, 2);
    assert_eq!(result.avg_score, 75.0);
    let result_path = state.storage.result_path(&result.id);
    let metadata = stdfs::symlink_metadata(&result_path).unwrap();
    assert!(metadata.file_type().is_symlink());
    let link_target = stdfs::read_link(&result_path).unwrap();
    assert_eq!(link_target.to_string_lossy(), "result.snapshot.json");
    assert!(state.storage.additional_path(&result.id).exists());

    let additional: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(state.storage.additional_path(&result.id))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(additional.get("startTime").is_none());
    assert!(additional.get("comment").is_none());

    let output = fs::read_to_string(state.storage.result_dir(&result.id).join("output/0000.txt"))
        .await
        .unwrap();
    assert_eq!(output, "out0");

    let additional_path = state.storage.additional_path(&result.id);
    let additional: serde_json::Value = serde_json::from_str(&fs::read_to_string(&additional_path).await.unwrap()).unwrap();
    let result_file_name = additional["resultFileName"].as_str().unwrap();
    fs::write(
        state.storage.pahcer_result_path(result_file_name),
        serde_json::json!({
            "start_time":"2026-03-14T15:14:01+09:00",
            "case_count":1,
            "total_score":999,
            "total_score_log10":1,
            "total_relative_score":100,
            "max_execution_time":0.1,
            "comment":"mutated",
            "tag_name":"nightly",
            "cases":[{"seed":0,"score":999,"execution_time":0.1,"error_message":""}]
        })
        .to_string(),
    )
    .await
    .unwrap();
    let materialized = fs::read_to_string(&result_path).await.unwrap();
    assert!(materialized.contains("\"comment\":\"memo\""));
}

#[tokio::test]
async fn run_api_detects_overwritten_same_name_result_file() {
    let dir = tempdir().unwrap();
    let frontend = dir.path().join("dist");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::write(frontend.join("index.html"), "<html></html>")
        .await
        .unwrap();
    fs::create_dir_all(dir.path().join("pahcer/json")).await.unwrap();
    fs::write(
        dir.path().join("pahcer/json/result_20260314_151401.json"),
        serde_json::json!({
            "start_time":"2026-03-14T15:14:00+09:00",
            "case_count":1,
            "total_score":1,
            "total_score_log10":0.0,
            "total_relative_score":1,
            "max_execution_time":0.1,
            "comment":"old",
            "tag_name":"nightly",
            "cases":[{"seed":0,"score":1,"execution_time":0.1,"error_message":""}]
        })
        .to_string(),
    )
    .await
    .unwrap();

    let script_path = dir.path().join("fake-pahcer-overwrite.sh");
    fs::write(
        &script_path,
        r#"#!/usr/bin/env bash
set -e
cmd="$1"
shift
if [[ "$cmd" == "run" ]]; then
  mkdir -p pahcer/json
  printf '{"start_time":"2026-03-14T15:14:01+09:00","case_count":1,"total_score":200,"total_score_log10":2.3,"total_relative_score":70,"max_execution_time":0.12,"comment":"memo","tag_name":"nightly","cases":[{"seed":0,"score":200,"execution_time":0.12,"error_message":""}]}' > "pahcer/json/result_20260314_151401.json"
  echo '{"seed":0,"score":200,"execution_time":0.12}'
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
    let _ = response.into_body().collect().await.unwrap();

    let history = state.storage.read_history().await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].comment, "memo");
    assert_eq!(history[0].avg_score, 200.0);
}

#[tokio::test]
async fn run_api_cleans_up_result_dir_and_marks_job_failed_when_output_copy_fails() {
    let dir = tempdir().unwrap();
    let frontend = dir.path().join("dist");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::write(frontend.join("index.html"), "<html></html>")
        .await
        .unwrap();
    fs::create_dir_all(dir.path().join("tools/out/0000.txt"))
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
  mkdir -p pahcer/json
  printf '{"start_time":"2026-03-14T15:14:01+09:00","case_count":1,"total_score":100,"total_score_log10":2,"total_relative_score":50,"max_execution_time":0.12,"comment":"memo","tag_name":"nightly","cases":[{"seed":0,"score":100,"execution_time":0.12,"error_message":""}]}' > "pahcer/json/result_20260314_151401.json"
  echo '{"seed":0,"score":100,"execution_time":0.12}'
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
    assert!(text.contains("Is a directory") || text.contains("directory"));
    assert!(state.storage.read_history().await.unwrap().is_empty());

    let jobs = state.storage.get_jobs().await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, pahcer_web::models::JobStatus::Failed);
    assert!(jobs[0].result.as_ref().unwrap()["error"].as_str().unwrap().contains("directory"));
    assert!(state.storage.read_history().await.unwrap().is_empty());
}

#[tokio::test]
async fn run_api_materializes_result_as_copy_when_configured() {
    let dir = tempdir().unwrap();
    let frontend = dir.path().join("dist");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::write(frontend.join("index.html"), "<html></html>")
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
    mkdir -p pahcer/json
        stamp="$(date --iso-8601=seconds)"
        file_name="result_$(date +%Y%m%d_%H%M%S).json"
        printf '{"start_time":"%s","case_count":1,"total_score":100,"total_score_log10":2,"total_relative_score":50,"max_execution_time":0.12,"comment":"memo","tag_name":"nightly","cases":[{"seed":0,"score":100,"execution_time":0.12,"error_message":""}]}' "$stamp" > "pahcer/json/$file_name"
  echo '{"seed":0,"score":100,"execution_time":0.12}'
fi
"#,
    )
    .await
    .unwrap();
    stdfs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let state = build_state(dir.path().to_path_buf(), frontend, Some(script_path))
        .await
        .unwrap();
    state
        .storage
        .save_global_config(
            serde_json::from_value(serde_json::json!({
                "resultJsonMode": "copy"
            }))
            .unwrap(),
        )
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
    let _ = response.into_body().collect().await.unwrap();

    let history = state.storage.read_history().await.unwrap();
    assert_eq!(history.len(), 1);
    let result_path = state.storage.result_path(&history[0].id);
    let metadata = stdfs::symlink_metadata(&result_path).unwrap();
    assert!(!metadata.file_type().is_symlink());

    let additional: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(state.storage.additional_path(&history[0].id))
            .await
            .unwrap(),
    )
    .unwrap();
    let result_file_name = additional["resultFileName"].as_str().unwrap();
    let result_body = fs::read_to_string(&result_path).await.unwrap();
    let source_body = fs::read_to_string(
        state.storage.pahcer_result_path(result_file_name),
    )
    .await
    .unwrap();
    assert_eq!(result_body, source_body);
    assert!(result_body.contains("\"comment\":\"memo\""));
}

#[tokio::test]
async fn run_api_fails_when_no_new_result_json_is_created() {
    let dir = tempdir().unwrap();
    let frontend = dir.path().join("dist");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::write(frontend.join("index.html"), "<html></html>")
        .await
        .unwrap();
    fs::create_dir_all(dir.path().join("pahcer/json")).await.unwrap();
    fs::write(
        dir.path().join("pahcer/json/result_20260320_120000.json"),
        serde_json::json!({
            "start_time":"2026-03-20T12:00:00+09:00",
            "case_count":1,
            "total_score":1,
            "total_score_log10":0,
            "total_relative_score":0,
            "max_execution_time":0.1,
            "comment":"old",
            "tag_name":"old",
            "cases":[{"seed":0,"score":1,"execution_time":0.1,"error_message":""}]
        })
        .to_string(),
    )
    .await
    .unwrap();

    let script_path = dir.path().join("fake-pahcer-no-json.sh");
    fs::write(
        &script_path,
        r#"#!/usr/bin/env bash
set -e
cmd="$1"
shift
if [[ "$cmd" == "run" ]]; then
  echo '{"seed":0,"score":100,"execution_time":0.12}'
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
    assert!(text.contains("pahcer result json was not found after run"));
    assert!(text.contains("\"type\":\"exit\",\"code\":1"));
    assert!(state.storage.read_history().await.unwrap().is_empty());
}
