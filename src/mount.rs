use crate::error::{OrbitError, Result};
use crate::plan::{MountMode, MountPlan};
use std::path::{Path, PathBuf};

pub const AGENT_ALIASES: &[&str] = &[
    "pi",
    "opencode",
    "codex",
    "claude",
    "amp",
    "cursor-agent",
    "agy",
    "gemini",
];

/// Agent-specific $HOME directories to mount read-write inside the container.
/// Each entry: (relative path under $HOME, container target under /home/orbit).
const AGENT_HOME_PATHS: &[(&str, &str)] = &[
    (".pi", "/home/orbit/.pi"),
    (".config/pi", "/home/orbit/.config/pi"),
    (".opencode", "/home/orbit/.opencode"),
    (".config/opencode", "/home/orbit/.config/opencode"),
    (".config/gh", "/home/orbit/.config/gh"),
    (".codex", "/home/orbit/.codex"),
    (".claude", "/home/orbit/.claude"),
    (".claude.json", "/home/orbit/.claude.json"),
    (".cursor", "/home/orbit/.cursor"),
    (".config/cursor", "/home/orbit/.config/cursor"),
    (".gemini", "/home/orbit/.gemini"),
    (".config/gemini", "/home/orbit/.config/gemini"),
    (".antigravity", "/home/orbit/.antigravity"),
    (".agy", "/home/orbit/.agy"),
    (".config/antigravity", "/home/orbit/.config/antigravity"),
    (".amp", "/home/orbit/.amp"),
    (".config/amp", "/home/orbit/.config/amp"),
];

/// Read-only identity/config files mounted for all agent aliases.
const IDENTITY_PATHS: &[(&str, &str, &str)] = &[
    (".gitconfig", "/home/orbit/.gitconfig", "global git config"),
    (
        ".config/git",
        "/home/orbit/.config/git",
        "global git config includes",
    ),
    (".git-hooks", "/home/orbit/.git-hooks", "global git hooks"),
    (
        ".ssh",
        "/home/orbit/.ssh",
        "SSH keys and known_hosts for git auth",
    ),
    (".gnupg", "/home/orbit/.gnupg", "GPG keys for git signing"),
];

/// Paths that must never be mounted (container escape vectors).
const FORBIDDEN_TARGETS: &[&str] = &[
    "/",
    "/home",
    "/host",
    "/var/run/docker.sock",
    "/run/docker.sock",
];

pub fn is_alias(name: &str) -> bool {
    AGENT_ALIASES.contains(&name)
}

pub fn agent_for_command(command: &[String]) -> String {
    command
        .first()
        .filter(|name| is_alias(name))
        .cloned()
        .unwrap_or_else(|| "generic".to_string())
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Find the git worktree root containing `cwd`.
pub fn find_git_root(cwd: &Path) -> Result<PathBuf> {
    for ancestor in cwd.ancestors() {
        if ancestor.join(".git").exists() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(OrbitError::refused(format!(
        "current directory `{}` is not inside a git worktree",
        cwd.display()
    )))
}

/// Validate workspace is a real git repo and return its canonical path.
pub fn validate_workspace(path: &Path) -> Result<PathBuf> {
    reject_dangerous_path(path)?;
    let canonical = path
        .canonicalize()
        .map_err(|err| OrbitError::refused(format!("workspace cannot be read: {err}")))?;
    reject_dangerous_path(&canonical)?;
    if !canonical.join(".git").exists() {
        return Err(OrbitError::refused(format!(
            "workspace `{}` is not a git worktree root",
            canonical.display()
        )));
    }
    Ok(canonical)
}

/// Map a host workspace path to its container target.
/// The workspace is always mounted at its original host path inside the
/// container. This preserves git worktree absolute path references (the .git
/// file in a worktree contains an absolute path to the worktree admin dir,
/// which must resolve identically inside the container).
pub fn container_target(path: &Path) -> Result<String> {
    Ok(path.display().to_string())
}

/// Build the full list of mounts for a run.
pub fn build_mounts(
    agent: &str,
    workspace: &Path,
    explicit_mounts: &[crate::plan::ExplicitMount],
) -> Result<Vec<MountPlan>> {
    let mut mounts = Vec::new();

    // Workspace (read-write)
    let ws_target = container_target(workspace)?;
    mounts.push(plan_mount(
        workspace,
        &ws_target,
        MountMode::Rw,
        "workspace",
    )?);

    // Git worktree support: if .git is a file (not a directory), the workspace
    // is a linked worktree. The .git file contains an absolute path to the
    // worktree admin dir, which in turn references the main git dir via
    // commondir. Both must be mounted at their original host paths for git to
    // resolve references correctly inside the container.
    let git_path = workspace.join(".git");
    if git_path.is_file() {
        for git_mount in resolve_git_worktree_mounts(&git_path)? {
            // Skip if already covered by an existing mount (e.g. workspace itself)
            if !mounts.iter().any(|m| git_mount.starts_with(&m.host_source)) {
                let target = git_mount.display().to_string();
                mounts.push(plan_mount(
                    &git_mount,
                    &target,
                    MountMode::Ro,
                    "git worktree",
                )?);
            }
        }
    }

    let home = match home_dir() {
        Some(h) => h,
        None => return Ok(mounts),
    };

    if agent != "generic" {
        // Agent home dirs (read-write)
        for (relative, target) in AGENT_HOME_PATHS {
            let source = home.join(relative);
            if source.exists() {
                mounts.push(plan_mount(&source, target, MountMode::Rw, "agent state")?);
            }
        }
        // Identity/config dirs (read-only)
        for (relative, target, reason) in IDENTITY_PATHS {
            let source = home.join(relative);
            if source.exists() {
                mounts.push(plan_mount(&source, target, MountMode::Ro, reason)?);
            }
        }

        // Resolve absolute symlinks inside agent home dirs.
        // Agent config directories often contain symlinks to files in other
        // repos (e.g. dotfiles managers). Docker bind mounts preserve symlinks
        // as-is, so absolute symlinks break inside the container. We detect
        // them and mount their canonical targets at the same absolute path.
        let symlink_targets = find_symlink_targets(&mounts, &home)?;
        for target in symlink_targets {
            let dst = target.display().to_string();
            mounts.push(plan_mount(&target, &dst, MountMode::Ro, "symlink target")?);
        }
    }

    // Explicit user mounts
    for mount in explicit_mounts {
        mounts.push(plan_mount(
            &mount.source,
            &mount.target,
            mount.mode,
            "user mount",
        )?);
    }

    // Check for duplicate targets
    let mut seen = std::collections::HashSet::new();
    for mount in &mounts {
        if !seen.insert(mount.target.clone()) {
            return Err(OrbitError::refused(format!(
                "duplicate mount target `{}`",
                mount.target
            )));
        }
    }

    // Sort mounts by target path depth (shallowest first). This ensures
    // parent directories are mounted before children, so a RO parent mount
    // (e.g. main git dir) doesn't shadow a RW child mount (e.g. workspace).
    mounts.sort_by_key(|m| m.target.matches('/').count());

    Ok(mounts)
}

/// Resolve git worktree metadata paths that need to be mounted inside the
/// container. Given the workspace's `.git` file, reads the `gitdir:` pointer
/// to find the worktree admin directory, then reads the `commondir` file
/// inside it to find the main git directory. Returns both paths (canonical).
fn resolve_git_worktree_mounts(git_file: &Path) -> Result<Vec<PathBuf>> {
    let contents = std::fs::read_to_string(git_file).map_err(|err| {
        OrbitError::refused(format!(
            "cannot read .git file `{}`: {err}",
            git_file.display()
        ))
    })?;

    // Parse "gitdir: /path/to/worktree-admin"
    let gitdir_line = contents.trim();
    let gitdir_path = gitdir_line
        .strip_prefix("gitdir:")
        .map(|s| s.trim())
        .ok_or_else(|| {
            OrbitError::refused(format!(
                "unexpected .git file content (expected `gitdir: <path>`): {contents}"
            ))
        })?;

    let gitdir = PathBuf::from(gitdir_path);
    let gitdir_canonical = gitdir.canonicalize().map_err(|err| {
        OrbitError::refused(format!(
            "git worktree admin dir `{}` not accessible: {err}",
            gitdir.display()
        ))
    })?;

    let mut paths = vec![gitdir_canonical.clone()];

    // Read commondir to find the main git dir
    let commondir_file = gitdir_canonical.join("commondir");
    if let Ok(commondir_contents) = std::fs::read_to_string(&commondir_file) {
        let commondir_line = commondir_contents.trim();
        let commondir_path = PathBuf::from(commondir_line);
        // commondir can be relative (resolved against the gitdir) or absolute
        let commondir = if commondir_path.is_absolute() {
            commondir_path
        } else {
            gitdir_canonical.join(commondir_path)
        };
        if let Ok(commondir_canonical) = commondir.canonicalize() {
            paths.push(commondir_canonical);
        }
    }

    Ok(paths)
}

/// Walk mounted agent home directories and find absolute symlinks whose
/// targets need to be mounted at their original host paths inside the
/// container. Returns a minimal set of canonical directories that cover all
/// symlink targets. Uses common-ancestor grouping to avoid hundreds of
/// individual mounts when symlinks point into the same repo.
///
/// Filters out:
/// - Targets outside `$HOME` (safety: don't mount system paths)
/// - Targets already covered by an existing mount (e.g. a symlink from
///   `~/.pi/foo` to `~/.claude/bar` is already covered by the `.claude` mount)
fn find_symlink_targets(mounts: &[MountPlan], home: &Path) -> Result<Vec<PathBuf>> {
    let home_canonical = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let mut target_dirs: Vec<PathBuf> = Vec::new();

    for mount in mounts {
        if mount.reason != "agent state" && mount.reason != "identity" {
            continue;
        }
        walk_for_abs_symlinks(&mount.host_source, &mut target_dirs)?;
    }

    // Deduplicate
    target_dirs.sort();
    target_dirs.dedup();

    // Filter out:
    // 1. Targets outside $HOME (safety)
    // 2. Targets already covered by an existing mount
    target_dirs.retain(|dir| {
        // Must be under $HOME
        if !dir.starts_with(&home_canonical) {
            return false;
        }
        // Skip if already covered by an existing mount's source
        let already_covered = mounts.iter().any(|m| dir.starts_with(&m.host_source));
        if already_covered {
            return false;
        }
        true
    });

    if target_dirs.is_empty() {
        return Ok(Vec::new());
    }

    // Group targets by common ancestor to minimize mount count.
    let minimal = merge_common_ancestors(target_dirs);

    Ok(minimal)
}

/// Merge a sorted list of directories by replacing groups that share a
/// close common ancestor with that ancestor. The ancestor must be at least
/// `MIN_DEPTH` path components deep to avoid mounting overly broad paths.
fn merge_common_ancestors(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    const MIN_DEPTH: usize = 4;

    if dirs.len() <= 1 {
        return dirs;
    }

    // First pass: remove exact subdirectories of other entries
    let mut deduped: Vec<PathBuf> = Vec::new();
    for dir in &dirs {
        let is_subdir = deduped
            .iter()
            .any(|existing| dir.starts_with(existing) && dir != existing);
        if !is_subdir {
            deduped.push(dir.clone());
        }
    }

    if deduped.len() <= 1 {
        return deduped;
    }

    // Second pass: find common ancestor of all remaining dirs.
    // If the common ancestor is deep enough (>= MIN_DEPTH components),
    // replace everything with it.
    let common = common_ancestor(&deduped);
    if let Some(common) = common {
        let depth = common.components().count();
        if depth >= MIN_DEPTH {
            return vec![common];
        }
    }

    // Fall back to the deduplicated list if no valid common ancestor
    deduped
}

/// Compute the longest common ancestor directory of a set of paths.
fn common_ancestor(paths: &[PathBuf]) -> Option<PathBuf> {
    if paths.is_empty() {
        return None;
    }
    let mut first: PathBuf = paths[0].clone();
    loop {
        if paths.iter().all(|p| p.starts_with(&first)) {
            return Some(first);
        }
        if !first.pop() {
            return None;
        }
    }
}

/// Recursively walk a directory and collect canonical target directories for
/// absolute symlinks. Relative symlinks are skipped (they resolve within the
/// mounted tree). For file symlinks, the parent directory of the target is
/// collected. For directory symlinks, the target directory itself is collected.
fn walk_for_abs_symlinks(dir: &Path, targets: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // skip unreadable dirs
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.file_type().is_symlink() {
            // Read the raw symlink target (not resolved)
            let link_target = match std::fs::read_link(&path) {
                Ok(t) => t,
                Err(_) => continue,
            };

            // Only handle absolute symlinks; relative ones work inside the mount
            if link_target.is_absolute() {
                // Resolve to canonical target
                if let Ok(canonical) = link_target.canonicalize() {
                    if canonical.is_dir() {
                        targets.push(canonical);
                    } else {
                        // File symlink: mount the parent directory
                        if let Some(parent) = canonical.parent() {
                            targets.push(parent.to_path_buf());
                        }
                    }
                }
            }
        } else if metadata.is_dir() {
            // Recurse into subdirectories
            walk_for_abs_symlinks(&path, targets)?;
        }
    }

    Ok(())
}

/// Create a validated mount plan. This is the core security gate.
fn plan_mount(source: &Path, target: &str, mode: MountMode, reason: &str) -> Result<MountPlan> {
    // Reject mount grammar injection (commas, control chars in paths)
    reject_mount_grammar(source, target)?;

    // Reject obviously dangerous paths before canonicalization
    reject_dangerous_path(source)?;
    reject_dangerous_target(target)?;

    // Canonicalize source
    let canonical = source.canonicalize().map_err(|err| {
        OrbitError::refused(format!("mount source `{}`: {err}", source.display()))
    })?;

    // Reject dangerous canonical paths (e.g. symlink to /etc)
    reject_dangerous_path(&canonical)?;

    // Reject docker socket by filename
    if canonical
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name == "docker.sock" || name == "podman.sock")
    {
        return Err(OrbitError::refused(
            "container control socket mount refused",
        ));
    }

    Ok(MountPlan {
        source: canonical.display().to_string(),
        host_source: canonical,
        target: target.to_string(),
        mode,
        reason: reason.to_string(),
    })
}

fn reject_mount_grammar(source: &Path, target: &str) -> Result<()> {
    let source_str = source.to_string_lossy();
    for value in [source_str.as_ref(), target] {
        if value.chars().any(|c| c == ',' || c.is_control()) {
            return Err(OrbitError::refused(
                "mount path contains Docker grammar or control characters",
            ));
        }
    }
    Ok(())
}

fn reject_dangerous_target(target: &str) -> Result<()> {
    if !target.starts_with('/') {
        return Err(OrbitError::refused(format!(
            "mount target `{target}` must be absolute"
        )));
    }
    if FORBIDDEN_TARGETS.contains(&target) {
        return Err(OrbitError::refused(format!(
            "mount target `{target}` is refused"
        )));
    }
    Ok(())
}

fn reject_dangerous_path(path: &Path) -> Result<()> {
    let s = path.to_string_lossy();

    // Root
    if s == "/" {
        return Err(OrbitError::refused("root mount refused"));
    }

    // Docker/containerd sockets (by filename, catches all paths)
    if s.ends_with("docker.sock")
        || s.ends_with("podman.sock")
        || s.ends_with("containerd.sock")
        || s.ends_with("buildkitd.sock")
    {
        return Err(OrbitError::refused(
            "container control socket mount refused",
        ));
    }

    // System config and kernel interface directories
    // Note: /var is NOT broadly blocked because macOS temp dirs live under
    // /var/folders (canonicalized as /private/var/folders). Only /var/run and
    // /var/log are blocked where container sockets and system logs reside.
    if s == "/etc"
        || s.starts_with("/etc/")
        || s == "/private/etc"
        || s.starts_with("/private/etc/")
        || s == "/var/run"
        || s.starts_with("/var/run/")
        || s == "/private/var/run"
        || s.starts_with("/private/var/run/")
        || s == "/var/log"
        || s.starts_with("/var/log/")
        || s == "/private/var/log"
        || s.starts_with("/private/var/log/")
        || s == "/run"
        || s.starts_with("/run/")
        || s == "/proc"
        || s.starts_with("/proc/")
        || s == "/sys"
        || s.starts_with("/sys/")
        || s == "/dev"
        || s.starts_with("/dev/")
        || s == "/Users"
        || s == "/home"
    {
        return Err(OrbitError::refused(format!(
            "dangerous system path `{s}` refused"
        )));
    }

    // Whole $HOME
    if let Some(home) = home_dir() {
        if path == home {
            return Err(OrbitError::refused(
                "whole $HOME mount refused; use agent aliases for specific dirs",
            ));
        }
        if let Ok(home_canonical) = home.canonicalize() {
            if path == home_canonical {
                return Err(OrbitError::refused(
                    "whole $HOME mount refused; use agent aliases for specific dirs",
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};
    use tempfile::tempdir;

    /// Serializes tests that manipulate the HOME env var to prevent
    /// cross-test interference when tests run in parallel.
    static HOME_MUTEX: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

    fn lock_home() -> MutexGuard<'static, ()> {
        HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn rejects_root_and_system_paths() {
        assert!(reject_dangerous_path(Path::new("/")).is_err());
        assert!(reject_dangerous_path(Path::new("/etc")).is_err());
        assert!(reject_dangerous_path(Path::new("/etc/hosts")).is_err());
        assert!(reject_dangerous_path(Path::new("/var/run/docker.sock")).is_err());
        assert!(reject_dangerous_path(Path::new("/proc")).is_err());
        assert!(reject_dangerous_path(Path::new("/sys")).is_err());
    }

    #[test]
    fn rejects_docker_socket_by_filename() {
        let dir = tempdir().unwrap();
        let sock = dir.path().join("docker.sock");
        std::fs::write(&sock, "x").unwrap();
        assert!(plan_mount(&sock, "/x", MountMode::Ro, "test").is_err());
    }

    #[test]
    fn rejects_mount_grammar_injection() {
        let dir = tempdir().unwrap();
        let safe = dir.path().join("safe.txt");
        std::fs::write(&safe, "x").unwrap();

        // Comma in target
        let err = plan_mount(&safe, "/safe,evil", MountMode::Ro, "test").unwrap_err();
        assert!(err.to_string().contains("grammar"));

        // Comma in source path
        let bad = dir.path().join("bad,name");
        std::fs::write(&bad, "x").unwrap();
        let err = plan_mount(&bad, "/safe", MountMode::Ro, "test").unwrap_err();
        assert!(err.to_string().contains("grammar"));
    }

    #[test]
    fn rejects_relative_targets() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("config");
        std::fs::write(&file, "x").unwrap();
        assert!(plan_mount(&file, "relative", MountMode::Ro, "test").is_err());
    }

    #[test]
    fn rejects_forbidden_targets() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("data");
        std::fs::write(&file, "x").unwrap();
        for target in ["/", "/home", "/var/run/docker.sock"] {
            assert!(plan_mount(&file, target, MountMode::Ro, "test").is_err());
        }
    }

    #[test]
    fn workspace_must_be_git_repo() {
        let dir = tempdir().unwrap();
        assert!(validate_workspace(dir.path()).is_err());
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(validate_workspace(dir.path()).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_to_system_path() {
        let dir = tempdir().unwrap();
        let link = dir.path().join("etc-link");
        std::os::unix::fs::symlink("/etc", &link).unwrap();
        assert!(plan_mount(&link, "/x", MountMode::Ro, "test").is_err());
    }

    #[test]
    fn detects_all_aliases() {
        for alias in AGENT_ALIASES {
            assert!(is_alias(alias));
        }
        assert!(!is_alias("vim"));
    }

    #[cfg(not(windows))]
    #[test]
    fn home_child_workspace_keeps_absolute() {
        let orig_home = std::env::var_os("HOME");
        let home = tempdir().unwrap();
        // SAFETY: tests are single-threaded
        unsafe {
            std::env::set_var("HOME", home.path());
        }
        let workspace = home.path().join("projects/myapp");
        std::fs::create_dir_all(&workspace).unwrap();
        let canonical = workspace.canonicalize().unwrap();
        let target = container_target(&canonical).unwrap();
        unsafe {
            std::env::remove_var("HOME");
            if let Some(home) = orig_home {
                std::env::set_var("HOME", home);
            }
        }
        // Workspace is always mounted at its original path (no remapping)
        assert_eq!(target, canonical.display().to_string());
    }

    #[cfg(not(windows))]
    #[test]
    fn non_home_workspace_keeps_absolute() {
        let workspace = tempdir().unwrap();
        let target = container_target(workspace.path()).unwrap();
        assert_eq!(target, workspace.path().display().to_string());
    }

    #[test]
    fn rejects_duplicate_targets() {
        let _guard = lock_home();
        let orig_home = std::env::var_os("HOME");
        let home = tempdir().unwrap();
        std::fs::create_dir(home.path().join(".pi")).unwrap();
        std::fs::create_dir(home.path().join(".codex")).unwrap();

        let workspace = tempdir().unwrap();
        std::fs::create_dir(workspace.path().join(".git")).unwrap();

        // SAFETY: tests are single-threaded
        unsafe {
            std::env::set_var("HOME", home.path());
        }
        let mounts = build_mounts("pi", workspace.path(), &[]).unwrap();
        unsafe {
            std::env::remove_var("HOME");
            if let Some(home) = orig_home {
                std::env::set_var("HOME", home);
            }
        }

        let mut targets = mounts.iter().map(|m| &m.target).collect::<Vec<_>>();
        targets.sort();
        targets.dedup();
        assert_eq!(targets.len(), mounts.len());
    }

    #[test]
    fn common_ancestor_of_siblings() {
        let paths = vec![
            PathBuf::from("/a/b/c/d1"),
            PathBuf::from("/a/b/c/d2"),
            PathBuf::from("/a/b/c/d3"),
        ];
        assert_eq!(common_ancestor(&paths), Some(PathBuf::from("/a/b/c")));
    }

    #[test]
    fn common_ancestor_of_nested() {
        let paths = vec![PathBuf::from("/a/b/c"), PathBuf::from("/a/b/c/d/e")];
        assert_eq!(common_ancestor(&paths), Some(PathBuf::from("/a/b/c")));
    }

    #[test]
    fn common_ancestor_disjoint() {
        let paths = vec![PathBuf::from("/a/b"), PathBuf::from("/x/y")];
        assert_eq!(common_ancestor(&paths), Some(PathBuf::from("/")));
    }

    #[test]
    fn merge_collapses_to_common_ancestor() {
        let dirs = vec![
            PathBuf::from("/Users/test/source/repo/main/coding-agent/pi"),
            PathBuf::from("/Users/test/source/repo/main/coding-agent/opencode"),
            PathBuf::from("/Users/test/source/repo/main/skills/ship"),
            PathBuf::from("/Users/test/source/repo/main/skills/tdd"),
        ];
        let merged = merge_common_ancestors(dirs);
        assert_eq!(merged, vec![PathBuf::from("/Users/test/source/repo/main")]);
    }

    #[test]
    fn merge_keeps_subdirs_removed() {
        let dirs = vec![
            PathBuf::from("/Users/test/source/repo/main"),
            PathBuf::from("/Users/test/source/repo/main/coding-agent"),
        ];
        let merged = merge_common_ancestors(dirs);
        assert_eq!(merged, vec![PathBuf::from("/Users/test/source/repo/main")]);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_targets_resolved_in_build_mounts() {
        let _guard = lock_home();
        let orig_home = std::env::var_os("HOME");
        let home = tempdir().unwrap();
        let home_canonical = home.path().canonicalize().unwrap();

        // Create a "real files" directory outside ~/.pi
        let real_dir = home.path().join("dotfiles/pi-config");
        std::fs::create_dir_all(&real_dir).unwrap();
        std::fs::write(real_dir.join("settings.json"), r#"{"theme":"test"}"#).unwrap();
        std::fs::create_dir_all(real_dir.join("themes")).unwrap();
        std::fs::write(real_dir.join("themes/custom.json"), "{}").unwrap();

        // Create ~/.pi with a symlink to the real files
        let pi_dir = home.path().join(".pi");
        std::fs::create_dir_all(pi_dir.join("agent")).unwrap();
        std::os::unix::fs::symlink(&real_dir, pi_dir.join("agent/themes")).unwrap();

        let workspace = tempdir().unwrap();
        std::fs::create_dir(workspace.path().join(".git")).unwrap();

        unsafe {
            std::env::set_var("HOME", home.path());
        }
        let mounts = build_mounts("pi", workspace.path(), &[]).unwrap();
        unsafe {
            std::env::remove_var("HOME");
            if let Some(home) = orig_home {
                std::env::set_var("HOME", home);
            }
        }

        // Should have a mount for the symlink target (the real_dir or its parent)
        let symlink_mounts: Vec<_> = mounts
            .iter()
            .filter(|m| m.reason == "symlink target")
            .collect();
        assert!(
            !symlink_mounts.is_empty(),
            "expected at least one symlink target mount, got: {:?}",
            mounts.iter().map(|m| &m.reason).collect::<Vec<_>>()
        );

        // The symlink target mount should point to the real_dir path
        let found = symlink_mounts.iter().any(|m| {
            m.host_source == real_dir.canonicalize().unwrap()
                || m.host_source == home_canonical.join("dotfiles")
        });
        assert!(
            found,
            "symlink target mount should cover the real files dir"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_outside_home_not_mounted() {
        let _guard = lock_home();
        let orig_home = std::env::var_os("HOME");
        let home = tempdir().unwrap();

        // Create a temp dir outside $HOME to use as symlink target
        let outside = tempdir().unwrap();
        std::fs::write(outside.path().join("data.txt"), "x").unwrap();

        // Create ~/.pi with a symlink pointing outside $HOME
        let pi_dir = home.path().join(".pi");
        std::fs::create_dir_all(pi_dir.join("agent")).unwrap();
        std::os::unix::fs::symlink(outside.path(), pi_dir.join("agent/external")).unwrap();

        let workspace = tempdir().unwrap();
        std::fs::create_dir(workspace.path().join(".git")).unwrap();

        unsafe {
            std::env::set_var("HOME", home.path());
        }
        let mounts = build_mounts("pi", workspace.path(), &[]).unwrap();
        unsafe {
            std::env::remove_var("HOME");
            if let Some(home) = orig_home {
                std::env::set_var("HOME", home);
            }
        }

        // Should NOT have any symlink target mount (target is outside $HOME)
        let symlink_mounts: Vec<_> = mounts
            .iter()
            .filter(|m| m.reason == "symlink target")
            .collect();
        assert!(
            symlink_mounts.is_empty(),
            "symlinks outside $HOME should not be mounted"
        );
    }

    #[cfg(unix)]
    #[test]
    fn git_worktree_mounts_added() {
        let _guard = lock_home();
        let orig_home = std::env::var_os("HOME");
        let home = tempdir().unwrap();

        // Create a bare-style main git dir
        let main_git = home.path().join("myrepo");
        std::fs::create_dir_all(&main_git).unwrap();
        std::fs::create_dir_all(main_git.join("objects")).unwrap();
        std::fs::create_dir_all(main_git.join("refs")).unwrap();
        std::fs::write(main_git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(main_git.join("config"), "[core]\n\tbare = true\n").unwrap();

        // Create worktree admin dir
        let wt_admin = main_git.join("worktrees").join("wt");
        std::fs::create_dir_all(&wt_admin).unwrap();
        std::fs::write(wt_admin.join("commondir"), "../..\n").unwrap();
        std::fs::write(wt_admin.join("HEAD"), "ref: refs/heads/main\n").unwrap();

        // Create worktree workspace with .git file
        let workspace = home.path().join("workspace-wt");
        std::fs::create_dir_all(&workspace).unwrap();
        let git_file = workspace.join(".git");
        std::fs::write(&git_file, format!("gitdir: {}\n", wt_admin.display())).unwrap();
        // Write gitdir back-reference in admin
        std::fs::write(
            wt_admin.join("gitdir"),
            format!("{}/.git\n", workspace.canonicalize().unwrap().display()),
        )
        .unwrap();

        unsafe {
            std::env::set_var("HOME", home.path());
        }
        let mounts = build_mounts("pi", &workspace.canonicalize().unwrap(), &[]).unwrap();
        unsafe {
            std::env::remove_var("HOME");
            if let Some(home) = orig_home {
                std::env::set_var("HOME", home);
            }
        }

        // Should have git worktree mounts for the admin dir and main git dir
        let wt_mounts: Vec<_> = mounts
            .iter()
            .filter(|m| m.reason == "git worktree")
            .collect();
        assert!(
            wt_mounts.len() >= 2,
            "expected at least 2 git worktree mounts, got {}",
            wt_mounts.len()
        );

        // Verify the worktree admin dir is mounted at its original path
        let wt_admin_canonical = wt_admin.canonicalize().unwrap();
        assert!(
            wt_mounts
                .iter()
                .any(|m| m.host_source == wt_admin_canonical),
            "worktree admin dir should be mounted"
        );

        // Verify the main git dir is mounted at its original path
        let main_git_canonical = main_git.canonicalize().unwrap();
        assert!(
            wt_mounts
                .iter()
                .any(|m| m.host_source == main_git_canonical),
            "main git dir should be mounted"
        );
    }

    #[test]
    fn non_worktree_git_no_extra_mounts() {
        let _guard = lock_home();
        let orig_home = std::env::var_os("HOME");
        let home = tempdir().unwrap();

        // Standard repo with .git directory (not a worktree)
        let workspace = home.path().join("myproject");
        std::fs::create_dir_all(workspace.join(".git")).unwrap();

        unsafe {
            std::env::set_var("HOME", home.path());
        }
        let mounts = build_mounts("pi", &workspace.canonicalize().unwrap(), &[]).unwrap();
        unsafe {
            std::env::remove_var("HOME");
            if let Some(home) = orig_home {
                std::env::set_var("HOME", home);
            }
        }

        // Should NOT have any git worktree mounts
        let wt_mounts: Vec<_> = mounts
            .iter()
            .filter(|m| m.reason == "git worktree")
            .collect();
        assert!(
            wt_mounts.is_empty(),
            "standard repos should not get git worktree mounts"
        );
    }
}
