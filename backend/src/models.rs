use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VisualizerPosition {
    Left,
    #[default]
    Right,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScoreType {
    Raw,
    Max,
    Min,
    RankMax,
    RankMin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TestRunOptions {
    pub shuffle: bool,
    pub setting_file: String,
    pub freeze_best_scores: bool,
    pub no_compile: bool,
}

impl Default for TestRunOptions {
    fn default() -> Self {
        Self {
            shuffle: false,
            setting_file: "pahcer_config.toml".to_string(),
            freeze_best_scores: false,
            no_compile: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GlobalConfig {
    pub visualizer_position: VisualizerPosition,
    pub visualizer_url: Option<String>,
    pub default_seed: u32,
    pub default_scale: f64,
    pub test_run_options: Option<TestRunOptions>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            visualizer_position: VisualizerPosition::Right,
            visualizer_url: None,
            default_seed: 0,
            default_scale: 1.0,
            test_run_options: None,
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct LocalConfig {
    pub visualizer_url: Option<String>,
    pub default_score_type: Option<ScoreType>,
    pub input_param_names: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobMetadata {
    pub id: String,
    pub datetime: String,
    pub command: String,
    pub args: Vec<String>,
    pub status: JobStatus,
    pub output_file: Option<String>,
    pub result: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ListResponse {
    pub raw: String,
    pub parsed: Vec<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    pub global: GlobalConfig,
    pub local: LocalConfig,
    pub problem_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StreamMessage {
    Stdout { data: String },
    Stderr { data: String },
    Exit { code: i32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredResult {
    pub id: String,
    pub datetime: String,
    pub args: Vec<String>,
    pub comment: String,
    pub tag: String,
    pub avg_score: f64,
    pub avg_log_score: f64,
    pub avg_relative_score: f64,
    pub max_time: f64,
    pub cases: usize,
    #[serde(rename = "ACcase")]
    pub ac_case: usize,
    pub details: Vec<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStats {
    pub avg_score: f64,
    pub avg_log_score: f64,
    pub avg_relative_score: f64,
    pub max_time: f64,
    pub cases: usize,
    pub ac_case: usize,
    pub details: Vec<Value>,
}
