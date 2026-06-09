mod support;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use support::*;
use tempfile::tempdir;

#[test]
fn unsafe_mounts_are_refused_before_docker_command_generation() {
    orbit()
        .args(["--dry-run", "--mount", "/:/host", "--", "echo", "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refused"))
        .stderr(predicate::str::contains("root"));

    orbit()
        .args([
            "--dry-run",
            "--mount",
            "/var/run/docker.sock:/var/run/docker.sock",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refused"))
        .stderr(predicate::str::contains("docker socket"));
}

#[test]
fn home_mount_and_dangerous_parent_mounts_are_refused() {
    if let Ok(home) = std::env::var("HOME") {
        let mount = format!("{home}:/host-home");
        orbit()
            .args(["--dry-run", "--mount", &mount, "--", "echo", "hi"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("refused"))
            .stderr(predicate::str::contains("home"));
    }

    orbit()
        .args(["--dry-run", "--mount", "/Users:/users", "--", "echo", "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refused"));

    for mount in ["/private/var:/host-var", "/private/etc/hosts:/host-hosts"] {
        orbit()
            .args(["--dry-run", "--mount", mount, "--", "echo", "hi"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("refused"));
    }
}

#[test]
fn duplicate_mount_targets_are_refused() {
    let dir = tempdir().expect("tempdir");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir_all(&a).expect("a");
    std::fs::create_dir_all(&b).expect("b");
    let mount_a = format!("{}:/dup", a.display());
    let mount_b = format!("{}:/dup", b.display());

    orbit()
        .args([
            "--dry-run",
            "--mount",
            &mount_a,
            "--mount",
            &mount_b,
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate target"));

    let workspace = repo_root();
    let workspace_file = workspace.join("README.md");
    let mount_workspace = format!(
        "{}:{}",
        workspace_file.display(),
        workspace_target(&workspace)
    );
    orbit()
        .args(["--dry-run", "--mount", &mount_workspace, "--", "echo", "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate target"));
}

#[test]
fn mount_grammar_injection_is_refused() {
    let injected = format!(
        "{}:/safe,src=/var/run/docker.sock,dst=/var/run/docker.sock:ro",
        repo_root().join("README.md").display()
    );
    orbit()
        .args(["--dry-run", "--mount", &injected, "--", "echo", "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mount grammar"));
}

#[test]
fn default_workspace_write_and_explicit_mounts_are_deliberate() {
    let dir = tempdir().expect("workspace");
    std::fs::create_dir(dir.path().join(".git")).expect("git dir");
    let cfg = dir.path().join("tool.conf");
    std::fs::write(&cfg, "x").expect("cfg");
    let mount = format!("{}:/tool.conf:ro", cfg.display());

    orbit()
        .args([
            "--workspace",
            dir.path().to_str().unwrap(),
            "--mount",
            &mount,
            "--dry-run",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "src={},dst={}",
            dir.path().canonicalize().unwrap().display(),
            workspace_target(&dir.path().canonicalize().unwrap())
        )))
        .stdout(predicate::str::contains("dst=/tool.conf,readonly"));

    let host_workspace = repo_root();
    let write_output = orbit()
        .args(["--dry-run", "--", "echo", "hi"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let write_output = String::from_utf8(write_output).expect("write dry-run");
    if let Some(git_common_dir) =
        repo_git_common_dir().filter(|path| !path.starts_with(&host_workspace))
    {
        assert!(write_output.contains(&git_metadata_mount_arg(&git_common_dir)));
    }
    assert!(write_output.contains(&format!(
        "--mount type=bind,src={},dst={}",
        host_workspace.display(),
        workspace_target(&host_workspace)
    )));
    assert!(!write_output.contains(&format!(
        "src={},dst={},readonly",
        host_workspace.display(),
        workspace_target(&host_workspace)
    )));

    let outside = tempdir().expect("outside");
    let outside_cfg = outside.path().join("tool.conf");
    std::fs::write(&outside_cfg, "x").expect("outside cfg");
    let outside_mount = format!("{}:/outside.conf:ro", outside_cfg.display());
    orbit()
        .args([
            "--workspace",
            dir.path().to_str().unwrap(),
            "--mount",
            &outside_mount,
            "--dry-run",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown host path"));
}
