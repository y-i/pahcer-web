use std::path::Path;

use serde::Deserialize;
use tokio::fs;

use crate::error::AppError;

#[derive(Debug, Deserialize)]
struct ProblemConfig {
    problem_name: Option<String>,
}

pub async fn read_problem_name(base_dir: &Path) -> Result<String, AppError> {
    let path = base_dir.join("pahcer_config.toml");
    match fs::read_to_string(path).await {
        Ok(content) => {
            let config: ProblemConfig = toml::from_str(&content)?;
            Ok(config.problem_name.unwrap_or_else(|| "unknown".to_string()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("unknown".to_string()),
        Err(error) => Err(error.into()),
    }
}
