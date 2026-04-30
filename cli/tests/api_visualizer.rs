use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use tempfile::tempdir;
use tokio::fs;
use tower::ServiceExt;

use pahcer_web::server::{build_app, build_state};

#[tokio::test]
async fn download_visualizer_rejects_excluded_url_without_removing_existing_files() {
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

    let visualizer_dir = state.storage.visualizer_dir();
    fs::create_dir_all(visualizer_dir.join("assets"))
        .await
        .unwrap();
    fs::write(state.storage.visualizer_path(), "<html>saved visualizer</html>")
        .await
        .unwrap();
    fs::write(
        visualizer_dir.join("assets/app.js"),
        "console.log('saved asset');",
    )
    .await
    .unwrap();

    let app = build_app(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/visualizer/download")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "url": "https://atcoder.jp/contests/abc/tasks/abc_a"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        fs::read_to_string(state.storage.visualizer_path())
            .await
            .unwrap(),
        "<html>saved visualizer</html>"
    );
    assert_eq!(
        fs::read_to_string(visualizer_dir.join("assets/app.js"))
            .await
            .unwrap(),
        "console.log('saved asset');"
    );
}