use std::{
    collections::HashMap,
    convert::Infallible,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path as AxumPath, Request, State},
    http::{HeaderValue, Response, StatusCode, Uri, header},
    routing::{delete, get, post},
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use hyper::header::HOST;
use hyper_util::{
    client::legacy::{Client as HyperClient, connect::HttpConnector},
    rt::TokioExecutor,
};
use mime_guess::from_path;
use serde_json::{Value, json};
use tokio::{
    fs,
    net::TcpListener,
    process::Command,
    sync::{Mutex, Notify, mpsc, oneshot},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::ReaderStream;

use crate::{
    analysis::{download_analysis_html, generate_input_csv, generate_result_csv},
    error::AppError,
    models::{
        AdditionalResultMetadata, ConfigResponse, InitRequest, InitializationState, JobMetadata,
        JobStatus, LocalConfig, RunRequest, RunTerminationReason, StoredResult, StreamMessage,
    },
    pahcer::{average_from_total, ensure_json_flag, extract_comment_tag, get_pahcer_list},
    problem_config::{ProblemConfigState, inspect_problem_config},
    storage::{PahcerResultFileState, Storage},
    visualizer::{download_recursive, inject_output_loader},
};

type FrontendProxyClient = HyperClient<HttpConnector, Body>;

#[derive(Debug, Clone)]
pub enum FrontendTarget {
    Static(PathBuf),
    Proxy(url::Url),
}

impl From<PathBuf> for FrontendTarget {
    fn from(path: PathBuf) -> Self {
        Self::Static(path)
    }
}

impl From<url::Url> for FrontendTarget {
    fn from(url: url::Url) -> Self {
        Self::Proxy(url)
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub base_dir: PathBuf,
    pub storage: Storage,
    pub client: reqwest::Client,
    pub frontend_target: FrontendTarget,
    pub frontend_proxy_client: FrontendProxyClient,
    pub pahcer_program: PathBuf,
    pub init_lock: Arc<Mutex<()>>,
    pub run_lock: Arc<Mutex<()>>,
    active_runs: Arc<Mutex<HashMap<String, ActiveRunHandle>>>,
}

#[derive(Debug, Clone)]
struct ActiveRunHandle {
    cancel_sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    completion: Arc<RunCompletion>,
}

#[derive(Debug)]
struct RunCompletion {
    finished: AtomicBool,
    notify: Notify,
}

impl RunCompletion {
    fn new() -> Self {
        Self {
            finished: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    fn mark_finished(&self) {
        self.finished.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    async fn wait(&self) {
        if self.finished.load(Ordering::Acquire) {
            return;
        }

        let notified = self.notify.notified();
        if self.finished.load(Ordering::Acquire) {
            return;
        }

        notified.await;
    }
}

#[derive(Debug, Clone)]
pub struct UiServerOptions {
    pub base_dir: PathBuf,
    pub port: u16,
    pub pahcer_program: Option<PathBuf>,
}

pub async fn start_ui_server(options: UiServerOptions) -> Result<(), AppError> {
    let base_dir = options.base_dir.canonicalize().unwrap_or(options.base_dir);
    let frontend_target = resolve_frontend_target(&staged_frontend_dir()).await?;
    let state = build_state(base_dir, frontend_target, options.pahcer_program).await?;
    let app = build_app(state.clone());
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, options.port)).await?;
    println!("Starting server on http://localhost:{}", options.port);
    axum::serve(listener, app)
        .await
        .map_err(|error| AppError::Internal(error.to_string()))
}

pub async fn build_state(
    base_dir: PathBuf,
    frontend_target: impl Into<FrontendTarget>,
    pahcer_program: Option<PathBuf>,
) -> Result<AppState, AppError> {
    let storage = Storage::new(base_dir.clone())?;

    Ok(AppState {
        base_dir,
        storage,
        client: reqwest::Client::new(),
        frontend_target: frontend_target.into(),
        frontend_proxy_client: HyperClient::builder(TokioExecutor::new()).build_http(),
        pahcer_program: pahcer_program
            .or_else(|| std::env::var_os("PAHCER_WEB_PAHCER_BIN").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("pahcer")),
        init_lock: Arc::new(Mutex::new(())),
        run_lock: Arc::new(Mutex::new(())),
        active_runs: Arc::new(Mutex::new(HashMap::new())),
    })
}

pub fn build_app(state: AppState) -> Router {
    let app = Router::new()
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
        .route("/api/run/{run_id}/cancel", post(cancel_run))
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
        );

    match &state.frontend_target {
        FrontendTarget::Static(_) => app.fallback(static_frontend_request).with_state(state),
        FrontendTarget::Proxy(_) => app.fallback(proxy_frontend_request).with_state(state),
    }
}

async fn static_frontend_request(
    State(state): State<AppState>,
    request: Request,
) -> Result<Response<Body>, AppError> {
    let dist_dir = match &state.frontend_target {
        FrontendTarget::Static(dist_dir) => dist_dir,
        FrontendTarget::Proxy(_) => {
            return Err(AppError::NotFound(
                "Static frontend assets are not configured".to_string(),
            ));
        }
    };

    serve_frontend_asset(dist_dir, request.uri().path()).await
}

async fn proxy_frontend_request(
    State(state): State<AppState>,
    mut request: Request<Body>,
) -> Result<Response<Body>, AppError> {
    let target = match &state.frontend_target {
        FrontendTarget::Proxy(target) => target.clone(),
        FrontendTarget::Static(_) => {
            return Err(AppError::NotFound(
                "Frontend dev server proxy is not configured".to_string(),
            ));
        }
    };

    let request_upgrade = is_upgrade_request(request.headers()).then(|| hyper::upgrade::on(&mut request));

    *request.uri_mut() = frontend_proxy_uri(&target, request.uri())?;
    set_proxy_host_header(request.headers_mut(), &target)?;

    let mut response = state
        .frontend_proxy_client
        .request(request)
        .await
        .map_err(|error| AppError::Internal(format!("Failed to proxy frontend request: {error}")))?;

    let response_upgrade = (response.status() == StatusCode::SWITCHING_PROTOCOLS)
        .then(|| hyper::upgrade::on(&mut response));

    if let (Some(request_upgrade), Some(response_upgrade)) = (request_upgrade, response_upgrade) {
        tokio::spawn(async move {
            let Ok(request_upgraded) = request_upgrade.await else {
                return;
            };
            let Ok(response_upgraded) = response_upgrade.await else {
                return;
            };

            let mut request_upgraded = hyper_util::rt::TokioIo::new(request_upgraded);
            let mut response_upgraded = hyper_util::rt::TokioIo::new(response_upgraded);
            let _ = tokio::io::copy_bidirectional(&mut request_upgraded, &mut response_upgraded)
                .await;
        });
    }

    Ok(response.map(Body::new))
}

fn is_upgrade_request(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get(header::CONNECTION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains("upgrade"))
        .unwrap_or(false)
        || headers.contains_key(header::UPGRADE)
}

fn frontend_proxy_uri(target: &url::Url, request_uri: &Uri) -> Result<Uri, AppError> {
    let mut url = target.clone();
    url.set_path(request_uri.path());
    url.set_query(request_uri.query());
    url.as_str()
        .parse()
        .map_err(|error| AppError::Internal(format!("Invalid frontend dev server URL: {error}")))
}

fn set_proxy_host_header(
    headers: &mut axum::http::HeaderMap,
    target: &url::Url,
) -> Result<(), AppError> {
    let authority = target
        .host_str()
        .map(|host| match target.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        })
        .ok_or_else(|| AppError::Internal("Frontend dev server URL must include a host".to_string()))?;

    let header_value = HeaderValue::from_str(&authority)
        .map_err(|error| AppError::Internal(format!("Invalid frontend dev server host: {error}")))?;
    headers.insert(HOST, header_value);
    Ok(())
}

async fn serve_frontend_asset(dist_dir: &Path, request_path: &str) -> Result<Response<Body>, AppError> {
    let relative_path = sanitize_relative_path(request_path);
    let asset_path = relative_path
        .as_ref()
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| dist_dir.join(path))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| dist_dir.join("index.html"));

    let content = fs::read(&asset_path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound(format!(
                "Frontend asset not found: {}",
                asset_path.display()
            ))
        } else {
            AppError::Io(error)
        }
    })?;

    let mime = from_path(&asset_path).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(mime.as_ref())
                .unwrap_or(HeaderValue::from_static("application/octet-stream")),
        )
        .body(Body::from(content))
        .map_err(|error| AppError::Internal(error.to_string()))
}

fn sanitize_relative_path(request_path: &str) -> Option<PathBuf> {
    let mut relative_path = PathBuf::new();

    for component in Path::new(request_path.trim_start_matches('/')).components() {
        match component {
            std::path::Component::Normal(segment) => relative_path.push(segment),
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }

    Some(relative_path)
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
        return Err(AppError::BadRequest("Problem name is required".to_string()));
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
            ProblemConfigState::Uninitialized => (InitializationState::Uninitialized, None, None),
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

fn validate_run_id(run_id: &str) -> Result<&str, AppError> {
    if !run_id.is_empty()
        && run_id
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'-' | b'_'))
    {
        Ok(run_id)
    } else {
        Err(AppError::BadRequest(
            "runId must contain only ASCII letters, digits, hyphens, or underscores".to_string(),
        ))
    }
}

fn resolve_run_id(requested_run_id: &str) -> Result<String, AppError> {
    let run_id = requested_run_id.trim();
    if run_id.is_empty() {
        Ok(Utc::now().timestamp_millis().to_string())
    } else {
        validate_run_id(run_id)?;
        Ok(run_id.to_string())
    }
}

async fn run_pahcer(
    State(state): State<AppState>,
    Json(request): Json<RunRequest>,
) -> Result<Response<Body>, AppError> {
    let _guard = state.run_lock.lock().await;

    let run_id = resolve_run_id(&request.run_id)?;
    let run_base_dir = request.directory.unwrap_or_else(|| state.base_dir.clone());
    let run_storage = Storage::new(run_base_dir.clone())?;

    if run_storage.run_id_exists(&run_id).await? {
        return Err(AppError::Conflict(format!("Run {run_id} already exists")));
    }

    let mut args = request.args;
    ensure_json_flag(&mut args);
    let (comment, tag) = extract_comment_tag(&args);
    let run_started_at = Utc::now();
    let existing_result_files = run_storage.list_pahcer_result_files().await?;

    {
        let active_runs = state.active_runs.lock().await;
        if active_runs.contains_key(&run_id) {
            return Err(AppError::Conflict(format!(
                "Run {run_id} is already in progress"
            )));
        }
    }

    let (sender, receiver) = mpsc::channel::<Result<Bytes, Infallible>>(64);
    let (cancel_sender, cancel_receiver) = oneshot::channel();
    let completion = Arc::new(RunCompletion::new());
    state.active_runs.lock().await.insert(
        run_id.clone(),
        ActiveRunHandle {
            cancel_sender: Arc::new(Mutex::new(Some(cancel_sender))),
            completion: completion.clone(),
        },
    );

    let state_clone = state.clone();
    let request = RunTaskRequest {
        base_dir: run_base_dir,
        storage: run_storage,
        run_id: run_id.clone(),
        args,
        comment,
        tag,
        run_started_at,
        existing_result_files,
    };

    tokio::spawn(async move {
        let outcome = run_pahcer_task(
            state_clone.clone(),
            request,
            cancel_receiver,
            sender.clone(),
        )
        .await;
        if let Err(error) = outcome {
            let message = StreamMessage::Stderr {
                data: format!("{error}\n"),
            };
            let _ = sender.send(Ok(serialize_stream_message(&message))).await;
            let exit = StreamMessage::Exit {
                code: 1,
                run_id: run_id.clone(),
                reason: RunTerminationReason::Failed,
            };
            let _ = sender.send(Ok(serialize_stream_message(&exit))).await;
        }
        completion.mark_finished();
        state_clone.active_runs.lock().await.remove(&run_id);
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

async fn cancel_run(
    State(state): State<AppState>,
    AxumPath(run_id): AxumPath<String>,
) -> Result<Json<Value>, AppError> {
    let run_id = validate_run_id(run_id.trim())?.to_string();

    let Some(active_run) = ({
        let active_runs = state.active_runs.lock().await;
        active_runs.get(&run_id).cloned()
    }) else {
        return Err(AppError::NotFound(format!("Run {run_id} is not active")));
    };

    if let Some(cancel_sender) = active_run.cancel_sender.lock().await.take() {
        let _ = cancel_sender.send(());
    }

    active_run.completion.wait().await;

    Ok(Json(json!({ "success": true, "runId": run_id })))
}

struct RunTaskRequest {
    base_dir: PathBuf,
    storage: Storage,
    run_id: String,
    args: Vec<String>,
    comment: String,
    tag: String,
    run_started_at: DateTime<Utc>,
    existing_result_files: HashMap<String, PahcerResultFileState>,
}

async fn run_pahcer_task(
    state: AppState,
    request: RunTaskRequest,
    mut cancel_receiver: oneshot::Receiver<()>,
    sender: mpsc::Sender<Result<Bytes, Infallible>>,
) -> Result<(), AppError> {
    let job = JobMetadata {
        id: request.run_id.clone(),
        datetime: Utc::now().to_rfc3339(),
        command: "run".to_string(),
        args: request.args.clone(),
        status: JobStatus::Running,
        output_file: None,
        result: None,
    };
    request.storage.save_job(job).await?;

    let mut child = Command::new(&state.pahcer_program)
        .arg("run")
        .args(&request.args)
        .current_dir(&request.base_dir)
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

    let (status, termination_reason) = tokio::select! {
        status = child.wait() => {
            let status = status?;
            let termination_reason = termination_reason_from_exit_status(status);
            (status, termination_reason)
        }
        _ = &mut cancel_receiver => {
            let kill_started = match child.start_kill() {
                Ok(()) => true,
                Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => false,
                Err(error) => return Err(error.into()),
            };
            let status = child.wait().await?;
            let termination_reason = termination_reason_after_cancel_attempt(kill_started, status);
            (status, termination_reason)
        }
    };
    let exit_code = status.code().unwrap_or(match termination_reason {
        RunTerminationReason::Canceled => 130,
        _ => 1,
    });
    stdout_task
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;
    stderr_task
        .await
        .map_err(|error| AppError::Internal(error.to_string()))??;

    let all_output = log_buffer.lock().await.clone();
    let global_config = request.storage.get_global_config().await?;

    if termination_reason == RunTerminationReason::Canceled {
        cleanup_canceled_run(&request, &all_output).await?;

        let exit = StreamMessage::Exit {
            code: exit_code,
            run_id: request.run_id,
            reason: RunTerminationReason::Canceled,
        };
        sender.send(Ok(serialize_stream_message(&exit))).await.ok();
        return Ok(());
    }

    let finalize_result = async {
        let Some((result_file_name, pahcer_result)) = request
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

        request
            .storage
            .materialize_result_json(
                &request.run_id,
                &result_file_name,
                global_config.result_json_mode,
            )
            .await?;
        copy_output_files(&request.base_dir, &request.storage, &request.run_id).await?;

        let additional = AdditionalResultMetadata {
            id: request.run_id.clone(),
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
        request
            .storage
            .save_additional_result(&request.run_id, &additional)
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
            request
                .storage
                .save_job(JobMetadata {
                    id: request.run_id.clone(),
                    datetime: Utc::now().to_rfc3339(),
                    command: "run".to_string(),
                    args: request.args,
                    status: if exit_code == 0 {
                        JobStatus::Success
                    } else {
                        JobStatus::Failed
                    },
                    output_file: None,
                    result: Some(json!({
                        "score": stored.avg_score,
                        "logs": all_output,
                        "terminationReason": termination_reason,
                    })),
                })
                .await?;
        }
        Ok(None) => {
            request.storage.delete_job(&request.run_id).await?;
            request.storage.delete_result(&request.run_id).await?;
        }
        Err(error) => {
            let error_message = error.to_string();
            cleanup_failed_run(&request, &all_output, &error_message).await?;
            return Err(error);
        }
    }

    let exit = StreamMessage::Exit {
        code: exit_code,
        run_id: request.run_id,
        reason: termination_reason,
    };
    sender.send(Ok(serialize_stream_message(&exit))).await.ok();
    Ok(())
}

fn termination_reason_from_exit_status(status: ExitStatus) -> RunTerminationReason {
    if status.success() {
        RunTerminationReason::Completed
    } else {
        RunTerminationReason::Failed
    }
}

fn termination_reason_after_cancel_attempt(
    kill_started: bool,
    status: ExitStatus,
) -> RunTerminationReason {
    if kill_started {
        RunTerminationReason::Canceled
    } else {
        termination_reason_from_exit_status(status)
    }
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

async fn resolve_frontend_target(
    frontend_assets_dir: &Path,
) -> Result<FrontendTarget, AppError> {
    if let Some(frontend_dev_url) = std::env::var_os("FRONTEND_DEV_URL") {
        let frontend_dev_url = frontend_dev_url.to_string_lossy().to_string();
        let url = url::Url::parse(&frontend_dev_url).map_err(|error| {
            AppError::BadRequest(format!("Invalid FRONTEND_DEV_URL value '{frontend_dev_url}': {error}"))
        })?;
        return Ok(FrontendTarget::Proxy(url));
    }

    if frontend_assets_dir.join("index.html").is_file() {
        return Ok(FrontendTarget::Static(frontend_assets_dir.to_path_buf()));
    }

    Err(AppError::NotFound(format!(
        "Frontend assets were not found in {}. Run `pnpm turbo run build` before starting pahcer-web ui.",
        frontend_assets_dir.display()
    )))
}

fn staged_frontend_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frontend-assets")
}

async fn copy_output_files(
    base_dir: &Path,
    storage: &Storage,
    run_id: &str,
) -> Result<(), AppError> {
    let tools_out_dir = base_dir.join("tools").join("out");
    let mut entries = match fs::read_dir(&tools_out_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    let output_dir = storage.result_dir(run_id).join("output");
    let mut created_output_dir = false;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("txt") {
            if !created_output_dir {
                fs::create_dir_all(&output_dir).await?;
                created_output_dir = true;
            }
            let destination = output_dir.join(entry.file_name());
            fs::copy(path, destination).await?;
        }
    }
    Ok(())
}

async fn cleanup_canceled_run(request: &RunTaskRequest, logs: &str) -> Result<(), AppError> {
    request.storage.delete_result(&request.run_id).await?;
    request
        .storage
        .save_job(JobMetadata {
            id: request.run_id.clone(),
            datetime: Utc::now().to_rfc3339(),
            command: "run".to_string(),
            args: request.args.clone(),
            status: JobStatus::Canceled,
            output_file: None,
            result: Some(json!({
                "logs": logs,
                "terminationReason": RunTerminationReason::Canceled,
            })),
        })
        .await
}

async fn cleanup_failed_run(
    request: &RunTaskRequest,
    logs: &str,
    error_message: &str,
) -> Result<(), AppError> {
    request.storage.delete_result(&request.run_id).await?;
    request
        .storage
        .save_job(JobMetadata {
            id: request.run_id.clone(),
            datetime: Utc::now().to_rfc3339(),
            command: "run".to_string(),
            args: request.args.clone(),
            status: JobStatus::Failed,
            output_file: None,
            result: Some(json!({
                "error": error_message,
                "logs": logs,
                "terminationReason": RunTerminationReason::Failed,
            })),
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
    use std::time::Duration;
    use std::{
        collections::BTreeMap,
        ffi::OsString,
        os::unix::process::ExitStatusExt,
        sync::{Arc, OnceLock},
    };

    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex as AsyncMutex;
    use tower::ServiceExt;
    use url::Url;

    use crate::{
        models::{
            GlobalConfig, HistoryScoreDisplayFormat, LocalConfig, ResultJsonMode,
            VisualizerInitialScrollPosition, VisualizerPosition,
        },
        server::{FrontendTarget, RunCompletion, build_app, build_state},
    };

    struct XdgConfigHomeGuard {
        previous: Option<OsString>,
    }

    struct EnvVarGuard {
        key: &'static str,
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

    impl EnvVarGuard {
        fn unset(key: &'static str) -> Self {
            let previous = std::env::var_os(key);
            unsafe {
                std::env::remove_var(key);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.previous.as_ref() {
                Some(value) => unsafe {
                    std::env::set_var(self.key, value);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    fn xdg_config_home_lock() -> Arc<AsyncMutex<()>> {
        static LOCK: OnceLock<Arc<AsyncMutex<()>>> = OnceLock::new();
        LOCK.get_or_init(|| Arc::new(AsyncMutex::new(()))).clone()
    }

    async fn spawn_proxy_server(response_body: &'static str) -> Url {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).await.unwrap();

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\ncontent-type: text/plain; charset=utf-8\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );

            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        Url::parse(&format!("http://{address}")).unwrap()
    }

    #[tokio::test]
    async fn run_completion_wait_returns_when_already_finished() {
        let completion = RunCompletion::new();
        completion.mark_finished();

        tokio::time::timeout(Duration::from_millis(100), completion.wait())
            .await
            .unwrap();
    }

    #[test]
    fn cancel_race_uses_completed_reason_when_child_already_exited_successfully() {
        let status = std::process::ExitStatus::from_raw(0);

        assert_eq!(
            super::termination_reason_after_cancel_attempt(false, status),
            crate::models::RunTerminationReason::Completed
        );
    }

    #[test]
    fn cancel_race_uses_failed_reason_when_child_already_exited_unsuccessfully() {
        let status = std::process::ExitStatus::from_raw(1 << 8);

        assert_eq!(
            super::termination_reason_after_cancel_attempt(false, status),
            crate::models::RunTerminationReason::Failed
        );
    }

    #[test]
    fn exit_stream_message_uses_run_id_camel_case_in_json() {
        let payload = super::serialize_stream_message(&crate::models::StreamMessage::Exit {
            code: 0,
            run_id: "run-123".to_string(),
            reason: crate::models::RunTerminationReason::Completed,
        });

        let message: serde_json::Value =
            serde_json::from_slice(&payload[..payload.len() - 1]).unwrap();

        assert_eq!(message["type"], "exit");
        assert_eq!(message["runId"], "run-123");
        assert!(message.get("run_id").is_none());

        let parsed: crate::models::StreamMessage = serde_json::from_value(message).unwrap();
        assert!(matches!(
            parsed,
            crate::models::StreamMessage::Exit {
                code: 0,
                run_id,
                reason: crate::models::RunTerminationReason::Completed,
            } if run_id == "run-123"
        ));
    }

    #[tokio::test]
    async fn config_roundtrip_works() {
        let dir = tempdir().unwrap();
        let lock = xdg_config_home_lock();
        let _lock = lock.lock().await;
        let _guard = XdgConfigHomeGuard::set(dir.path());
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
    }

    #[tokio::test]
    async fn local_config_roundtrip_preserves_history_score_display_format() {
        let dir = tempdir().unwrap();
        let lock = xdg_config_home_lock();
        let _lock = lock.lock().await;
        let _guard = XdgConfigHomeGuard::set(dir.path());
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

        for (format, expected) in [
            (HistoryScoreDisplayFormat::Plain, "plain"),
            (HistoryScoreDisplayFormat::Scientific, "scientific"),
        ] {
            let payload = serde_json::to_vec(&LocalConfig {
                visualizer_url: Some("https://example.com/vis.html".to_string()),
                default_score_type: None,
                history_score_display_format: Some(format),
                input_param_names: Some("N,M".to_string()),
                extra: BTreeMap::new(),
            })
            .unwrap();

            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/config/local")
                        .header("content-type", "application/json")
                        .body(Body::from(payload))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), 200);

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
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json["local"]["historyScoreDisplayFormat"], expected);

            let saved: serde_json::Value = serde_json::from_str(
                &tokio::fs::read_to_string(state.storage.local_config_path())
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(saved["historyScoreDisplayFormat"], expected);
        }
    }

    #[tokio::test]
    async fn static_frontend_fallback_serves_index_html() {
        let dir = tempdir().unwrap();
        let frontend = dir.path().join("frontend-assets");
        tokio::fs::create_dir_all(&frontend).await.unwrap();
        tokio::fs::write(frontend.join("index.html"), "<html>static app</html>")
            .await
            .unwrap();

        let state = build_state(
            dir.path().to_path_buf(),
            FrontendTarget::Static(frontend),
            Some(dir.path().join("fake-pahcer")),
        )
        .await
        .unwrap();
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/dashboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(String::from_utf8_lossy(&body), "<html>static app</html>");
    }

    #[tokio::test]
    async fn proxy_frontend_fallback_forwards_unknown_routes() {
        let dir = tempdir().unwrap();
        let proxy_url = spawn_proxy_server("proxied app").await;

        let state = build_state(
            dir.path().to_path_buf(),
            FrontendTarget::Proxy(proxy_url),
            Some(dir.path().join("fake-pahcer")),
        )
        .await
        .unwrap();
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/dashboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(String::from_utf8_lossy(&body), "proxied app");
    }

    #[tokio::test]
    async fn proxy_mode_keeps_api_routes_on_backend() {
        let dir = tempdir().unwrap();
        let lock = xdg_config_home_lock();
        let _lock = lock.lock().await;
        let _guard = XdgConfigHomeGuard::set(dir.path());
        let proxy_url = spawn_proxy_server("proxied app").await;

        let state = build_state(
            dir.path().to_path_buf(),
            FrontendTarget::Proxy(proxy_url),
            Some(dir.path().join("fake-pahcer")),
        )
        .await
        .unwrap();
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["initializationState"], "uninitialized");
    }

    #[tokio::test]
    async fn proxy_mode_keeps_visualizer_routes_on_backend() {
        let dir = tempdir().unwrap();
        let lock = xdg_config_home_lock();
        let _lock = lock.lock().await;
        let _guard = XdgConfigHomeGuard::set(dir.path());
        let proxy_url = spawn_proxy_server("proxied app").await;

        let visualizer_dir = dir.path().join(".pahcer-web/visualizer/assets");
        tokio::fs::create_dir_all(&visualizer_dir).await.unwrap();
        tokio::fs::write(
            dir.path().join(".pahcer-web/visualizer/index.html"),
            "<html><body>visualizer</body></html>",
        )
        .await
        .unwrap();
        tokio::fs::write(visualizer_dir.join("app.js"), "console.log('visualizer');")
            .await
            .unwrap();

        let state = build_state(
            dir.path().to_path_buf(),
            FrontendTarget::Proxy(proxy_url),
            Some(dir.path().join("fake-pahcer")),
        )
        .await
        .unwrap();
        let app = build_app(state);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/visualizer.html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("visualizer"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/visualizer/assets/app.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(String::from_utf8_lossy(&body), "console.log('visualizer');");
    }

    #[tokio::test]
    async fn proxy_mode_keeps_analysis_routes_on_backend() {
        let dir = tempdir().unwrap();
        let lock = xdg_config_home_lock();
        let _lock = lock.lock().await;
        let _guard = XdgConfigHomeGuard::set(dir.path());
        let proxy_url = spawn_proxy_server("proxied app").await;

        tokio::fs::create_dir_all(dir.path().join(".pahcer-web"))
            .await
            .unwrap();
        tokio::fs::write(
            dir.path().join(".pahcer-web/analysis.html"),
            "<html><body>analysis</body></html>",
        )
        .await
        .unwrap();
        tokio::fs::create_dir_all(dir.path().join("tools/in"))
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("tools/seeds.txt"), "7\n")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("tools/in/0000.txt"), "1 2\n")
            .await
            .unwrap();

        let state = build_state(
            dir.path().to_path_buf(),
            FrontendTarget::Proxy(proxy_url),
            Some(dir.path().join("fake-pahcer")),
        )
        .await
        .unwrap();
        let app = build_app(state);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/analysis/index.html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(String::from_utf8_lossy(&body), "<html><body>analysis</body></html>");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/analysis/ahc999/input.csv")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("file,seed,N,M"));
        assert!(text.contains("0000.txt,7,1,2"));
    }

    #[tokio::test]
    async fn resolve_frontend_target_requires_staged_assets_without_dev_proxy() {
        let _guard = EnvVarGuard::unset("FRONTEND_DEV_URL");
        let dir = tempdir().unwrap();

        let error = super::resolve_frontend_target(&dir.path().join("frontend-assets"))
            .await
            .unwrap_err();

        match error {
            crate::error::AppError::NotFound(message) => {
                assert!(message.contains("pnpm turbo run build"));
                assert!(message.contains("frontend-assets"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
