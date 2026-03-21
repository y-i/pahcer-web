use std::{
    collections::hash_map::DefaultHasher,
    collections::HashMap,
    env,
    hash::{Hash, Hasher},
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Duration, Local, NaiveDateTime, TimeZone, Utc};
use directories::BaseDirs;
use tokio::{fs, sync::Mutex};

use crate::{
    error::AppError,
    models::{
        AdditionalResultMetadata, GlobalConfig, JobMetadata, LocalConfig, PahcerResultFile,
        ResultJsonMode, StoredResult,
    },
    pahcer::{is_ac, parse_result_file_datetime},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PahcerResultFileState {
    size: u64,
    content_hash: u64,
}

#[derive(Clone, Debug)]
pub struct Storage {
    base_dir: PathBuf,
    global_config_path: PathBuf,
    jobs_lock: Arc<Mutex<()>>,
}

impl Storage {
    pub fn new(base_dir: impl Into<PathBuf>) -> Result<Self, AppError> {
        let base_dir = base_dir.into();
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| BaseDirs::new().map(|dirs| dirs.home_dir().join(".config")))
            .ok_or_else(|| AppError::Internal("Failed to resolve config directory".to_string()))?;

        Ok(Self {
            base_dir,
            global_config_path: config_home.join("pahcer-web").join("config.json"),
            jobs_lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn local_dir(&self) -> PathBuf {
        self.base_dir.join(".pahcer-web")
    }

    pub fn global_config_path(&self) -> &Path {
        &self.global_config_path
    }

    pub fn local_config_path(&self) -> PathBuf {
        self.local_dir().join("config.json")
    }

    pub fn jobs_path(&self) -> PathBuf {
        self.local_dir().join("jobs.json")
    }

    pub fn visualizer_dir(&self) -> PathBuf {
        self.local_dir().join("visualizer")
    }

    pub fn visualizer_path(&self) -> PathBuf {
        self.visualizer_dir().join("index.html")
    }

    pub fn analysis_path(&self) -> PathBuf {
        self.local_dir().join("analysis.html")
    }

    pub fn results_dir(&self) -> PathBuf {
        self.local_dir().join("results")
    }

    pub fn pahcer_json_dir(&self) -> PathBuf {
        self.base_dir.join("pahcer").join("json")
    }

    pub fn result_dir(&self, timestamp: &str) -> PathBuf {
        self.results_dir().join(timestamp)
    }

    pub fn result_path(&self, timestamp: &str) -> PathBuf {
        self.result_dir(timestamp).join("result.json")
    }

    pub fn additional_path(&self, timestamp: &str) -> PathBuf {
        self.result_dir(timestamp).join("additional.json")
    }

    pub fn pahcer_result_path(&self, file_name: &str) -> PathBuf {
        self.pahcer_json_dir().join(file_name)
    }

    pub async fn get_global_config(&self) -> Result<GlobalConfig, AppError> {
        self.read_json_or_default(self.global_config_path()).await
    }

    pub async fn save_global_config(&self, mut config: GlobalConfig) -> Result<(), AppError> {
        if let Ok(existing) = self.get_global_config().await {
            for (key, value) in existing.extra {
                config.extra.entry(key).or_insert(value);
            }
        }
        self.write_json(self.global_config_path(), &config).await
    }

    pub async fn get_local_config(&self) -> Result<LocalConfig, AppError> {
        self.read_json_or_default(&self.local_config_path()).await
    }

    pub async fn save_local_config(&self, mut config: LocalConfig) -> Result<(), AppError> {
        if let Ok(existing) = self.get_local_config().await {
            for (key, value) in existing.extra {
                config.extra.entry(key).or_insert(value);
            }
        }
        self.write_json(&self.local_config_path(), &config).await
    }

    pub async fn get_jobs(&self) -> Result<Vec<JobMetadata>, AppError> {
        self.read_json_or_default(&self.jobs_path()).await
    }

    pub async fn save_job(&self, job: JobMetadata) -> Result<(), AppError> {
        let _guard = self.jobs_lock.lock().await;
        let mut jobs = self.get_jobs().await.unwrap_or_default();
        if let Some(existing) = jobs.iter_mut().find(|current| current.id == job.id) {
            *existing = job;
        } else {
            jobs.push(job);
        }
        self.write_json(&self.jobs_path(), &jobs).await
    }

    pub async fn delete_job(&self, id: &str) -> Result<(), AppError> {
        let _guard = self.jobs_lock.lock().await;
        let mut jobs = self.get_jobs().await.unwrap_or_default();
        let original_len = jobs.len();
        jobs.retain(|job| job.id != id);
        if jobs.len() != original_len {
            self.write_json(&self.jobs_path(), &jobs).await?;
        }
        Ok(())
    }

    pub async fn has_visualizer(&self) -> bool {
        matches!(fs::metadata(self.visualizer_path()).await, Ok(metadata) if metadata.is_file())
    }

    pub async fn delete_result(&self, timestamp: &str) -> Result<(), AppError> {
        let path = self.result_dir(timestamp);
        match fs::remove_dir_all(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn save_additional_result(
        &self,
        timestamp: &str,
        additional: &AdditionalResultMetadata,
    ) -> Result<(), AppError> {
        self.write_json(&self.additional_path(timestamp), additional).await
    }

    pub async fn materialize_result_json(
        &self,
        timestamp: &str,
        file_name: &str,
        mode: ResultJsonMode,
    ) -> Result<(), AppError> {
        let source_path = self.pahcer_result_path(file_name);
        fs::metadata(&source_path).await?;

        let destination = self.result_path(timestamp);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).await?;
        }
        remove_file_if_exists(&destination).await?;

        match mode {
            ResultJsonMode::Symlink => {
                let parent = destination.parent().ok_or_else(|| {
                    AppError::Internal("result.json parent directory is missing".to_string())
                })?;
                let target = path_relative_from(&source_path, parent)
                    .unwrap_or_else(|| source_path.clone());
                create_symlink(&target, &destination)?
            }
            ResultJsonMode::Copy => {
                fs::copy(&source_path, &destination).await?;
            }
        }

        Ok(())
    }

    pub async fn list_pahcer_result_files(&self) -> Result<HashMap<String, PahcerResultFileState>, AppError> {
        let mut entries = match fs::read_dir(self.pahcer_json_dir()).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(HashMap::new());
            }
            Err(error) => return Err(error.into()),
        };

        let mut files = HashMap::new();
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            if !file_type.is_file() {
                continue;
            }

            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with("result_") && file_name.ends_with(".json") {
                let metadata = entry.metadata().await?;
                let content = fs::read(entry.path()).await?;
                files.insert(file_name, PahcerResultFileState::from_content(&metadata, &content));
            }
        }

        Ok(files)
    }

    pub async fn read_pahcer_result(
        &self,
        file_name: &str,
    ) -> Result<Option<PahcerResultFile>, AppError> {
        match self.read_result_file(self.pahcer_result_path(file_name)).await? {
            Some(result) => Ok(Some(result)),
            None => Ok(None),
        }
    }

    pub async fn read_materialized_result(
        &self,
        timestamp: &str,
    ) -> Result<Option<PahcerResultFile>, AppError> {
        self.read_result_file(self.result_path(timestamp)).await
    }

    async fn read_result_file(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<Option<PahcerResultFile>, AppError> {
        match fs::read_to_string(path.as_ref()).await {
            Ok(content) => Ok(serde_json::from_str(&content).ok()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn find_pahcer_result_for_run(
        &self,
        existing_files: &HashMap<String, PahcerResultFileState>,
        run_started_at: DateTime<Utc>,
        expected_comment: &str,
        expected_tag: &str,
    ) -> Result<Option<(String, PahcerResultFile)>, AppError> {
        let current_files = self.list_pahcer_result_files().await?;
        let changed_files = current_files
            .into_iter()
            .filter(|(file_name, state)| existing_files.get(file_name) != Some(state))
            .collect::<Vec<_>>();

        if changed_files.is_empty() {
            return Ok(None);
        }

        let threshold = run_started_at - Duration::minutes(5);
        let mut candidates = Vec::new();
        for (file_name, _) in changed_files {
            let Some(result) = self.read_pahcer_result(&file_name).await? else {
                continue;
            };
            let Some(started_at) = parse_pahcer_start_time(&result.start_time) else {
                continue;
            };
            if started_at < threshold {
                continue;
            }
            if !expected_comment.trim().is_empty() && result.comment != expected_comment {
                continue;
            }
            if !expected_tag.trim().is_empty()
                && result.tag_name.as_deref().unwrap_or_default() != expected_tag
            {
                continue;
            }

            candidates.push((file_name, result, started_at));
        }

        Ok(candidates
            .into_iter()
            .min_by(|left, right| {
                let left_delta = left.2.signed_duration_since(run_started_at).num_milliseconds().abs();
                let right_delta = right.2.signed_duration_since(run_started_at).num_milliseconds().abs();
                left_delta
                    .cmp(&right_delta)
                    .then_with(|| right.2.cmp(&left.2))
                    .then_with(|| left.0.cmp(&right.0))
            })
            .map(|(file_name, result, _)| (file_name, result)))
    }

    pub async fn read_history(&self) -> Result<Vec<StoredResult>, AppError> {
        let results_dir = self.results_dir();
        let mut entries = match fs::read_dir(results_dir).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };

        let mut results = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }

            if let Some(value) = self.read_history_entry(&entry.path()).await? {
                results.push(value);
            }
        }

        results.sort_by_key(|value| std::cmp::Reverse(datetime_of(value)));
        Ok(results)
    }

    async fn read_json_or_default<T>(&self, path: &Path) -> Result<T, AppError>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        match fs::read_to_string(path).await {
            Ok(content) => Ok(serde_json::from_str(&content)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(error) => Err(error.into()),
        }
    }

    async fn write_json<T>(&self, path: &Path, value: &T) -> Result<(), AppError>
    where
        T: serde::Serialize,
    {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let body = serde_json::to_vec_pretty(value)?;
        fs::write(path, body).await?;
        Ok(())
    }

    async fn read_history_entry(&self, result_dir: &Path) -> Result<Option<StoredResult>, AppError> {
        let additional_path = result_dir.join("additional.json");
        let additional = match fs::read_to_string(&additional_path).await {
            Ok(content) => match serde_json::from_str::<AdditionalResultMetadata>(&content) {
                Ok(additional) => additional,
                Err(_) => return Ok(None),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };

        let timestamp = result_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let Some(result) = self.read_materialized_result(timestamp).await? else {
            return Ok(None);
        };

        Ok(Some(normalize_history_result(timestamp, additional, result)?))
    }
}

impl PahcerResultFileState {
    fn from_content(metadata: &std::fs::Metadata, content: &[u8]) -> Self {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        Self {
            size: metadata.len(),
            content_hash: hasher.finish(),
        }
    }
}

fn normalize_history_result(
    timestamp: &str,
    additional: AdditionalResultMetadata,
    pahcer_result: PahcerResultFile,
) -> Result<StoredResult, AppError> {
    let details = pahcer_result
        .cases
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()?;
    let ac_case = details.iter().filter(|detail| is_ac(detail)).count();

    Ok(StoredResult {
        id: if additional.id.trim().is_empty() {
            timestamp.to_string()
        } else {
            additional.id
        },
        datetime: normalize_datetime_source(&pahcer_result.start_time, &additional.result_file_name),
        args: additional.args,
        comment: pahcer_result.comment,
        tag: pahcer_result.tag_name.unwrap_or_default(),
        avg_score: additional.avg_score,
        avg_log_score: additional.avg_log_score,
        avg_relative_score: additional.avg_relative_score,
        max_time: pahcer_result.max_execution_time * 1000.0,
        cases: pahcer_result.case_count,
        ac_case,
        details,
        extra: Default::default(),
    })
}

fn normalize_datetime_source(start_time: &str, file_name: &str) -> String {
    if !start_time.trim().is_empty() {
        return start_time.to_string();
    }

    parse_result_file_datetime(file_name)
        .map(|value| value.format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_default()
}

fn datetime_of(value: &StoredResult) -> Option<DateTime<Utc>> {
    history_timestamp(&value.datetime)
}

fn history_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| value.parse::<DateTime<Utc>>().ok())
        .or_else(|| parse_naive_local_timestamp(value))
}

fn parse_pahcer_start_time(value: &str) -> Option<DateTime<Utc>> {
    if value.trim().is_empty() {
        return None;
    }

    history_timestamp(value)
}

fn parse_naive_local_timestamp(value: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").ok()?;
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|value| value.with_timezone(&Utc))
}

async fn remove_file_if_exists(path: &Path) -> Result<(), AppError> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn path_relative_from(target: &Path, base_dir: &Path) -> Option<PathBuf> {
    let target_components = normalized_components(target)?;
    let base_components = normalized_components(base_dir)?;

    let common_len = target_components
        .iter()
        .zip(base_components.iter())
        .take_while(|(left, right)| left == right)
        .count();

    let mut relative = PathBuf::new();
    for _ in common_len..base_components.len() {
        relative.push("..");
    }
    for component in target_components.iter().skip(common_len) {
        relative.push(component);
    }

    Some(relative)
}

fn normalized_components(path: &Path) -> Option<Vec<PathBuf>> {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => components.push(PathBuf::from(prefix.as_os_str())),
            Component::RootDir => components.push(PathBuf::from(component.as_os_str())),
            Component::CurDir => {}
            Component::ParentDir => return None,
            Component::Normal(value) => components.push(PathBuf::from(value)),
        }
    }
    Some(components)
}

#[cfg(unix)]
fn create_symlink(target: &Path, destination: &Path) -> Result<(), AppError> {
    std::os::unix::fs::symlink(target, destination)?;
    Ok(())
}

#[cfg(windows)]
fn create_symlink(target: &Path, destination: &Path) -> Result<(), AppError> {
    std::os::windows::fs::symlink_file(target, destination)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs as stdfs, path::PathBuf};

    use chrono::{DateTime, Utc};
    use tempfile::tempdir;
    use tokio::fs;

    use crate::models::{AdditionalResultMetadata, ResultJsonMode};

    use super::Storage;

    #[tokio::test]
    async fn read_history_normalizes_additional_and_pahcer_result() {
        let dir = tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        fs::create_dir_all(storage.result_dir("123")).await.unwrap();
        storage
            .save_additional_result(
                "123",
                &AdditionalResultMetadata {
                    id: "123".to_string(),
                    args: vec!["--comment".to_string(), "memo".to_string()],
                    result_file_name: "result_20260314_151401.json".to_string(),
                    avg_score: 50.0,
                    avg_log_score: 1.5,
                    avg_relative_score: 75.0,
                    extra: Default::default(),
                },
            )
            .await
            .unwrap();
        fs::write(
            storage.result_path("123"),
            serde_json::json!({
                "start_time": "2026-03-14T15:14:01+09:00",
                "case_count": 2,
                "total_score": 100.0,
                "total_score_log10": 3.0,
                "total_relative_score": 150.0,
                "max_execution_time": 0.25,
                "comment": "memo",
                "tag_name": "nightly",
                "cases": [
                    { "seed": 0, "score": 40, "execution_time": 0.1, "error_message": "" },
                    { "seed": 1, "score": 60, "execution_time": 0.25, "error_message": "" }
                ]
            })
            .to_string(),
        )
        .await
        .unwrap();

        let history = storage.read_history().await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].comment, "memo");
        assert_eq!(history[0].tag, "nightly");
        assert_eq!(history[0].max_time, 250.0);
        assert_eq!(history[0].ac_case, 2);
    }

    #[tokio::test]
    async fn read_history_skips_entry_when_additional_is_missing() {
        let dir = tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        fs::create_dir_all(storage.result_dir("321")).await.unwrap();
        fs::write(
            storage.result_path("321"),
            "{}",
        )
        .await
        .unwrap();

        let history = storage.read_history().await.unwrap();
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn read_history_skips_new_format_when_primary_result_is_missing() {
        let dir = tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        fs::create_dir_all(storage.result_dir("555")).await.unwrap();
        storage
            .save_additional_result(
                "555",
                &AdditionalResultMetadata {
                    id: "555".to_string(),
                    args: vec!["--comment".to_string(), "memo".to_string()],
                    result_file_name: "result_20260314_151401.json".to_string(),
                    avg_score: 10.0,
                    avg_log_score: 1.0,
                    avg_relative_score: 20.0,
                    extra: Default::default(),
                },
            )
            .await
            .unwrap();

        let history = storage.read_history().await.unwrap();
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn read_history_skips_new_format_when_primary_result_is_broken() {
        let dir = tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        fs::create_dir_all(storage.result_dir("556")).await.unwrap();
        storage
            .save_additional_result(
                "556",
                &AdditionalResultMetadata {
                    id: "556".to_string(),
                    args: vec!["--comment".to_string(), "memo".to_string()],
                    result_file_name: "result_20260314_151401.json".to_string(),
                    avg_score: 10.0,
                    avg_log_score: 1.0,
                    avg_relative_score: 20.0,
                    extra: Default::default(),
                },
            )
            .await
            .unwrap();
        fs::write(
            storage.result_path("556"),
            "{broken json",
        )
        .await
        .unwrap();

        let history = storage.read_history().await.unwrap();
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn read_history_uses_local_datetime_string_when_start_time_is_missing() {
        let dir = tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        fs::create_dir_all(storage.result_dir("557")).await.unwrap();
        storage
            .save_additional_result(
                "557",
                &AdditionalResultMetadata {
                    id: "557".to_string(),
                    args: vec![],
                    result_file_name: "result_20260314_151401.json".to_string(),
                    avg_score: 10.0,
                    avg_log_score: 1.0,
                    avg_relative_score: 20.0,
                    extra: Default::default(),
                },
            )
            .await
            .unwrap();
        fs::write(
            storage.result_path("557"),
            serde_json::json!({
                "start_time": "",
                "case_count": 1,
                "total_score": 10.0,
                "total_score_log10": 1.0,
                "total_relative_score": 20.0,
                "max_execution_time": 0.1,
                "comment": "memo",
                "tag_name": "nightly",
                "cases": [
                    { "seed": 0, "score": 10, "execution_time": 0.1, "error_message": "" }
                ]
            })
            .to_string(),
        )
        .await
        .unwrap();

        let history = storage.read_history().await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].datetime, "2026-03-14T15:14:01");
    }

    #[tokio::test]
    async fn materialize_result_json_uses_relative_symlink_by_default_mode() {
        let dir = tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        fs::create_dir_all(storage.pahcer_json_dir()).await.unwrap();
        fs::write(
            storage.pahcer_result_path("result_20260314_151401.json"),
            "{\"comment\":\"memo\"}",
        )
        .await
        .unwrap();

        storage
            .materialize_result_json("123", "result_20260314_151401.json", ResultJsonMode::Symlink)
            .await
            .unwrap();

        let link_path = storage.result_path("123");
        let metadata = stdfs::symlink_metadata(&link_path).unwrap();
        assert!(metadata.file_type().is_symlink());
        let target = stdfs::read_link(&link_path).unwrap();
        assert_eq!(target, PathBuf::from("../../../pahcer/json/result_20260314_151401.json"));
        assert_eq!(
            fs::read_to_string(&link_path).await.unwrap(),
            "{\"comment\":\"memo\"}"
        );
    }

    #[tokio::test]
    async fn materialize_result_json_copies_file_when_configured() {
        let dir = tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        fs::create_dir_all(storage.pahcer_json_dir()).await.unwrap();
        fs::write(
            storage.pahcer_result_path("result_20260314_151401.json"),
            "{\"comment\":\"memo\"}",
        )
        .await
        .unwrap();

        storage
            .materialize_result_json("123", "result_20260314_151401.json", ResultJsonMode::Copy)
            .await
            .unwrap();

        let result_path = storage.result_path("123");
        let metadata = stdfs::symlink_metadata(&result_path).unwrap();
        assert!(!metadata.file_type().is_symlink());
        assert_eq!(
            fs::read_to_string(&result_path).await.unwrap(),
            "{\"comment\":\"memo\"}"
        );
    }

    #[tokio::test]
    async fn find_pahcer_result_for_run_uses_only_new_matching_files() {
        let dir = tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        fs::create_dir_all(storage.pahcer_json_dir()).await.unwrap();
        fs::write(
            storage.pahcer_result_path("result_20260314_151400.json"),
            serde_json::json!({
                "start_time": "2026-03-14T15:14:00+09:00",
                "case_count": 1,
                "total_score": 10.0,
                "total_score_log10": 1.0,
                "total_relative_score": 20.0,
                "max_execution_time": 0.1,
                "comment": "old",
                "tag_name": "old",
                "cases": [{ "seed": 0, "score": 10 }]
            })
            .to_string(),
        )
        .await
        .unwrap();

        let existing_files = storage.list_pahcer_result_files().await.unwrap();

        fs::write(
            storage.pahcer_result_path("result_20260314_151401.json"),
            serde_json::json!({
                "start_time": "2026-03-14T15:14:01+09:00",
                "case_count": 1,
                "total_score": 20.0,
                "total_score_log10": 1.3,
                "total_relative_score": 30.0,
                "max_execution_time": 0.2,
                "comment": "memo",
                "tag_name": "nightly",
                "cases": [{ "seed": 1, "score": 20 }]
            })
            .to_string(),
        )
        .await
        .unwrap();
        fs::write(
            storage.pahcer_result_path("result_20260314_151402.json"),
            serde_json::json!({
                "start_time": "2026-03-14T15:14:02+09:00",
                "case_count": 1,
                "total_score": 30.0,
                "total_score_log10": 1.4,
                "total_relative_score": 40.0,
                "max_execution_time": 0.3,
                "comment": "wrong",
                "tag_name": "nightly",
                "cases": [{ "seed": 2, "score": 30 }]
            })
            .to_string(),
        )
        .await
        .unwrap();

        let found = storage
            .find_pahcer_result_for_run(
                &existing_files,
                DateTime::parse_from_rfc3339("2026-03-14T06:14:01Z")
                    .unwrap()
                    .with_timezone(&Utc),
                "memo",
                "nightly",
            )
            .await
            .unwrap();

        let (file_name, result) = found.unwrap();
        assert_eq!(file_name, "result_20260314_151401.json");
        assert_eq!(result.comment, "memo");
    }

    #[tokio::test]
    async fn find_pahcer_result_for_run_detects_overwritten_same_name_file() {
        let dir = tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        fs::create_dir_all(storage.pahcer_json_dir()).await.unwrap();
        fs::write(
            storage.pahcer_result_path("result_20260314_151401.json"),
            serde_json::json!({
                "start_time": "2026-03-14T15:14:00+09:00",
                "case_count": 1,
                "total_score": 10.0,
                "total_score_log10": 1.0,
                "total_relative_score": 20.0,
                "max_execution_time": 0.1,
                "comment": "old",
                "tag_name": "nightly",
                "cases": [{ "seed": 0, "score": 10 }]
            })
            .to_string(),
        )
        .await
        .unwrap();

        let existing_files = storage.list_pahcer_result_files().await.unwrap();

        fs::write(
            storage.pahcer_result_path("result_20260314_151401.json"),
            serde_json::json!({
                "start_time": "2026-03-14T15:14:01+09:00",
                "case_count": 1,
                "total_score": 99.0,
                "total_score_log10": 1.99,
                "total_relative_score": 88.0,
                "max_execution_time": 0.2,
                "comment": "memo",
                "tag_name": "nightly",
                "cases": [{ "seed": 1, "score": 99 }]
            })
            .to_string(),
        )
        .await
        .unwrap();

        let found = storage
            .find_pahcer_result_for_run(
                &existing_files,
                DateTime::parse_from_rfc3339("2026-03-14T06:14:01Z")
                    .unwrap()
                    .with_timezone(&Utc),
                "memo",
                "nightly",
            )
            .await
            .unwrap();

        let (file_name, result) = found.unwrap();
        assert_eq!(file_name, "result_20260314_151401.json");
        assert_eq!(result.comment, "memo");
        assert_eq!(result.total_score, 99.0);
    }

    #[tokio::test]
    async fn find_pahcer_result_for_run_rejects_candidates_without_start_time() {
        let dir = tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        fs::create_dir_all(storage.pahcer_json_dir()).await.unwrap();
        fs::write(
            storage.pahcer_result_path("result_20260314_151401.json"),
            serde_json::json!({
                "start_time": "",
                "case_count": 1,
                "total_score": 20.0,
                "total_score_log10": 1.3,
                "total_relative_score": 30.0,
                "max_execution_time": 0.2,
                "comment": "memo",
                "tag_name": "nightly",
                "cases": [{ "seed": 1, "score": 20 }]
            })
            .to_string(),
        )
        .await
        .unwrap();

        let found = storage
            .find_pahcer_result_for_run(&HashMap::new(), Utc::now(), "memo", "nightly")
            .await
            .unwrap();

        assert!(found.is_none());
    }
}
