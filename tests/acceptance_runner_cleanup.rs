mod support;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use support::*;
use tempfile::tempdir;

#[test]
fn dry_run_never_spawns_container_engine() {
    let dir = tempdir().expect("tempdir");
    let fake_docker = dir.path().join("docker");
    let log = dir.path().join("docker.log");
    std::fs::write(
        &fake_docker,
        format!("#!/bin/sh\necho invoked >> {}\nexit 99\n", log.display()),
    )
    .expect("fake docker");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&fake_docker)
            .expect("metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_docker, perms).expect("chmod");
    }
    let old_path = std::env::var("PATH").unwrap_or_default();
    orbit()
        .env("PATH", format!("{}:{old_path}", dir.path().display()))
        .args(["--dry-run", "--", "echo", "hi"])
        .assert()
        .success();
    assert!(!log.exists(), "dry-run must not invoke docker");
}

#[cfg(unix)]
#[test]
fn sigint_runs_restricted_network_cleanup() {
    signal_runs_restricted_network_cleanup("-INT", 130);
}

#[cfg(unix)]
#[test]
fn sigterm_runs_restricted_network_cleanup() {
    signal_runs_restricted_network_cleanup("-TERM", 143);
}

#[cfg(unix)]
fn signal_runs_restricted_network_cleanup(signal: &str, expected_code: i32) {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().expect("tempdir");
    let fake_docker = dir.path().join("docker");
    let log = dir.path().join("docker.log");
    let mut file = std::fs::File::create(&fake_docker).expect("fake docker");
    writeln!(
        file,
        r#"#!/bin/sh
echo "$@" >> "$ORBIT_DOCKER_LOG"
case "$1" in
  inspect)
    echo true
    exit 0
    ;;
esac
case "$*" in
  *--network=container:orbit-restricted-proxy*) sleep 5 ;;
esac
exit 0
"#
    )
    .expect("script");
    let mut perms = std::fs::metadata(&fake_docker)
        .expect("metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&fake_docker, perms).expect("chmod");

    let old_path = std::env::var("PATH").unwrap_or_default();
    let mut child = orbit()
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
        .spawn()
        .expect("spawn orbit");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let proxy_name = loop {
        let contents = std::fs::read_to_string(&log).unwrap_or_default();
        if let Some(name) = extract_proxy_name(&contents) {
            break name;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for main container invocation; log={contents}"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    Command::new("kill")
        .args([signal, &child.id().to_string()])
        .status()
        .expect("kill");
    let status = child.wait().expect("wait");
    assert_eq!(status.code(), Some(expected_code));
    let log = std::fs::read_to_string(log).expect("log");
    assert!(
        log.contains(&format!("rm -f {proxy_name}")),
        "cleanup must target spawned proxy {proxy_name}; log={log}"
    );
}

#[cfg(unix)]
fn extract_proxy_name(log: &str) -> Option<String> {
    log.split_whitespace()
        .find_map(|part| part.strip_prefix("--network=container:"))
        .map(str::to_string)
}

#[cfg(unix)]
#[test]
fn cleanup_removes_labeled_restricted_proxy_containers() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().expect("tempdir");
    let artifact = dir.path().join("orbit-old-artifact");
    std::fs::create_dir(&artifact).expect("artifact");
    let fake_docker = dir.path().join("docker");
    let log = dir.path().join("docker.log");
    std::fs::write(
        &fake_docker,
        r#"#!/bin/sh
echo "$@" >> "$ORBIT_DOCKER_LOG"
if [ "$1" = "ps" ]; then
  echo orbit-restricted-proxy-old
  echo orbit-restricted-proxy-other
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
        .env("TMPDIR", dir.path())
        .env("ORBIT_DOCKER_LOG", &log)
        .arg("cleanup")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "1 orbit temp artifact(s), 2 restricted proxy container(s) removed",
        ));

    assert!(!artifact.exists());
    let log = std::fs::read_to_string(log).expect("log");
    assert!(
        log.contains("ps -a --filter label=dev.orbit.role=restricted-proxy --format {{.Names}}")
    );
    assert!(log.contains("rm -f orbit-restricted-proxy-old"));
    assert!(log.contains("rm -f orbit-restricted-proxy-other"));
}
