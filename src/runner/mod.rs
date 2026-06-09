mod gh_auth;
mod git_metadata;
mod process;
mod proxy;

use crate::Result;
#[cfg(test)]
use crate::agents::{GH_CONFIG_TARGET, GH_HOSTS_TARGET};
use crate::docker;
#[cfg(test)]
use crate::mount_policy::plan_mount;
use crate::plan::RunPlan;
#[cfg(test)]
use crate::plan::{CleanupAction, CleanupStep, MountCategory, MountMode};
use std::sync::{Arc, atomic::AtomicBool};

use gh_auth::prepare_runtime_gh_auth_config;
#[cfg(test)]
use gh_auth::{
    merge_gh_hosts_token, stage_runtime_gh_auth_config, stage_runtime_gh_auth_config_in_temp_root,
};
use git_metadata::prepare_git_metadata_rewrites;
#[cfg(test)]
use git_metadata::prepare_git_metadata_rewrites_in_temp_root;
pub use process::run_cleanup_steps;
#[cfg(test)]
use process::{FakeRunner, run_command};
use process::{
    install_signal_cleanup, run_cleanup_command, run_cleanup_once, run_command_quiet_stdout,
    run_command_with_cleanup, warn_cleanup_failure,
};
use proxy::{ensure_restricted_proxy_ready, prepare_runtime_proxy_secret};

pub fn execute(plan: &RunPlan) -> Result<i32> {
    let mut runtime_plan = plan.clone();
    let prepare_result: Result<()> = (|| {
        prepare_git_metadata_rewrites(&mut runtime_plan)?;
        prepare_runtime_gh_auth_config(&mut runtime_plan)?;
        prepare_runtime_proxy_secret(&mut runtime_plan)?;
        Ok(())
    })();
    if let Err(err) = prepare_result {
        let cleanup_done = AtomicBool::new(false);
        if let Err(cleanup_err) = run_cleanup_once(
            runtime_plan.engine.binary(),
            &runtime_plan.cleanup,
            &cleanup_done,
        ) {
            warn_cleanup_failure(
                runtime_plan.engine.binary(),
                "runtime preparation",
                &cleanup_err,
            );
        }
        return Err(err);
    }

    let cleanup_done = Arc::new(AtomicBool::new(false));
    let engine = runtime_plan.engine.binary().to_string();
    install_signal_cleanup(
        engine.clone(),
        runtime_plan.cleanup.clone(),
        cleanup_done.clone(),
    );

    if let Some(proxy_args) = docker::restricted_proxy_command(&runtime_plan, false) {
        if let Some(proxy_container) = runtime_plan.network.proxy_container.as_deref() {
            let _ = run_cleanup_command(&engine, &["rm", "-f", proxy_container]);
        }
        run_command_quiet_stdout(
            &proxy_args,
            &docker::shell_join(&docker::restricted_proxy_command(&runtime_plan, true).unwrap()),
            &engine,
            &runtime_plan.cleanup,
            &cleanup_done,
        )?;
        ensure_restricted_proxy_ready(
            &runtime_plan,
            &engine,
            &runtime_plan.cleanup,
            &cleanup_done,
        )?;
    }

    let args = docker::command_args(&runtime_plan, false);
    let display = docker::dry_run_command(&runtime_plan);
    run_command_with_cleanup(
        &args,
        &display,
        &engine,
        &runtime_plan.cleanup,
        &cleanup_done,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parse_and_plan_for_test;

    fn cleanup_file_step(path: &std::path::Path) -> CleanupStep {
        CleanupStep {
            id: "tmp".to_string(),
            action: CleanupAction::RemovePath,
            target: path.display().to_string(),
        }
    }

    #[test]
    fn gh_hosts_token_is_inserted_without_losing_config() {
        let existing = "github.com:\n    git_protocol: ssh\n    users:\n        assagman:\n    user: assagman\n";
        let merged = merge_gh_hosts_token(existing, "gho_secret");

        assert!(merged.starts_with("github.com:\n"));
        assert!(merged.contains("    oauth_token: gho_secret\n"));
        assert!(merged.contains("    git_protocol: ssh\n"));
        assert!(merged.contains("    user: assagman\n"));
    }

    #[test]
    fn gh_hosts_token_replaces_stale_token() {
        let existing = "github.com:\n    oauth_token: stale\n    git_protocol: ssh\n";
        let merged = merge_gh_hosts_token(existing, "gho_fresh");

        assert!(merged.contains("    oauth_token: gho_fresh\n"));
        assert!(!merged.contains("stale"));
    }

    #[test]
    fn gh_auth_config_is_staged_as_writable_ephemeral_secret() {
        let host_config = tempfile::tempdir().unwrap();
        std::fs::write(host_config.path().join("config.yml"), "version: 1\n").unwrap();
        std::fs::write(
            host_config.path().join("hosts.yml"),
            "github.com:\n    git_protocol: ssh\n",
        )
        .unwrap();
        let mut plan =
            parse_and_plan_for_test(["--network", "none", "--dry-run", "--", "echo", "hi"])
                .unwrap();
        plan.mounts.push(
            plan_mount(
                host_config.path(),
                GH_CONFIG_TARGET,
                MountMode::Ro,
                MountCategory::Secret,
                "host gh config",
                None,
                true,
            )
            .unwrap(),
        );
        plan.mounts.push(
            plan_mount(
                &host_config.path().join("hosts.yml"),
                GH_HOSTS_TARGET,
                MountMode::Ro,
                MountCategory::Secret,
                "host gh hosts",
                None,
                true,
            )
            .unwrap(),
        );

        stage_runtime_gh_auth_config(&mut plan, host_config.path(), "gho_fresh").unwrap();

        assert_staged_gh_config(&mut plan, host_config.path());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn gh_auth_staging_allows_macos_tmpdir_under_private_var() {
        let Some(temp_root) = writable_macos_private_var_tmpdir() else {
            return;
        };
        let host_config = tempfile::tempdir().unwrap();
        std::fs::write(host_config.path().join("config.yml"), "version: 1\n").unwrap();
        std::fs::write(
            host_config.path().join("hosts.yml"),
            "github.com:\n    git_protocol: ssh\n",
        )
        .unwrap();
        let mut plan =
            parse_and_plan_for_test(["--network", "none", "--dry-run", "--", "echo", "hi"])
                .unwrap();
        plan.mounts.push(
            plan_mount(
                host_config.path(),
                GH_CONFIG_TARGET,
                MountMode::Ro,
                MountCategory::Secret,
                "host gh config",
                None,
                true,
            )
            .unwrap(),
        );
        stage_runtime_gh_auth_config_in_temp_root(
            &mut plan,
            host_config.path(),
            "gho_fresh",
            &temp_root,
        )
        .unwrap();
        assert_staged_gh_config(&mut plan, host_config.path());
    }

    fn assert_staged_gh_config(plan: &mut RunPlan, host_config: &std::path::Path) {
        let mount = plan
            .mounts
            .iter()
            .find(|mount| mount.target == GH_CONFIG_TARGET)
            .unwrap();
        assert_eq!(mount.mode, MountMode::Rw);
        assert_eq!(mount.category, MountCategory::Secret);
        assert!(mount.redacted);
        assert_ne!(mount.host_source, host_config);
        assert!(
            !plan
                .mounts
                .iter()
                .any(|mount| mount.target == GH_HOSTS_TARGET)
        );
        assert_eq!(
            std::fs::read_to_string(mount.host_source.join("config.yml")).unwrap(),
            "version: 1\n"
        );
        assert!(
            std::fs::read_to_string(mount.host_source.join("hosts.yml"))
                .unwrap()
                .contains("oauth_token: gho_fresh")
        );
        let staged_dir = mount.host_source.clone();
        run_cleanup_steps("docker", &plan.cleanup).unwrap();
        assert!(!staged_dir.exists());
    }

    #[test]
    fn git_metadata_rewrites_are_staged_and_cleaned() {
        let mut plan =
            parse_and_plan_for_test(["--network", "none", "--dry-run", "--", "echo", "hi"])
                .unwrap();
        plan.git_metadata_rewrites = vec![crate::plan::GitMetadataRewrite {
            target: "/home/orbit/source/repo/.git".to_string(),
            content: "gitdir: /home/orbit/source/repo/.git/worktrees/repo\n".to_string(),
        }];

        prepare_git_metadata_rewrites(&mut plan).unwrap();

        let mount = plan
            .mounts
            .iter()
            .find(|mount| mount.target == "/home/orbit/source/repo/.git")
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&mount.host_source).unwrap(),
            plan.git_metadata_rewrites[0].content
        );
        run_cleanup_steps("docker", &plan.cleanup).unwrap();
        assert!(!mount.host_source.exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn git_metadata_rewrites_allow_macos_tmpdir_under_private_var() {
        let Some(temp_root) = writable_macos_private_var_tmpdir() else {
            return;
        };
        let mut plan =
            parse_and_plan_for_test(["--network", "none", "--dry-run", "--", "echo", "hi"])
                .unwrap();
        plan.git_metadata_rewrites = vec![crate::plan::GitMetadataRewrite {
            target: "/home/orbit/source/repo/.git".to_string(),
            content: "gitdir: /home/orbit/source/repo/.git/worktrees/repo\n".to_string(),
        }];

        prepare_git_metadata_rewrites_in_temp_root(&mut plan, &temp_root).unwrap();

        let mount = plan
            .mounts
            .iter()
            .find(|mount| mount.target == "/home/orbit/source/repo/.git")
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&mount.host_source).unwrap(),
            plan.git_metadata_rewrites[0].content
        );
        run_cleanup_steps("docker", &plan.cleanup).unwrap();
        assert!(!mount.host_source.exists());
    }

    #[cfg(target_os = "macos")]
    fn writable_macos_private_var_tmpdir() -> Option<std::path::PathBuf> {
        let folders = std::path::Path::new("/var/folders");
        for first in std::fs::read_dir(folders).ok()?.flatten() {
            for second in std::fs::read_dir(first.path()).ok()?.flatten() {
                let candidate = second.path().join("T");
                if !candidate.is_dir() {
                    continue;
                }
                let Ok(canonical) = candidate.canonicalize() else {
                    continue;
                };
                if !canonical.starts_with("/private/var/folders") {
                    continue;
                }
                if tempfile::Builder::new()
                    .prefix("orbit-probe-")
                    .tempdir_in(&candidate)
                    .is_ok()
                {
                    return Some(candidate);
                }
            }
        }
        None
    }

    #[test]
    fn production_runner_cleanup_runs_on_success_and_failure() {
        let dir = tempfile::tempdir().unwrap();
        let success_file = dir.path().join("success.tmp");
        std::fs::write(&success_file, "x").unwrap();
        let args = vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "exit 0".to_string(),
        ];
        let cleanup_done = AtomicBool::new(false);
        assert!(
            run_command_with_cleanup(
                &args,
                "safe",
                "docker",
                &[cleanup_file_step(&success_file)],
                &cleanup_done,
            )
            .is_ok()
        );
        assert!(!success_file.exists());

        let failure_file = dir.path().join("failure.tmp");
        std::fs::write(&failure_file, "x").unwrap();
        let args = vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "exit 7".to_string(),
        ];
        let cleanup_done = AtomicBool::new(false);
        let err = run_command(
            &args,
            "safe <redacted>",
            "docker",
            &[cleanup_file_step(&failure_file)],
            &cleanup_done,
        )
        .unwrap_err();
        assert!(!failure_file.exists());
        assert!(err.to_string().contains("safe <redacted>"));
    }

    #[test]
    fn cleanup_failure_does_not_mask_primary_failure() {
        let args = vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "exit 7".to_string(),
        ];
        let cleanup = vec![CleanupStep {
            id: "proxy".to_string(),
            action: CleanupAction::StopProxy,
            target: "missing-proxy".to_string(),
        }];
        let cleanup_done = AtomicBool::new(false);
        let err = run_command(
            &args,
            "primary <redacted>",
            "false",
            &cleanup,
            &cleanup_done,
        )
        .unwrap_err();
        assert!(err.to_string().contains("primary <redacted>"));
        assert!(!err.to_string().contains("false rm"));
    }

    #[test]
    fn cleanup_failure_does_not_mask_success_exit() {
        let args = vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "exit 0".to_string(),
        ];
        let cleanup = vec![CleanupStep {
            id: "proxy".to_string(),
            action: CleanupAction::StopProxy,
            target: "missing-proxy".to_string(),
        }];
        let cleanup_done = AtomicBool::new(false);
        let status = run_command_with_cleanup(
            &args,
            "primary <redacted>",
            "false",
            &cleanup,
            &cleanup_done,
        )
        .unwrap();
        assert_eq!(status, 0);
    }

    #[test]
    fn cleanup_steps_continue_after_failure() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cleanup.tmp");
        std::fs::write(&file, "x").unwrap();
        let cleanup = vec![
            cleanup_file_step(&file),
            CleanupStep {
                id: "proxy".to_string(),
                action: CleanupAction::StopProxy,
                target: "missing-proxy".to_string(),
            },
        ];

        let err = run_cleanup_steps("false", &cleanup).unwrap_err();

        assert!(!file.exists());
        assert!(err.to_string().contains("false rm"));
    }

    #[test]
    fn fake_runner_runs_cleanup_on_failure() {
        let mut plan = parse_and_plan_for_test([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--dry-run",
            "--",
            "curl",
            "https://example.com",
        ])
        .unwrap();
        assert!(!plan.cleanup.is_empty());
        let mut runner = FakeRunner {
            fail: true,
            ..Default::default()
        };
        assert!(runner.run(&plan).is_err());
        assert!(!runner.cleanup.is_empty());

        plan.cleanup.clear();
        let mut runner = FakeRunner::default();
        runner.run(&plan).unwrap();
        assert!(runner.cleanup.is_empty());
    }
}
