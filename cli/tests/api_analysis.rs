use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tempfile::tempdir;
use tokio::fs;
use tower::ServiceExt;

use pahcer_web::server::{build_app, build_state};

#[tokio::test]
async fn analysis_csv_endpoints_match_expected_shape() {
    let dir = tempdir().unwrap();
    let frontend = dir.path().join("dist");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::write(frontend.join("index.html"), "<html></html>")
        .await
        .unwrap();
    fs::create_dir_all(dir.path().join("tools/in"))
        .await
        .unwrap();
    fs::write(dir.path().join("tools/seeds.txt"), "0\n1\n")
        .await
        .unwrap();
    fs::write(dir.path().join("tools/in/0000.txt"), "10 20\n")
        .await
        .unwrap();
    fs::write(dir.path().join("tools/in/0001.txt"), "30 40\n")
        .await
        .unwrap();

    let state = build_state(
        dir.path().to_path_buf(),
        frontend,
        Some(dir.path().join("fake-pahcer")),
    )
    .await
    .unwrap();
    state
        .storage
        .save_local_config(
            serde_json::from_value(serde_json::json!({
                "inputParamNames": "X,Y",
                "visualizerUrl": "https://example.com/vis.html"
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    fs::create_dir_all(state.storage.result_dir("200"))
        .await
        .unwrap();
    fs::write(
        state.storage.additional_path("200"),
        serde_json::json!({
            "id": "200",
            "args": ["--tag", "run1"],
            "resultFileName": "result_20260314_151401.json",
            "avgScore": 16.5,
            "avgLogScore": 1.2,
            "avgRelativeScore": 50
        })
        .to_string(),
    )
    .await
    .unwrap();
    fs::write(
        state.storage.result_path("200"),
        serde_json::json!({
            "start_time": "2026-03-14T15:14:01+09:00",
            "case_count": 2,
            "total_score": 33,
            "total_score_log10": 2.4,
            "total_relative_score": 100,
            "max_execution_time": 0.2,
            "comment": "memo",
            "tag_name": "run1",
            "cases": [
                { "seed": 0, "score": 11 },
                { "seed": 1, "score": 22 }
            ]
        })
        .to_string(),
    )
    .await
    .unwrap();

    let app = build_app(state.clone());
    let input_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/analysis/ahc/input.csv")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let input_text = String::from_utf8(
        input_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(input_text.contains("file,seed,X,Y"));
    assert!(input_text.contains("0001.txt,1,30,40"));

    let result_response = app
        .oneshot(
            Request::builder()
                .uri("/analysis/ahc/result.csv")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let result_text = String::from_utf8(
        result_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(result_text.starts_with("raw,1000000000,https://example.com/vis.html"));
    assert!(result_text.contains("run1,11,22"));
}
