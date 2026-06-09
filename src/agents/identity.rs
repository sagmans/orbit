use super::targets::{
    GIT_CONFIG_DIR_TARGET, GIT_HOOKS_TARGET, GITCONFIG_TARGET, GNUPG_HOME_TARGET, SSH_HOME_TARGET,
};
use crate::Result;
use crate::mount_policy::plan_mount;
use crate::plan::{MountCategory, MountMode, MountPlan};
use std::path::Path;

pub(super) fn git_identity_mounts(home: &Path, workspace: &Path) -> Result<Vec<MountPlan>> {
    let mut mounts = Vec::new();
    for (relative, target, reason) in [
        (
            ".gitconfig",
            GITCONFIG_TARGET,
            "global Git config mounted read-only so identity, signing, aliases, and includes resolve inside the container",
        ),
        (
            ".config/git",
            GIT_CONFIG_DIR_TARGET,
            "global Git config include directory mounted read-only so ~/.gitconfig references resolve inside the container",
        ),
        (
            ".git-hooks",
            GIT_HOOKS_TARGET,
            "global Git hooks directory mounted read-only so core.hooksPath references resolve inside the container",
        ),
    ] {
        let source = home.join(relative);
        let metadata = match std::fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => continue,
        };
        if metadata.file_type().is_symlink() {
            let Ok(canonical) = source.canonicalize() else {
                continue;
            };
            if is_raw_ssh_or_gpg_material(home, &canonical) {
                continue;
            }
        }
        if metadata.is_file() || metadata.is_dir() || metadata.file_type().is_symlink() {
            mounts.push(plan_mount(
                &source,
                target,
                MountMode::Ro,
                MountCategory::Secret,
                reason,
                Some(workspace),
                true,
            )?);
        }
    }
    Ok(mounts)
}

fn is_raw_ssh_or_gpg_material(home: &Path, canonical: &Path) -> bool {
    [home.join(".ssh"), home.join(".gnupg")]
        .into_iter()
        .filter_map(|path| path.canonicalize().ok())
        .any(|secret_home| canonical == secret_home || canonical.starts_with(secret_home))
}

pub(super) fn ssh_identity_mounts(home: &Path, workspace: &Path) -> Result<Vec<MountPlan>> {
    identity_dir_mount(
        &home.join(".ssh"),
        SSH_HOME_TARGET,
        workspace,
        "SSH home mounted read-only for host-equivalent Git SSH config, trust, and identity files",
    )
}

pub(super) fn gpg_identity_mounts(home: &Path, workspace: &Path) -> Result<Vec<MountPlan>> {
    identity_dir_mount(
        &home.join(".gnupg"),
        GNUPG_HOME_TARGET,
        workspace,
        "GPG home mounted read-only for host-equivalent Git signing config and keyrings",
    )
}

fn identity_dir_mount(
    source: &Path,
    target: &str,
    workspace: &Path,
    reason: &'static str,
) -> Result<Vec<MountPlan>> {
    if !source.is_dir() {
        return Ok(Vec::new());
    }
    Ok(vec![plan_mount(
        source,
        target,
        MountMode::Ro,
        MountCategory::Secret,
        reason,
        Some(workspace),
        true,
    )?])
}
