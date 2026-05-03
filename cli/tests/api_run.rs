use std::{ffi::OsString, fs as stdfs, os::unix::fs::PermissionsExt, sync::OnceLock};

use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tempfile::tempdir;
use tokio::{fs, sync::Mutex as AsyncMutex};
use tower::ServiceExt;

use pahcer_web::models::{JobMetadata, JobStatus};
use pahcer_web::server::{build_app, build_state};
use pahcer_web::storage::Storage;

struct XdgConfigHomeGuard {
    previous: Option<OsString>,
}

impl XdgConfigHomeGuard {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("XDG_CONFIG_HOME");
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", path);
        }
        Self { previous }
    }
}

impl Drop for XdgConfigHomeGuard {
    fn drop(&mut self) {
        match self.previous.as_ref() {
            Some(value) => unsafe {
                std::env::set_var("XDG_CONFIG_HOME", value);
            },
            None => unsafe {
                std::env::remove_var("XDG_CONFIG_HOME");
            },
        }
    }
}

fn xdg_config_home_lock() -> &'static AsyncMutex<()> {
    static LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| AsyncMutex::new(()))
}

async fn isolate_xdg_config_home(
    path: &std::path::Path,
) -> (tokio::sync::MutexGuard<'static, ()>, XdgConfigHomeGuard) {
    let lock = xdg_config_home_lock().lock().await;
    let guard = XdgConfigHomeGuard::set(path);
    (lock, guard)
}

#[tokio::test]
async fn run_api_streams_and_materializes_result_as_symlink_by_default() {
    let dir = tempdir().unwrap();
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
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
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
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
    stamp="$(date --iso-8601=seconds)"
    printf '{"start_time":"%s","case_count":1,"total_score":200,"total_score_log10":2.3,"total_relative_score":70,"max_execution_time":0.12,"comment":"memo","tag_name":"nightly","cases":[{"seed":0,"score":200,"execution_time":0.12,"error_message":""}]}' "$stamp" > "pahcer/json/result_20260314_151401.json"
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
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
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
    stamp="$(date --iso-8601=seconds)"
    printf '{"start_time":"%s","case_count":1,"total_score":100,"total_score_log10":2,"total_relative_score":50,"max_execution_time":0.12,"comment":"memo","tag_name":"nightly","cases":[{"seed":0,"score":100,"execution_time":0.12,"error_message":""}]}' "$stamp" > "pahcer/json/result_20260314_151401.json"
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
    assert!(text.contains("\"type\":\"stderr\""));
    assert!(text.contains("\"type\":\"exit\",\"code\":1"));
    assert!(state.storage.read_history().await.unwrap().is_empty());

    let jobs = state.storage.get_jobs().await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, pahcer_web::models::JobStatus::Failed);
    assert!(!jobs[0].result.as_ref().unwrap()["error"].as_str().unwrap().is_empty());
    assert!(state.storage.read_history().await.unwrap().is_empty());
}

#[tokio::test]
async fn run_api_materializes_result_as_copy_when_configured() {
    let dir = tempdir().unwrap();
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
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
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
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

#[tokio::test]
async fn run_api_uses_request_directory_for_execution_and_result_storage() {
    let dir = tempdir().unwrap();
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
    let frontend = dir.path().join("dist");
    let contest_dir = dir.path().join("contest");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::create_dir_all(&contest_dir).await.unwrap();
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
  printf '%s' "$PWD" > request-dir.txt
  mkdir -p pahcer/json
    stamp="$(date --iso-8601=seconds)"
    printf '{"start_time":"%s","case_count":1,"total_score":123,"total_score_log10":2.0,"total_relative_score":80,"max_execution_time":0.12,"comment":"memo","tag_name":"nightly","cases":[{"seed":0,"score":123,"execution_time":0.12,"error_message":""}]}' "$stamp" > "pahcer/json/result_20260328_120000.json"
  echo '{"seed":0,"score":123,"execution_time":0.12}'
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
                .body(Body::from(
                    serde_json::json!({
                        "args": ["-c", "memo", "--tag", "nightly"],
                        "directory": contest_dir,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("\"type\":\"exit\",\"code\":0"));

    let recorded_dir = fs::read_to_string(contest_dir.join("request-dir.txt"))
        .await
        .unwrap();
    assert_eq!(recorded_dir, contest_dir.to_string_lossy());

    let contest_storage = Storage::new(contest_dir.clone()).unwrap();
    let history = contest_storage.read_history().await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].avg_score, 123.0);
    assert!(state.storage.read_history().await.unwrap().is_empty());
}

#[tokio::test]
async fn run_api_rejects_invalid_run_id_before_starting_process() {
    let dir = tempdir().unwrap();
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
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
    let app = build_app(state.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/run")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "runId": "../escape",
                        "args": ["-c", "memo", "--tag", "nightly"],
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    assert!(state.storage.get_jobs().await.unwrap().is_empty());
    assert!(state.storage.read_history().await.unwrap().is_empty());
    assert!(!dir.path().join("escape").exists());
}

#[tokio::test]
async fn run_api_rejects_reused_run_id_when_job_exists() {
    let dir = tempdir().unwrap();
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
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
printf 'started' > invoked.txt
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
        .save_job(JobMetadata {
            id: "existing-job".to_string(),
            datetime: "2026-05-03T00:00:00Z".to_string(),
            command: "run".to_string(),
            args: vec!["-c".to_string(), "memo".to_string()],
            status: JobStatus::Success,
            output_file: None,
            result: Some(serde_json::json!({ "score": 123.0 })),
        })
        .await
        .unwrap();

    let app = build_app(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/run")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "runId": "existing-job",
                        "args": ["-c", "memo", "--tag", "nightly"],
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    assert!(!dir.path().join("invoked.txt").exists());

    let jobs = state.storage.get_jobs().await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, "existing-job");
    assert_eq!(jobs[0].status, JobStatus::Success);
}

#[tokio::test]
async fn run_api_rejects_reused_run_id_when_result_dir_exists() {
    let dir = tempdir().unwrap();
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
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
printf 'started' > invoked.txt
"#,
    )
    .await
    .unwrap();
    stdfs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();

    let state = build_state(dir.path().to_path_buf(), frontend, Some(script_path))
        .await
        .unwrap();
    let result_dir = state.storage.result_dir("existing-result");
    fs::create_dir_all(&result_dir).await.unwrap();
    fs::write(result_dir.join("keep.txt"), "preserve-me")
        .await
        .unwrap();

    let app = build_app(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/run")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "runId": "existing-result",
                        "args": ["-c", "memo", "--tag", "nightly"],
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    assert!(!dir.path().join("invoked.txt").exists());
    assert_eq!(
        fs::read_to_string(state.storage.result_dir("existing-result").join("keep.txt"))
            .await
            .unwrap(),
        "preserve-me"
    );
}

#[tokio::test]
async fn run_api_cancels_active_run_and_keeps_logs_without_history_pollution() {
    let dir = tempdir().unwrap();
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
    let frontend = dir.path().join("dist");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::write(frontend.join("index.html"), "<html></html>")
        .await
        .unwrap();

    let script_path = dir.path().join("fake-pahcer-cancel.sh");
    fs::write(
        &script_path,
        r#"#!/usr/bin/env bash
set -e
cmd="$1"
shift
if [[ "$cmd" == "run" ]]; then
  trap 'exit 130' TERM INT
  mkdir -p pahcer/json
  printf '{"start_time":"2026-03-28T12:00:00+09:00","case_count":1,"total_score":321,"total_score_log10":2.5,"total_relative_score":90,"max_execution_time":0.12,"comment":"memo","tag_name":"nightly","cases":[{"seed":0,"score":321,"execution_time":0.12,"error_message":""}]}' > "pahcer/json/result_20260328_120000.json"
  echo 'before cancel stdout'
  echo 'before cancel stderr' >&2
    printf 'ready' > cancel-ready.txt
  while true; do
    sleep 1
  done
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
    let run_id = "run-cancel-1";

    let run_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/run")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "runId": run_id,
                        "args": ["-c", "memo", "--tag", "nightly"],
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(run_response.status(), 200);

    let ready_path = dir.path().join("cancel-ready.txt");
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if ready_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();

    let cancel_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/run/{run_id}/cancel"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(cancel_response.status(), 200);

    let body = run_response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("before cancel stdout"));
    assert!(text.contains("before cancel stderr"));
    assert!(text.contains("\"type\":\"exit\""));
    assert!(text.contains("\"reason\":\"canceled\""));

    let jobs = state.storage.get_jobs().await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, run_id);
    assert_eq!(jobs[0].status, pahcer_web::models::JobStatus::Canceled);
    let job_result = jobs[0].result.as_ref().unwrap();
    assert_eq!(job_result["terminationReason"], "canceled");
    assert!(job_result["logs"].as_str().unwrap().contains("before cancel stdout"));
    assert!(job_result["logs"].as_str().unwrap().contains("before cancel stderr"));

    assert!(!state.storage.result_dir(run_id).exists());
    assert!(state.storage.read_history().await.unwrap().is_empty());
}

#[tokio::test]
async fn cancel_api_returns_not_found_for_unknown_run_id() {
    let dir = tempdir().unwrap();
    let (_lock, _guard) = isolate_xdg_config_home(dir.path()).await;
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
    let app = build_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/run/missing-run/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}
