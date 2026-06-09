mod support;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use support::*;
use tempfile::tempdir;

#[test]
fn dry_run_pi_alias_prints_hardened_docker_command() {
    let home = tempdir().expect("home");
    let output = orbit()
        .env("HOME", home.path())
        .args(["--dry-run", "pi", "--version"])
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
        let git_mount = format!(
            "--mount type=bind,src={},dst={}",
            git_common_dir.display(),
            workspace_target_with_home(&git_common_dir, Some(home.path()))
        );
        assert!(output.contains(&git_mount));
        assert!(!output.contains(&format!("{git_mount},readonly")));
    }
    let workspace_mount = workspace_rw_mount_arg_with_home(&workspace, Some(home.path()));
    assert!(output.contains(&workspace_mount));
    assert!(!output.contains(&format!("{workspace_mount},readonly")));
    assert!(output.contains(&format!(
        "--workdir={}",
        workspace_target_with_home(&workspace, Some(home.path()))
    )));
    assert!(output.contains("--env HTTP_PROXY=http://127.0.0.1:18080"));
    assert!(output.contains("--env HTTPS_PROXY=http://127.0.0.1:18080"));
    assert!(output.ends_with("orbit-agent:latest orbit-agent-entrypoint pi --version\n"));
}

#[test]
fn pi_alias_uses_direct_settings_without_runtime_copy() {
    let home = tempdir().expect("home");
    let settings_dir = home.path().join(".pi/agent");
    std::fs::create_dir_all(&settings_dir).expect("settings dir");
    std::fs::write(
        settings_dir.join("settings.json"),
        r#"{"packages":["npm:pi-subagents","npm:pi-sandbox@1.2.3"]}"#,
    )
    .expect("settings");
    std::fs::write(settings_dir.join("mcp.json"), r#"{"mcpServers":{}}"#).expect("mcp");

    orbit()
        .env("HOME", home.path())
        .args(["--dry-run", "pi", "--version"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ORBIT_PI_SETTINGS_SOURCE").not())
        .stdout(predicate::str::contains("ORBIT_PI_MCP_SOURCE").not())
        .stdout(predicate::str::contains("ORBIT_EXCLUDED_EXTENSIONS").not())
        .stdout(predicate::str::contains("/run/orbit-pi-agent-copy-source").not())
        .stdout(predicate::str::contains(
            "orbit-agent:latest orbit-agent-entrypoint pi --version",
        ))
        .stdout(predicate::str::contains("pi --no-sandbox --version").not());

    orbit()
        .env("HOME", home.path())
        .args(["--dry-run", "pi", "--no-sandbox", "--version"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "orbit-agent:latest orbit-agent-entrypoint pi --no-sandbox --version",
        ));

    let output = orbit()
        .env("HOME", home.path())
        .args(["explain", "--json", "--", "pi", "--version"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("valid json plan");
    assert_eq!(json["command"], serde_json::json!(["pi", "--version"]));
    assert!(json["env"].as_array().expect("env").iter().all(|entry| {
        !matches!(
            entry["name"].as_str(),
            Some("ORBIT_PI_SETTINGS_SOURCE" | "ORBIT_PI_MCP_SOURCE" | "ORBIT_EXCLUDED_EXTENSIONS")
        )
    }));
    assert!(json["mounts"].as_array().expect("mounts").iter().all(|m| {
        m["target"] != "/run/orbit-pi-settings-source.json"
            && m["target"] != "/run/orbit-pi-mcp-source.json"
            && m["target"] != "/run/orbit-pi-linear-tokens.json"
    }));

    let workspace = tempdir().expect("workspace");
    std::fs::create_dir(workspace.path().join(".git")).expect("git");
    let host_settings = workspace.path().join("settings.json");
    std::fs::write(&host_settings, "{}").expect("host settings");
    let settings_mount = format!(
        "{}:/home/orbit/.pi/agent/settings.json:rw",
        host_settings.display()
    );
    orbit()
        .env("HOME", home.path())
        .args([
            "--workspace",
            workspace.path().to_str().unwrap(),
            "--mount",
            &settings_mount,
            "--dry-run",
            "pi",
            "--version",
        ])
        .assert()
        .success();
}

#[test]
fn pi_home_mounts_entire_top_level_rw_without_subgroups() {
    let home = tempdir().expect("home");
    let pi_home = home.path().join(".pi");
    let agent = pi_home.join("agent");
    std::fs::create_dir_all(agent.join("sessions/other-project")).expect("sessions");
    std::fs::create_dir_all(agent.join("bin")).expect("bin");
    std::fs::create_dir_all(agent.join("cache")).expect("cache");
    std::fs::create_dir_all(agent.join("git")).expect("git");
    std::fs::create_dir_all(agent.join("npm")).expect("npm");
    std::fs::create_dir_all(agent.join("extensions/qna")).expect("extension");
    std::fs::write(agent.join("settings.json"), "{}").expect("settings");
    std::fs::write(agent.join("mcp.json"), r#"{"mcpServers":{}}"#).expect("mcp");
    std::fs::write(agent.join("tokens.json"), "token").expect("token");
    std::fs::write(pi_home.join("auth.json"), "legacy-token").expect("legacy auth");

    let output = orbit()
        .env("HOME", home.path())
        .args(["explain", "--json", "--", "pi", "--version"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("json");
    let mounts = json["mounts"].as_array().expect("mounts");
    assert!(mounts.iter().any(|m| {
        m["target"] == "/home/orbit/.pi"
            && m["source"] == pi_home.canonicalize().unwrap().display().to_string()
            && m["category"] == "state"
            && m["mode"] == "rw"
    }));
    for subgroup in [
        "/home/orbit/.pi/auth.json",
        "/home/orbit/.pi/agent",
        "/home/orbit/.pi/agent/settings.json",
        "/home/orbit/.pi/agent/mcp.json",
        "/home/orbit/.pi/agent/tokens.json",
        "/home/orbit/.pi/agent/sessions",
        "/home/orbit/.pi/agent/sessions/orbit-current",
        "/home/orbit/.pi/agent/bin",
        "/home/orbit/.pi/agent/cache",
        "/home/orbit/.pi/agent/git",
        "/home/orbit/.pi/agent/npm",
        "/home/orbit/.pi/agent/extensions",
        "/run/orbit-pi-linear-tokens.json",
    ] {
        assert!(
            mounts.iter().all(|m| m["target"] != subgroup),
            "unexpected subgroup mount for {subgroup}"
        );
    }
    assert!(
        json["env"]
            .as_array()
            .expect("env")
            .iter()
            .all(|entry| { entry["name"] != "PI_CODING_AGENT_SESSION_DIR" })
    );
    assert!(
        json["audit"]
            .as_array()
            .expect("audit")
            .iter()
            .any(|a| a["code"] == "agent_homes_mounted")
    );

    let dry_run = orbit()
        .env("HOME", home.path())
        .args(["--dry-run", "pi", "--version"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let dry_run = String::from_utf8(dry_run).expect("dry-run");
    assert!(dry_run.contains("dst=/home/orbit/.pi"));
    assert!(!dry_run.contains("dst=/home/orbit/.pi/agent"));
    assert!(!dry_run.contains("PI_CODING_AGENT_SESSION_DIR"));
    assert!(!dry_run.contains("/run/orbit-pi-agent-copy-source"));

    orbit()
        .env("HOME", home.path())
        .args(["--network", "open", "--dry-run", "pi", "--version"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--network=bridge"));
}

#[cfg(unix)]
#[test]
fn pi_home_mounts_symlinked_extension_targets_read_only() {
    let home = tempdir().expect("home");
    let pi_home = home.path().join(".pi");
    let extensions = pi_home.join("agent/extensions");
    std::fs::create_dir_all(&extensions).expect("extensions");

    let extension_source = home.path().join("custom-extension-source");
    std::fs::create_dir_all(extension_source.join("src")).expect("extension source");
    std::fs::write(
        extension_source.join("package.json"),
        r#"{"name":"custom-extension","pi":{"extensions":["./src/index.ts"]}}"#,
    )
    .expect("package");
    std::os::unix::fs::symlink(&extension_source, extensions.join("custom-extension"))
        .expect("extension symlink");

    let output = orbit()
        .env("HOME", home.path())
        .args(["explain", "--json", "--", "pi", "--version"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("json");
    let mounts = json["mounts"].as_array().expect("mounts");
    let source = extension_source
        .canonicalize()
        .unwrap()
        .display()
        .to_string();
    let target = std::fs::read_link(extensions.join("custom-extension"))
        .unwrap()
        .display()
        .to_string();

    assert!(mounts.iter().any(|m| {
        m["source"] == source
            && m["target"] == target
            && m["category"] == "state"
            && m["mode"] == "ro"
    }));
    assert!(
        mounts
            .iter()
            .all(|m| { m["target"] != "/home/orbit/.pi/agent/extensions/custom-extension" })
    );
}

#[cfg(unix)]
#[test]
fn pi_home_refuses_secret_like_extension_symlink_targets() {
    let home = tempdir().expect("home");
    let extensions = home.path().join(".pi/agent/extensions");
    std::fs::create_dir_all(&extensions).expect("extensions");
    let secret_like_target = home.path().join("secret-token-extension.json");
    std::fs::write(&secret_like_target, "{}").expect("secret-like target");
    std::os::unix::fs::symlink(
        &secret_like_target,
        extensions.join("secret-token-extension"),
    )
    .expect("extension symlink");

    orbit()
        .env("HOME", home.path())
        .args(["--dry-run", "pi", "--version"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("looks secret-like"));
}

#[cfg(unix)]
#[test]
fn pi_symlinked_home_or_agent_dir_is_refused() {
    let home = tempfile::Builder::new()
        .prefix("tokenuser")
        .tempdir()
        .expect("home");
    let outside = tempdir().expect("outside");
    std::fs::create_dir_all(outside.path().join("agent")).expect("outside agent");
    std::os::unix::fs::symlink(outside.path(), home.path().join(".pi")).expect("pi symlink");

    orbit()
        .env("HOME", home.path())
        .args(["--dry-run", "pi", "--version"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pi home"));

    let home = tempdir().expect("home with symlinked agent");
    let outside = tempdir().expect("outside agent");
    std::fs::create_dir_all(home.path().join(".pi")).expect("pi dir");
    std::os::unix::fs::symlink(outside.path(), home.path().join(".pi/agent"))
        .expect("agent symlink");

    orbit()
        .env("HOME", home.path())
        .args(["--dry-run", "pi", "--version"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pi agent dir"));
}

#[test]
fn whole_pi_home_mount_includes_legacy_pi_auth_without_submounts() {
    let home = tempdir().expect("home");
    let pi_home = home.path().join(".pi");
    std::fs::create_dir_all(&pi_home).expect("pi dir");
    std::fs::write(pi_home.join("auth.json"), "secret").expect("pi auth");

    let output = orbit()
        .env("HOME", home.path())
        .args(["explain", "--json", "--", "pi", "--version"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("json");
    let mounts = json["mounts"].as_array().expect("mounts");
    assert!(mounts.iter().any(|m| {
        m["target"] == "/home/orbit/.pi"
            && m["source"] == pi_home.canonicalize().unwrap().display().to_string()
            && m["category"] == "state"
            && m["mode"] == "rw"
    }));
    assert!(
        mounts
            .iter()
            .all(|m| m["target"] != "/home/orbit/.pi/auth.json")
    );
    assert!(
        json["hardening"]["tmpfs"]
            .as_array()
            .expect("tmpfs")
            .iter()
            .all(|entry| entry
                .as_str()
                .is_none_or(|value| !value.starts_with("/home/orbit/.pi:")))
    );
}
