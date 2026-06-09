mod support;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use support::*;
use tempfile::tempdir;

#[test]
fn dry_run_echo_prints_hardened_docker_command() {
    let output = orbit()
        .args(["--dry-run", "--", "echo", "hi"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let workspace = repo_root();
    let output = String::from_utf8(output).unwrap();
    assert!(output.starts_with("docker run --detach --name=orbit-restricted-proxy-"));
    assert!(output.contains(default_allowed_domains_env()));
    assert!(output.contains("--network=container:orbit-restricted-proxy-"));
    if let Some(git_common_dir) = repo_git_common_dir().filter(|path| !path.starts_with(&workspace))
    {
        assert!(output.contains(&git_metadata_mount_arg(&git_common_dir)));
    }
    assert!(output.contains(&workspace_rw_mount_arg(&workspace)));
    assert!(!output.contains(&workspace_mount_arg(&workspace)));
    assert!(output.contains(&format!("--workdir={}", workspace_target(&workspace))));
    assert!(output.contains("--env HTTP_PROXY=http://127.0.0.1:18080"));
    assert!(output.contains("--env HTTPS_PROXY=http://127.0.0.1:18080"));
    assert!(output.ends_with("orbit-agent:latest echo hi\n"));
}

#[test]
fn workspace_mounts_read_write_by_default() {
    let workspace = repo_root();
    let output = orbit()
        .args(["--network", "none", "--dry-run", "--", "echo", "hi"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();

    assert!(output.contains(&workspace_rw_mount_arg(&workspace)));
    assert!(!output.contains(&workspace_mount_arg(&workspace)));
}

#[test]
fn home_child_workspace_maps_to_container_home() {
    let home = tempdir().expect("home");
    let workspace = home.path().join("source/me/orbit/main");
    std::fs::create_dir_all(workspace.join(".git")).expect("git");

    let output = orbit()
        .env("HOME", home.path())
        .args([
            "--network",
            "none",
            "--workspace",
            workspace.to_str().unwrap(),
            "--dry-run",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();
    let workspace = workspace.canonicalize().expect("canonical workspace");

    let workspace_mount = workspace_rw_mount_arg_with_home(&workspace, Some(home.path()));
    assert!(output.contains(&workspace_mount));
    assert!(!output.contains(&format!("{workspace_mount},readonly")));
    assert!(output.contains("--workdir=/home/orbit/source/me/orbit/main"));
}

#[test]
fn generated_workspace_targets_under_reserved_home_paths_are_refused() {
    let home = tempdir().expect("home");
    let workspace = home.path().join(".ssh/project");
    std::fs::create_dir_all(workspace.join(".git")).expect("git");

    orbit()
        .env("HOME", home.path())
        .args([
            "--network",
            "none",
            "--workspace",
            workspace.to_str().unwrap(),
            "--dry-run",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
}

#[test]
fn no_arg_alias_auto_interactive_and_flag_forces_tty() {
    let home = tempdir().expect("home");

    orbit()
        .env("HOME", home.path())
        .args(["--dry-run", "--network", "open", "pi"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--interactive --tty"))
        .stdout(predicate::str::contains("--network=bridge"))
        .stdout(predicate::str::contains(
            "orbit-agent:latest orbit-agent-entrypoint pi",
        ));

    orbit()
        .env("HOME", home.path())
        .args(["--dry-run", "--network", "open", "pi", "say hi"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--interactive --tty").not())
        .stdout(predicate::str::contains(
            "orbit-agent:latest orbit-agent-entrypoint pi 'say hi'",
        ));

    let output = orbit()
        .env("HOME", home.path())
        .args(["explain", "--json", "-i", "--", "sh"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("valid json plan");
    assert_eq!(json["interactive"], true);
}

#[test]
fn help_flags_after_command_boundary_stay_command_args() {
    orbit()
        .args(["--dry-run", "--", "echo", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("orbit-agent:latest echo --help"))
        .stdout(predicate::str::contains("Usage:").not());

    let home = tempdir().expect("home");
    orbit()
        .env("HOME", home.path())
        .args(["--dry-run", "pi", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "orbit-agent:latest orbit-agent-entrypoint pi --help",
        ))
        .stdout(predicate::str::contains("Usage:").not());
}

#[test]
fn explain_and_dry_run_are_incompatible() {
    orbit()
        .args(["explain", "--dry-run", "--", "echo", "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be combined"));
}

#[test]
fn explain_pi_shows_security_review_sections() {
    let home = tempdir().expect("home");
    orbit()
        .env("HOME", home.path())
        .args(["explain", "--", "pi", "--version"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Engine:"))
        .stdout(predicate::str::contains("Command:"))
        .stdout(predicate::str::contains("Mounts:"))
        .stdout(predicate::str::contains("Env:"))
        .stdout(predicate::str::contains("Network:"))
        .stdout(predicate::str::contains("Hardening:"))
        .stdout(predicate::str::contains("pi --version"));
}

#[test]
fn explain_json_serializes_full_run_plan() {
    let home = tempdir().expect("home");
    let output = orbit()
        .env("HOME", home.path())
        .args(["explain", "--json", "--", "pi", "--version"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("valid json plan");

    let workspace = repo_root();
    assert_eq!(json["engine"], "docker");
    assert_eq!(json["agent"], "pi");
    assert_eq!(json["command"], serde_json::json!(["pi", "--version"]));
    assert_eq!(
        json["cwd"],
        workspace_target_with_home(&workspace, Some(home.path()))
    );
    let mounts = json["mounts"].as_array().expect("mounts");
    if let Some(git_common_dir) = repo_git_common_dir().filter(|path| !path.starts_with(&workspace))
    {
        assert!(mounts.iter().any(|m| {
            m["category"] == "state"
                && m["mode"] == "rw"
                && m["target"] == workspace_target_with_home(&git_common_dir, Some(home.path()))
        }));
    }
    assert!(mounts.iter().any(|m| {
        m["category"] == "workspace"
            && m["mode"] == "rw"
            && m["target"] == workspace_target_with_home(&workspace, Some(home.path()))
    }));
    assert_eq!(json["network"]["mode"], "restricted");
    assert_eq!(
        json["network"]["allowed_domains"],
        serde_json::json!([
            "api.linear.app",
            "api.openai.com",
            "auth.openai.com",
            "chatgpt.com",
            "context7.com",
            "github.com",
            "mcp.cloudflare.com",
            "mcp.mdn.mozilla.net",
            "registry.npmjs.org"
        ])
    );
    assert_eq!(json["network"]["proxy"], "http://127.0.0.1:18080");
    assert_eq!(json["hardening"]["read_only_rootfs"], true);
    assert_eq!(json["hardening"]["cap_drop_all"], true);
    assert_eq!(json["hardening"]["no_new_privileges"], true);
}

#[test]
fn aliases_generate_agent_commands() {
    let home = tempdir().expect("home");
    for alias in [
        "pi",
        "opencode",
        "codex",
        "claude",
        "amp",
        "cursor-agent",
        "agy",
        "gemini",
    ] {
        orbit()
            .env("HOME", home.path())
            .args(["--dry-run", alias, "--version"])
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("{alias} --version")));
    }
}

#[test]
fn podman_and_orbstack_engines_have_separate_command_surfaces() {
    orbit()
        .args(["--engine", "podman", "--dry-run", "--", "echo", "hi"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("podman run"));

    orbit()
        .args(["--engine", "orbstack", "--dry-run", "--", "echo", "hi"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("docker run"))
        .stdout(predicate::str::contains("label=dev.orbit.engine=orbstack"));
}
