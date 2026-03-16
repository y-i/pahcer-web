use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    process::Stdio,
};

use serde_json::Value;
use tokio::{io::AsyncReadExt, process::Command};

use crate::{
    error::AppError,
    models::{ComputedStats, ListResponse},
};

pub fn ensure_json_flag(args: &mut Vec<String>) {
    if !args.iter().any(|arg| arg == "--json" || arg == "-j") {
        args.push("--json".to_string());
    }
}

pub fn extract_comment_tag(args: &[String]) -> (String, String) {
    let mut comment = String::new();
    let mut tag = String::new();

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-c" | "--comment" => {
                comment = args.get(index + 1).cloned().unwrap_or_default();
                index += 1;
            }
            "-t" | "--tag" => {
                tag = args.get(index + 1).cloned().unwrap_or_default();
                index += 1;
            }
            _ => {}
        }
        index += 1;
    }

    (comment, tag)
}

pub async fn get_pahcer_list(base_dir: &Path, program: &Path) -> Result<ListResponse, AppError> {
    let output = Command::new(program)
        .arg("list")
        .current_dir(base_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        return Ok(ListResponse::default());
    }

    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    let parsed = parse_list_output(&raw);
    Ok(ListResponse { raw, parsed })
}

pub fn parse_list_output(raw: &str) -> Vec<BTreeMap<String, String>> {
    let mut lines = raw.lines().filter(|line| !line.trim().is_empty());
    let Some(header_line) = lines.next() else {
        return Vec::new();
    };
    let header: Vec<String> = header_line
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect();

    lines
        .map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            header
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    (
                        name.clone(),
                        columns.get(index).copied().unwrap_or_default().to_string(),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .collect()
}

pub fn collect_details(all_output: &str) -> Vec<Value> {
    let mut collected = Vec::new();

    for line in all_output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        match value {
            Value::Object(map) => {
                if has_result_shape(&Value::Object(map.clone())) {
                    collected.push(Value::Object(map));
                } else if let Some(Value::Array(items)) = map.get("results") {
                    collected.extend(items.iter().filter(|item| has_result_shape(item)).cloned());
                }
            }
            Value::Array(items) => {
                collected.extend(items.into_iter().filter(has_result_shape));
            }
            _ => {}
        }
    }

    dedupe_by_seed(collected)
}

pub fn compute_stats(details: Vec<Value>, all_output: &str) -> ComputedStats {
    if details.is_empty() {
        let fallback_score = regex::Regex::new(r"Score\s*=\s*([\d,]+)")
            .ok()
            .and_then(|regex| regex.captures(all_output))
            .and_then(|captures| captures.get(1).map(|value| value.as_str().replace(',', "")))
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0);

        let avg_log_score = if fallback_score > 0.0 {
            fallback_score.max(1.0).log10()
        } else {
            0.0
        };

        return ComputedStats {
            avg_score: fallback_score,
            avg_log_score,
            avg_relative_score: 0.0,
            max_time: 0.0,
            cases: usize::from(fallback_score > 0.0),
            ac_case: usize::from(fallback_score > 0.0),
            details,
        };
    }

    let cases = details.len();
    let avg_score = details.iter().map(score_of).sum::<f64>() / cases as f64;
    let avg_log_score = details
        .iter()
        .map(|detail| score_of(detail).max(1.0).log10())
        .sum::<f64>()
        / cases as f64;
    let avg_relative_score = details.iter().map(relative_score_of).sum::<f64>() / cases as f64;
    let max_time = details.iter().map(time_ms_of).fold(0.0, f64::max);
    let ac_case = details.iter().filter(|detail| is_ac(detail)).count();

    ComputedStats {
        avg_score,
        avg_log_score,
        avg_relative_score,
        max_time,
        cases,
        ac_case,
        details,
    }
}

pub async fn read_all_output(
    mut child: tokio::process::Child,
) -> Result<(String, String, i32), AppError> {
    let mut stdout = String::new();
    let mut stderr = String::new();

    if let Some(mut stream) = child.stdout.take() {
        stream.read_to_string(&mut stdout).await?;
    }
    if let Some(mut stream) = child.stderr.take() {
        stream.read_to_string(&mut stderr).await?;
    }

    let status = child.wait().await?;
    Ok((stdout, stderr, status.code().unwrap_or(1)))
}

fn has_result_shape(value: &Value) -> bool {
    value.get("seed").is_some() && value.get("score").is_some()
}

fn dedupe_by_seed(details: Vec<Value>) -> Vec<Value> {
    let mut by_seed = HashMap::new();
    for detail in details {
        if let Some(seed) = seed_of(&detail) {
            by_seed.insert(seed, detail);
        }
    }
    by_seed.into_values().collect()
}

pub fn seed_of(value: &Value) -> Option<String> {
    let seed = value.get("seed")?;
    match seed {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

pub fn score_of(value: &Value) -> f64 {
    value.get("score").and_then(number_like).unwrap_or_default()
}

fn relative_score_of(value: &Value) -> f64 {
    value
        .get("relative_score")
        .and_then(number_like)
        .unwrap_or_default()
}

fn time_ms_of(value: &Value) -> f64 {
    let execution_time = value.get("execution_time").and_then(number_like);
    let time = value.get("time").and_then(number_like);
    execution_time.or(time).unwrap_or_default() * 1000.0
}

fn is_ac(value: &Value) -> bool {
    let no_error = match value.get("error_message") {
        None | Some(Value::Null) => true,
        Some(Value::String(message)) => message.trim().is_empty(),
        Some(_) => false,
    };
    score_of(value) > 0.0 && no_error
}

fn number_like(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_details, compute_stats, ensure_json_flag, extract_comment_tag, parse_list_output,
    };

    #[test]
    fn ensure_json_flag_appends_when_missing() {
        let mut args = vec!["-c".to_string(), "memo".to_string()];
        ensure_json_flag(&mut args);
        assert!(args.contains(&"--json".to_string()));
    }

    #[test]
    fn extract_comment_and_tag() {
        let args = vec![
            "-c".to_string(),
            "hello world".to_string(),
            "--tag".to_string(),
            "nightly".to_string(),
        ];
        assert_eq!(
            extract_comment_tag(&args),
            ("hello world".to_string(), "nightly".to_string())
        );
    }

    #[test]
    fn collect_details_supports_multiple_shapes() {
        let output = r#"{"seed":1,"score":10,"execution_time":0.1}
[{"seed":2,"score":20,"execution_time":0.2}]
{"results":[{"seed":1,"score":30,"execution_time":0.3}]}"#;
        let details = collect_details(output);
        let stats = compute_stats(details, output);
        assert_eq!(stats.cases, 2);
        assert_eq!(stats.ac_case, 2);
        assert!(stats.avg_score > 20.0);
        assert_eq!(stats.max_time, 300.0);
    }

    #[test]
    fn empty_error_message_is_treated_as_ac() {
        let output = r#"{"seed":0,"score":10,"execution_time":0.1,"error_message":""}"#;
        let stats = compute_stats(collect_details(output), output);
        assert_eq!(stats.cases, 1);
        assert_eq!(stats.ac_case, 1);
    }

    #[test]
    fn parse_list_output_maps_rows() {
        let parsed = parse_list_output("seed score\n0 10\n1 20\n");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].get("seed").map(String::as_str), Some("0"));
    }
}
