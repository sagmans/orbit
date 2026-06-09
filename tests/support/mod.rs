#![allow(dead_code)]

use assert_cmd::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn orbit() -> Command {
    Command::cargo_bin("orbit").expect("orbit binary")
}

pub(crate) fn repo_root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("cwd");
    loop {
        if dir.join(".git").exists() {
            return dir.canonicalize().expect("canonical root");
        }
        assert!(dir.pop(), "test must run inside a git worktree");
    }
}

pub(crate) fn scoped_pi_session_path(sessions: &Path, workspace: &Path) -> PathBuf {
    workspace
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(encode_session_component(value)),
            std::path::Component::Prefix(prefix) => {
                Some(encode_session_component(prefix.as_os_str()))
            }
            _ => None,
        })
        .fold(sessions.join("orbit-scoped"), |path, component| {
            path.join(component)
        })
}

fn encode_session_component(value: &std::ffi::OsStr) -> String {
    let text = value.to_string_lossy();
    let encoded = text
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect::<String>();
    if encoded.is_empty() || encoded == "." || encoded == ".." {
        text.bytes().map(|byte| format!("%{byte:02X}")).collect()
    } else {
        encoded
    }
}

pub(crate) fn default_allowed_domains_env() -> &'static str {
    "ORBIT_ALLOWED_DOMAINS=api.linear.app,api.openai.com,auth.openai.com,chatgpt.com,context7.com,github.com,mcp.cloudflare.com,mcp.mdn.mozilla.net,registry.npmjs.org"
}

pub(crate) fn workspace_target(workspace: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    workspace_target_with_home(workspace, home.as_deref())
}

pub(crate) fn workspace_target_with_home(workspace: &Path, home: Option<&Path>) -> String {
    #[cfg(windows)]
    {
        let _ = (workspace, home);
        "/workspace".to_string()
    }
    #[cfg(not(windows))]
    {
        let home = home.and_then(|home| home.canonicalize().ok());
        if let Some(home) = home
            && workspace != home
            && workspace.starts_with(&home)
            && let Ok(relative) = workspace.strip_prefix(&home)
            && relative.components().next().is_some()
        {
            return format!("/home/orbit/{}", relative.display());
        }
        workspace.display().to_string()
    }
}

pub(crate) fn workspace_mount_arg(workspace: &Path) -> String {
    format!(
        "--mount type=bind,src={},dst={},readonly",
        workspace.display(),
        workspace_target(workspace)
    )
}

pub(crate) fn workspace_rw_mount_arg_with_home(workspace: &Path, home: Option<&Path>) -> String {
    format!(
        "--mount type=bind,src={},dst={}",
        workspace.display(),
        workspace_target_with_home(workspace, home)
    )
}

pub(crate) fn workspace_rw_mount_arg(workspace: &Path) -> String {
    workspace_rw_mount_arg_with_home(
        workspace,
        std::env::var_os("HOME").map(PathBuf::from).as_deref(),
    )
}

pub(crate) fn repo_git_common_dir() -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .ok()?;
    output.status.success().then(|| {
        PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
            .canonicalize()
            .expect("canonical git common dir")
    })
}

pub(crate) fn git_metadata_mount_arg(git_dir: &Path) -> String {
    format!(
        "--mount type=bind,src={},dst={}",
        git_dir.display(),
        workspace_target(git_dir)
    )
}

pub(crate) fn git_metadata_readonly_mount_arg(git_dir: &Path) -> String {
    format!("{},readonly", git_metadata_mount_arg(git_dir))
}
