mod support;

use assert_cmd::prelude::*;
use std::process::Command;
use support::*;
use tempfile::tempdir;

#[test]
fn linked_worktree_mounts_git_metadata_not_sibling_source_and_keeps_workdir() {
    let dir = tempdir().expect("tempdir");
    let main = dir.path().join("main");
    let linked = dir.path().join("linked");
    Command::new("git")
        .args(["init", main.to_str().unwrap()])
        .assert()
        .success();
    Command::new("git")
        .args([
            "-C",
            main.to_str().unwrap(),
            "config",
            "user.email",
            "orbit@example.com",
        ])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", main.to_str().unwrap(), "config", "user.name", "Orbit"])
        .assert()
        .success();
    std::fs::write(main.join("README.md"), "init").expect("readme");
    Command::new("git")
        .args(["-C", main.to_str().unwrap(), "add", "README.md"])
        .assert()
        .success();
    Command::new("git")
        .args(["-C", main.to_str().unwrap(), "commit", "-m", "init"])
        .assert()
        .success();
    Command::new("git")
        .args([
            "-C",
            main.to_str().unwrap(),
            "worktree",
            "add",
            "../linked",
            "-b",
            "linked",
        ])
        .assert()
        .success();

    let output = orbit()
        .args([
            "--workspace",
            linked.to_str().unwrap(),
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
    let output = String::from_utf8(output).expect("dry-run");
    let main_mount = workspace_mount_arg(&main.canonicalize().unwrap());
    let git_mount = git_metadata_mount_arg(&main.join(".git").canonicalize().unwrap());
    let linked_mount = workspace_rw_mount_arg(&linked.canonicalize().unwrap());
    assert!(!output.contains(&main_mount));
    assert!(output.contains(&git_mount));
    assert!(!output.contains(&git_metadata_readonly_mount_arg(
        &main.join(".git").canonicalize().unwrap()
    )));
    assert!(output.contains(&linked_mount));
    assert!(output.contains(&format!(
        "--workdir={}",
        workspace_target(&linked.canonicalize().unwrap())
    )));
    assert!(!output.contains(&format!(
        "src={},dst={}",
        main.join(".git").display(),
        workspace_target(&main.join(".git"))
    )));
}

#[test]
fn bare_common_metadata_overmounts_source_children_read_only() {
    let dir = tempdir().expect("tempdir");
    let common = dir.path().join("common");
    let workspace = common.join("linked-source");
    let sibling = common.join("main-source");
    let git_dir = common.join("worktrees/linked-source");
    std::fs::create_dir_all(common.join("objects")).expect("objects");
    std::fs::create_dir_all(common.join("refs/heads")).expect("refs");
    std::fs::create_dir_all(&git_dir).expect("git dir");
    std::fs::create_dir_all(&workspace).expect("workspace");
    std::fs::create_dir_all(&sibling).expect("sibling");
    std::fs::write(common.join("FETCH_HEAD"), "old fetch head\n").expect("FETCH_HEAD");
    std::fs::write(common.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    std::fs::write(common.join("config"), "[core]\n\tbare = false\n").expect("config");
    std::fs::write(git_dir.join("commondir"), "../..\n").expect("commondir");
    std::fs::write(
        workspace.join(".git"),
        format!("gitdir: {}\n", git_dir.display()),
    )
    .expect("git file");

    let output = orbit()
        .args([
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
    let output = String::from_utf8(output).expect("dry-run");
    let common_mount = git_metadata_mount_arg(&common.canonicalize().unwrap());
    let sibling_mount = workspace_mount_arg(&sibling.canonicalize().unwrap());
    let workspace_mount = workspace_rw_mount_arg(&workspace.canonicalize().unwrap());
    let fetch_head_ro_mount =
        workspace_mount_arg(&common.join("FETCH_HEAD").canonicalize().unwrap());

    assert!(output.contains(&common_mount));
    assert!(!output.contains(&git_metadata_readonly_mount_arg(
        &common.canonicalize().unwrap()
    )));
    assert!(!output.contains(&fetch_head_ro_mount));
    assert!(output.contains(&sibling_mount));
    assert!(output.contains(&workspace_mount));
    assert!(
        output.find(&common_mount).unwrap() < output.find(&sibling_mount).unwrap(),
        "writable common Git metadata root must mount before read-only sibling source overmount"
    );
}

#[test]
fn git_metadata_is_writable_with_default_workspace_writes() {
    let dir = tempdir().expect("workspace");
    std::fs::create_dir(dir.path().join(".git")).expect("git dir");
    let workspace = dir.path().canonicalize().unwrap();
    let git_dir = workspace.join(".git");

    let output = orbit()
        .args([
            "--workspace",
            dir.path().to_str().unwrap(),
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
    let output = String::from_utf8(output).expect("dry-run");

    let workspace_mount = workspace_rw_mount_arg(&workspace);
    let git_mount = git_metadata_mount_arg(&git_dir);
    assert!(output.contains(&workspace_mount));
    assert!(output.contains(&git_mount));
    assert!(!output.contains(&git_metadata_readonly_mount_arg(&git_dir)));
    assert!(
        output.find(&workspace_mount).unwrap() < output.find(&git_mount).unwrap(),
        "in-workspace .git must mount after workspace so Git metadata stays writable"
    );
}
