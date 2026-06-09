use crate::network::NetworkMode;
use crate::plan::{BuildOptions, Engine, EngineChoice};
use crate::{OrbitError, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileConfig>,
    #[serde(default)]
    pub excluded_extensions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProfileConfig {
    pub engine: Option<String>,
    pub network: Option<String>,
    pub image: Option<String>,
    pub proxy_image: Option<String>,
    #[serde(default)]
    pub allow_domains: Vec<String>,
    pub proxy: Option<String>,
    pub interactive: Option<bool>,
    #[serde(default)]
    pub excluded_extensions: Vec<String>,
}

pub fn merge_user_config(profile_name: Option<&str>, options: &mut BuildOptions) -> Result<()> {
    let Some(path) = user_config_path() else {
        return Ok(());
    };
    if path.is_file() {
        merge_config_with_policy(&path, profile_name, options, false)?;
    }
    Ok(())
}

pub fn user_config_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/orbit/config.json"))
}

pub fn merge_config(
    path: &Path,
    profile_name: Option<&str>,
    options: &mut BuildOptions,
) -> Result<()> {
    merge_config_with_policy(path, profile_name, options, true)
}

fn merge_config_with_policy(
    path: &Path,
    profile_name: Option<&str>,
    options: &mut BuildOptions,
    require_profile: bool,
) -> Result<()> {
    let raw = std::fs::read_to_string(path)?;
    let config: ConfigFile = serde_json::from_str(&raw)?;
    merge_excluded_extensions(
        &mut options.excluded_extensions,
        &config.excluded_extensions,
    )?;
    let Some(name) = profile_name else {
        return Ok(());
    };
    let Some(profile) = config.profiles.get(name) else {
        if require_profile {
            return Err(OrbitError::Usage(format!(
                "profile `{name}` not found in `{}`",
                path.display()
            )));
        }
        return Ok(());
    };
    merge_excluded_extensions(
        &mut options.excluded_extensions,
        &profile.excluded_extensions,
    )?;
    if let Some(engine) = &profile.engine {
        options.engine = EngineChoice::Explicit(match engine.as_str() {
            "docker" => Engine::Docker,
            "orbstack" => Engine::OrbStack,
            "podman" => Engine::Podman,
            "auto" => detect_engine(),
            other => {
                return Err(OrbitError::Usage(format!(
                    "unknown engine `{other}` in profile `{name}`"
                )));
            }
        });
    }
    if let Some(network) = &profile.network {
        options.network_mode = NetworkMode::parse(network)?;
    }
    if let Some(image) = &profile.image {
        options.image = image.clone();
    }
    if let Some(proxy_image) = &profile.proxy_image {
        options.proxy_image = proxy_image.clone();
    }
    options
        .allowed_domains
        .extend(profile.allow_domains.clone());
    if let Some(proxy) = &profile.proxy {
        options.proxy = Some(proxy.clone());
    }
    if let Some(interactive) = profile.interactive {
        options.interactive = interactive;
    }
    Ok(())
}

fn merge_excluded_extensions(target: &mut Vec<String>, values: &[String]) -> Result<()> {
    for value in values {
        let normalized = normalize_excluded_extension(value)?;
        if !target.iter().any(|existing| existing == &normalized) {
            target.push(normalized);
        }
    }
    Ok(())
}

fn normalize_excluded_extension(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if !is_valid_excluded_extension(&normalized) {
        return Err(OrbitError::Usage(format!(
            "invalid excluded extension `{value}`"
        )));
    }
    Ok(normalized)
}

fn is_valid_excluded_extension(value: &str) -> bool {
    if value.is_empty()
        || value.starts_with('-')
        || value.contains('\\')
        || value.contains(',')
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return false;
    }
    if let Some(scoped) = value.strip_prefix('@') {
        let parts = scoped.split('/').collect::<Vec<_>>();
        return parts.len() == 2
            && parts
                .iter()
                .all(|part| !part.is_empty() && is_valid_package_component(part));
    }
    !value.contains('/') && is_valid_package_component(value)
}

fn is_valid_package_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

pub fn detect_engine() -> Engine {
    if std::env::var("ORBIT_ENGINE")
        .map(|value| value == "orbstack")
        .unwrap_or(false)
        || std::env::var("ORBSTACK")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    {
        Engine::OrbStack
    } else {
        Engine::Docker
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_config(raw: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, raw).unwrap();
        (dir, path)
    }

    #[test]
    fn merge_config_applies_global_and_profile_values() {
        let (_dir, path) = write_config(
            r#"{
              "excluded_extensions": ["qna"],
              "profiles": {
                "dev": {
                  "engine": "podman",
                  "network": "none",
                  "image": "orbit-dev:latest",
                  "proxy_image": "orbit-proxy:latest",
                  "allow_domains": ["example.com"],
                  "proxy": "http://proxy.example:8080",
                  "interactive": true,
                  "excluded_extensions": ["@scope/pkg", "qna"]
                }
              }
            }"#,
        );
        let mut options = BuildOptions::default();

        merge_config(&path, Some("dev"), &mut options).unwrap();

        assert_eq!(options.engine, EngineChoice::Explicit(Engine::Podman));
        assert_eq!(options.network_mode, NetworkMode::None);
        assert_eq!(options.image, "orbit-dev:latest");
        assert_eq!(options.proxy_image, "orbit-proxy:latest");
        assert_eq!(options.allowed_domains, vec!["example.com"]);
        assert_eq!(options.proxy.as_deref(), Some("http://proxy.example:8080"));
        assert!(options.interactive);
        assert_eq!(
            options.excluded_extensions,
            vec!["pi-sandbox", "qna", "@scope/pkg"]
        );
    }

    #[test]
    fn merge_config_rejects_missing_required_profile() {
        let (_dir, path) = write_config(r#"{"profiles": {}}"#);
        let mut options = BuildOptions::default();

        let err = merge_config(&path, Some("missing"), &mut options).unwrap_err();

        assert!(err.to_string().contains("profile `missing` not found"));
    }

    #[test]
    fn merge_config_rejects_invalid_excluded_extension() {
        let (_dir, path) = write_config(r#"{"excluded_extensions": ["bad token"]}"#);
        let mut options = BuildOptions::default();

        let err = merge_config(&path, None, &mut options).unwrap_err();

        assert!(
            err.to_string()
                .contains("invalid excluded extension `bad token`")
        );
    }

    #[test]
    fn merge_config_rejects_unknown_engine() {
        let (_dir, path) = write_config(r#"{"profiles": {"dev": {"engine": "containerd"}}}"#);
        let mut options = BuildOptions::default();

        let err = merge_config(&path, Some("dev"), &mut options).unwrap_err();

        assert!(err.to_string().contains("unknown engine `containerd`"));
    }
}
