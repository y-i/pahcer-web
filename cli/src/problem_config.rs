use std::path::Path;

use serde::Deserialize;
use tokio::fs;

#[derive(Debug, Deserialize)]
struct ProblemConfig {
    problem: Option<ProblemSection>,
}

#[derive(Debug, Deserialize)]
struct ProblemSection {
    problem_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProblemConfigState {
    Initialized { problem_name: String },
    Uninitialized,
    Invalid { message: String },
}

pub async fn inspect_problem_config(base_dir: &Path) -> ProblemConfigState {
    let path = base_dir.join("pahcer_config.toml");
    match fs::read_to_string(path).await {
        Ok(content) => match toml::from_str::<ProblemConfig>(&content) {
            Ok(config) => validate_problem_config(config),
            Err(error) => ProblemConfigState::Invalid {
                message: error.to_string(),
            },
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ProblemConfigState::Uninitialized
        }
        Err(error) => ProblemConfigState::Invalid {
            message: error.to_string(),
        },
    }
}

fn validate_problem_config(config: ProblemConfig) -> ProblemConfigState {
    let Some(problem) = config.problem else {
        return missing_field("problem.problem_name");
    };

    let Some(problem_name) = non_empty(problem.problem_name) else {
        return missing_field("problem.problem_name");
    };

    ProblemConfigState::Initialized { problem_name }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn missing_field(field: &str) -> ProblemConfigState {
    ProblemConfigState::Invalid {
        message: format!("Missing required field: {field}"),
    }
}
