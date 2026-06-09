use crate::mount_policy::{is_secret_like, plan_mount};
use crate::plan::{MountCategory, MountMode, MountPlan};
use crate::{OrbitError, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const PI_HOME_TARGET: &str = "/home/orbit/.pi";
const PI_EXTENSION_TARGET_REASON: &str = "symlinked Pi extension target mounted read-only so ~/.pi/agent/extensions links resolve inside the container";

pub(super) fn materialized_pi_home_mounts_with_mode(
    home: &Path,
    workspace: &Path,
) -> Result<Vec<MountPlan>> {
    let pi_home = home.join(".pi");
    let Some(canonical_pi_home) = canonical_pi_home_for_mount(&pi_home)? else {
        return Ok(Vec::new());
    };
    reject_symlinked_pi_agent_dir(&pi_home)?;

    let mut mounts = vec![plan_mount(
        &canonical_pi_home,
        PI_HOME_TARGET,
        MountMode::Rw,
        MountCategory::State,
        "Pi home mounted read-write by default so global settings, packages, extensions, and sessions stay host-equivalent",
        Some(workspace),
        true,
    )?];
    mounts.extend(pi_extension_symlink_target_mounts(&pi_home, workspace)?);
    Ok(mounts)
}

fn pi_extension_symlink_target_mounts(pi_home: &Path, workspace: &Path) -> Result<Vec<MountPlan>> {
    let extensions_dir = pi_home.join("agent/extensions");
    let Some(extensions_metadata) = pi_extensions_metadata(&extensions_dir)? else {
        return Ok(Vec::new());
    };
    if !extensions_metadata.is_dir() {
        return Ok(Vec::new());
    }

    let mut targets = HashSet::new();
    let mut mounts = Vec::new();
    for entry in std::fs::read_dir(&extensions_dir).map_err(|err| {
        OrbitError::refused(
            "pi_extensions_dir_missing",
            format!(
                "pi extensions dir `{}` cannot be read: {err}",
                extensions_dir.display()
            ),
            Some(extensions_dir.clone()),
        )
    })? {
        let entry = entry.map_err(|err| {
            OrbitError::refused(
                "pi_extension_entry_missing",
                format!(
                    "pi extension entry under `{}` cannot be read: {err}",
                    extensions_dir.display()
                ),
                Some(extensions_dir.clone()),
            )
        })?;
        let link = entry.path();
        let metadata = std::fs::symlink_metadata(&link).map_err(|err| {
            OrbitError::refused(
                "pi_extension_entry_missing",
                format!(
                    "pi extension entry `{}` cannot be inspected: {err}",
                    link.display()
                ),
                Some(link.clone()),
            )
        })?;
        if !metadata.file_type().is_symlink() {
            continue;
        }
        let Some(container_target) = absolute_symlink_target(&link)? else {
            continue;
        };
        if !targets.insert(container_target.clone()) {
            continue;
        }
        let canonical_target = link.canonicalize().map_err(|err| {
            OrbitError::refused(
                "pi_extension_symlink_target_missing",
                format!(
                    "pi extension symlink `{}` target cannot be inspected: {err}",
                    link.display()
                ),
                Some(link.clone()),
            )
        })?;
        if is_secret_like(&canonical_target) {
            return Err(OrbitError::refused(
                "pi_extension_symlink_target_secret",
                format!(
                    "pi extension symlink `{}` target `{}` looks secret-like and is refused",
                    link.display(),
                    canonical_target.display()
                ),
                Some(canonical_target),
            ));
        }
        mounts.push(plan_mount(
            &canonical_target,
            &container_target,
            MountMode::Ro,
            MountCategory::State,
            PI_EXTENSION_TARGET_REASON,
            Some(workspace),
            true,
        )?);
    }
    Ok(mounts)
}

fn absolute_symlink_target(link: &Path) -> Result<Option<String>> {
    let target = std::fs::read_link(link).map_err(|err| {
        OrbitError::refused(
            "pi_extension_symlink_target_missing",
            format!(
                "pi extension symlink `{}` cannot be read: {err}",
                link.display()
            ),
            Some(link.to_path_buf()),
        )
    })?;
    if !target.is_absolute() {
        return Ok(None);
    }
    Ok(Some(target.display().to_string()))
}

fn canonical_pi_home_for_mount(pi_home: &Path) -> Result<Option<PathBuf>> {
    let Some(pi_metadata) = pi_home_metadata(pi_home)? else {
        return Ok(None);
    };
    if pi_metadata.file_type().is_symlink() {
        return Err(OrbitError::refused(
            "pi_home_symlink_escape",
            format!("pi home `{}` must not be a symlink", pi_home.display()),
            Some(pi_home.to_path_buf()),
        ));
    }
    if !pi_metadata.is_dir() {
        return Ok(None);
    }
    pi_home.canonicalize().map(Some).map_err(|err| {
        OrbitError::refused(
            "pi_home_missing",
            format!("pi home `{}` cannot be inspected: {err}", pi_home.display()),
            Some(pi_home.to_path_buf()),
        )
    })
}

fn reject_symlinked_pi_agent_dir(pi_home: &Path) -> Result<()> {
    let agent_dir = pi_home.join("agent");
    let Some(agent_metadata) = pi_agent_dir_metadata(&agent_dir)? else {
        return Ok(());
    };
    if agent_metadata.file_type().is_symlink() {
        return Err(OrbitError::refused(
            "pi_agent_symlink_escape",
            format!(
                "pi agent dir `{}` must not be a symlink",
                agent_dir.display()
            ),
            Some(agent_dir),
        ));
    }
    Ok(())
}

fn pi_home_metadata(pi_home: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(pi_home) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(OrbitError::refused(
            "pi_home_missing",
            format!("pi home `{}` cannot be inspected: {err}", pi_home.display()),
            Some(pi_home.to_path_buf()),
        )),
    }
}

fn pi_agent_dir_metadata(agent_dir: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(agent_dir) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(OrbitError::refused(
            "pi_agent_dir_missing",
            format!(
                "pi agent dir `{}` cannot be inspected: {err}",
                agent_dir.display()
            ),
            Some(agent_dir.to_path_buf()),
        )),
    }
}

fn pi_extensions_metadata(extensions_dir: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(extensions_dir) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(OrbitError::refused(
            "pi_extensions_dir_missing",
            format!(
                "pi extensions dir `{}` cannot be inspected: {err}",
                extensions_dir.display()
            ),
            Some(extensions_dir.to_path_buf()),
        )),
    }
}
