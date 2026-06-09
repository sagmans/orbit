use crate::error::{OrbitError, Result};
use crate::mount_policy::plan_mount;
use crate::plan::{GitMetadataRewrite, MountCategory, MountMode};
use std::path::{Path, PathBuf};

pub(super) struct GitMetadataPlan {
    pub(super) mounts: Vec<crate::plan::MountPlan>,
    pub(super) rewrites: Vec<GitMetadataRewrite>,
}

struct GitMetadataPath {
    path: PathBuf,
    absolute: bool,
}

pub(super) fn git_metadata_mounts(workspace: &Path) -> Result<GitMetadataPlan> {
    let git_path = workspace.join(".git");
    let mut rewrites = Vec::new();
    let git_dir = if git_path.is_dir() {
        git_path.canonicalize()?
    } else if git_path.is_file() {
        let parsed = parse_gitdir_file(workspace, &git_path)?;
        let git_dir = parsed.path.canonicalize()?;
        reject_git_metadata_source(&git_dir)?;
        if parsed.absolute {
            rewrites.push(GitMetadataRewrite {
                target: child_container_target(&container_workspace_target(workspace)?, ".git"),
                content: format!("gitdir: {}\n", container_workspace_target(&git_dir)?),
            });
        }
        git_dir
    } else {
        return Ok(GitMetadataPlan {
            mounts: Vec::new(),
            rewrites,
        });
    };
    reject_git_metadata_source(&git_dir)?;
    let common = git_common_dir(&git_dir)?;
    let common_dir = common.path;
    reject_git_metadata_source(&common_dir)?;
    if common.absolute {
        rewrites.push(GitMetadataRewrite {
            target: child_container_target(&container_workspace_target(&git_dir)?, "commondir"),
            content: format!("{}\n", container_workspace_target(&common_dir)?),
        });
    }
    let mut mounts = Vec::new();
    if common_dir != workspace {
        mounts.push(git_metadata_mount(&common_dir)?);
        mounts.extend(readonly_common_dir_children(&common_dir, workspace)?);
    }
    if git_dir != workspace && !git_dir.starts_with(&common_dir) {
        mounts.push(git_metadata_mount(&git_dir)?);
    }
    Ok(GitMetadataPlan { mounts, rewrites })
}

fn git_metadata_mount(source: &Path) -> Result<crate::plan::MountPlan> {
    let target = container_workspace_target(source)?;
    plan_mount(
        source,
        &target,
        MountMode::Rw,
        MountCategory::State,
        "git metadata mounted writable at its container project path so fetch, push tracking, and PR workflows can update FETCH_HEAD, refs, and config",
        None,
        true,
    )
}

fn readonly_common_dir_children(
    common_dir: &Path,
    workspace: &Path,
) -> Result<Vec<crate::plan::MountPlan>> {
    if common_dir.file_name().is_some_and(|name| name == ".git") {
        return Ok(Vec::new());
    }
    let Ok(entries) = std::fs::read_dir(common_dir) else {
        return Ok(Vec::new());
    };
    let mut paths = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            (!is_git_common_metadata_entry(name)).then_some(path)
        })
        .collect::<Vec<_>>();
    paths.sort();

    let mut mounts = Vec::new();
    for path in paths {
        if path.canonicalize().is_ok_and(|path| path == workspace) {
            continue;
        }
        let target = container_workspace_target(&path)?;
        mounts.push(plan_mount(
            &path,
            &target,
            MountMode::Ro,
            MountCategory::State,
            "non-Git child of writable Git common root over-mounted read-only to preserve source read-only defaults",
            None,
            true,
        )?);
    }
    Ok(mounts)
}

fn is_git_common_metadata_entry(name: &str) -> bool {
    matches!(
        name,
        "FETCH_HEAD"
            | "HEAD"
            | "branches"
            | "config"
            | "description"
            | "hooks"
            | "index"
            | "info"
            | "logs"
            | "objects"
            | "packed-refs"
            | "refs"
            | "shallow"
            | "worktrees"
    )
}

fn parse_gitdir_file(workspace: &Path, git_file: &Path) -> Result<GitMetadataPath> {
    let raw = std::fs::read_to_string(git_file)?;
    let Some(path) = raw.trim().strip_prefix("gitdir:") else {
        return Err(OrbitError::refused(
            "gitdir_invalid",
            format!(
                "git file `{}` does not contain a gitdir entry",
                git_file.display()
            ),
            Some(git_file.to_path_buf()),
        ));
    };
    let path = PathBuf::from(path.trim());
    let absolute = path.is_absolute();
    let path = if absolute { path } else { workspace.join(path) };
    Ok(GitMetadataPath { path, absolute })
}

fn git_common_dir(git_dir: &Path) -> Result<GitMetadataPath> {
    let commondir = git_dir.join("commondir");
    if !commondir.is_file() {
        return Ok(GitMetadataPath {
            path: git_dir.to_path_buf(),
            absolute: false,
        });
    }
    let raw = std::fs::read_to_string(&commondir)?;
    let path = PathBuf::from(raw.trim());
    let absolute = path.is_absolute();
    let common = if absolute { path } else { git_dir.join(path) };
    Ok(GitMetadataPath {
        path: common.canonicalize()?,
        absolute,
    })
}

fn reject_git_metadata_source(path: &Path) -> Result<()> {
    if crate::mount_policy::is_secret_like(path)
        || path.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|value| matches!(value, ".ssh" | ".gnupg" | ".pi"))
        })
    {
        return Err(OrbitError::refused(
            "git_metadata_refused",
            format!(
                "git metadata path `{}` resolves into a reserved or secret-like host path",
                path.display()
            ),
            Some(path.to_path_buf()),
        ));
    }
    Ok(())
}

pub(super) fn container_workspace_target(workspace: &Path) -> Result<String> {
    #[cfg(windows)]
    {
        let _ = workspace;
        Ok("/workspace".to_string())
    }
    #[cfg(not(windows))]
    {
        let home = crate::mount_policy::home_dir().and_then(|home| home.canonicalize().ok());
        container_workspace_target_with_home(workspace, home.as_deref())
    }
}

#[cfg(not(windows))]
pub(super) fn container_workspace_target_with_home(
    workspace: &Path,
    home: Option<&Path>,
) -> Result<String> {
    if let Some(home) = home
        && workspace != home
        && workspace.starts_with(home)
        && let Ok(relative) = workspace.strip_prefix(home)
        && relative.components().next().is_some()
    {
        let target = child_container_target("/home/orbit", &relative.to_string_lossy());
        reject_reserved_generated_target(&target)?;
        return Ok(target);
    }
    let target = workspace.display().to_string();
    reject_reserved_generated_target(&target)?;
    Ok(target)
}

fn child_container_target(base: &str, child: &str) -> String {
    let base = base.trim_end_matches('/');
    let child = child.trim_start_matches('/');
    if child.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{child}")
    }
}

fn reject_reserved_generated_target(target: &str) -> Result<()> {
    let target_path = Path::new(target);
    for reserved in RESERVED_CONTAINER_TARGETS {
        let reserved_path = Path::new(reserved);
        if target_path == reserved_path
            || target_path.starts_with(reserved_path)
            || reserved_path.starts_with(target_path)
        {
            return Err(OrbitError::refused(
                "reserved_generated_target",
                format!(
                    "generated project target `{target}` overlaps reserved container path `{reserved}`"
                ),
                None,
            ));
        }
    }
    Ok(())
}

const RESERVED_CONTAINER_TARGETS: &[&str] = &[
    "/home/orbit/.pi",
    "/home/orbit/.config/pi",
    "/home/orbit/.opencode",
    "/home/orbit/.config/opencode",
    "/home/orbit/.config/gh",
    "/home/orbit/.codex",
    "/home/orbit/.claude",
    "/home/orbit/.claude.json",
    "/home/orbit/.cursor",
    "/home/orbit/.config/cursor",
    "/home/orbit/.gemini",
    "/home/orbit/.config/gemini",
    "/home/orbit/.antigravity",
    "/home/orbit/.agy",
    "/home/orbit/.config/antigravity",
    "/home/orbit/.amp",
    "/home/orbit/.config/amp",
    "/home/orbit/.ssh",
    "/home/orbit/.gnupg",
    "/home/orbit/.gitconfig",
    "/home/orbit/.config/git",
    "/home/orbit/.git-hooks",
];
