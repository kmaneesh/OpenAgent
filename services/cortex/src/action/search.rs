use crate::action::catalog::{ActionCatalog, ActionEntry, ActionKind};
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct SearchQuery {
    pub query: String,
    pub kind: Option<String>,
    pub owner: Option<String>,
    pub limit: usize,
    pub include_params: bool,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub total_matches: usize,
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub kind: String,
    pub owner: String,
    pub runtime: String,
    pub manifest_path: String,
    pub name: String,
    pub summary: String,
    pub required: Vec<String>,
    pub param_names: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub steps: Vec<String>,
    pub constraints: Vec<String>,
    pub completion_criteria: Vec<String>,
    pub guidance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

pub fn search_catalog(catalog: &ActionCatalog, query: SearchQuery) -> SearchResponse {
    let owner_filter = query.owner.as_ref().map(|v| v.to_lowercase());
    let kind_filter = query.kind.as_ref().map(|v| v.to_lowercase());
    let terms = tokenize(&query.query);
    let mut ranked = Vec::new();

    for entry in catalog.entries() {
        if let Some(filter) = &owner_filter {
            if entry.owner.to_lowercase() != *filter {
                continue;
            }
        }
        if let Some(filter) = &kind_filter {
            if entry.kind.as_str() != filter {
                continue;
            }
        }

        let score = score_entry(entry, &terms);
        if score <= 0 {
            continue;
        }
        ranked.push((score, entry));
    }

    ranked.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.name.cmp(&right.name))
    });

    let total_matches = ranked.len();
    let results = ranked
        .into_iter()
        .take(query.limit)
        .map(|(_, entry)| SearchResult {
            kind: entry.kind.as_str().to_string(),
            owner: entry.owner.clone(),
            runtime: entry.runtime.clone(),
            manifest_path: entry.manifest_path.display().to_string(),
            name: entry.name.clone(),
            summary: entry.summary.clone(),
            required: entry.required.clone(),
            param_names: entry.param_names.clone(),
            allowed_tools: entry.allowed_tools.clone(),
            steps: entry.steps.clone(),
            constraints: entry.constraints.clone(),
            completion_criteria: entry.completion_criteria.clone(),
            guidance: entry.guidance.clone(),
            params: query.include_params.then(|| entry.params.clone()),
        })
        .collect();

    SearchResponse {
        query: query.query,
        total_matches,
        results,
    }
}

fn score_entry(entry: &ActionEntry, terms: &[String]) -> i32 {
    if terms.is_empty() {
        return 1;
    }

    let name = entry.name.to_lowercase();
    let owner = entry.owner.to_lowercase();
    let summary = entry.summary.to_lowercase();
    let kind = entry.kind.as_str();
    let mut score = 0;

    for term in terms {
        if name == *term {
            score += 120;
        }
        if name.starts_with(term) {
            score += 80;
        }
        if name.contains(term) {
            score += 60;
        }
        if owner == *term || owner.contains(term) {
            score += 40;
        }
        if summary.contains(term) {
            score += 20;
        }
        if entry
            .param_names
            .iter()
            .any(|value| value.eq_ignore_ascii_case(term))
        {
            score += 12;
        }
        if entry
            .allowed_tools
            .iter()
            .any(|value| value.to_lowercase().contains(term))
        {
            score += 12;
        }
        if entry
            .steps
            .iter()
            .any(|value| value.to_lowercase().contains(term))
        {
            score += 10;
        }
        if kind.contains(term) {
            score += 8;
        }
        if entry.search_blob.contains(term) {
            score += 5;
        }
        if matches!(entry.kind, ActionKind::SkillGuidance) && looks_procedural(term) {
            score += 6;
        }
    }

    score
}

fn looks_procedural(term: &str) -> bool {
    matches!(
        term,
        "how" | "workflow" | "steps" | "automate" | "process" | "login" | "browser" | "form"
    )
}

fn tokenize(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '_' && c != '-')
        .filter(|part| !part.is_empty())
        .map(|part| part.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{search_catalog, SearchQuery};
    use crate::action::catalog::ActionCatalog;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn prefers_skill_for_procedural_browser_query() {
        let temp = unique_temp_dir("cortex-action-search");
        let services_dir = temp.join("services").join("browser");
        let skills_dir = temp.join("skills").join("browser-skill");
        fs::create_dir_all(&services_dir).unwrap();
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(
            services_dir.join("service.json"),
            r#"{
              "name":"browser",
              "runtime":"rust",
              "tools":[
                {"name":"browser.open","description":"Open url","params":{"type":"object","properties":{"url":{"type":"string"}}}}
              ]
            }"#,
        )
        .unwrap();
        fs::write(
            skills_dir.join("SKILL.md"),
            r#"---
name: browser-skill
description: Browser workflow guidance
allowed-tools:
  - browser.open
---

1. Open the page
2. Snapshot before interacting
"#,
        )
        .unwrap();
        let catalog = ActionCatalog::discover_from_root(&temp).unwrap();

        let result = search_catalog(
            &catalog,
            SearchQuery {
                query: "browser workflow steps".to_string(),
                kind: None,
                owner: None,
                limit: 5,
                include_params: false,
            },
        );

        assert_eq!(result.results[0].kind, "skill_guidance");
        fs::remove_dir_all(temp).unwrap();
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("{prefix}-{now}"))
    }
}
