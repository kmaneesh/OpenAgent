//! SearXNG search client — async reqwest.
//!
//! GETs `/search?q=<query>&format=json` on the configured SearXNG instance.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const TIMEOUT_SECS: u64 = 10;

/// A single search result returned to the LLM.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub snippet: String,
}

/// Query SearXNG and return up to `max_results` results.
pub async fn search(base_url: &str, query: &str, max_results: usize) -> Result<Vec<SearchResult>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .build()
        .context("failed to build HTTP client")?;

    let endpoint = format!("{}/search", base_url.trim_end_matches('/'));

    let response = client
        .get(&endpoint)
        .query(&[("q", query), ("format", "json")])
        .send()
        .await
        .with_context(|| format!("SearXNG request to {endpoint} failed"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("SearXNG request failed with status {}: {}", status, body));
    }

    let text = response.text().await.context("failed to read SearXNG response body")?;
    let resp: serde_json::Value = match serde_json::from_str(&text) {
        Ok(val) => val,
        Err(e) => {
            // Log the raw response for debugging
            eprintln!("[SearXNG] JSON parse error: {}\nRaw response: {}", e, text);
            return Err(anyhow::anyhow!("failed to parse SearXNG JSON response: {}\nRaw response: {}", e, text));
        }
    };

    let results = resp["results"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("SearXNG response missing 'results' field. Full response: {}", resp))?
        .iter()
        .take(max_results)
        .filter_map(|r| {
            // Robust mapping: handle both 'content' and 'snippet', fallback to empty string
            let url = r["url"].as_str().unwrap_or("").to_string();
            let title = r["title"].as_str().unwrap_or("").to_string();
            let snippet = r["content"].as_str()
                .or_else(|| r["snippet"].as_str())
                .unwrap_or("").to_string();
            if url.is_empty() {
                None
            } else {
                Some(SearchResult { url, title, snippet })
            }
        })
        .collect();

    Ok(results)
}
