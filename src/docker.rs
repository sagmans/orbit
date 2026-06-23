use crate::plan::{Engine, MountMode, NetworkMode, RunPlan};

pub fn command_args(plan: &RunPlan) -> Vec<String> {
    let mut args = vec![
        plan.engine.binary().to_string(),
        "run".to_string(),
        "--rm".to_string(),
        "--init".to_string(),
    ];

    if plan.interactive {
        args.push("--interactive".to_string());
        args.push("--tty".to_string());
    }

    // Hardening
    args.push("--read-only".to_string());
    args.push("--cap-drop=ALL".to_string());
    args.push("--security-opt=no-new-privileges".to_string());
    args.push("--user=1000:1000".to_string());

    if plan.engine == Engine::Podman {
        args.push("--userns=keep-id".to_string());
    }

    args.push(format!(
        "--label=dev.orbit.engine={}",
        engine_label(plan.engine)
    ));
    args.push(format!("--workdir={}", plan.cwd));

    // tmpfs for writable runtime paths
    for tmpfs in &[
        "/tmp:rw,noexec,nosuid,nodev",
        "/var/tmp:rw,noexec,nosuid,nodev",
        "/home/orbit:rw,exec,nosuid,nodev,uid=1000,gid=1000,mode=755",
        "/usr/local/cargo/registry:rw,noexec,nosuid,nodev,uid=1000,gid=1000",
        "/usr/local/cargo/git:rw,noexec,nosuid,nodev,uid=1000,gid=1000",
    ] {
        args.push("--tmpfs".to_string());
        args.push(tmpfs.to_string());
    }

    // Network
    args.push(match plan.network {
        NetworkMode::None => "--network=none".to_string(),
        NetworkMode::Bridge => "--network=bridge".to_string(),
    });

    // Mounts
    for mount in &plan.mounts {
        args.push("--mount".to_string());
        let mut spec = format!("type=bind,src={},dst={}", mount.source, mount.target);
        if mount.mode == MountMode::Ro {
            spec.push_str(",readonly");
        }
        if plan.engine == Engine::Podman && cfg!(target_os = "linux") {
            spec.push_str(",relabel=private");
        }
        args.push(spec);
    }

    // Environment
    for (name, value) in &plan.env {
        args.push("--env".to_string());
        args.push(format!("{name}={value}"));
    }

    args.push(plan.image.clone());

    // Agent aliases go through the entrypoint; generic commands run directly
    if plan.agent != "generic" {
        args.push("orbit-entrypoint".to_string());
    }
    args.extend(plan.command.clone());

    args
}

pub fn dry_run_command(plan: &RunPlan) -> String {
    let args = command_args(plan);
    // Mask sensitive env var values for display safety
    let masked: Vec<String> = args
        .iter()
        .map(|arg| {
            if let Some(rest) = arg.strip_prefix("--env ") {
                if let Some(eq_pos) = rest.find('=') {
                    let name = &rest[..eq_pos];
                    if is_sensitive_env(name) {
                        return format!("--env {name}=***");
                    }
                }
            }
            arg.clone()
        })
        .collect();
    shell_join(&masked)
}

/// Check if an env var name likely contains a secret value.
fn is_sensitive_env(name: &str) -> bool {
    let upper = name.to_uppercase();
    upper.contains("TOKEN")
        || upper.contains("SECRET")
        || upper.contains("KEY")
        || upper.contains("PASSWORD")
        || upper.contains("CREDENTIAL")
}

pub fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|a| shell_quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_' | ':' | '=' | ','))
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

fn engine_label(engine: Engine) -> &'static str {
    match engine {
        Engine::Docker => "docker",
        Engine::OrbStack => "orbstack",
        Engine::Podman => "podman",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_plan() -> RunPlan {
        use std::path::PathBuf;
        RunPlan {
            engine: Engine::Docker,
            image: "orbit-agent:latest".to_string(),
            agent: "generic".to_string(),
            command: vec!["echo".to_string(), "hi".to_string()],
            cwd: "/workspace".to_string(),
            interactive: false,
            mounts: vec![crate::plan::MountPlan {
                source: "/host/project".to_string(),
                host_source: PathBuf::from("/host/project"),
                target: "/workspace".to_string(),
                mode: MountMode::Rw,
                reason: "workspace".to_string(),
            }],
            env: vec![("HOME".to_string(), "/home/orbit".to_string())],
            network: NetworkMode::Bridge,
        }
    }

    #[test]
    fn command_contains_hardening() {
        let plan = make_plan();
        let args = command_args(&plan);
        let rendered = shell_join(&args);

        assert!(rendered.contains("--read-only"));
        assert!(rendered.contains("--cap-drop=ALL"));
        assert!(rendered.contains("--security-opt=no-new-privileges"));
        assert!(rendered.contains("--user=1000:1000"));
        assert!(rendered.contains("--network=bridge"));
        assert!(rendered.contains("--tmpfs"));
        assert!(rendered.contains("/home/orbit:rw,exec"));
    }

    #[test]
    fn interactive_adds_tty() {
        let mut plan = make_plan();
        plan.interactive = true;
        let rendered = dry_run_command(&plan);
        assert!(rendered.contains("--interactive --tty"));
    }

    #[test]
    fn none_network_uses_none() {
        let mut plan = make_plan();
        plan.network = NetworkMode::None;
        let rendered = dry_run_command(&plan);
        assert!(rendered.contains("--network=none"));
    }

    #[test]
    fn agent_aliases_use_entrypoint() {
        let mut plan = make_plan();
        plan.agent = "pi".to_string();
        plan.command = vec!["pi".to_string()];
        let rendered = dry_run_command(&plan);
        assert!(rendered.contains("orbit-entrypoint"));
    }

    #[test]
    fn generic_commands_skip_entrypoint() {
        let plan = make_plan();
        let rendered = dry_run_command(&plan);
        assert!(!rendered.contains("orbit-entrypoint"));
    }

    #[test]
    fn readonly_mounts_get_readonly_flag() {
        let mut plan = make_plan();
        plan.mounts[0].mode = MountMode::Ro;
        let rendered = dry_run_command(&plan);
        assert!(rendered.contains(",readonly"));
    }

    #[test]
    fn shell_quote_handles_special_chars() {
        assert_eq!(shell_quote("simple"), "simple");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }
}
