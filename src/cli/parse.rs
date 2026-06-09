use crate::agents;
use crate::config::{detect_engine, merge_config, merge_user_config};
use crate::error::{OrbitError, Result};
use crate::network::NetworkMode;
use crate::plan::{BuildOptions, Engine, EngineChoice, ExplicitMount, MountMode, RunRequest};
use std::path::PathBuf;

use super::action::{Action, PlanMode};

pub(super) fn parse(args: Vec<String>) -> Result<Action> {
    if args.is_empty() {
        return Ok(Action::Help);
    }
    match args[0].as_str() {
        "--help" | "-h" => Ok(Action::Help),
        "doctor" => parse_doctor(&args[1..]),
        "cleanup" => parse_cleanup(&args[1..]),
        "image" => parse_image(&args[1..]),
        "explain" => parse_plan(&args[1..], true),
        _ => parse_plan(&args, false),
    }
}

fn parse_doctor(args: &[String]) -> Result<Action> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Action::Help),
            "--json" => json = true,
            other => {
                return Err(OrbitError::Usage(format!(
                    "unknown doctor option `{other}`"
                )));
            }
        }
    }
    Ok(Action::Doctor { json })
}

fn parse_cleanup(args: &[String]) -> Result<Action> {
    let mut dry_run = false;
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Action::Help),
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            other => {
                return Err(OrbitError::Usage(format!(
                    "unknown cleanup option `{other}`"
                )));
            }
        }
    }
    Ok(Action::Cleanup { dry_run, json })
}

fn parse_image(args: &[String]) -> Result<Action> {
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return Ok(Action::Help);
    }
    if args.first().map(String::as_str) != Some("build") {
        return Err(OrbitError::Usage(
            "expected `orbit image build`".to_string(),
        ));
    }
    let mut dry_run = false;
    let mut host_mise_tools = true;
    let mut engine = Engine::Docker;
    let mut engine_from_cli = false;
    let mut tag = "orbit-agent:latest".to_string();
    let mut host_pi_version = true;
    let mut pi_version: Option<String> = None;
    let mut config: Option<PathBuf> = None;
    let mut profile: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => return Ok(Action::Help),
            "--dry-run" => dry_run = true,
            "--no-host-mise-tools" => host_mise_tools = false,
            "--no-host-pi-version" => host_pi_version = false,
            "--pi-version" => {
                i += 1;
                pi_version = Some(require_value(args, i, "--pi-version")?.to_string());
            }
            "--engine" => {
                i += 1;
                engine = parse_engine(require_value(args, i, "--engine")?)?;
                engine_from_cli = true;
            }
            "--tag" => {
                i += 1;
                tag = require_value(args, i, "--tag")?.to_string();
            }
            "--config" => {
                i += 1;
                config = Some(PathBuf::from(require_value(args, i, "--config")?));
            }
            "--profile" => {
                i += 1;
                profile = Some(require_value(args, i, "--profile")?.to_string());
            }
            other => {
                return Err(OrbitError::Usage(format!(
                    "unknown image build option `{other}`"
                )));
            }
        }
        i += 1;
    }
    let mut options = BuildOptions::default();
    merge_user_config(profile.as_deref(), &mut options)?;
    if let Some(config) = config.as_ref() {
        merge_config(config, profile.as_deref(), &mut options)?;
    }
    if !engine_from_cli {
        engine = match options.engine {
            EngineChoice::Auto => detect_engine(),
            EngineChoice::Explicit(engine) => engine,
        };
    }
    Ok(Action::ImageBuild {
        dry_run,
        engine,
        tag,
        host_mise_tools,
        host_pi_version,
        pi_version,
    })
}

fn parse_plan(args: &[String], explain_mode: bool) -> Result<Action> {
    let mut options = BuildOptions::default();
    let (pre_config, pre_profile) = scan_profile(args)?;
    merge_user_config(pre_profile.as_deref(), &mut options)?;
    if let Some(config) = pre_config.as_ref() {
        merge_config(config, pre_profile.as_deref(), &mut options)?;
    }
    let mut dry_run = false;
    let mut json = false;
    let mut command = Vec::new();
    let mut explicit_agent: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                command = args[i + 1..].to_vec();
                break;
            }
            "--dry-run" => dry_run = true,
            "--interactive" | "-i" => options.interactive = true,
            "--help" | "-h" => return Ok(Action::Help),
            "--json" if explain_mode => json = true,
            "--engine" => {
                i += 1;
                options.engine = parse_engine_choice(require_value(args, i, "--engine")?)?;
            }
            "--network" => {
                i += 1;
                options.network_mode = NetworkMode::parse(require_value(args, i, "--network")?)?;
            }
            "--allow-domain" => {
                i += 1;
                options
                    .allowed_domains
                    .push(require_value(args, i, "--allow-domain")?.to_string());
            }
            "--proxy" => {
                i += 1;
                options.proxy = Some(require_value(args, i, "--proxy")?.to_string());
            }
            "--mount" => {
                i += 1;
                options
                    .explicit_mounts
                    .push(parse_mount(require_value(args, i, "--mount")?)?);
            }
            "--workspace" => {
                i += 1;
                options.workspace = Some(PathBuf::from(require_value(args, i, "--workspace")?));
            }
            "--image" => {
                i += 1;
                options.image = require_value(args, i, "--image")?.to_string();
            }
            "--proxy-image" => {
                i += 1;
                options.proxy_image = require_value(args, i, "--proxy-image")?.to_string();
            }
            "--config" => {
                i += 1;
                let _ = require_value(args, i, "--config")?;
            }
            "--profile" => {
                i += 1;
                let _ = require_value(args, i, "--profile")?;
            }
            "--allow-agent-state" => options.allow_agent_state = true,
            "--forward-ssh" => options.forward_ssh = true,
            "--forward-gpg" => options.forward_gpg = true,
            token if agents::is_alias(token) => {
                explicit_agent = Some(token.to_string());
                command.push(token.to_string());
                let rest = if args.get(i + 1).map(String::as_str) == Some("--") {
                    &args[i + 2..]
                } else {
                    &args[i + 1..]
                };
                command.extend(rest.iter().cloned());
                break;
            }
            token if token.starts_with('-') => {
                return Err(OrbitError::Usage(format!("unknown option `{token}`")));
            }
            token => {
                return Err(OrbitError::Usage(format!(
                    "command `{token}` must follow `--` or be a known alias"
                )));
            }
        }
        i += 1;
    }

    if command.is_empty() {
        return Err(OrbitError::Usage(
            "missing command; use `orbit -- <cmd...>` or `orbit <agent-alias> <args...>`"
                .to_string(),
        ));
    }
    if explicit_agent.is_some() && command.len() == 1 {
        options.interactive = true;
    }
    let agent = explicit_agent.unwrap_or_else(|| agents::agent_for_command(&command));
    if explain_mode && dry_run {
        return Err(OrbitError::Usage(
            "explain and --dry-run cannot be combined".to_string(),
        ));
    }
    let mode = if explain_mode {
        if json {
            PlanMode::ExplainJson
        } else {
            PlanMode::ExplainHuman
        }
    } else if dry_run {
        PlanMode::DryRun
    } else {
        PlanMode::Run
    };
    Ok(Action::Plan {
        request: Box::new(RunRequest {
            agent,
            command,
            options,
        }),
        mode,
    })
}

fn scan_profile(args: &[String]) -> Result<(Option<PathBuf>, Option<String>)> {
    let mut config = None;
    let mut profile = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--" => break,
            token if agents::is_alias(token) => break,
            "--config" => {
                i += 1;
                config = Some(PathBuf::from(require_value(args, i, "--config")?));
            }
            "--profile" => {
                i += 1;
                profile = Some(require_value(args, i, "--profile")?.to_string());
            }
            _ => {}
        }
        i += 1;
    }
    Ok((config, profile))
}

fn require_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| OrbitError::Usage(format!("{flag} requires a value")))
}

fn parse_engine_choice(value: &str) -> Result<EngineChoice> {
    if value == "auto" {
        Ok(EngineChoice::Auto)
    } else {
        Ok(EngineChoice::Explicit(parse_engine(value)?))
    }
}

fn parse_engine(value: &str) -> Result<Engine> {
    match value {
        "docker" => Ok(Engine::Docker),
        "orbstack" => Ok(Engine::OrbStack),
        "podman" => Ok(Engine::Podman),
        other => Err(OrbitError::Usage(format!("unknown engine `{other}`"))),
    }
}

pub(super) fn validate_image_reference(flag: &str, image: &str) -> Result<()> {
    if image.is_empty()
        || image != image.trim()
        || image.starts_with('-')
        || image
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || !c.is_ascii())
        || !image
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '/' | '@'))
    {
        return Err(OrbitError::Usage(format!(
            "{flag} value `{image}` is not a valid container image reference"
        )));
    }
    Ok(())
}

fn parse_mount(value: &str) -> Result<ExplicitMount> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() < 2 || parts.len() > 3 {
        return Err(OrbitError::Usage(format!(
            "mount `{value}` must be source:target[:ro|rw]"
        )));
    }
    let mode = match parts.get(2).copied().unwrap_or("ro") {
        "ro" => MountMode::Ro,
        "rw" => MountMode::Rw,
        other => return Err(OrbitError::Usage(format!("unknown mount mode `{other}`"))),
    };
    Ok(ExplicitMount {
        source: PathBuf::from(parts[0]),
        target: parts[1].to_string(),
        mode,
    })
}
