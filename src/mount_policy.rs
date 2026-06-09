use crate::error::{OrbitError, Result};
use crate::plan::{MountCategory, MountMode, MountPlan};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn validate_workspace_root(raw_workspace: &Path) -> Result<PathBuf> {
    reject_mount_grammar_path("workspace", raw_workspace)?;
    reject_raw_dangerous_path(raw_workspace, MountCategory::Workspace)?;
    let canonical = canonicalize_source(raw_workspace, "workspace_missing", "workspace")?;
    reject_mount_grammar_path("workspace", &canonical)?;
    reject_canonical_dangerous_path(&canonical, MountCategory::Workspace)?;
    if !canonical.join(".git").exists() {
        return Err(OrbitError::refused(
            "workspace_not_repo",
            format!(
                "workspace `{}` is refused because it is not a git worktree root",
                canonical.display()
            ),
            Some(canonical),
        ));
    }
    Ok(canonical)
}

pub fn plan_mount(
    raw_source: &Path,
    target: &str,
    mode: MountMode,
    category: MountCategory,
    reason: impl Into<String>,
    workspace_root: Option<&Path>,
    allow_outside_workspace: bool,
) -> Result<MountPlan> {
    reject_mount_grammar_path("mount source", raw_source)?;
    reject_raw_dangerous_path(raw_source, category)?;
    reject_target(target)?;

    let canonical = canonicalize_source(raw_source, "mount_source_missing", "mount source")?;
    reject_mount_grammar_path("mount source", &canonical)?;
    reject_canonical_dangerous_path(&canonical, category)?;

    if let Some(workspace_root) = workspace_root {
        let raw_workspace = workspace_root.to_path_buf();
        let workspace = workspace_root.canonicalize()?;
        if (raw_source.starts_with(&raw_workspace) || raw_source.starts_with(workspace.as_path()))
            && !canonical.starts_with(&workspace)
        {
            return Err(OrbitError::refused(
                "symlink_escape",
                format!(
                    "symlink escape refused: `{}` resolves outside workspace",
                    raw_source.display()
                ),
                Some(canonical),
            ));
        }
        if !allow_outside_workspace && !canonical.starts_with(&workspace) {
            return Err(OrbitError::refused(
                "unknown_host_path",
                format!(
                    "unknown host path refused: `{}` is outside workspace `{}`",
                    canonical.display(),
                    workspace.display()
                ),
                Some(canonical),
            ));
        }
    }

    mount_plan_from_canonical(canonical, target, mode, category, reason)
}

pub fn plan_staged_mount(
    raw_source: &Path,
    staging_root: &Path,
    target: &str,
    mode: MountMode,
    category: MountCategory,
    reason: impl Into<String>,
) -> Result<MountPlan> {
    reject_mount_grammar_path("mount source", raw_source)?;
    reject_mount_grammar_path("staging root", staging_root)?;
    reject_target(target)?;

    let root_metadata = std::fs::symlink_metadata(staging_root).map_err(|err| {
        OrbitError::refused(
            "staging_root_missing",
            format!(
                "staging root `{}` cannot be inspected: {err}",
                staging_root.display()
            ),
            Some(staging_root.to_path_buf()),
        )
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(OrbitError::refused(
            "staging_root_invalid",
            format!(
                "staging root `{}` must be a real directory",
                staging_root.display()
            ),
            Some(staging_root.to_path_buf()),
        ));
    }

    let canonical_root = canonicalize_source(staging_root, "staging_root_missing", "staging root")?;
    let Some(root_name) = canonical_root.file_name().and_then(|name| name.to_str()) else {
        return Err(OrbitError::refused(
            "staging_root_invalid",
            format!("staging root `{}` is refused", canonical_root.display()),
            Some(canonical_root),
        ));
    };
    if !root_name.starts_with("orbit-") {
        return Err(OrbitError::refused(
            "staging_root_invalid",
            format!(
                "staging root `{}` must be orbit-owned",
                canonical_root.display()
            ),
            Some(canonical_root),
        ));
    }

    let canonical = canonicalize_source(raw_source, "mount_source_missing", "mount source")?;
    reject_mount_grammar_path("mount source", &canonical)?;
    if canonical != canonical_root && !canonical.starts_with(&canonical_root) {
        return Err(OrbitError::refused(
            "staged_mount_escape",
            format!(
                "staged mount source `{}` resolves outside staging root `{}`",
                canonical.display(),
                canonical_root.display()
            ),
            Some(canonical),
        ));
    }
    if is_docker_socket(&canonical) || is_runtime_socket_path_refused(&canonical, category) {
        return Err(OrbitError::refused(
            "staged_mount_refused",
            format!("staged mount source `{}` is refused", canonical.display()),
            Some(canonical),
        ));
    }

    mount_plan_from_canonical(canonical, target, mode, category, reason)
}

fn mount_plan_from_canonical(
    canonical: PathBuf,
    target: &str,
    mode: MountMode,
    category: MountCategory,
    reason: impl Into<String>,
) -> Result<MountPlan> {
    let redacted = is_secret_like(&canonical)
        || matches!(category, MountCategory::Secret | MountCategory::Socket);
    let source = if redacted {
        redact_path(&canonical)
    } else {
        canonical.display().to_string()
    };

    Ok(MountPlan {
        source,
        host_source: canonical,
        target: target.to_string(),
        mode,
        category,
        reason: reason.into(),
        redacted,
    })
}

pub fn check_duplicate_targets(mounts: &[MountPlan]) -> Result<()> {
    let mut seen = HashSet::new();
    for mount in mounts {
        if !seen.insert(mount.target.clone()) {
            return Err(OrbitError::refused(
                "duplicate_mount_target",
                format!("duplicate target `{}` is refused", mount.target),
                None,
            ));
        }
    }
    Ok(())
}

pub fn check_duplicate_explicit_targets(targets: &[String]) -> Result<()> {
    let mut seen = HashSet::new();
    for target in targets {
        if !seen.insert(target.clone()) {
            return Err(OrbitError::refused(
                "duplicate_mount_target",
                format!("duplicate target `{target}` is refused"),
                None,
            ));
        }
    }
    Ok(())
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn canonicalize_source(path: &Path, code: &'static str, label: &str) -> Result<PathBuf> {
    path.canonicalize().map_err(|err| {
        OrbitError::refused(
            code,
            format!(
                "{label} `{}` cannot be canonicalized: {err}",
                path.display()
            ),
            Some(path.to_path_buf()),
        )
    })
}

fn reject_target(target: &str) -> Result<()> {
    reject_mount_grammar_value("mount target", target, None)?;
    if !target.starts_with('/') {
        return Err(OrbitError::refused(
            "relative_mount_target",
            format!("mount target `{target}` must be absolute"),
            None,
        ));
    }
    if target == "/" || target == "/home" || target == "/host" {
        return Err(OrbitError::refused(
            "broad_mount_target",
            format!("mount target `{target}` is too broad"),
            None,
        ));
    }
    if target == "/var/run/docker.sock" || target == "/run/docker.sock" {
        return Err(OrbitError::refused(
            "docker_socket_target",
            "docker socket target is refused",
            None,
        ));
    }
    Ok(())
}

fn reject_mount_grammar_path(label: &str, path: &Path) -> Result<()> {
    let value = path.to_string_lossy();
    reject_mount_grammar_value(label, &value, Some(path.to_path_buf()))
}

fn reject_mount_grammar_value(label: &str, value: &str, path: Option<PathBuf>) -> Result<()> {
    if value.chars().any(|c| c == ',' || c.is_control()) {
        return Err(OrbitError::refused(
            "mount_grammar_character",
            format!("{label} contains Docker mount grammar/control characters"),
            path,
        ));
    }
    Ok(())
}

fn reject_raw_dangerous_path(path: &Path, category: MountCategory) -> Result<()> {
    let s = path.to_string_lossy();
    if s == "/" {
        return Err(OrbitError::refused(
            "root_mount",
            "root mount `/` is refused",
            Some(path.to_path_buf()),
        ));
    }
    if is_docker_socket(path) {
        return Err(OrbitError::refused(
            "docker_socket",
            "docker socket mount is refused",
            Some(path.to_path_buf()),
        ));
    }
    if is_runtime_socket_path_refused(path, category) {
        return Err(OrbitError::refused(
            "docker_socket_parent",
            "docker socket parent mount is refused",
            Some(path.to_path_buf()),
        ));
    }
    if s == "/Users"
        || s == "/home"
        || s == "/private"
        || s == "/var"
        || path.starts_with("/etc")
        || is_macos_dangerous_alias(path, category)
    {
        return Err(OrbitError::refused(
            "dangerous_parent",
            format!("dangerous parent path `{s}` is refused"),
            Some(path.to_path_buf()),
        ));
    }
    if let Some(home) = home_dir()
        && path == home
    {
        return Err(OrbitError::refused(
            "home_mount",
            "home directory mount is refused",
            Some(path.to_path_buf()),
        ));
    }
    Ok(())
}

fn reject_canonical_dangerous_path(path: &Path, category: MountCategory) -> Result<()> {
    if path == Path::new("/") {
        return Err(OrbitError::refused(
            "root_mount",
            "root mount `/` is refused",
            Some(path.to_path_buf()),
        ));
    }
    if is_docker_socket(path) {
        return Err(OrbitError::refused(
            "docker_socket",
            "docker socket mount is refused",
            Some(path.to_path_buf()),
        ));
    }
    if is_runtime_socket_path_refused(path, category)
        || path.starts_with("/etc")
        || is_macos_dangerous_alias(path, category)
        || path == Path::new("/Users")
        || path == Path::new("/home")
        || path == Path::new("/private")
        || path == Path::new("/var")
    {
        return Err(OrbitError::refused(
            "dangerous_parent",
            format!("dangerous parent path `{}` is refused", path.display()),
            Some(path.to_path_buf()),
        ));
    }
    if let Some(home) = home_dir().and_then(|p| p.canonicalize().ok())
        && path == home
    {
        return Err(OrbitError::refused(
            "home_mount",
            "home directory mount is refused",
            Some(path.to_path_buf()),
        ));
    }
    Ok(())
}

fn is_macos_dangerous_alias(path: &Path, category: MountCategory) -> bool {
    path.starts_with("/private/etc")
        || (path.starts_with("/private/var")
            && !(category == MountCategory::Socket && path.starts_with("/private/var/run/")))
}

fn is_runtime_socket_path_refused(path: &Path, category: MountCategory) -> bool {
    is_runtime_socket_parent(path)
        || (category != MountCategory::Socket && is_socket_parent_or_descendant(path))
}

fn is_runtime_socket_parent(path: &Path) -> bool {
    ["/var/run", "/private/var/run", "/run"]
        .iter()
        .map(Path::new)
        .any(|parent| path == parent)
}

fn is_socket_parent_or_descendant(path: &Path) -> bool {
    ["/var/run", "/private/var/run", "/run"]
        .iter()
        .map(Path::new)
        .any(|parent| path == parent || path.starts_with(parent))
}

fn is_docker_socket(path: &Path) -> bool {
    path == Path::new("/var/run/docker.sock")
        || path == Path::new("/private/var/run/docker.sock")
        || path == Path::new("/run/docker.sock")
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "docker.sock")
}

pub fn is_secret_like(path: &Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    [
        "secret",
        "token",
        "credential",
        "password",
        "id_rsa",
        "id_ed25519",
        "private_key",
        ".env",
        ".pem",
        ".key",
    ]
    .iter()
    .any(|needle| s.contains(needle))
}

pub fn redact_path(path: &Path) -> String {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("path");
    format!("<redacted:{name}>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn refuses_root_home_and_socket_before_canonicalization() {
        assert!(
            plan_mount(
                Path::new("/"),
                "/x",
                MountMode::Ro,
                MountCategory::Config,
                "test",
                None,
                true
            )
            .is_err()
        );
        assert!(
            plan_mount(
                Path::new("/var/run/docker.sock"),
                "/x",
                MountMode::Ro,
                MountCategory::Socket,
                "test",
                None,
                true
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_docker_mount_grammar_in_sources_and_targets() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("safe.txt");
        std::fs::write(&source, "x").unwrap();
        let err = plan_mount(
            &source,
            "/safe,src=/var/run/docker.sock,dst=/var/run/docker.sock",
            MountMode::Ro,
            MountCategory::Config,
            "test",
            None,
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("mount grammar"));

        let bad_source = dir.path().join("bad,src=evil");
        std::fs::write(&bad_source, "x").unwrap();
        let err = plan_mount(
            &bad_source,
            "/safe",
            MountMode::Ro,
            MountCategory::Config,
            "test",
            None,
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("mount grammar"));
    }

    #[test]
    fn refuses_socket_parents_and_broad_config_paths() {
        for path in [
            "/var/run",
            "/run",
            "/etc",
            "/private/var",
            "/private/var/log",
            "/private/etc",
            "/private/etc/hosts",
        ] {
            assert!(
                plan_mount(
                    Path::new(path),
                    "/x",
                    MountMode::Ro,
                    MountCategory::Config,
                    "test",
                    None,
                    true,
                )
                .is_err(),
                "{path} should be refused"
            );
        }
    }

    #[test]
    fn canonical_macos_dangerous_paths_are_refused() {
        for path in [
            "/private/var",
            "/private/var/log",
            "/private/etc",
            "/private/etc/hosts",
        ] {
            assert!(
                reject_canonical_dangerous_path(Path::new(path), MountCategory::Config).is_err(),
                "{path} should be refused"
            );
        }
    }

    #[test]
    fn socket_policy_allows_specific_runtime_sockets_only() {
        let runtime_socket = Path::new("/run/user/1000/ssh-agent.sock");
        assert!(reject_raw_dangerous_path(runtime_socket, MountCategory::Socket).is_ok());
        assert!(reject_canonical_dangerous_path(runtime_socket, MountCategory::Socket).is_ok());
        assert!(reject_raw_dangerous_path(runtime_socket, MountCategory::Config).is_err());
        assert!(reject_raw_dangerous_path(Path::new("/run"), MountCategory::Socket).is_err());
        assert!(
            reject_raw_dangerous_path(
                Path::new("/private/var/run/com.apple.launchd/Listeners"),
                MountCategory::Socket,
            )
            .is_ok()
        );
        assert!(
            reject_raw_dangerous_path(
                Path::new("/run/user/1000/docker.sock"),
                MountCategory::Socket,
            )
            .is_err()
        );
    }

    #[test]
    fn workspace_root_must_be_git_worktree() {
        let dir = tempdir().unwrap();
        let err = validate_workspace_root(dir.path()).unwrap_err();
        assert!(err.to_string().contains("not a git worktree"));
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(validate_workspace_root(dir.path()).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_escape_from_workspace() {
        let workspace = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let outside_file = outside.path().join("outside.conf");
        std::fs::write(&outside_file, "x").unwrap();
        let link = workspace.path().join("link.conf");
        std::os::unix::fs::symlink(&outside_file, &link).unwrap();

        let err = plan_mount(
            &link,
            "/config.conf",
            MountMode::Ro,
            MountCategory::Config,
            "test",
            Some(workspace.path()),
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("symlink escape"));
    }

    #[test]
    fn redacts_secret_like_mount_sources() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("secret-token.txt");
        std::fs::write(&file, "x").unwrap();
        let mount = plan_mount(
            &file,
            "/secret",
            MountMode::Ro,
            MountCategory::Secret,
            "test",
            None,
            true,
        )
        .unwrap();
        assert!(mount.redacted);
        assert!(mount.source.starts_with("<redacted:"));
    }

    #[test]
    fn staged_mounts_must_stay_under_orbit_owned_staging_roots() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("orbit-stage");
        std::fs::create_dir(&root).unwrap();
        let file = root.join("rewrite");
        std::fs::write(&file, "gitdir: /repo/.git\n").unwrap();
        let mount = plan_staged_mount(
            &file,
            &root,
            "/repo/.git",
            MountMode::Ro,
            MountCategory::State,
            "test",
        )
        .unwrap();
        assert_eq!(mount.host_source, file.canonicalize().unwrap());

        let outside = dir.path().join("outside");
        std::fs::write(&outside, "x").unwrap();
        let err = plan_staged_mount(
            &outside,
            &root,
            "/repo/.git",
            MountMode::Ro,
            MountCategory::State,
            "test",
        )
        .unwrap_err();
        assert!(err.to_string().contains("outside staging root"));

        let non_orbit_root = dir.path().join("stage");
        std::fs::create_dir(&non_orbit_root).unwrap();
        let file = non_orbit_root.join("rewrite");
        std::fs::write(&file, "x").unwrap();
        let err = plan_staged_mount(
            &file,
            &non_orbit_root,
            "/repo/.git",
            MountMode::Ro,
            MountCategory::State,
            "test",
        )
        .unwrap_err();
        assert!(err.to_string().contains("orbit-owned"));
    }
}
