use std::{collections::HashMap, path::Path};

use tokio::fs;

use crate::{
    error::AppError,
    models::{GlobalConfig, LocalConfig},
    pahcer::{score_of, seed_of},
    storage::Storage,
};

pub async fn get_seeds(base_dir: &Path) -> Result<Vec<String>, AppError> {
    let seeds_path = base_dir.join("tools").join("seeds.txt");
    let input_dir = base_dir.join("tools").join("in");

    if let Ok(content) = fs::read_to_string(&seeds_path).await {
        let seeds = content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        return Ok(seeds);
    }

    let mut seeds = Vec::new();
    let mut entries = match fs::read_dir(&input_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(seeds),
        Err(error) => return Err(error.into()),
    };

    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if let Some(stem) = file_name.strip_suffix(".txt")
            && stem.chars().all(|ch| ch.is_ascii_digit())
        {
            seeds.push(stem.parse::<u32>().unwrap_or_default().to_string());
        }
    }

    seeds.sort_by_key(|seed| seed.parse::<u32>().unwrap_or_default());
    Ok(seeds)
}

pub async fn download_analysis_html(
    client: &reqwest::Client,
    storage: &Storage,
) -> Result<String, AppError> {
    let url = "https://img.atcoder.jp/ahc_standings/index.html";
    let response = client.get(url).send().await?;
    let response = response.error_for_status()?;
    let html = response.text().await?;
    if let Some(parent) = storage.analysis_path().parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(storage.analysis_path(), &html).await?;
    Ok(html)
}

pub async fn generate_input_csv(
    base_dir: &Path,
    local_config: &LocalConfig,
) -> Result<String, AppError> {
    let seeds = get_seeds(base_dir).await?;
    if seeds.is_empty() {
        return Ok("file,seed\n".to_string());
    }

    let input_dir = base_dir.join("tools").join("in");
    let param_names = if let Some(names) = &local_config.input_param_names {
        split_param_names(names)
    } else {
        infer_param_names(&input_dir).await?
    };

    let mut csv = String::from("file,seed");
    if !param_names.is_empty() {
        csv.push(',');
        csv.push_str(&param_names.join(","));
    }
    csv.push('\n');

    for (index, seed) in seeds.iter().enumerate() {
        let file_name = format!("{index:04}.txt");
        let file_path = input_dir.join(&file_name);
        let params = match fs::read_to_string(&file_path).await {
            Ok(content) => content
                .lines()
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };

        csv.push_str(&file_name);
        csv.push(',');
        csv.push_str(seed);
        if !param_names.is_empty() {
            csv.push(',');
            if params.is_empty() {
                csv.push_str(&vec![String::new(); param_names.len()].join(","));
            } else {
                csv.push_str(&params.join(","));
            }
        }
        csv.push('\n');
    }

    Ok(csv)
}

pub async fn generate_result_csv(
    storage: &Storage,
    global_config: &GlobalConfig,
    local_config: &LocalConfig,
) -> Result<String, AppError> {
    let seeds = get_seeds(storage.base_dir()).await?;
    let visualizer_url = local_config
        .visualizer_url
        .clone()
        .or_else(|| global_config.visualizer_url.clone())
        .unwrap_or_default();

    let mut csv = format!("raw,1000000000,{visualizer_url}\n");
    let history = storage.read_history().await?;

    for result in history {
        let author = (!result.tag.trim().is_empty())
            .then_some(result.tag.as_str())
            .or_else(|| (!result.comment.trim().is_empty()).then_some(result.comment.as_str()))
            .or_else(|| (!result.datetime.trim().is_empty()).then_some(result.datetime.as_str()))
            .unwrap_or("unknown")
            .replace(',', " ")
            .replace('"', "")
            .trim()
            .to_string();

        let score_map = result
            .details
            .iter()
            .fold(HashMap::new(), |mut map, detail| {
                if let Some(seed) = seed_of(detail) {
                    map.insert(seed, score_of(detail));
                }
                map
            });

        let mut scores = Vec::with_capacity(seeds.len());
        for seed in &seeds {
            let score = score_map.get(seed).copied().unwrap_or(-1.0);
            if score.fract() == 0.0 {
                scores.push(format!("{score:.0}"));
            } else {
                scores.push(score.to_string());
            }
        }

        csv.push_str(&author);
        if !scores.is_empty() {
            csv.push(',');
            csv.push_str(&scores.join(","));
        }
        csv.push('\n');
    }

    Ok(csv)
}

fn split_param_names(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

async fn infer_param_names(input_dir: &Path) -> Result<Vec<String>, AppError> {
    let first_file = input_dir.join("0000.txt");
    let content = match fs::read_to_string(first_file).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    let first_line = content.lines().next().unwrap_or_default();
    let columns = first_line.split_whitespace().count();
    let defaults = ["N", "M", "L", "K", "T", "S"];

    Ok((0..columns)
        .map(|index| defaults.get(index).copied().unwrap_or("p"))
        .enumerate()
        .map(|(index, name)| {
            if name == "p" {
                format!("p_{index}")
            } else {
                name.to_string()
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tempfile::tempdir;
    use tokio::fs;

    use crate::{
        models::{
            GlobalConfig, LocalConfig, ResultJsonMode, VisualizerInitialScrollPosition,
            VisualizerPosition,
        },
        storage::Storage,
    };

    use super::{generate_input_csv, generate_result_csv};

    #[tokio::test]
    async fn input_csv_uses_configured_param_names() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("tools/in"))
            .await
            .unwrap();
        fs::write(dir.path().join("tools/seeds.txt"), "10\n11\n")
            .await
            .unwrap();
        fs::write(dir.path().join("tools/in/0000.txt"), "1 2\n")
            .await
            .unwrap();
        fs::write(dir.path().join("tools/in/0001.txt"), "3 4\n")
            .await
            .unwrap();

        let csv = generate_input_csv(
            dir.path(),
            &LocalConfig {
                input_param_names: Some("A,B".to_string()),
                ..LocalConfig::default()
            },
        )
        .await
        .unwrap();

        assert!(csv.contains("file,seed,A,B"));
        assert!(csv.contains("0001.txt,11,3,4"));
    }

    #[tokio::test]
    async fn result_csv_uses_visualizer_url_and_scores() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("tools")).await.unwrap();
        fs::write(dir.path().join("tools/seeds.txt"), "0\n1\n")
            .await
            .unwrap();
        let storage = Storage::new(dir.path()).unwrap();
        fs::create_dir_all(storage.result_dir("100")).await.unwrap();
        fs::write(
            storage.additional_path("100"),
            serde_json::json!({
                "id": "100",
                "args": ["-c", "memo"],
                "resultFileName": "result_20260314_151401.json",
                "avgScore": 15,
                "avgLogScore": 1,
                "avgRelativeScore": 80
            })
            .to_string(),
        )
        .await
        .unwrap();
        fs::write(
            storage.result_path("100"),
            serde_json::json!({
                "start_time": "2026-03-14T15:14:01+09:00",
                "case_count": 2,
                "total_score": 30,
                "total_score_log10": 2,
                "total_relative_score": 160,
                "max_execution_time": 0.1,
                "comment": "memo",
                "tag_name": "tag1",
                "cases": [
                    { "seed": 0, "score": 10 },
                    { "seed": 1, "score": 20 }
                ]
            })
            .to_string(),
        )
        .await
        .unwrap();

        let csv = generate_result_csv(
            &storage,
            &GlobalConfig {
                visualizer_position: VisualizerPosition::Right,
                visualizer_initial_scroll_position: VisualizerInitialScrollPosition::Bottom,
                visualizer_url: Some("https://example.com/vis.html".to_string()),
                result_json_mode: ResultJsonMode::Symlink,
                default_seed: 0,
                default_scale: 1.0,
                test_run_options: None,
                extra: BTreeMap::new(),
            },
            &LocalConfig::default(),
        )
        .await
        .unwrap();

        assert!(csv.starts_with("raw,1000000000,https://example.com/vis.html"));
        assert!(csv.contains("tag1,10,20"));
    }
}
