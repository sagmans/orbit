use super::targets::{GPG_AGENT_SOCK_TARGET, SSH_AUTH_SOCK_TARGET};
use crate::mount_policy::plan_mount;
use crate::plan::{EnvCategory, EnvPlan, MountCategory, MountMode, MountPlan, env_plan};
use crate::{OrbitError, Result};
use std::path::{Path, PathBuf};

pub(super) fn maybe_forward_ssh_socket(
    required: bool,
    auto: bool,
    workspace: &Path,
    env: &mut Vec<EnvPlan>,
    mounts: &mut Vec<MountPlan>,
) -> Result<()> {
    let sock = std::env::var_os("SSH_AUTH_SOCK").map(PathBuf::from);
    let Some(sock) = sock.filter(|path| path.exists()) else {
        if required {
            return Err(OrbitError::refused(
                "ssh_socket_missing",
                "--forward-ssh requires SSH_AUTH_SOCK to point at an existing Unix socket",
                None,
            ));
        }
        return Ok(());
    };
    if let Err(err) = validate_socket_path(&sock, "SSH_AUTH_SOCK") {
        if required {
            return Err(err);
        }
        return Ok(());
    }
    if required || auto {
        mounts.push(plan_mount(
            &sock,
            SSH_AUTH_SOCK_TARGET,
            MountMode::Ro,
            MountCategory::Socket,
            if required {
                "explicit SSH agent forwarding; grants host identity power"
            } else {
                "agent SSH socket forwarding for host-equivalent Git identity; grants host identity power"
            },
            Some(workspace),
            true,
        )?);
        env.push(env_plan(
            "SSH_AUTH_SOCK",
            SSH_AUTH_SOCK_TARGET,
            EnvCategory::Agent,
            if required {
                "explicit SSH socket forwarding"
            } else {
                "agent SSH socket forwarding for host-equivalent Git identity"
            },
            false,
        ));
    }
    Ok(())
}

pub(super) fn maybe_forward_gpg_socket(
    required: bool,
    auto: bool,
    home: Option<&Path>,
    workspace: &Path,
    env: &mut Vec<EnvPlan>,
    mounts: &mut Vec<MountPlan>,
) -> Result<()> {
    let sock = std::env::var_os("GPG_AGENT_SOCK")
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".gnupg/S.gpg-agent")));
    let Some(sock) = sock.filter(|path| path.exists()) else {
        if required {
            return Err(OrbitError::refused(
                "gpg_socket_missing",
                "--forward-gpg requires GPG_AGENT_SOCK or ~/.gnupg/S.gpg-agent to exist",
                None,
            ));
        }
        return Ok(());
    };
    if let Err(err) = validate_socket_path(&sock, "GPG_AGENT_SOCK") {
        if required {
            return Err(err);
        }
        return Ok(());
    }
    if required || auto {
        mounts.push(plan_mount(
            &sock,
            GPG_AGENT_SOCK_TARGET,
            MountMode::Ro,
            MountCategory::Socket,
            if required {
                "explicit GPG agent forwarding; grants host signing power"
            } else {
                "agent GPG socket forwarding for host-equivalent Git signing; grants host signing power"
            },
            Some(workspace),
            true,
        )?);
        env.push(env_plan(
            "GPG_AGENT_SOCK",
            GPG_AGENT_SOCK_TARGET,
            EnvCategory::Agent,
            if required {
                "explicit GPG socket forwarding"
            } else {
                "agent GPG socket forwarding for host-equivalent Git signing"
            },
            false,
        ));
    }
    Ok(())
}

pub(super) fn validate_socket_path(path: &Path, name: &str) -> Result<()> {
    let canonical = path.canonicalize().map_err(|err| {
        OrbitError::refused(
            "socket_missing",
            format!("{name} `{}` cannot be inspected: {err}", path.display()),
            Some(path.to_path_buf()),
        )
    })?;
    reject_control_socket_path(&canonical, name)?;
    let metadata = std::fs::metadata(&canonical).map_err(|err| {
        OrbitError::refused(
            "socket_missing",
            format!(
                "{name} `{}` cannot be inspected: {err}",
                canonical.display()
            ),
            Some(canonical.clone()),
        )
    })?;
    if !is_unix_socket(&metadata) {
        return Err(OrbitError::refused(
            "socket_not_unix_socket",
            format!("{name} `{}` is not a Unix socket", canonical.display()),
            Some(canonical),
        ));
    }
    Ok(())
}

fn reject_control_socket_path(path: &Path, name: &str) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        file_name.as_str(),
        "docker.sock" | "podman.sock" | "buildkitd.sock" | "containerd.sock" | "cri-dockerd.sock"
    ) {
        return Err(OrbitError::refused(
            "socket_control_api_refused",
            format!(
                "{name} `{}` appears to be a container control socket, not an SSH/GPG agent socket",
                path.display()
            ),
            Some(path.to_path_buf()),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn is_unix_socket(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::FileTypeExt;
    metadata.file_type().is_socket()
}

#[cfg(not(unix))]
fn is_unix_socket(_metadata: &std::fs::Metadata) -> bool {
    false
}
