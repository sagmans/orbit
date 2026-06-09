mod support;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use support::*;
use tempfile::tempdir;

#[test]
fn forwarded_socket_paths_must_be_unix_sockets() {
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("not-a-socket");
    std::fs::write(&file, "x").expect("file");
    orbit()
        .env("SSH_AUTH_SOCK", &file)
        .args(["--forward-ssh", "--dry-run", "--", "echo", "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a Unix socket"));
}

#[cfg(unix)]
#[test]
fn forwarded_socket_paths_are_audited_and_redacted() {
    let dir = tempdir().expect("tempdir");
    let ssh_socket = dir.path().join("ssh.sock");
    let gpg_socket = dir.path().join("gpg.sock");
    let _ssh = std::os::unix::net::UnixListener::bind(&ssh_socket).expect("ssh socket");
    let _gpg = std::os::unix::net::UnixListener::bind(&gpg_socket).expect("gpg socket");

    let output = orbit()
        .env("SSH_AUTH_SOCK", &ssh_socket)
        .env("GPG_AGENT_SOCK", &gpg_socket)
        .args([
            "explain",
            "--forward-ssh",
            "--forward-gpg",
            "--json",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("json");
    let mounts = json["mounts"].as_array().expect("mounts");
    assert!(mounts.iter().any(|m| m["category"] == "socket"
        && m["target"] == "/run/host-ssh-agent.sock"
        && m["redacted"] == true));
    assert!(mounts.iter().any(|m| m["category"] == "socket"
        && m["target"] == "/run/host-gpg-agent.sock"
        && m["redacted"] == true));
    assert!(
        json["env"]
            .as_array()
            .expect("env")
            .iter()
            .any(|e| e["name"] == "SSH_AUTH_SOCK")
    );
    let audit = json["audit"].as_array().expect("audit");
    assert!(audit.iter().any(|a| a["code"] == "ssh_socket_forwarded"));
    assert!(audit.iter().any(|a| a["code"] == "gpg_socket_forwarded"));
}

#[cfg(unix)]
#[test]
fn agent_alias_mounts_git_identity_ssh_gpg_homes_and_sockets() {
    let home = tempdir().expect("home");
    std::fs::write(
        home.path().join(".gitconfig"),
        "[include]\n    path = ~/.config/git/config-local\n",
    )
    .expect("gitconfig");
    std::fs::create_dir_all(home.path().join(".config/git")).expect("git config dir");
    std::fs::write(
        home.path().join(".config/git/config-local"),
        "[user]\n    name = Test\n",
    )
    .expect("git config include");
    let hook_store = tempdir().expect("hook store");
    std::fs::write(hook_store.path().join("commit-msg"), "#!/bin/sh\n").expect("git hook");
    std::os::unix::fs::symlink(hook_store.path(), home.path().join(".git-hooks"))
        .expect("git hooks symlink");
    std::fs::create_dir_all(home.path().join(".ssh")).expect("ssh dir");
    std::fs::write(home.path().join(".ssh/config"), "Host github.com\n").expect("ssh config");
    std::fs::write(
        home.path().join(".ssh/known_hosts"),
        "github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI\n",
    )
    .expect("known hosts");
    std::fs::write(home.path().join(".ssh/id_ed25519.pub"), "ssh-ed25519 pub\n")
        .expect("ssh pubkey");
    std::fs::write(home.path().join(".ssh/id_ed25519"), "secret").expect("ssh key");
    std::fs::create_dir_all(home.path().join(".gnupg/private-keys-v1.d")).expect("gnupg dir");
    std::fs::write(home.path().join(".gnupg/gpg.conf"), "use-agent\n").expect("gpg config");
    std::fs::write(home.path().join(".gnupg/pubring.kbx"), "public").expect("gpg pubring");
    std::fs::write(
        home.path().join(".gnupg/private-keys-v1.d/key.key"),
        "secret",
    )
    .expect("gpg key");
    let socket_dir = tempdir().expect("socket dir");
    let ssh_socket = socket_dir.path().join("ssh.sock");
    let gpg_socket = socket_dir.path().join("gpg.sock");
    let _ssh = std::os::unix::net::UnixListener::bind(&ssh_socket).expect("ssh socket");
    let _gpg = std::os::unix::net::UnixListener::bind(&gpg_socket).expect("gpg socket");

    let output = orbit()
        .env("HOME", home.path())
        .env("SSH_AUTH_SOCK", &ssh_socket)
        .env("GPG_AGENT_SOCK", &gpg_socket)
        .args(["explain", "--json", "--", "pi", "--version"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("json");
    let mounts = json["mounts"].as_array().expect("mounts");
    for target in [
        "/home/orbit/.gitconfig",
        "/home/orbit/.config/git",
        "/home/orbit/.git-hooks",
        "/home/orbit/.ssh",
        "/home/orbit/.gnupg",
    ] {
        assert!(
            mounts.iter().any(|mount| mount["target"] == target
                && mount["mode"] == "ro"
                && mount["redacted"] == true),
            "missing redacted mount for {target}"
        );
    }
    assert!(
        mounts
            .iter()
            .any(|mount| mount["target"] == "/run/host-ssh-agent.sock"
                && mount["category"] == "socket"
                && mount["redacted"] == true)
    );
    assert!(
        mounts
            .iter()
            .any(|mount| mount["target"] == "/run/host-gpg-agent.sock"
                && mount["category"] == "socket"
                && mount["redacted"] == true)
    );
    let env = json["env"].as_array().expect("env");
    assert!(env.iter().any(|entry| entry["name"] == "SSH_AUTH_SOCK"));
    assert!(env.iter().any(|entry| entry["name"] == "GPG_AGENT_SOCK"));
    assert!(env.iter().any(|entry| {
        entry["name"] == "GIT_SSH_COMMAND"
            && entry["value_or_redacted"]
                .as_str()
                .is_some_and(|value| value.contains("ProxyCommand=socat"))
    }));
}

#[test]
fn coding_agent_home_dirs_mount_rw_without_subgroups() {
    let home = tempdir().expect("home");
    let codex = home.path().join(".codex");
    let claude = home.path().join(".claude");
    let cursor = home.path().join(".cursor");
    let gh = home.path().join(".config/gh");
    std::fs::create_dir_all(&codex).expect("codex dir");
    std::fs::create_dir_all(&claude).expect("claude dir");
    std::fs::create_dir_all(&cursor).expect("cursor dir");
    std::fs::create_dir_all(&gh).expect("gh dir");
    std::fs::write(codex.join("auth.json"), "token").expect("codex auth");
    std::fs::write(claude.join("credentials.json"), "token").expect("claude credentials");
    std::fs::write(gh.join("hosts.yml"), "github.com:\n  oauth_token: secret\n").expect("gh hosts");

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

    for target in [
        "/home/orbit/.codex",
        "/home/orbit/.claude",
        "/home/orbit/.cursor",
        "/home/orbit/.config/gh",
    ] {
        assert!(
            mounts.iter().any(|m| {
                m["target"] == target && m["category"] == "state" && m["mode"] == "rw"
            }),
            "missing rw top-level mount for {target}"
        );
    }
    for subgroup in [
        "/home/orbit/.codex/auth.json",
        "/home/orbit/.claude/credentials.json",
        "/home/orbit/.config/gh/hosts.yml",
    ] {
        assert!(
            mounts.iter().all(|m| m["target"] != subgroup),
            "unexpected subgroup mount for {subgroup}"
        );
    }

    let writable_output = orbit()
        .env("HOME", home.path())
        .args([
            "explain",
            "--json",
            "--allow-agent-state",
            "--",
            "pi",
            "--version",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let writable_json: Value = serde_json::from_slice(&writable_output).expect("writable json");
    let writable_mounts = writable_json["mounts"].as_array().expect("mounts");
    assert!(writable_mounts.iter().any(|m| {
        m["target"] == "/home/orbit/.config/gh" && m["category"] == "state" && m["mode"] == "rw"
    }));
    assert!(writable_mounts.iter().all(|m| {
        m["target"] != "/home/orbit/.codex/auth.json"
            && m["target"] != "/home/orbit/.config/gh/hosts.yml"
    }));
}
