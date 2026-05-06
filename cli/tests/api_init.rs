use std::{fs as stdfs, os::unix::fs::PermissionsExt, path::Path};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tempfile::tempdir;
use tokio::fs;
use tower::ServiceExt;

use pahcer_web::server::{build_app, build_state};

#[tokio::test]
async fn config_api_reports_uninitialized_and_invalid_states() {
    let dir = tempdir().unwrap();
    let app = build_test_app(dir.path(), dir.path().join("fake-pahcer")).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = read_json(response).await;
    assert_eq!(json["initializationState"], "uninitialized");
    assert!(json["problemName"].is_null());

    fs::write(dir.path().join("pahcer_config.toml"), "[problem\n")
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = read_json(response).await;
    assert_eq!(json["initializationState"], "invalid");
    assert!(!json["initializationError"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn config_api_marks_minimal_parseable_config_as_initialized() {
    let dir = tempdir().unwrap();
    let app = build_test_app(dir.path(), dir.path().join("fake-pahcer")).await;

    fs::write(
        dir.path().join("pahcer_config.toml"),
        r#"[general]
version = "0.3.1"

[problem]
problem_name = "ahc999"
"#,
    )
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = read_json(response).await;
    assert_eq!(json["initializationState"], "initialized");
    assert_eq!(json["problemName"], "ahc999");
    assert!(json["initializationError"].is_null());
}

#[tokio::test]
async fn init_api_runs_pahcer_init_with_short_flags_and_returns_latest_config() {
    let dir = tempdir().unwrap();
    let script_path = dir.path().join("fake-pahcer.sh");
    write_executable(
        &script_path,
        r#"#!/usr/bin/env bash
set -euo pipefail
cmd="$1"
shift
if [[ "$cmd" != "init" ]]; then
  echo "unexpected command: $cmd" >&2
  exit 1
fi

expected=("-p" "ahc999" "-o" "max" "-l" "rust" "-i")
if [[ "$#" -ne "${#expected[@]}" ]]; then
  echo "unexpected arg count: $#" >&2
  exit 64
fi

index=0
for arg in "$@"; do
  if [[ "$arg" != "${expected[$index]}" ]]; then
    echo "unexpected option: $arg" >&2
    exit 64
  fi
  index=$((index + 1))
done

printf '%s\n' "$cmd" "$@" > init-args.txt
cat > pahcer_config.toml <<EOF
[general]
version = "0.3.1"

[problem]
problem_name = "ahc999"
objective = "Max"
EOF
"#,
    )
    .await;

    let app = build_test_app(dir.path(), script_path).await;

    let response = app
        .clone()
        .oneshot(init_request(
            r#"{"problem":"ahc999","objective":"max","language":"rust","interactive":true}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = read_json(response).await;
    assert_eq!(json["initializationState"], "initialized");
    assert_eq!(json["problemName"], "ahc999");

    let init_args = fs::read_to_string(dir.path().join("init-args.txt"))
        .await
        .unwrap();
    assert_eq!(
        init_args.lines().collect::<Vec<_>>(),
        vec!["init", "-p", "ahc999", "-o", "max", "-l", "rust", "-i"]
    );
}

#[tokio::test]
async fn init_api_rejects_empty_problem() {
    let dir = tempdir().unwrap();
    let app = build_test_app(dir.path(), dir.path().join("fake-pahcer")).await;

    let response = app
        .oneshot(init_request(
            r#"{"problem":"   ","objective":"max","language":"rust","interactive":false}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = read_json(response).await;
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("Problem name is required")
    );
}

#[tokio::test]
async fn init_api_rejects_already_initialized_directory() {
    let dir = tempdir().unwrap();
    let app = build_test_app(dir.path(), dir.path().join("fake-pahcer")).await;

    fs::write(
        dir.path().join("pahcer_config.toml"),
        valid_config("ahc001"),
    )
    .await
    .unwrap();

    let response = app
        .oneshot(init_request(
            r#"{"problem":"ahc999","objective":"max","language":"rust","interactive":false}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let json = read_json(response).await;
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("already initialized")
    );
}

#[tokio::test]
async fn init_api_treats_minimal_existing_config_as_initialized() {
    let dir = tempdir().unwrap();
    let app = build_test_app(dir.path(), dir.path().join("fake-pahcer")).await;

    fs::write(
        dir.path().join("pahcer_config.toml"),
        "[general]\nversion = \"0.3.1\"\n\n[problem]\nproblem_name = \"ahc999\"\n",
    )
    .await
    .unwrap();

    let response = app
        .oneshot(init_request(
            r#"{"problem":"ahc999","objective":"max","language":"rust","interactive":false}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let json = read_json(response).await;
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("already initialized")
    );
}

#[tokio::test]
async fn init_api_rejects_invalid_existing_config() {
    let dir = tempdir().unwrap();
    let app = build_test_app(dir.path(), dir.path().join("fake-pahcer")).await;

    fs::write(dir.path().join("pahcer_config.toml"), "[problem\n")
        .await
        .unwrap();

    let response = app
        .oneshot(init_request(
            r#"{"problem":"ahc999","objective":"max","language":"rust","interactive":false}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let json = read_json(response).await;
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("invalid pahcer_config.toml")
    );
}

#[tokio::test]
async fn init_api_returns_command_failure_when_pahcer_exits_non_zero() {
    let dir = tempdir().unwrap();
    let script_path = dir.path().join("fake-pahcer.sh");
    write_executable(
        &script_path,
        r#"#!/usr/bin/env bash
echo "init failed" >&2
exit 7
"#,
    )
    .await;

    let app = build_test_app(dir.path(), script_path).await;
    let response = app
        .oneshot(init_request(
            r#"{"problem":"ahc999","objective":"max","language":"rust","interactive":false}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = read_json(response).await;
    assert!(json["error"].as_str().unwrap().contains("init failed"));
}

#[tokio::test]
async fn init_api_fails_when_pahcer_succeeds_without_creating_config() {
    let dir = tempdir().unwrap();
    let script_path = dir.path().join("fake-pahcer.sh");
    write_executable(
        &script_path,
        r#"#!/usr/bin/env bash
echo "init completed"
exit 0
"#,
    )
    .await;

    let app = build_test_app(dir.path(), script_path).await;
    let response = app
        .oneshot(init_request(
            r#"{"problem":"ahc999","objective":"max","language":"rust","interactive":false}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = read_json(response).await;
    assert!(json["error"].as_str().unwrap().contains("was not created"));
}

#[tokio::test]
async fn init_api_fails_when_pahcer_succeeds_with_invalid_config() {
    let dir = tempdir().unwrap();
    let script_path = dir.path().join("fake-pahcer.sh");
    write_executable(
        &script_path,
        r#"#!/usr/bin/env bash
cat > pahcer_config.toml <<EOF
[problem
EOF
exit 0
"#,
    )
    .await;

    let app = build_test_app(dir.path(), script_path).await;
    let response = app
        .oneshot(init_request(
            r#"{"problem":"ahc999","objective":"max","language":"rust","interactive":false}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = read_json(response).await;
    assert!(json["error"].as_str().unwrap().contains("is invalid"));
}

#[tokio::test]
async fn init_api_allows_only_one_concurrent_initialization() {
    let dir = tempdir().unwrap();
    let script_path = dir.path().join("fake-pahcer.sh");
    write_executable(
        &script_path,
        r#"#!/usr/bin/env bash
set -euo pipefail
sleep 0.2
cat > pahcer_config.toml <<EOF
[general]
version = "0.3.1"

[problem]
problem_name = "ahc777"
objective = "Max"
EOF
"#,
    )
    .await;

    let app = build_test_app(dir.path(), script_path).await;
    let request = r#"{"problem":"ahc777","objective":"max","language":"rust","interactive":false}"#;

    let first = app.clone().oneshot(init_request(request));
    let second = app.clone().oneshot(init_request(request));
    let (first, second) = tokio::join!(first, second);

    let first = first.unwrap();
    let second = second.unwrap();
    let statuses = [first.status(), second.status()];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::CONFLICT));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = read_json(response).await;
    assert_eq!(json["initializationState"], "initialized");
    assert_eq!(json["problemName"], "ahc777");
}

async fn build_test_app(base_dir: &Path, pahcer_program: std::path::PathBuf) -> axum::Router {
    let frontend = base_dir.join("dist");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::write(frontend.join("index.html"), "<html></html>")
        .await
        .unwrap();

    let state = build_state(base_dir.to_path_buf(), frontend, Some(pahcer_program))
        .await
        .unwrap();
    build_app(state)
}

fn init_request(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/init")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn read_json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn write_executable(path: &Path, script: &str) {
    fs::write(path, script).await.unwrap();
    stdfs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn valid_config(problem_name: &str) -> String {
    format!(
        "[general]\nversion = \"0.3.1\"\n\n[problem]\nproblem_name = \"{problem_name}\"\nobjective = \"Max\"\n"
    )
}
