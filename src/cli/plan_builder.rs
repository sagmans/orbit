use crate::agents;
use crate::config::detect_engine;
use crate::error::{OrbitError, Result};
use crate::mount_policy::{
    check_duplicate_explicit_targets, check_duplicate_targets, plan_mount, validate_workspace_root,
};
use crate::network::{
    NetworkMode, default_allowed_domains, normalize_allowed_domains, redact_proxy_url,
    validate_proxy_url, validate_restricted_command,
};
use crate::plan::{
    AuditEntry, AuditLevel, BuildOptions, CleanupAction, CleanupStep, Engine, EngineChoice,
    EnvCategory, HardeningPlan, MountCategory, MountMode, NetworkPlan, RunPlan, RunRequest,
    env_plan, runtime_env,
};
use std::path::{Path, PathBuf};

use super::git_metadata::{container_workspace_target, git_metadata_mounts};
use super::parse::validate_image_reference;

pub(super) fn build_plan(mut request: RunRequest, cwd: &Path) -> Result<RunPlan> {
    let engine = match request.options.engine {
        EngineChoice::Auto => detect_engine(),
        EngineChoice::Explicit(engine) => engine,
    };
    validate_image_reference("--image", &request.options.image)?;
    validate_image_reference("--proxy-image", &request.options.proxy_image)?;
    if let Some(proxy) = &request.options.proxy {
        validate_proxy_url(proxy)?;
    }

    let workspace_raw = match request.options.workspace.clone() {
        Some(workspace) => workspace,
        None => find_git_root(cwd)?,
    };
    let workspace = validate_workspace_root(&workspace_raw)?;

    let explicit_targets = request
        .options
        .explicit_mounts
        .iter()
        .map(|mount| mount.target.clone())
        .collect::<Vec<_>>();
    check_duplicate_explicit_targets(&explicit_targets)?;

    if request.options.network_mode == NetworkMode::Restricted {
        let mut allowed_domains = default_allowed_domains();
        allowed_domains.append(&mut request.options.allowed_domains);
        request.options.allowed_domains = allowed_domains;
        normalize_allowed_domains(&mut request.options.allowed_domains)?;
        let bypass_refusals =
            validate_restricted_command(&request.command, &request.options.allowed_domains)?;
        return finish_plan(request, engine, workspace, bypass_refusals);
    }
    request.options.allowed_domains.clear();
    finish_plan(request, engine, workspace, Vec::new())
}

fn finish_plan(
    request: RunRequest,
    engine: Engine,
    workspace: PathBuf,
    bypass_refusals: Vec<String>,
) -> Result<RunPlan> {
    let mut mounts = Vec::new();
    let workspace_mode = MountMode::Rw;
    let git_metadata = git_metadata_mounts(&workspace)?;
    let workspace_target = container_workspace_target(&workspace)?;
    let workspace_child_prefix = format!("{workspace_target}/");
    let (workspace_child_git_mounts, external_git_mounts): (Vec<_>, Vec<_>) = git_metadata
        .mounts
        .into_iter()
        .partition(|mount| mount.target.starts_with(&workspace_child_prefix));
    mounts.extend(external_git_mounts);
    mounts.push(plan_mount(
        &workspace,
        &workspace_target,
        workspace_mode,
        MountCategory::Workspace,
        "workspace mounted read-write by default so container commands and agents can edit the current worktree",
        None,
        true,
    )?);
    mounts.extend(workspace_child_git_mounts);

    for explicit in &request.options.explicit_mounts {
        mounts.push(plan_mount(
            &explicit.source,
            &explicit.target,
            explicit.mode,
            MountCategory::Config,
            "explicit user mount constrained to workspace",
            Some(&workspace),
            false,
        )?);
    }

    let mut env = vec![
        runtime_env("HOME", "/home/orbit", "container-owned home"),
        runtime_env(
            "XDG_CACHE_HOME",
            "/home/orbit/.cache",
            "container-owned cache",
        ),
        runtime_env("TMPDIR", "/home/orbit", "container-owned executable temp"),
        runtime_env(
            "PATH",
            "/usr/local/share/mise/installs/cargo-tools/bin:/usr/local/share/mise/cargo/bin:/usr/local/share/mise/shims:/usr/local/bin:/usr/bin:/bin:/usr/local/games:/usr/games",
            "pinned image tools and mise shims",
        ),
    ];

    mounts.extend(agents::manifest_mounts(
        &request.agent,
        &request.options,
        &workspace,
        &mut env,
    )?);
    check_duplicate_targets(&mounts)?;

    if request.options.network_mode == NetworkMode::Restricted {
        let allowed = request.options.allowed_domains.join(",");
        env.push(env_plan(
            "ORBIT_ALLOWED_DOMAINS",
            &allowed,
            EnvCategory::Proxy,
            "restricted network domain allowlist",
            false,
        ));
        let local_proxy = "http://127.0.0.1:18080";
        env.push(env_plan(
            "HTTP_PROXY",
            local_proxy,
            EnvCategory::Proxy,
            "forced local restricted-network proxy",
            false,
        ));
        env.push(env_plan(
            "HTTPS_PROXY",
            local_proxy,
            EnvCategory::Proxy,
            "forced local restricted-network proxy",
            false,
        ));
        env.push(env_plan(
            "GIT_SSH_COMMAND",
            "ssh -o 'ProxyCommand=socat - PROXY:127.0.0.1:%h:%p,proxyport=18080'",
            EnvCategory::Proxy,
            "routes Git SSH remotes through the restricted-network HTTP CONNECT proxy so allowed domains resolve via the proxy container",
            false,
        ));
    }

    let proxy_container = if request.options.network_mode == NetworkMode::Restricted {
        Some(format!("orbit-restricted-proxy-{}", std::process::id()))
    } else {
        None
    };

    let mut cleanup = Vec::new();
    if let Some(proxy_container) = &proxy_container {
        cleanup.push(CleanupStep {
            id: "restricted-proxy".to_string(),
            action: CleanupAction::StopProxy,
            target: proxy_container.clone(),
        });
    }

    let mut audit = vec![AuditEntry {
        level: AuditLevel::Info,
        code: "workspace_rw".to_string(),
        message: "workspace source mount is read-write by default so container commands and agents can edit the current worktree; Git metadata mounts remain writable for fetch, push tracking, and PR workflows".to_string(),
    }];
    if request.agent != "generic" {
        audit.push(AuditEntry {
            level: AuditLevel::Info,
            code: "agent_homes_mounted".to_string(),
            message: "supported coding-agent top-level state directories are mounted read-write by default for agent aliases; Orbit does not snapshot, sanitize, or subgroup those agent homes".to_string(),
        });
    }
    if request.options.interactive {
        audit.push(AuditEntry {
            level: AuditLevel::Info,
            code: "interactive_tty".to_string(),
            message: "container stdin and TTY are attached for interactive agent UI".to_string(),
        });
    }
    if mounts
        .iter()
        .any(|mount| mount.target == agents::SSH_AUTH_SOCK_TARGET)
    {
        audit.push(AuditEntry {
            level: AuditLevel::Warn,
            code: "ssh_socket_forwarded".to_string(),
            message: "SSH socket forwarding grants host identity power".to_string(),
        });
    }
    if mounts
        .iter()
        .any(|mount| mount.target == agents::GPG_AGENT_SOCK_TARGET)
    {
        audit.push(AuditEntry {
            level: AuditLevel::Warn,
            code: "gpg_socket_forwarded".to_string(),
            message: "GPG socket forwarding grants host signing power".to_string(),
        });
    }
    if request.options.network_mode == NetworkMode::Restricted
        && request.options.proxy_image != BuildOptions::default().proxy_image
    {
        audit.push(AuditEntry {
            level: AuditLevel::Warn,
            code: "custom_proxy_image".to_string(),
            message: "custom restricted proxy image runs as root with NET_ADMIN; only use trusted Orbit-derived images".to_string(),
        });
    }

    let local_proxy = if request.options.network_mode == NetworkMode::Restricted {
        Some("http://127.0.0.1:18080".to_string())
    } else {
        None
    };
    let raw_upstream_proxy = request.options.proxy;
    let upstream_proxy = raw_upstream_proxy
        .as_ref()
        .map(|proxy| redact_proxy_url(proxy));
    let mut tmpfs = vec![
        "/tmp:rw,noexec,nosuid,nodev".to_string(),
        "/var/tmp:rw,noexec,nosuid,nodev".to_string(),
        "/home/orbit:rw,exec,nosuid,nodev,uid=1000,gid=1000,mode=755".to_string(),
    ];
    for target in [agents::SSH_HOME_TARGET, agents::GNUPG_HOME_TARGET] {
        if !mounts.iter().any(|mount| mount.target == target)
            && mounts
                .iter()
                .any(|mount| mount.target.starts_with(&format!("{target}/")))
        {
            tmpfs.push(format!(
                "{target}:rw,noexec,nosuid,nodev,uid=1000,gid=1000,mode=700"
            ));
        }
    }

    let pi_home_bound = mounts.iter().any(|mount| mount.target == "/home/orbit/.pi");
    let pi_agent_bound = mounts
        .iter()
        .any(|mount| mount.target == "/home/orbit/.pi/agent");
    if !pi_home_bound
        && mounts
            .iter()
            .any(|mount| mount.target.starts_with("/home/orbit/.pi/"))
    {
        tmpfs.push("/home/orbit/.pi:rw,exec,nosuid,nodev,uid=1000,gid=1000,mode=755".to_string());
    }
    if !pi_home_bound
        && !pi_agent_bound
        && mounts
            .iter()
            .any(|mount| mount.target.starts_with("/home/orbit/.pi/agent/"))
    {
        tmpfs.push(
            "/home/orbit/.pi/agent:rw,exec,nosuid,nodev,uid=1000,gid=1000,mode=755".to_string(),
        );
    }
    if !pi_home_bound
        && !pi_agent_bound
        && !mounts
            .iter()
            .any(|mount| mount.target == "/home/orbit/.pi/agent/extensions")
        && mounts.iter().any(|mount| {
            mount
                .target
                .starts_with("/home/orbit/.pi/agent/extensions/")
        })
    {
        tmpfs.push(
            "/home/orbit/.pi/agent/extensions:rw,exec,nosuid,nodev,uid=1000,gid=1000,mode=755"
                .to_string(),
        );
    }
    Ok(RunPlan {
        engine,
        agent: request.agent,
        image: request.options.image,
        command: request.command,
        cwd: workspace_target,
        interactive: request.options.interactive,
        mounts,
        env,
        network: NetworkPlan {
            mode: request.options.network_mode,
            allowed_domains: request.options.allowed_domains,
            proxy: local_proxy,
            proxy_image: request.options.proxy_image,
            upstream_proxy,
            proxy_container,
            raw_upstream_proxy,
            upstream_proxy_file: None,
            bypass_refusals,
        },
        hardening: HardeningPlan {
            read_only_rootfs: true,
            cap_drop_all: true,
            no_new_privileges: true,
            user: "1000:1000".to_string(),
            tmpfs,
            denied_mounts: vec![
                "/".to_string(),
                "$HOME".to_string(),
                "/Users".to_string(),
                "/home".to_string(),
                "/var/run/docker.sock".to_string(),
                "/run/docker.sock".to_string(),
            ],
        },
        cleanup,
        audit,
        git_metadata_rewrites: git_metadata.rewrites,
    })
}

fn find_git_root(cwd: &Path) -> Result<PathBuf> {
    for ancestor in cwd.ancestors() {
        if ancestor.join(".git").exists() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(OrbitError::refused(
        "workspace_not_repo",
        format!(
            "current directory `{}` is not inside a git worktree",
            cwd.display()
        ),
        Some(cwd.to_path_buf()),
    ))
}
