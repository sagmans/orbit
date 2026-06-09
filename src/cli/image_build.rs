use crate::error::{OrbitError, Result};
use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::process::Command;

pub(super) fn mise_tools_build_arg(host_mise_tools: bool) -> Result<String> {
    if !host_mise_tools {
        return Ok(String::new());
    }
    let Some(home) = crate::mount_policy::home_dir() else {
        return Ok(String::new());
    };
    let config = home.join(".config/mise/config.toml");
    if !config.is_file() {
        return Ok(String::new());
    }
    let mut tools = parse_mise_tools_config(&std::fs::read_to_string(&config)?)?;
    ensure_mise_tool(&mut tools, "node", "24.16.0");
    ensure_mise_tool(&mut tools, "rust", "1.95.0");
    ensure_mise_tool(&mut tools, "gh", "2.93.0");
    Ok(tools.join(" "))
}

pub(super) fn pi_version_build_arg(
    host_pi_version: bool,
    explicit_pi_version: Option<&str>,
) -> Result<Option<String>> {
    if let Some(version) = explicit_pi_version {
        return validate_pi_version(version).map(|version| Some(version.to_string()));
    }
    if !host_pi_version {
        return Ok(None);
    }
    host_pi_version_arg()
}

fn host_pi_version_arg() -> Result<Option<String>> {
    let output = match Command::new("pi").arg("--version").output() {
        Ok(output) => output,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    if !output.status.success() {
        return Ok(None);
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let version = raw.lines().next().unwrap_or_default();
    validate_pi_version(version).map(|version| Some(version.to_string()))
}

fn validate_pi_version(raw: &str) -> Result<&str> {
    let version = raw.trim();
    if version.is_empty()
        || version.len() > 64
        || !version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
    {
        return Err(OrbitError::Usage(format!(
            "unsupported Pi version `{}` for image build",
            redacted_package_source(version)
        )));
    }
    Ok(version)
}

pub(super) fn parse_mise_tools_config(raw: &str) -> Result<Vec<String>> {
    let value = toml::from_str::<toml::Value>(raw)
        .map_err(|err| OrbitError::Usage(format!("mise config TOML parse failed: {err}")))?;
    let Some(tools_table) = value.get("tools").and_then(toml::Value::as_table) else {
        return Ok(Vec::new());
    };

    let mut tools = Vec::new();
    let mut seen = BTreeSet::new();
    for (name, value) in tools_table {
        append_mise_tool_specs(&mut tools, &mut seen, name, value)?;
    }
    Ok(tools)
}

fn append_mise_tool_specs(
    tools: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    name: &str,
    value: &toml::Value,
) -> Result<()> {
    match value {
        toml::Value::String(version) => push_mise_tool_spec(tools, seen, name, version),
        toml::Value::Array(versions) => {
            for version in versions {
                let Some(version) = version.as_str() else {
                    return Err(unsupported_mise_tool_value(name));
                };
                push_mise_tool_spec(tools, seen, name, version)?;
            }
            Ok(())
        }
        toml::Value::Table(table) => {
            let Some(version) = table.get("version").and_then(toml::Value::as_str) else {
                return Err(unsupported_mise_tool_value(name));
            };
            push_mise_tool_spec(tools, seen, name, version)
        }
        _ => Err(unsupported_mise_tool_value(name)),
    }
}

fn unsupported_mise_tool_value(name: &str) -> OrbitError {
    OrbitError::Usage(format!(
        "mise tool `{name}` must be a string, string array, or table with a string `version`"
    ))
}

fn push_mise_tool_spec(
    tools: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    name: &str,
    version: &str,
) -> Result<()> {
    if name.is_empty() || version.is_empty() {
        return Err(OrbitError::Usage(
            "mise [tools] entries must use non-empty string values".to_string(),
        ));
    }
    if !is_safe_mise_tool_token(name) || !is_safe_mise_tool_token(version) {
        return Err(OrbitError::Usage(format!(
            "mise tool `{name}` has unsupported characters for image build"
        )));
    }
    if package_source_contains_secret(name) || package_source_contains_secret(version) {
        let redacted = redacted_package_source(name);
        return Err(OrbitError::Usage(format!(
            "mise tool `{redacted}` cannot be baked because it appears to contain credentials or secret query parameters"
        )));
    }
    if name.eq_ignore_ascii_case("node") && !is_node_24_version(version) {
        return Err(OrbitError::Usage(format!(
            "mise tool `node` must be 24.x for Orbit image build; found `{version}`"
        )));
    }
    let spec = format!("{name}@{version}");
    if seen.insert(spec.to_ascii_lowercase()) {
        tools.push(spec);
    }
    Ok(())
}

fn ensure_mise_tool(tools: &mut Vec<String>, name: &str, version: &str) {
    if tools.iter().any(|tool| tool_name(tool) == name) {
        return;
    }
    tools.push(format!("{name}@{version}"));
}

fn tool_name(spec: &str) -> &str {
    spec.rsplit_once('@').map(|(name, _)| name).unwrap_or(spec)
}

fn is_node_24_version(version: &str) -> bool {
    version == "24" || version.starts_with("24.")
}

fn is_safe_mise_tool_token(value: &str) -> bool {
    value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | ':' | '/' | '@'))
}

fn package_source_contains_secret(source: &str) -> bool {
    source.chars().any(|c| c.is_control())
        || url_authority(source).is_some_and(|authority| authority.contains('@'))
        || source.split(['?', '#']).skip(1).any(contains_secret_name)
}

fn url_authority(source: &str) -> Option<&str> {
    let scheme = source.find("://")?;
    let rest = &source[scheme + 3..];
    Some(rest.split(['/', '?', '#']).next().unwrap_or(rest))
}

fn contains_secret_name(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "credential",
        "apikey",
        "api_key",
        "access_key",
        "auth",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn redacted_package_source(source: &str) -> String {
    if source.chars().any(|c| c.is_control()) {
        return "<redacted>".to_string();
    }
    if let Some(authority) = url_authority(source)
        && authority.contains('@')
    {
        return source.replacen(authority, "<redacted>@host", 1);
    }
    source
        .split(['?', '#'])
        .next()
        .unwrap_or(source)
        .to_string()
}
