use crate::config::detect_engine;
use crate::error::Result;
use crate::plan::Engine;

pub(super) fn doctor(json: bool) -> Result<String> {
    #[derive(serde::Serialize)]
    struct Doctor<'a> {
        platform: &'a str,
        docker: bool,
        podman: bool,
        default_engine: Engine,
        warnings: Vec<&'a str>,
    }
    let report = Doctor {
        platform: std::env::consts::OS,
        docker: command_available("docker"),
        podman: command_available("podman"),
        default_engine: detect_engine(),
        warnings: if cfg!(windows) {
            vec!["Windows is out of scope"]
        } else {
            Vec::new()
        },
    };
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(&report)?));
    }
    Ok(format!(
        "Platform: {}\nDocker: {}\nPodman: {}\nDefault engine: {:?}\n",
        report.platform,
        if report.docker {
            "available"
        } else {
            "missing"
        },
        if report.podman {
            "available"
        } else {
            "missing"
        },
        report.default_engine
    ))
}

fn command_available(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

pub(super) fn cleanup(dry_run: bool, json: bool) -> Result<String> {
    let temp = std::env::temp_dir();
    let mut candidates = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&temp) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("orbit-") {
                candidates.push(entry.path());
            }
        }
    }
    let engine = detect_engine();
    let proxy_containers = restricted_proxy_containers(engine);
    if !dry_run {
        for candidate in &candidates {
            if candidate.is_dir() {
                let _ = std::fs::remove_dir_all(candidate);
            } else {
                let _ = std::fs::remove_file(candidate);
            }
        }
        remove_restricted_proxy_containers(engine, &proxy_containers);
    }
    if json {
        let paths = candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>();
        return Ok(format!(
            "{}\n",
            serde_json::json!({
                "dry_run": dry_run,
                "candidates": paths,
                "proxy_containers": proxy_containers,
            })
        ));
    }
    Ok(format!(
        "cleanup plan: {} orbit temp artifact(s), {} restricted proxy container(s){}\n",
        candidates.len(),
        proxy_containers.len(),
        if dry_run { " (dry-run)" } else { " removed" }
    ))
}

fn restricted_proxy_containers(engine: Engine) -> Vec<String> {
    let Ok(output) = std::process::Command::new(engine.binary())
        .args([
            "ps",
            "-a",
            "--filter",
            "label=dev.orbit.role=restricted-proxy",
            "--format",
            "{{.Names}}",
        ])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

fn remove_restricted_proxy_containers(engine: Engine, containers: &[String]) {
    for container in containers {
        let _ = std::process::Command::new(engine.binary())
            .args(["rm", "-f", container])
            .status();
    }
}

pub(super) fn help() -> String {
    "Usage:\n  orbit [flags] -- <cmd...>\n  orbit [flags] <agent-alias> <args...>\n  orbit explain [--json] [flags] -- <cmd...>\n  orbit doctor [--json]\n  orbit cleanup [--dry-run]\n  orbit image build [--dry-run] [--engine docker|orbstack|podman] [--tag TAG] [--pi-version VERSION] [--no-host-pi-version] [--no-host-mise-tools]\n\nFlags: -i, --interactive attaches stdin and TTY. Agent aliases attach stdin/TTY when run with no args. --proxy-image selects a trusted Orbit-derived image for the privileged restricted proxy.\nAgent aliases: pi, opencode, codex, claude, amp, cursor-agent, agy, gemini.\nBase image installs pinned mise, host ~/.config/mise/config.toml [tools] unless --no-host-mise-tools is set (fallback Node 24.16.0/Rust 1.95.0 plus gh 2.93.0), host `pi --version` unless --no-host-pi-version or --pi-version overrides it, rg, fd, bwrap, socat, sem, inspect-mcp, and pinned npm-backed agent CLIs. Image builds do not read host Pi settings or bake host Pi packages.\nOrbit never adds extension-owned Pi flags automatically; pass Pi flags explicitly when the container has that extension.\nCursor Agent and Antigravity require vendor installers or custom images.\n\nSafety defaults: Docker, restricted network allowlisting registry.npmjs.org and OpenAI Codex endpoints, workspace read-write by default, no Docker socket. Use --network none to block egress, --network open for full bridge egress, or --allow-domain to extend the restricted proxy allowlist. Generic commands mount no agent state; agent aliases mount supported coding-agent top-level state directories read-write by default, without subgroup mounts or runtime snapshots.\n".to_string()
}
