use std::{
    collections::HashMap,
    convert::Infallible,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path as AxumPath, State},
    http::{HeaderValue, Response, StatusCode, header},
    routing::{delete, get, post},
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use mime_guess::from_path;
use serde_json::{Value, json};
use tokio::{
    fs,
    net::TcpListener,
    process::Command,
    sync::{Mutex, mpsc},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::ReaderStream;
use tower_http::services::{ServeDir, ServeFile};

use crate::{
    analysis::{download_analysis_html, generate_input_csv, generate_result_csv},
    error::AppError,
    models::{
        AdditionalResultMetadata, ConfigResponse, InitRequest, InitializationState, JobMetadata,
        JobStatus, LocalConfig, RunRequest, StoredResult, StreamMessage,
    },
    pahcer::{average_from_total, ensure_json_flag, extract_comment_tag, get_pahcer_list},
    problem_config::{ProblemConfigState, inspect_problem_config},
    storage::{PahcerResultFileState, Storage},
    visualizer::{download_recursive, inject_output_loader},
};

#[derive(Debug, Clone)]
pub struct AppState {
    pub base_dir: PathBuf,
    pub storage: Storage,
    pub client: reqwest::Client,
    pub frontend_dist_dir: PathBuf,
    pub pahcer_program: PathBuf,
    pub init_lock: Arc<Mutex<()>>,
    pub run_lock: Arc<Mutex<()>>,
}

#[derive(Debug, Clone)]
pub struct UiServerOptions {
    pub base_dir: PathBuf,
    pub port: u16,
    pub build_frontend: bool,
    pub frontend_dir: Option<PathBuf>,
    pub pahcer_program: Option<PathBuf>,
}

pub async fn start_ui_server(options: UiServerOptions) -> Result<(), AppError> {
    let base_dir = options.base_dir.canonicalize().unwrap_or(options.base_dir);
    let frontend_dir = options.frontend_dir.unwrap_or_else(default_frontend_dir);

    if options.build_frontend {
        build_frontend(&frontend_dir).await?;
    }

    let state = build_state(base_dir, frontend_dir.join("dist"), options.pahcer_program).await?;
    let app = build_app(state.clone());
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, options.port)).await?;
    println!("Starting server on http://localhost:{}", options.port);
    axum::serve(listener, app)
        .await
        .map_err(|error| AppError::Internal(error.to_string()))
}

pub async fn build_state(
    base_dir: PathBuf,
    frontend_dist_dir: PathBuf,
    pahcer_program: Option<PathBuf>,
) -> Result<AppState, AppError> {
    let storage = Storage::new(base_dir.clone())?;

    Ok(AppState {
        base_dir,
        storage,
        client: reqwest::Client::new(),
        frontend_dist_dir,
        pahcer_program: pahcer_program
            .or_else(|| std::env::var_os("PAHCER_WEB_PAHCER_BIN").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("pahcer")),
        init_lock: Arc::new(Mutex::new(())),
        run_lock: Arc::new(Mutex::new(())),
    })
}

pub fn build_app(state: AppState) -> Router {
    let dist_dir = state.frontend_dist_dir.clone();
    let fallback =
        ServeDir::new(&dist_dir).not_found_service(ServeFile::new(dist_dir.join("index.html")));

    Router::new()
        .route("/api/config", get(get_config))
        .route("/api/init", post(run_init))
        .route("/api/config/global", post(save_global_config))
        .route("/api/config/local", post(save_local_config))
        .route("/api/jobs", get(get_jobs))
        .route("/api/history", get(get_history))
        .route("/api/history/{timestamp}", delete(delete_history))
        .route(
            "/api/history/{timestamp}/output/{filename}",
            get(get_history_output),
        )
        .route("/api/run", post(run_pahcer))
        .route("/api/list", get(list_pahcer))
        .route("/api/visualizer/status", get(get_visualizer_status))
        .route("/api/visualizer/download", post(download_visualizer))
        .route("/api/analysis/download", post(download_analysis))
        .route("/visualizer.html", get(get_visualizer_html))
        .route("/visualizer/{*path}", get(get_visualizer_asset))
        .route("/analysis/index.html", get(get_analysis_index))
        .route("/analysis/{contest}/input.csv", get(get_analysis_input_csv))
        .route(
            "/analysis/{contest}/result.csv",
            get(get_analysis_result_csv),
        )
        .with_state(state)
        .fallback_service(fallback)
}

async fn get_config(State(state): State<AppState>) -> Result<Json<ConfigResponse>, AppError> {
    Ok(Json(load_config_response(&state).await?))
}

async fn run_init(
    State(state): State<AppState>,
    Json(request): Json<InitRequest>,
) -> Result<Json<ConfigResponse>, AppError> {
    let problem = request.problem.trim();
    if problem.is_empty() {
        return Err(AppError::BadRequest(
            "Problem name is required".to_string(),
        ));
    }

    let _init_guard = state.init_lock.lock().await;

    match inspect_problem_config(&state.base_dir).await {
        ProblemConfigState::Initialized { .. } => {
            return Err(AppError::Conflict(
                "Directory is already initialized".to_string(),
            ));
        }
        ProblemConfigState::Invalid { .. } => {
            return Err(AppError::Conflict(
                "Directory has an invalid pahcer_config.toml. Fix or remove it before initializing."
                    .to_string(),
            ));
        }
        ProblemConfigState::Uninitialized => {}
    }

    let mut command = Command::new(&state.pahcer_program);
    command
        .arg("init")
        .arg("-p")
        .arg(problem)
        .arg("-o")
        .arg(request.objective.as_arg())
        .arg("-l")
        .arg(request.language.as_arg())
        .current_dir(&state.base_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if request.interactive {
        command.arg("-i");
    }

    let output = command
        .output()
        .await
        .map_err(|error| AppError::CommandFailed(format!("Failed to spawn pahcer: {error}")))?;

    if !output.status.success() {
        return Err(AppError::CommandFailed(command_output_message(
            &output,
            format!("pahcer init failed with status {}", output.status),
        )));
    }

    match inspect_problem_config(&state.base_dir).await {
        ProblemConfigState::Initialized { .. } => {}
        ProblemConfigState::Uninitialized => {
            return Err(AppError::CommandFailed(command_output_message(
                &output,
                "pahcer init succeeded but pahcer_config.toml was not created".to_string(),
            )));
        }
        ProblemConfigState::Invalid { message } => {
            return Err(AppError::CommandFailed(command_output_message(
                &output,
                format!("pahcer init succeeded but pahcer_config.toml is invalid: {message}"),
            )));
        }
    }

    Ok(Json(load_config_response(&state).await?))
}

fn command_output_message(output: &std::process::Output, fallback: String) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return format!("{fallback}: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return format!("{fallback}: {stdout}");
    }

    fallback
}

async fn load_config_response(state: &AppState) -> Result<ConfigResponse, AppError> {
    let global = state.storage.get_global_config().await?;
    let local = state.storage.get_local_config().await?;
    let (initialization_state, initialization_error, problem_name) =
        match inspect_problem_config(&state.base_dir).await {
            ProblemConfigState::Initialized { problem_name } => {
                (InitializationState::Initialized, None, Some(problem_name))
            }
            ProblemConfigState::Uninitialized => {
                (InitializationState::Uninitialized, None, None)
            }
            ProblemConfigState::Invalid { message } => {
                (InitializationState::Invalid, Some(message), None)
            }
        };

    Ok(ConfigResponse {
        global,
        local,
        initialization_state,
        initialization_error,
        problem_name,
        base_dir: state.base_dir.to_string_lossy().to_string(),
    })
}

async fn save_global_config(
    State(state): State<AppState>,
    Json(config): Json<crate::models::GlobalConfig>,
) -> Result<Json<Value>, AppError> {
    state.storage.save_global_config(config).await?;
    Ok(Json(json!({ "success": true })))
}

async fn save_local_config(
    State(state): State<AppState>,
    Json(config): Json<LocalConfig>,
) -> Result<Json<Value>, AppError> {
    state.storage.save_local_config(config).await?;
    Ok(Json(json!({ "success": true })))
}

async fn get_jobs(State(state): State<AppState>) -> Result<Json<Vec<JobMetadata>>, AppError> {
    Ok(Json(state.storage.get_jobs().await?))
}

async fn get_history(State(state): State<AppState>) -> Result<Json<Vec<StoredResult>>, AppError> {
    Ok(Json(state.storage.read_history().await?))
}

async fn get_history_output(
    State(state): State<AppState>,
    AxumPath((timestamp, filename)): AxumPath<(String, String)>,
) -> Result<Response<Body>, AppError> {
    let path = state
        .storage
        .result_dir(&timestamp)
        .join("output")
        .join(filename);
    let content = fs::read(path).await?;
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )
        .body(Body::from(content))
        .map_err(|error| AppError::Internal(error.to_string()))
}

async fn delete_history(
    State(state): State<AppState>,
    AxumPath(timestamp): AxumPath<String>,
) -> Result<Json<Value>, AppError> {
    state.storage.delete_result(&timestamp).await?;
    Ok(Json(json!({ "success": true })))
}

async fn get_visualizer_status(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "exists": state.storage.has_visualizer().await }))
}

async fn download_visualizer(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let url = payload
        .get("url")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("URL is required".to_string()))?;

    let destination = state.storage.visualizer_dir();
    match fs::remove_dir_all(&destination).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    download_recursive(&state.client, url, &destination).await?;
    Ok(Json(json!({ "success": true })))
}

async fn download_analysis(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    download_analysis_html(&state.client, &state.storage).await?;
    Ok(Json(json!({ "success": true })))
}

async fn list_pahcer(
    State(state): State<AppState>,
) -> Result<Json<crate::models::ListResponse>, AppError> {
    Ok(Json(
        get_pahcer_list(&state.base_dir, &state.pahcer_program).await?,
    ))
}

async fn run_pahcer(
    State(state): State<AppState>,
    Json(request): Json<RunRequest>,
) -> Result<Response<Body>, AppError> {
    let _guard = state.run_lock.lock().await;

    let mut args = request.args;
    ensure_json_flag(&mut args);
    let (comment, tag) = extract_comment_tag(&args);
    let run_started_at = Utc::now();
    let timestamp = run_started_at.timestamp().to_string();
    let result_dir = state.storage.result_dir(&timestamp);
    let output_dir = result_dir.join("output");
    fs::create_dir_all(&output_dir).await?;
    let existing_result_files = state.storage.list_pahcer_result_files().await?;

    let (sender, receiver) = mpsc::channel::<Result<Bytes, Infallible>>(64);
    let job_id = Utc::now().timestamp_millis().to_string();
    let state_clone = state.clone();
    let request = RunTaskRequest {
        args,
        comment,
        tag,
        timestamp,
        job_id,
        run_started_at,
        existing_result_files,
        output_dir,
    };

    tokio::spawn(async move {
        let outcome = run_pahcer_task(state_clone, request, sender.clone()).await;
        if let Err(error) = outcome {
            let message = StreamMessage::Stderr {
                data: format!("{error}\n"),
            };
            let _ = sender.send(Ok(serialize_stream_message(&message))).await;
            let exit = StreamMessage::Exit { code: 1 };
            let _ = sender.send(Ok(serialize_stream_message(&exit))).await;
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
        )
        .body(Body::from_stream(ReceiverStream::new(receiver)))
        .map_err(|error| AppError::Internal(error.to_string()))
}

struct RunTaskRequest {
    args: Vec<String>,
    comment: String,
    tag: String,
    timestamp: String,
    job_id: String,
    run_started_at: DateTime<Utc>,
    existing_result_files: HashMap<String, PahcerResultFileState>,
    output_dir: PathBuf,
}

async fn run_pahcer_task(
    state: AppState,
    request: RunTaskRequest,
    sender: mpsc::Sender<Result<Bytes, Infallible>>,
) -> Result<(), AppError> {
    let job = JobMetadata {
        id: request.job_id.clone(),
        datetime: Utc::now().to_rfc3339(),
        command: "run".to_string(),
        args: request.args.clone(),
        status: JobStatus::Running,
        output_file: None,
        result: None,
    };
    state.storage.save_job(job).await?;

    let mut child = Command::new(&state.pahcer_program)
        .arg("run")
        .args(&request.args)
        .current_dir(&state.base_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| AppError::CommandFailed(format!("Failed to spawn pahcer: {error}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Internal("stdout pipe missing".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Internal("stderr pipe missing".to_string()))?;
    let log_buffer = Arc::new(Mutex::new(String::new()));

    let stdout_task = pipe_child_output(stdout, log_buffer.clone(), sender.clone(), true);
    let stderr_task = pipe_child_output(stderr, log_buffer.clone(), sender.clone(), false);

    let status = child.wait().await?;
    let exit_code = status.code().unwrap_or(1);
    stdout_task
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;
    stderr_task
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;

    let all_output = log_buffer.lock().await.clone();
    let global_config = state.storage.get_global_config().await?;

    let finalize_result = async {
        let Some((result_file_name, pahcer_result)) = state
            .storage
            .find_pahcer_result_for_run(
                &request.existing_result_files,
                request.run_started_at,
                &request.comment,
                &request.tag,
            )
            .await?
        else {
            return Err(AppError::CommandFailed(
                "pahcer result json was not found after run".to_string(),
            ));
        };

        if pahcer_result.case_count == 0 {
            return Ok(None);
        }

        state
            .storage
            .materialize_result_json(
                &request.timestamp,
                &result_file_name,
                global_config.result_json_mode,
            )
            .await?;
        copy_output_files(&state.base_dir, &request.output_dir).await?;

        let additional = AdditionalResultMetadata {
            id: request.timestamp.clone(),
            args: request.args.clone(),
            result_file_name,
            avg_score: average_from_total(pahcer_result.total_score, pahcer_result.case_count),
            avg_log_score: average_from_total(
                pahcer_result.total_score_log10,
                pahcer_result.case_count,
            ),
            avg_relative_score: average_from_total(
                pahcer_result.total_relative_score,
                pahcer_result.case_count,
            ),
            extra: Default::default(),
        };
        state
            .storage
            .save_additional_result(&request.timestamp, &additional)
            .await?;

        Ok(Some((additional, pahcer_result)))
    }
    .await;

    match finalize_result {
        Ok(Some((additional, pahcer_result))) => {
            let stored = StoredResult {
                id: additional.id.clone(),
                datetime: pahcer_result.start_time.clone(),
                args: additional.args.clone(),
                comment: pahcer_result.comment.clone(),
                tag: pahcer_result.tag_name.clone().unwrap_or_default(),
                avg_score: additional.avg_score,
                avg_log_score: additional.avg_log_score,
                avg_relative_score: additional.avg_relative_score,
                max_time: pahcer_result.max_execution_time * 1000.0,
                cases: pahcer_result.case_count,
                ac_case: pahcer_result
                    .cases
                    .iter()
                    .filter(|case| {
                        serde_json::to_value(case)
                            .ok()
                            .as_ref()
                            .is_some_and(crate::pahcer::is_ac)
                    })
                    .count(),
                details: pahcer_result
                    .cases
                    .iter()
                    .filter_map(|case| serde_json::to_value(case).ok())
                    .collect(),
                extra: Default::default(),
            };
            state
                .storage
                .save_job(JobMetadata {
                    id: request.job_id,
                    datetime: Utc::now().to_rfc3339(),
                    command: "run".to_string(),
                    args: request.args,
                    status: if exit_code == 0 {
                        JobStatus::Success
                    } else {
                        JobStatus::Failed
                    },
                    output_file: None,
                    result: Some(json!({ "score": stored.avg_score, "logs": all_output })),
                })
                .await?;
        }
        Ok(None) => {
            state.storage.delete_job(&request.job_id).await?;
            state.storage.delete_result(&request.timestamp).await?;
        }
        Err(error) => {
            let error_message = error.to_string();
            cleanup_failed_run(&state, &request, &all_output, &error_message).await?;
            return Err(error);
        }
    }

    let exit = StreamMessage::Exit { code: exit_code };
    sender.send(Ok(serialize_stream_message(&exit))).await.ok();
    Ok(())
}

fn pipe_child_output<R>(
    reader: R,
    log_buffer: Arc<Mutex<String>>,
    sender: mpsc::Sender<Result<Bytes, Infallible>>,
    is_stdout: bool,
) -> tokio::task::JoinHandle<Result<(), AppError>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut stream = ReaderStream::new(reader);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(AppError::Io)?;
            let text = String::from_utf8_lossy(&chunk).to_string();
            {
                let mut logs = log_buffer.lock().await;
                logs.push_str(&text);
            }
            let message = if is_stdout {
                StreamMessage::Stdout { data: text }
            } else {
                StreamMessage::Stderr { data: text }
            };
            sender
                .send(Ok(serialize_stream_message(&message)))
                .await
                .ok();
        }
        Ok(())
    })
}

async fn get_visualizer_html(State(state): State<AppState>) -> Result<Response<Body>, AppError> {
    serve_visualizer_file(state.storage.visualizer_path()).await
}

async fn get_visualizer_asset(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Result<Response<Body>, AppError> {
    let relative = if path.is_empty() {
        PathBuf::from("index.html")
    } else {
        PathBuf::from(path)
    };
    serve_visualizer_file(state.storage.visualizer_dir().join(relative)).await
}

async fn serve_visualizer_file(path: PathBuf) -> Result<Response<Body>, AppError> {
    let content = fs::read(&path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound(
                "Visualizer not found. Please download it from settings.".to_string(),
            )
        } else {
            AppError::Io(error)
        }
    })?;

    let body = if path.extension().and_then(|ext| ext.to_str()) == Some("html") {
        Body::from(inject_output_loader(&String::from_utf8_lossy(&content)).into_bytes())
    } else {
        Body::from(content)
    };

    let mime = from_path(&path).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(mime.as_ref())
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .body(body)
        .map_err(|error| AppError::Internal(error.to_string()))
}

async fn get_analysis_index(State(state): State<AppState>) -> Result<Response<Body>, AppError> {
    let path = state.storage.analysis_path();
    let html = match fs::read_to_string(&path).await {
        Ok(html) => html,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            download_analysis_html(&state.client, &state.storage).await?
        }
        Err(error) => return Err(error.into()),
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        )
        .body(Body::from(html))
        .map_err(|error| AppError::Internal(error.to_string()))
}

async fn get_analysis_input_csv(
    State(state): State<AppState>,
    AxumPath(_contest): AxumPath<String>,
) -> Result<Response<Body>, AppError> {
    let local = state.storage.get_local_config().await?;
    let csv = generate_input_csv(&state.base_dir, &local).await?;
    csv_response(csv)
}

async fn get_analysis_result_csv(
    State(state): State<AppState>,
    AxumPath(_contest): AxumPath<String>,
) -> Result<Response<Body>, AppError> {
    let global = state.storage.get_global_config().await?;
    let local = state.storage.get_local_config().await?;
    let csv = generate_result_csv(&state.storage, &global, &local).await?;
    csv_response(csv)
}

fn csv_response(csv: String) -> Result<Response<Body>, AppError> {
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/csv; charset=utf-8"),
        )
        .body(Body::from(csv))
        .map_err(|error| AppError::Internal(error.to_string()))
}

async fn build_frontend(frontend_dir: &Path) -> Result<(), AppError> {
    let status = Command::new("npm")
        .arg("run")
        .arg("build")
        .current_dir(frontend_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;

    if status.success() {
        Ok(())
    } else {
        Err(AppError::CommandFailed(
            "Failed to build frontend".to_string(),
        ))
    }
}

fn default_frontend_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("frontend")
}

async fn copy_output_files(base_dir: &Path, output_dir: &Path) -> Result<(), AppError> {
    let tools_out_dir = base_dir.join("tools").join("out");
    let mut entries = match fs::read_dir(&tools_out_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("txt") {
            let destination = output_dir.join(entry.file_name());
            fs::copy(path, destination).await?;
        }
    }
    Ok(())
}

async fn cleanup_failed_run(
    state: &AppState,
    request: &RunTaskRequest,
    logs: &str,
    error_message: &str,
) -> Result<(), AppError> {
    state.storage.delete_result(&request.timestamp).await?;
    state
        .storage
        .save_job(JobMetadata {
            id: request.job_id.clone(),
            datetime: Utc::now().to_rfc3339(),
            command: "run".to_string(),
            args: request.args.clone(),
            status: JobStatus::Failed,
            output_file: None,
            result: Some(json!({ "error": error_message, "logs": logs })),
        })
        .await
}

fn serialize_stream_message(message: &StreamMessage) -> Bytes {
    let mut payload = serde_json::to_vec(message).unwrap_or_default();
    payload.push(b'\n');
    Bytes::from(payload)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tempfile::tempdir;
    use tower::ServiceExt;

    use crate::{
        models::{
            GlobalConfig, ResultJsonMode, VisualizerInitialScrollPosition, VisualizerPosition,
        },
        server::{build_app, build_state},
    };

    #[tokio::test]
    async fn config_roundtrip_works() {
        let dir = tempdir().unwrap();
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", dir.path());
        }
        let frontend = dir.path().join("dist");
        tokio::fs::create_dir_all(&frontend).await.unwrap();
        tokio::fs::write(frontend.join("index.html"), "<html></html>")
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

        let payload = serde_json::to_vec(&GlobalConfig {
            visualizer_position: VisualizerPosition::Right,
            visualizer_initial_scroll_position: VisualizerInitialScrollPosition::Bottom,
            visualizer_url: Some("https://example.com".to_string()),
            result_json_mode: ResultJsonMode::Copy,
            default_seed: 0,
            default_scale: 1.0,
            test_run_options: None,
            extra: BTreeMap::new(),
        })
        .unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config/global")
                    .header("content-type", "application/json")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["global"]["defaultSeed"], 0);
        assert_eq!(json["global"]["resultJsonMode"], "copy");
        assert_eq!(json["global"]["visualizerPosition"], "right");
        assert_eq!(json["global"]["visualizerInitialScrollPosition"], "bottom");
        assert_eq!(json["initializationState"], "uninitialized");
        assert!(json["problemName"].is_null());

        let saved: serde_json::Value = serde_json::from_str(
            &tokio::fs::read_to_string(state.storage.global_config_path())
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(saved["resultJsonMode"], "copy");
        assert_eq!(saved["visualizerInitialScrollPosition"], "bottom");
    }
}
