mod support;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use support::*;
use tempfile::tempdir;

#[test]
fn restricted_network_allows_allowlisted_domains_and_refuses_bypass() {
    orbit()
        .args([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--dry-run",
            "--",
            "curl",
            "https://example.com/index.html",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ORBIT_ALLOWED_DOMAINS=api.linear.app,api.openai.com,auth.openai.com,chatgpt.com,context7.com,example.com,github.com,mcp.cloudflare.com,mcp.mdn.mozilla.net,registry.npmjs.org",
        ))
        .stdout(predicate::str::contains(
            "--network=container:orbit-restricted-proxy",
        ));

    orbit()
        .args([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--dry-run",
            "--",
            "curl",
            "https://evil.example.net",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("network bypass refused"));

    orbit()
        .args([
            "--network",
            "restricted",
            "--dry-run",
            "--",
            "curl",
            "https://example.com",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("outside restricted allowlist"));

    orbit()
        .args([
            "--network",
            "restricted",
            "--allow-domain",
            " ",
            "--dry-run",
            "--",
            "curl",
            "https://example.com",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid"));

    orbit()
        .args([
            "--network",
            "restricted",
            "--allow-domain",
            "internal",
            "--dry-run",
            "--",
            "curl",
            "https://internal",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("dotted DNS domain"));

    let open_output = orbit()
        .args([
            "explain",
            "--json",
            "--network",
            "open",
            "--allow-domain",
            "example.com",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let open_json: Value = serde_json::from_slice(&open_output).expect("open json");
    assert_eq!(open_json["network"]["mode"], "open");
    assert!(
        open_json["network"]["allowed_domains"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let none_output = orbit()
        .args([
            "explain",
            "--json",
            "--network",
            "none",
            "--allow-domain",
            "example.com",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let none_json: Value = serde_json::from_slice(&none_output).expect("none json");
    assert_eq!(none_json["network"]["mode"], "none");
    assert!(
        none_json["network"]["allowed_domains"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    orbit()
        .args([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--dry-run",
            "--",
            "curl",
            "https://1.2.3.4",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("direct IP"));

    orbit()
        .args([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--dry-run",
            "--",
            "curl",
            "https://[::1]/",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("direct IP"));

    orbit()
        .args([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--proxy",
            "http://proxy.example:8080\nacl evil dstdomain .evil.com",
            "--dry-run",
            "--",
            "curl",
            "https://example.com",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("proxy must be an http(s) URL"))
        .stderr(predicate::str::contains("acl evil").not());

    orbit()
        .args([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--proxy",
            "http://proxy.example:8080/path",
            "--dry-run",
            "--",
            "curl",
            "https://example.com",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("proxy must be an http(s) URL"));
}

#[test]
fn explain_redacts_proxy_credentials() {
    let output = orbit()
        .args([
            "explain",
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--proxy",
            "http://user:pass@proxy.example:8080",
            "--json",
            "--",
            "curl",
            "https://example.com",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(!text.contains("user:pass"));
    assert!(text.contains("http://<redacted>@proxy.example:8080"));
}

#[cfg(unix)]
#[test]
fn restricted_proxy_credentials_do_not_reach_engine_argv() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().expect("tempdir");
    let fake_docker = dir.path().join("docker");
    let log = dir.path().join("docker.log");
    std::fs::write(
        &fake_docker,
        r#"#!/bin/sh
echo "$@" >> "$ORBIT_DOCKER_LOG"
if [ "$1" = "inspect" ]; then
  echo true
fi
exit 0
"#,
    )
    .expect("fake docker");
    let mut perms = std::fs::metadata(&fake_docker)
        .expect("metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&fake_docker, perms).expect("chmod");

    let old_path = std::env::var("PATH").unwrap_or_default();
    orbit()
        .env("PATH", format!("{}:{old_path}", dir.path().display()))
        .env("ORBIT_DOCKER_LOG", &log)
        .args([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--proxy",
            "http://user:pass@proxy.example:8080",
            "--",
            "curl",
            "https://example.com",
        ])
        .assert()
        .success();

    let log = std::fs::read_to_string(log).expect("log");
    assert!(
        !log.contains("user:pass"),
        "proxy credentials leaked into argv: {log}"
    );
    assert!(log.contains("ORBIT_UPSTREAM_PROXY_FILE=/run/orbit-upstream-proxy"));
    assert!(log.contains("dst=/run/orbit-upstream-proxy,readonly"));
}

#[cfg(unix)]
#[test]
fn restricted_proxy_exit_is_reported_before_main_container() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().expect("tempdir");
    let fake_docker = dir.path().join("docker");
    let log = dir.path().join("docker.log");
    std::fs::write(
        &fake_docker,
        r#"#!/bin/sh
echo "$@" >> "$ORBIT_DOCKER_LOG"
case "$1" in
  run)
    if [ "$2" = "--detach" ]; then
      echo proxy-container-id
      exit 0
    fi
    echo "main container should not start" >&2
    exit 7
    ;;
  inspect)
    echo false
    exit 0
    ;;
  logs)
    echo "iptables failed"
    exit 0
    ;;
  rm)
    exit 0
    ;;
  *)
    echo "unexpected docker command: $@" >&2
    exit 7
    ;;
esac
"#,
    )
    .expect("fake docker");
    let mut perms = std::fs::metadata(&fake_docker)
        .expect("metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&fake_docker, perms).expect("chmod");

    let old_path = std::env::var("PATH").unwrap_or_default();
    orbit()
        .env("PATH", format!("{}:{old_path}", dir.path().display()))
        .env("ORBIT_DOCKER_LOG", &log)
        .args([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--",
            "curl",
            "https://example.com",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("restricted proxy"))
        .stderr(predicate::str::contains("iptables failed"));

    let log = std::fs::read_to_string(log).expect("log");
    assert!(log.contains("run --detach"));
    assert!(log.contains("inspect --format={{.State.Running}}"));
    assert!(log.contains("logs orbit-restricted-proxy"));
    assert!(log.contains("rm -f orbit-restricted-proxy"));
    assert!(!log.contains("--network=container:orbit-restricted-proxy"));
}
