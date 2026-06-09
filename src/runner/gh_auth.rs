use crate::Result;
use crate::agents::{GH_CONFIG_TARGET, GH_HOSTS_TARGET};
use crate::mount_policy::plan_staged_mount;
use crate::plan::{CleanupAction, CleanupStep, MountCategory, MountMode, RunPlan};
use std::process::{Command, Stdio};

pub(super) fn prepare_runtime_gh_auth_config(plan: &mut RunPlan) -> Result<()> {
    if plan.agent == "generic" {
        return Ok(());
    }
    let Some(config_mount) = plan
        .mounts
        .iter()
        .find(|mount| mount.target == GH_CONFIG_TARGET)
        .cloned()
    else {
        return Ok(());
    };
    let Some(token) = host_gh_auth_token()? else {
        return Ok(());
    };
    stage_runtime_gh_auth_config(plan, &config_mount.host_source, &token)
}

pub(super) fn stage_runtime_gh_auth_config(
    plan: &mut RunPlan,
    host_config: &std::path::Path,
    token: &str,
) -> Result<()> {
    stage_runtime_gh_auth_config_in_temp_root(plan, host_config, token, &std::env::temp_dir())
}

pub(super) fn stage_runtime_gh_auth_config_in_temp_root(
    plan: &mut RunPlan,
    host_config: &std::path::Path,
    token: &str,
    temp_root: &std::path::Path,
) -> Result<()> {
    let hosts = host_config.join("hosts.yml");
    let existing = match std::fs::read_to_string(&hosts) {
        Ok(existing) => existing,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err.into()),
    };
    let staged_hosts = merge_gh_hosts_token(&existing, token);

    let dir = temp_root.join(format!(
        "orbit-gh-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let stage_result: Result<()> = (|| {
        let host_config_yml = host_config.join("config.yml");
        if host_config_yml.is_file() {
            std::fs::copy(&host_config_yml, dir.join("config.yml"))?;
        }
        let staged_file = dir.join("hosts.yml");
        std::fs::write(&staged_file, staged_hosts)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staged_file, std::fs::Permissions::from_mode(0o600))?;
        }
        let staged_mount = plan_staged_mount(
            &dir,
            &dir,
            GH_CONFIG_TARGET,
            MountMode::Rw,
            MountCategory::Secret,
            "writable ephemeral GitHub CLI config with host keyring token for container gh auth and gh migrations",
        )?;
        plan.mounts
            .retain(|mount| mount.target != GH_CONFIG_TARGET && mount.target != GH_HOSTS_TARGET);
        plan.mounts.push(staged_mount);
        Ok(())
    })();
    if let Err(err) = stage_result {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(err);
    }
    plan.cleanup.push(CleanupStep {
        id: "gh-auth-config".to_string(),
        action: CleanupAction::RemovePath,
        target: dir.display().to_string(),
    });
    Ok(())
}

fn host_gh_auth_token() -> Result<Option<String>> {
    let output = match Command::new("gh")
        .args(["auth", "token", "-h", "github.com"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    if !output.status.success() {
        return Ok(None);
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() || token.chars().any(|c| c.is_control()) {
        return Ok(None);
    }
    Ok(Some(token))
}

pub(super) fn merge_gh_hosts_token(existing: &str, token: &str) -> String {
    let token_line = format!("    oauth_token: {token}");
    if existing.trim().is_empty() {
        return format!("github.com:\n{token_line}\n    git_protocol: ssh\n");
    }

    let mut output = Vec::new();
    let mut in_github = false;
    let mut inserted = false;
    let mut found_github = false;
    for line in existing.lines() {
        let is_top_level = !line.starts_with([' ', '\t']) && line.trim_end().ends_with(':');
        if is_top_level {
            if in_github && !inserted {
                output.push(token_line.clone());
                inserted = true;
            }
            in_github = line.trim() == "github.com:";
            found_github |= in_github;
            output.push(line.to_string());
            continue;
        }
        if in_github && line.trim_start().starts_with("oauth_token:") {
            if !inserted {
                output.push(token_line.clone());
                inserted = true;
            }
            continue;
        }
        output.push(line.to_string());
    }
    if in_github && !inserted {
        output.push(token_line);
    }
    if !found_github {
        output.push("github.com:".to_string());
        output.push(format!("    oauth_token: {token}"));
        output.push("    git_protocol: ssh".to_string());
    }
    format!("{}\n", output.join("\n"))
}
