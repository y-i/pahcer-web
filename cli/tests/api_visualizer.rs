use std::{net::Ipv4Addr, sync::{Arc, Mutex}};

use axum::{
    body::Body,
    extract::{Path as AxumPath, State},
    http::{Request, StatusCode, header},
    response::IntoResponse,
    routing::get,
    Router,
};
use tempfile::tempdir;
use tokio::{fs, net::TcpListener, task::JoinHandle};
use tower::ServiceExt;

use pahcer_web::server::{build_app, build_state};

#[derive(Clone)]
struct SourceServerState {
    requests: Arc<Mutex<Vec<String>>>,
}

async fn spawn_visualizer_source() -> (String, Arc<Mutex<Vec<String>>>, JoinHandle<()>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let state = SourceServerState {
        requests: requests.clone(),
    };

    let app = Router::new()
        .route("/{*path}", get(serve_visualizer_source))
        .with_state(state);
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (format!("http://{address}"), requests, handle)
}

async fn serve_visualizer_source(
    State(state): State<SourceServerState>,
    AxumPath(path): AxumPath<String>,
) -> impl IntoResponse {
    let request_path = format!("/{path}");
    state.requests.lock().unwrap().push(request_path.clone());

    match path.as_str() {
        "index.html" => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            r#"<html><body><script src="/static/app.js"></script><a href="/child/page.js">child</a></body></html>"#,
        )
            .into_response(),
        "static/app.js" => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/javascript")],
            "console.log('app');",
        )
            .into_response(),
        "child/page.js" => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            r#"<html><body><script src="/nested/app.js"></script></body></html>"#,
        )
            .into_response(),
        "nested/app.js" => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/javascript")],
            "console.log('nested');",
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

#[tokio::test]
async fn download_visualizer_keeps_root_html_and_skips_recursive_html() {
    let dir = tempdir().unwrap();
    let frontend = dir.path().join("dist");
    fs::create_dir_all(&frontend).await.unwrap();
    fs::write(frontend.join("index.html"), "<html></html>")
        .await
        .unwrap();
    let (source_url, requests, source_handle) = spawn_visualizer_source().await;

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
                .uri("/api/visualizer/download")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "url": format!("{source_url}/index.html")
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    source_handle.abort();

    let visualizer_dir = state.storage.visualizer_dir();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        fs::read_to_string(state.storage.visualizer_path())
            .await
            .unwrap(),
        r#"<html><body><script src="/static/app.js"></script><a href="/child/page.js">child</a></body></html>"#
    );
    assert_eq!(
        fs::read_to_string(visualizer_dir.join("static/app.js"))
            .await
            .unwrap(),
        "console.log('app');"
    );
    assert!(!visualizer_dir.join("child/page.js").exists());
    assert!(!visualizer_dir.join("nested/app.js").exists());

    let requests = requests.lock().unwrap().clone();
    assert!(requests.iter().any(|path| path == "/index.html"));
    assert!(requests.iter().any(|path| path == "/static/app.js"));
    assert!(requests.iter().any(|path| path == "/child/page.js"));
    assert!(!requests.iter().any(|path| path == "/nested/app.js"));
}