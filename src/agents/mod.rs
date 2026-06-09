mod aliases;
mod home_state;
mod identity;
mod pi_state;
mod sockets;
mod targets;

use crate::Result;
use crate::mount_policy::home_dir;
use crate::plan::{BuildOptions, MountPlan};
use std::path::Path;

use home_state::agent_home_mounts;
use identity::{git_identity_mounts, gpg_identity_mounts, ssh_identity_mounts};
use sockets::{maybe_forward_gpg_socket, maybe_forward_ssh_socket};

pub use aliases::{ALIASES, agent_for_command, is_alias};
pub use targets::{
    GH_CONFIG_TARGET, GH_HOSTS_TARGET, GIT_CONFIG_DIR_TARGET, GIT_HOOKS_TARGET, GITCONFIG_TARGET,
    GNUPG_HOME_TARGET, GPG_AGENT_SOCK_TARGET, SSH_AUTH_SOCK_TARGET, SSH_HOME_TARGET,
};

pub fn manifest_mounts(
    agent: &str,
    options: &BuildOptions,
    workspace: &Path,
    env: &mut Vec<crate::plan::EnvPlan>,
) -> Result<Vec<MountPlan>> {
    let mut mounts = Vec::new();
    let home = home_dir();

    let auto_identity = agent != "generic";
    maybe_forward_ssh_socket(
        options.forward_ssh,
        auto_identity,
        workspace,
        env,
        &mut mounts,
    )?;
    maybe_forward_gpg_socket(
        options.forward_gpg,
        auto_identity,
        home.as_deref(),
        workspace,
        env,
        &mut mounts,
    )?;

    let Some(home) = home else {
        return Ok(mounts);
    };

    if agent != "generic" {
        mounts.extend(git_identity_mounts(&home, workspace)?);
        mounts.extend(ssh_identity_mounts(&home, workspace)?);
        mounts.extend(gpg_identity_mounts(&home, workspace)?);
        mounts.extend(agent_home_mounts(&home, workspace)?);
    }

    Ok(mounts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::MountMode;
    use tempfile::tempdir;

    #[cfg(test)]
    use pi_state::materialized_pi_home_mounts_with_mode;
    #[cfg(test)]
    use sockets::validate_socket_path;

    #[test]
    fn rejects_regular_files_as_forwarded_sockets() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("not-a-socket");
        std::fs::write(&file, "x").unwrap();
        let err = validate_socket_path(&file, "SSH_AUTH_SOCK").unwrap_err();
        assert!(err.to_string().contains("not a Unix socket"));
    }

    #[cfg(unix)]
    #[test]
    fn accepts_unix_socket_paths() {
        let dir = tempdir().unwrap();
        let socket = dir.path().join("agent.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        assert!(validate_socket_path(&socket, "SSH_AUTH_SOCK").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_container_control_sockets_as_forwarded_agent_sockets() {
        let dir = tempdir().unwrap();
        let socket = dir.path().join("podman.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let err = validate_socket_path(&socket, "SSH_AUTH_SOCK").unwrap_err();
        assert!(err.to_string().contains("container control socket"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_container_control_sockets_as_forwarded_agent_sockets() {
        let dir = tempdir().unwrap();
        let socket = dir.path().join("podman.sock");
        let link = dir.path().join("agent.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        std::os::unix::fs::symlink(&socket, &link).unwrap();

        let err = validate_socket_path(&link, "SSH_AUTH_SOCK").unwrap_err();

        assert!(err.to_string().contains("container control socket"));
    }

    #[test]
    fn recognizes_all_planned_aliases() {
        for alias in ALIASES {
            assert!(is_alias(alias));
        }
        assert!(!is_alias("vim"));
    }

    #[cfg(unix)]
    #[test]
    fn git_identity_mounts_skip_symlinks_to_raw_ssh_or_gpg_material() {
        for (relative, secret_target) in [
            (".gitconfig", ".ssh/id_ed25519"),
            (".config/git", ".ssh"),
            (".git-hooks", ".gnupg/private-keys-v1.d"),
        ] {
            let home = tempdir().unwrap();
            let workspace = tempdir().unwrap();
            std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
            std::fs::write(home.path().join(".ssh/id_ed25519"), "secret").unwrap();
            std::fs::create_dir_all(home.path().join(".gnupg/private-keys-v1.d")).unwrap();
            let link = home.path().join(relative);
            if let Some(parent) = link.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::os::unix::fs::symlink(home.path().join(secret_target), &link).unwrap();

            let mounts = git_identity_mounts(home.path(), workspace.path()).unwrap();
            assert!(mounts.is_empty());
        }
    }

    #[cfg(unix)]
    #[test]
    fn git_identity_mounts_allow_safe_symlinked_sources() {
        let home = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let hooks = outside.path().join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        std::fs::write(hooks.join("commit-msg"), "#!/bin/sh\n").unwrap();
        std::os::unix::fs::symlink(&hooks, home.path().join(".git-hooks")).unwrap();

        let mounts = git_identity_mounts(home.path(), workspace.path()).unwrap();

        let mount = mounts
            .iter()
            .find(|mount| mount.target == GIT_HOOKS_TARGET)
            .expect("git hooks mount");
        assert_eq!(mount.host_source, hooks.canonicalize().unwrap());
        assert_eq!(mount.mode, MountMode::Ro);
        assert!(mount.redacted);
    }

    #[test]
    fn pi_home_mounts_top_level_rw_without_copy_sources_or_subgroups() {
        let home = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let pi_home = home.path().join(".pi");
        let agent_dir = pi_home.join("agent");
        std::fs::create_dir_all(agent_dir.join("sessions/other-project")).unwrap();
        std::fs::create_dir_all(agent_dir.join("bin")).unwrap();
        std::fs::create_dir_all(agent_dir.join("npm/node_modules/pi-subagents")).unwrap();
        std::fs::write(agent_dir.join("settings.json"), r#"{"packages":[]}"#).unwrap();
        std::fs::write(agent_dir.join("tokens.json"), "token").unwrap();
        std::fs::write(pi_home.join("auth.json"), "legacy-token").unwrap();

        let mounts = materialized_pi_home_mounts_with_mode(home.path(), workspace.path()).unwrap();

        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].host_source, pi_home.canonicalize().unwrap());
        assert_eq!(mounts[0].target, "/home/orbit/.pi");
        assert_eq!(mounts[0].mode, MountMode::Rw);
    }

    #[cfg(unix)]
    #[test]
    fn pi_home_mounts_reject_symlinked_pi_home_or_agent_dir() {
        let home = tempdir().unwrap();
        let outside = tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), home.path().join(".pi")).unwrap();

        let err = materialized_pi_home_mounts_with_mode(home.path(), home.path()).unwrap_err();
        assert!(err.to_string().contains("pi home"));

        let home = tempdir().unwrap();
        let outside = tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".pi")).unwrap();
        std::os::unix::fs::symlink(outside.path(), home.path().join(".pi/agent")).unwrap();

        let err = materialized_pi_home_mounts_with_mode(home.path(), home.path()).unwrap_err();
        assert!(err.to_string().contains("pi agent dir"));
    }
}
