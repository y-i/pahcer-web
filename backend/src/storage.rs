use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use directories::BaseDirs;
use tokio::{fs, sync::Mutex};

use crate::{
    error::AppError,
    models::{GlobalConfig, JobMetadata, LocalConfig},
};

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

    pub fn result_dir(&self, timestamp: &str) -> PathBuf {
        self.results_dir().join(timestamp)
    }

    pub fn result_path(&self, timestamp: &str) -> PathBuf {
        self.result_dir(timestamp).join("result.json")
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

    pub async fn read_history(&self) -> Result<Vec<serde_json::Value>, AppError> {
        let results_dir = self.results_dir();
        let mut entries = match fs::read_dir(results_dir).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };

        let mut results = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path().join("result.json");
            if let Ok(content) = fs::read_to_string(&path).await
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(&content)
            {
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
}

fn datetime_of(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    value
        .get("datetime")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<DateTime<Utc>>().ok())
}
