use crate::mount_policy::plan_staged_mount;
use crate::plan::{CleanupAction, CleanupStep, MountCategory, MountMode, RunPlan};
use crate::{OrbitError, Result};

pub(super) fn prepare_git_metadata_rewrites(plan: &mut RunPlan) -> Result<()> {
    prepare_git_metadata_rewrites_in_temp_root(plan, &std::env::temp_dir())
}

pub(super) fn prepare_git_metadata_rewrites_in_temp_root(
    plan: &mut RunPlan,
    temp_root: &std::path::Path,
) -> Result<()> {
    if plan.git_metadata_rewrites.is_empty() {
        return Ok(());
    }

    let dir = temp_root.join(format!(
        "orbit-git-metadata-{}-{}",
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

    let result = (|| {
        for (index, rewrite) in plan.git_metadata_rewrites.clone().iter().enumerate() {
            if plan
                .mounts
                .iter()
                .any(|mount| mount.target == rewrite.target)
            {
                return Err(OrbitError::refused(
                    "duplicate_mount_target",
                    format!("duplicate target `{}` is refused", rewrite.target),
                    None,
                ));
            }
            let file = dir.join(format!("rewrite-{index}"));
            std::fs::write(&file, &rewrite.content)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
            }
            plan.mounts.push(plan_staged_mount(
                &file,
                &dir,
                &rewrite.target,
                MountMode::Ro,
                MountCategory::State,
                "staged git metadata rewrite for container project path compatibility",
            )?);
        }
        Ok(())
    })();
    if let Err(err) = result {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(err);
    }

    plan.cleanup.push(CleanupStep {
        id: "git-metadata-rewrites".to_string(),
        action: CleanupAction::RemovePath,
        target: dir.display().to_string(),
    });
    Ok(())
}
