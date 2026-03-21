use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InitializationState {
    Initialized,
    Uninitialized,
    Invalid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InitObjective {
    Max,
    Min,
}

impl InitObjective {
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Max => "max",
            Self::Min => "min",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InitLanguage {
    Cpp,
    Python,
    Rust,
    Go,
}

impl InitLanguage {
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Cpp => "cpp",
            Self::Python => "python",
            Self::Rust => "rust",
            Self::Go => "go",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VisualizerPosition {
    Left,
    #[default]
    Right,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VisualizerInitialScrollPosition {
    Top,
    #[default]
    Bottom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResultJsonMode {
    #[default]
    Symlink,
    Copy,
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
    pub visualizer_initial_scroll_position: VisualizerInitialScrollPosition,
    pub visualizer_url: Option<String>,
    pub result_json_mode: ResultJsonMode,
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
            visualizer_initial_scroll_position: VisualizerInitialScrollPosition::Bottom,
            visualizer_url: None,
            result_json_mode: ResultJsonMode::Symlink,
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
    pub initialization_state: InitializationState,
    pub initialization_error: Option<String>,
    pub problem_name: Option<String>,
    pub base_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitRequest {
    pub problem: String,
    pub objective: InitObjective,
    pub language: InitLanguage,
    pub interactive: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct PahcerCaseResult {
    pub seed: Value,
    pub score: f64,
    pub relative_score: Option<f64>,
    pub execution_time: Option<f64>,
    pub error_message: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct PahcerResultFile {
    pub start_time: String,
    pub case_count: usize,
    pub total_score: f64,
    pub total_score_log10: f64,
    pub total_relative_score: f64,
    pub max_execution_time: f64,
    pub comment: String,
    pub tag_name: Option<String>,
    pub wa_seeds: Vec<Value>,
    pub cases: Vec<PahcerCaseResult>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AdditionalResultMetadata {
    pub id: String,
    pub args: Vec<String>,
    pub result_file_name: String,
    pub avg_score: f64,
    pub avg_log_score: f64,
    pub avg_relative_score: f64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
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
