use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::path::PathBuf;

const DEFAULT_CONFIG_PATHS: &[&str] = &["config/openagent.yaml", "config/openagent.yml"];
const DEFAULT_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) ",
    "AppleWebKit/537.36 (KHTML, like Gecko) ",
    "Chrome/134.0.0.0 Safari/537.36"
);

#[derive(Clone, Debug)]
pub struct BrowserDefaults {
    pub identity: BrowserIdentity,
}

impl BrowserDefaults {
    pub fn load() -> Result<Self> {
        let path = resolve_config_path()?;
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read browser config {}", path.display()))?;
        let cfg: OpenAgentConfig = serde_yaml::from_str(&raw)
            .with_context(|| format!("failed to parse browser config {}", path.display()))?;
        Ok(Self {
            identity: cfg.browser.identity.into_identity(),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct BrowserIdentity {
    pub user_agent: String,
    pub color_scheme: String,
    pub headed: bool,
    pub viewport_width: Option<u32>,
    pub viewport_height: Option<u32>,
    pub extra_headers: Map<String, Value>,
    pub launch_args: Vec<String>,
}

impl BrowserIdentity {
    #[must_use]
    pub fn normalized(self) -> Self {
        let user_agent = if self.user_agent.trim().is_empty() {
            DEFAULT_USER_AGENT.to_string()
        } else {
            self.user_agent.trim().to_string()
        };
        let color_scheme = match self.color_scheme.trim() {
            "dark" | "light" | "no-preference" => self.color_scheme.trim().to_string(),
            _ => "light".to_string(),
        };
        Self {
            user_agent,
            color_scheme,
            headed: self.headed,
            viewport_width: self.viewport_width.or(Some(1440)),
            viewport_height: self.viewport_height.or(Some(900)),
            extra_headers: self.extra_headers,
            launch_args: self
                .launch_args
                .into_iter()
                .map(|arg| arg.trim().to_string())
                .filter(|arg| !arg.is_empty())
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct OpenAgentConfig {
    #[serde(default)]
    browser: BrowserSection,
}

#[derive(Debug, Default, Deserialize)]
struct BrowserSection {
    #[serde(default)]
    identity: BrowserIdentityYaml,
}

#[derive(Debug, Default, Deserialize)]
struct BrowserIdentityYaml {
    #[serde(default)]
    user_agent: String,
    #[serde(default)]
    color_scheme: String,
    #[serde(default)]
    headed: bool,
    #[serde(default)]
    viewport_width: Option<u32>,
    #[serde(default)]
    viewport_height: Option<u32>,
    #[serde(default)]
    extra_headers: Map<String, Value>,
    #[serde(default)]
    launch_args: Vec<String>,
}

impl BrowserIdentityYaml {
    fn into_identity(self) -> BrowserIdentity {
        BrowserIdentity {
            user_agent: self.user_agent,
            color_scheme: self.color_scheme,
            headed: self.headed,
            viewport_width: self.viewport_width,
            viewport_height: self.viewport_height,
            extra_headers: self.extra_headers,
            launch_args: self.launch_args,
        }
        .normalized()
    }
}

fn resolve_config_path() -> Result<PathBuf> {
    if let Ok(path) = env::var("OPENAGENT_CONFIG_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }

    let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    DEFAULT_CONFIG_PATHS
        .iter()
        .map(|candidate| root.join(candidate))
        .find(|path| path.is_file())
        .ok_or_else(|| anyhow::anyhow!("could not find openagent config file"))
}

#[cfg(test)]
mod tests {
    use super::BrowserIdentity;

    #[test]
    fn browser_identity_applies_defaults() {
        let identity = BrowserIdentity::default().normalized();
        assert!(identity.user_agent.contains("Macintosh"));
        assert_eq!(identity.color_scheme, "light");
        assert_eq!(identity.viewport_width, Some(1440));
        assert_eq!(identity.viewport_height, Some(900));
    }
}
