mod support;

use assert_cmd::prelude::*;
use std::process::Command;
use support::*;
use tempfile::tempdir;

#[test]
fn orbit_agent_entrypoint_execs_command_without_state_sync() {
    Command::new("sh")
        .arg(repo_root().join("docker/orbit-agent-entrypoint"))
        .arg("true")
        .env(
            "ORBIT_PI_AGENT_COPY_SOURCE_ROOT",
            "/tmp/missing-copy-source",
        )
        .env("ORBIT_PI_SETTINGS_SOURCE", "/tmp/settings.json")
        .env("ORBIT_PI_MCP_SOURCE", "/tmp/mcp.json")
        .env("ORBIT_PI_LINEAR_TOKENS_SOURCE", "/tmp/linear-tokens.json")
        .env("ORBIT_BAKED_PI_AGENT_DIR", "/tmp/baked-agent")
        .assert()
        .success();
}

#[test]
fn orbit_agent_entrypoint_does_not_create_or_copy_pi_state() {
    let dir = tempdir().expect("entrypoint dir");
    let runtime = dir.path().join("runtime-agent");
    let source = dir.path().join("copy-source");
    std::fs::create_dir_all(&source).expect("copy source");
    std::fs::write(
        source.join("settings.json"),
        r#"{"packages":["npm:pi-sandbox"]}"#,
    )
    .expect("settings");

    Command::new("sh")
        .arg(repo_root().join("docker/orbit-agent-entrypoint"))
        .arg("true")
        .env("PI_CODING_AGENT_DIR", &runtime)
        .env("ORBIT_PI_AGENT_COPY_SOURCE_ROOT", &source)
        .env("ORBIT_PI_SETTINGS_SOURCE", source.join("settings.json"))
        .assert()
        .success();

    assert!(!runtime.exists());
}

#[test]
fn orbit_agent_entrypoint_propagates_command_failure() {
    Command::new("sh")
        .arg(repo_root().join("docker/orbit-agent-entrypoint"))
        .arg("false")
        .assert()
        .failure();
}
