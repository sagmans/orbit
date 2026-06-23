use assert_cmd::Command;
use tempfile::tempdir;

fn orbit() -> Command {
    Command::cargo_bin("orbit").unwrap()
}

fn make_git_repo(dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join(".git")).unwrap();
}

#[test]
fn help_works() {
    orbit()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("Orbit"));
}

#[test]
fn no_args_shows_help() {
    orbit()
        .assert()
        .success()
        .stdout(predicates::str::contains("USAGE"));
}

#[test]
fn dry_run_echo() {
    let dir = tempdir().unwrap();
    make_git_repo(dir.path());

    orbit()
        .current_dir(dir.path())
        .args(["--dry-run", "--", "echo", "hi"])
        .assert()
        .success()
        .stdout(predicates::str::contains("docker run"))
        .stdout(predicates::str::contains("--read-only"))
        .stdout(predicates::str::contains("--cap-drop=ALL"))
        .stdout(predicates::str::contains("--user=1000:1000"))
        .stdout(predicates::str::contains("echo hi"));
}

#[test]
fn dry_run_agent_uses_entrypoint() {
    let dir = tempdir().unwrap();
    make_git_repo(dir.path());

    orbit()
        .current_dir(dir.path())
        .args(["--dry-run", "pi", "--version"])
        .assert()
        .success()
        .stdout(predicates::str::contains("orbit-entrypoint"))
        .stdout(predicates::str::contains("--no-sandbox"))
        .stdout(predicates::str::contains("--version"));
}

#[test]
fn no_arg_agent_is_interactive() {
    let dir = tempdir().unwrap();
    make_git_repo(dir.path());

    orbit()
        .current_dir(dir.path())
        .args(["--dry-run", "pi"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--interactive"))
        .stdout(predicates::str::contains("--tty"));
}

#[test]
fn explain_shows_plan() {
    let dir = tempdir().unwrap();
    make_git_repo(dir.path());

    orbit()
        .current_dir(dir.path())
        .args(["explain", "--", "echo", "hi"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Orbit run plan"))
        .stdout(predicates::str::contains("Hardening"));
}

#[test]
fn network_none_in_dry_run() {
    let dir = tempdir().unwrap();
    make_git_repo(dir.path());

    orbit()
        .current_dir(dir.path())
        .args(["--dry-run", "--network", "none", "--", "echo"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--network=none"));
}

#[test]
fn network_bridge_in_dry_run() {
    let dir = tempdir().unwrap();
    make_git_repo(dir.path());

    orbit()
        .current_dir(dir.path())
        .args(["--dry-run", "--", "echo"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--network=bridge"));
}

#[test]
fn non_git_dir_errors() {
    let dir = tempdir().unwrap();

    orbit()
        .current_dir(dir.path())
        .args(["--dry-run", "--", "echo"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("git"));
}

#[test]
fn unknown_flag_errors() {
    let dir = tempdir().unwrap();
    make_git_repo(dir.path());

    orbit()
        .current_dir(dir.path())
        .args(["--bogus", "--", "echo"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unknown option"));
}

#[test]
fn doctor_works() {
    orbit()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicates::str::contains("Platform:"));
}

#[test]
fn image_build_dry_run() {
    orbit()
        .args(["image", "build", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("docker build"))
        .stdout(predicates::str::contains("docker/Dockerfile"))
        .stdout(predicates::str::contains("orbit-agent:latest"));
}

#[test]
fn custom_image_in_dry_run() {
    let dir = tempdir().unwrap();
    make_git_repo(dir.path());

    orbit()
        .current_dir(dir.path())
        .args(["--dry-run", "--image", "custom:latest", "--", "echo"])
        .assert()
        .success()
        .stdout(predicates::str::contains("custom:latest"));
}

#[test]
fn explicit_mount_in_dry_run() {
    let dir = tempdir().unwrap();
    let extra = tempdir().unwrap();
    std::fs::write(extra.path().join("data.txt"), "x").unwrap();
    make_git_repo(dir.path());

    orbit()
        .current_dir(dir.path())
        .args([
            "--dry-run",
            "--mount",
            &format!("{}:/data:ro", extra.path().display()),
            "--",
            "echo",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("dst=/data"));
}

#[test]
fn podman_engine_in_dry_run() {
    let dir = tempdir().unwrap();
    make_git_repo(dir.path());

    orbit()
        .current_dir(dir.path())
        .args(["--dry-run", "--engine", "podman", "--", "echo"])
        .assert()
        .success()
        .stdout(predicates::str::contains("podman run"))
        .stdout(predicates::str::contains("--userns=keep-id"));
}
