use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
};

use regex::Regex;
use scraper::{Html, Selector};
use tokio::fs;
use url::Url;

use crate::error::AppError;

pub async fn download_recursive(
    client: &reqwest::Client,
    url: &str,
    dest_dir: &Path,
) -> Result<(), AppError> {
    let root_url = url.to_string();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([(root_url.clone(), PathBuf::from("index.html"))]);

    if let Ok(base_url) = Url::parse(url)
        && let Some(stem) = Path::new(base_url.path())
            .file_stem()
            .and_then(|value| value.to_str())
        && let Some(parent) = base_url.path_segments().map(|segments| {
            let items = segments.collect::<Vec<_>>();
            if items.len() > 1 {
                items[..items.len() - 1].join("/")
            } else {
                String::new()
            }
        })
    {
        let base_prefix = if parent.is_empty() {
            format!(
                "{}://{}/",
                base_url.scheme(),
                base_url.host_str().unwrap_or_default()
            )
        } else {
            format!(
                "{}://{}/{parent}/",
                base_url.scheme(),
                base_url.host_str().unwrap_or_default()
            )
        };
        queue.push_back((
            format!("{base_prefix}{stem}.js"),
            PathBuf::from(format!("{stem}.js")),
        ));
        queue.push_back((
            format!("{base_prefix}{stem}_bg.wasm"),
            PathBuf::from(format!("{stem}_bg.wasm")),
        ));
    }

    fs::create_dir_all(dest_dir).await?;
    let css_url_regex = Regex::new(r#"url\(['\"]?([^'\")]+)['\"]?\)"#)
        .map_err(|error| AppError::Internal(error.to_string()))?;
    let src_selector =
        Selector::parse("[src], [href]").map_err(|error| AppError::Internal(error.to_string()))?;

    while let Some((current_url, relative_path)) = queue.pop_front() {
        if !visited.insert(current_url.clone()) {
            continue;
        }

        let response = match client.get(&current_url).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => response,
                Err(error) if current_url == root_url => return Err(error.into()),
                Err(_) => continue,
            },
            Err(error) if current_url == root_url => return Err(error.into()),
            Err(_) => continue,
        };

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let normalized_content_type = content_type.to_ascii_lowercase();
        let is_html_content = normalized_content_type.contains("text/html");
        let is_css_content = normalized_content_type.contains("text/css");
        let is_root_resource = current_url == root_url;
        if !is_root_resource && is_html_content {
            continue;
        }

        let bytes = response.bytes().await?;
        let destination = dest_dir.join(&relative_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&destination, &bytes).await?;

        if !is_html_content && !is_css_content {
            continue;
        }

        let text = String::from_utf8_lossy(&bytes);
        if is_html_content {
            let document = Html::parse_document(&text);
            for element in document.select(&src_selector) {
                for attribute in ["src", "href"] {
                    if let Some(value) = element.value().attr(attribute) {
                        push_resource(&mut queue, &visited, &current_url, value);
                    }
                }
            }
        }

        for capture in css_url_regex.captures_iter(&text) {
            if let Some(value) = capture.get(1) {
                push_resource(&mut queue, &visited, &current_url, value.as_str());
            }
        }
    }

    Ok(())
}

pub fn inject_output_loader(html: &str) -> String {
    const SCRIPT: &str = r#"<script>
(async function() {
    const params = new URLSearchParams(window.location.search);
    const initialScrollPosition = params.get('initial_scroll') === 'top' ? 'top' : 'bottom';
    const updateOutputWaitTimeoutMs = 10000;
    const updateOutputRetryIntervalMs = 50;
    const applyInitialScroll = () => {
        if (initialScrollPosition === 'bottom') {
            window.scrollTo(0, document.body.scrollHeight);
        }
    };

    const wait = (ms) => new Promise((resolve) => {
        window.setTimeout(resolve, ms);
    });

    const waitForUpdateOutput = async () => {
        const deadline = Date.now() + updateOutputWaitTimeoutMs;

        while (typeof window.updateOutput !== 'function' && Date.now() < deadline) {
            await wait(updateOutputRetryIntervalMs);
        }

        return typeof window.updateOutput === 'function';
    };

    const recalculateOutputAndWaitForLayout = async () => {
        if (!await waitForUpdateOutput()) {
            return;
        }

        window.updateOutput();

        await new Promise((resolve) => {
            requestAnimationFrame(() => {
                requestAnimationFrame(resolve);
            });
        });
    };

    try {
        const outputUrl = params.get('output_url');
        if (outputUrl) {
            const response = await fetch(outputUrl);
            if (response.ok) {
                const text = await response.text();
                const el = document.getElementById('output') || document.getElementById('input') || document.querySelector('textarea');
                if (el) {
                    el.value = text;
                }
            }
        }
    } catch (error) {
        console.error('Failed to inject output:', error);
    }

    await recalculateOutputAndWaitForLayout();
    applyInitialScroll();
})();
</script>"#;

    if html.contains("</body>") {
        html.replacen("</body>", &format!("{SCRIPT}</body>"), 1)
    } else {
        format!("{html}{SCRIPT}")
    }
}

fn push_resource(
    queue: &mut VecDeque<(String, PathBuf)>,
    visited: &HashSet<String>,
    current_url: &str,
    raw_path: &str,
) {
    if raw_path.starts_with("http://")
        || raw_path.starts_with("https://")
        || raw_path.starts_with("//")
        || raw_path.starts_with("data:")
    {
        if (raw_path.starts_with("http://") || raw_path.starts_with("https://"))
            && let Some(relative_path) = relative_path_from_url(raw_path)
            && !visited.contains(raw_path)
        {
            queue.push_back((raw_path.to_string(), relative_path));
        }
        return;
    }

    let Ok(url) = Url::parse(current_url).and_then(|base| base.join(raw_path)) else {
        return;
    };
    let resolved = url.to_string();
    if visited.contains(&resolved) {
        return;
    }
    if let Some(relative_path) = relative_path_from_url(url.as_str()) {
        queue.push_back((resolved, relative_path));
    }
}

fn relative_path_from_url(url: &str) -> Option<PathBuf> {
    let url = Url::parse(url).ok()?;
    let path = url.path().trim_start_matches('/');
    if path.is_empty() {
        Some(PathBuf::from("index.html"))
    } else {
        Some(PathBuf::from(path))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashSet, VecDeque};

    use tempfile::tempdir;

    use super::{download_recursive, inject_output_loader, push_resource};

    #[test]
    fn injects_script_before_body_end() {
        let html = "<html><body><textarea></textarea></body></html>";
        let injected = inject_output_loader(html);
        let update_output_index = injected.find("window.updateOutput();").unwrap();
        let apply_scroll_index = injected.find("applyInitialScroll();").unwrap();

        assert!(injected.contains("output_url"));
        assert!(injected.contains("initial_scroll"));
        assert!(injected.contains("applyInitialScroll"));
        assert!(injected.contains("if (initialScrollPosition === 'bottom')"));
        assert!(injected.contains("window.scrollTo(0, document.body.scrollHeight);"));
        assert!(injected.contains("waitForUpdateOutput"));
        assert!(injected.contains("recalculateOutputAndWaitForLayout"));
        assert!(!injected.contains("MutationObserver"));
        assert!(!injected.contains("ResizeObserver"));
        assert!(!injected.contains("beginInitialScrollStabilization"));
        assert!(!injected.contains("layoutSettlingTimeoutMs"));
        assert!(!injected.contains("maxStabilizationDurationMs"));
        assert!(!injected.contains("restartSettlingTimer"));
        assert!(!injected.contains("lastProgrammaticScrollTop"));
        assert!(!injected.contains("dispatchEvent(new Event('input'"));
        assert!(update_output_index < apply_scroll_index);
        assert!(injected.contains("</body>"));
    }

    #[tokio::test]
    async fn returns_error_when_entrypoint_cannot_be_downloaded() {
        let dir = tempdir().unwrap();
        let client = reqwest::Client::new();
        let result = download_recursive(&client, "://invalid", dir.path()).await;
        assert!(result.is_err());
    }

    #[test]
    fn queues_absolute_assets_and_relative_urls() {
        let mut queue = VecDeque::new();
        let visited = HashSet::new();

        push_resource(
            &mut queue,
            &visited,
            "https://example.com/visualizer/index.html",
            "https://cdn.example.com/assets/foo.js",
        );
        push_resource(
            &mut queue,
            &visited,
            "https://example.com/visualizer/index.html",
            "./app.css",
        );

        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].0, "https://cdn.example.com/assets/foo.js");
        assert_eq!(queue[1].0, "https://example.com/visualizer/app.css");
    }
}
