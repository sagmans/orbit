use crate::plan::{Engine, MountMode, RunPlan};

pub fn command_args(plan: &RunPlan, redacted: bool) -> Vec<String> {
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
    args.push(format!(
        "--label=dev.orbit.engine={}",
        engine_label(plan.engine)
    ));
    args.push(format!("--workdir={}", plan.cwd));

    if plan.hardening.read_only_rootfs {
        args.push("--read-only".to_string());
    }
    if plan.hardening.cap_drop_all {
        args.push("--cap-drop=ALL".to_string());
    }
    if plan.hardening.no_new_privileges {
        args.push("--security-opt=no-new-privileges".to_string());
    }
    if !plan.hardening.user.is_empty() {
        if plan.engine == Engine::Podman {
            args.push("--userns=keep-id".to_string());
        }
        args.push(format!("--user={}", plan.hardening.user));
    }

    for tmpfs in &plan.hardening.tmpfs {
        args.push("--tmpfs".to_string());
        args.push(tmpfs.clone());
    }

    args.push(match plan.network.mode {
        crate::network::NetworkMode::None => "--network=none".to_string(),
        crate::network::NetworkMode::Open => "--network=bridge".to_string(),
        crate::network::NetworkMode::Restricted => format!(
            "--network=container:{}",
            plan.network
                .proxy_container
                .as_deref()
                .unwrap_or("orbit-restricted-proxy")
        ),
    });

    for mount in &plan.mounts {
        args.push("--mount".to_string());
        let source = if redacted {
            mount.source.clone()
        } else {
            mount.host_source.display().to_string()
        };
        let mut spec = format!("type=bind,src={source},dst={}", mount.target);
        if mount.mode == MountMode::Ro {
            spec.push_str(",readonly");
        }
        if plan.engine == Engine::Podman && cfg!(target_os = "linux") {
            spec.push_str(",relabel=private");
        }
        args.push(spec);
    }

    for env in &plan.env {
        args.push("--env".to_string());
        let value = if redacted {
            &env.value_or_redacted
        } else {
            &env.value
        };
        args.push(format!("{}={value}", env.name));
    }

    args.push(plan.image.clone());
    if plan.agent == "generic" {
        args.extend(plan.command.clone());
    } else {
        args.push("orbit-agent-entrypoint".to_string());
        args.extend(plan.command.clone());
    }
    args
}

pub fn restricted_proxy_command(plan: &RunPlan, redacted: bool) -> Option<Vec<String>> {
    if plan.network.mode != crate::network::NetworkMode::Restricted {
        return None;
    }
    let domains = plan.network.allowed_domains.join(",");
    let proxy_container = plan
        .network
        .proxy_container
        .as_deref()
        .unwrap_or("orbit-restricted-proxy");
    let redacted_upstream_proxy = redacted
        .then_some(plan.network.upstream_proxy.as_deref())
        .flatten();
    let mut args = vec![
        plan.engine.binary().to_string(),
        "run".to_string(),
        "--detach".to_string(),
        format!("--name={proxy_container}"),
        "--label=dev.orbit.role=restricted-proxy".to_string(),
        "--cap-add=NET_ADMIN".to_string(),
        "--user=0:0".to_string(),
        "--network=bridge".to_string(),
        "--env".to_string(),
        format!("ORBIT_ALLOWED_DOMAINS={domains}"),
    ];
    if let Some(proxy_file) = &plan.network.upstream_proxy_file {
        let source = if redacted {
            "<redacted:upstream-proxy>".to_string()
        } else {
            proxy_file.display().to_string()
        };
        args.push("--mount".to_string());
        args.push(format!(
            "type=bind,src={source},dst=/run/orbit-upstream-proxy,readonly"
        ));
        args.push("--env".to_string());
        args.push("ORBIT_UPSTREAM_PROXY_FILE=/run/orbit-upstream-proxy".to_string());
    } else if let Some(proxy) = redacted_upstream_proxy {
        // Dry-run/explain may show that an upstream proxy exists, but execution
        // uses a temporary secret file prepared by the runner so raw credentials
        // never travel through container-engine argv.
        args.push("--env".to_string());
        args.push(format!("ORBIT_UPSTREAM_PROXY={proxy}"));
    }
    args.push(plan.network.proxy_image.clone());
    args.push("orbit-restricted-proxy".to_string());
    Some(args)
}

pub fn dry_run_command(plan: &RunPlan) -> String {
    let run = shell_join(&command_args(plan, true));
    if let Some(proxy) = restricted_proxy_command(plan, true) {
        format!("{} && {}", shell_join(&proxy), run)
    } else {
        run
    }
}

pub fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
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
    use crate::cli::parse_and_plan_for_test;

    fn expected_workspace_target(workspace: &std::path::Path) -> String {
        #[cfg(windows)]
        {
            let _ = workspace;
            "/workspace".to_string()
        }
        #[cfg(not(windows))]
        {
            let home = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .and_then(|home| home.canonicalize().ok());
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

    #[test]
    fn docker_command_for_echo_contains_hardening_contract() {
        let plan = parse_and_plan_for_test(["--dry-run", "--", "echo", "hi"]).unwrap();
        let workspace = std::env::current_dir().unwrap().canonicalize().unwrap();
        let workspace_target = expected_workspace_target(&workspace);
        let run_args = command_args(&plan, true);
        let rendered = shell_join(&run_args);

        assert_eq!(run_args[0], "docker");
        assert!(run_args.contains(&"--rm".to_string()));
        assert!(run_args.contains(&"--read-only".to_string()));
        assert!(run_args.contains(&"--cap-drop=ALL".to_string()));
        assert!(run_args.contains(&"--security-opt=no-new-privileges".to_string()));
        assert!(run_args.contains(&"--user=1000:1000".to_string()));
        assert!(run_args.contains(&format!("--workdir={workspace_target}")));
        assert!(rendered.contains(&format!(
            "src={},dst={workspace_target}",
            workspace.display()
        )));
        assert!(rendered.contains("PATH=/usr/local/share/mise/installs/cargo-tools/bin"));
        assert!(rendered.contains("/usr/local/share/mise/cargo/bin"));
        assert!(rendered.ends_with(" orbit-agent:latest echo hi"));

        let proxy_args = restricted_proxy_command(&plan, true).unwrap();
        assert!(shell_join(&proxy_args).contains("ORBIT_ALLOWED_DOMAINS="));
    }

    #[test]
    fn interactive_plan_adds_stdin_and_tty() {
        let plan = parse_and_plan_for_test(["--interactive", "--dry-run", "--", "sh"]).unwrap();
        let rendered = dry_run_command(&plan);
        assert!(rendered.contains("--interactive --tty"));
    }

    #[test]
    fn restricted_proxy_uses_selected_proxy_image_only_when_explicit() {
        let custom_app = parse_and_plan_for_test([
            "--dry-run",
            "--image",
            "custom/orbit:test",
            "--",
            "echo",
            "hi",
        ])
        .unwrap();
        let custom_app_rendered = dry_run_command(&custom_app);
        assert!(custom_app_rendered.contains("orbit-agent:latest orbit-restricted-proxy"));
        assert!(custom_app_rendered.ends_with(" custom/orbit:test echo hi"));

        let custom_proxy = parse_and_plan_for_test([
            "--dry-run",
            "--image",
            "custom/orbit:test",
            "--proxy-image",
            "custom/proxy:test",
            "--",
            "echo",
            "hi",
        ])
        .unwrap();
        let custom_proxy_rendered = dry_run_command(&custom_proxy);
        assert!(custom_proxy_rendered.contains("custom/proxy:test orbit-restricted-proxy"));
        assert!(custom_proxy_rendered.ends_with(" custom/orbit:test echo hi"));
    }

    #[test]
    fn network_mode_goldens_are_exact() {
        let default = parse_and_plan_for_test(["--dry-run", "--", "echo", "hi"]).unwrap();
        assert_eq!(
            default.network.mode,
            crate::network::NetworkMode::Restricted
        );
        assert_eq!(
            default.network.allowed_domains,
            vec![
                "api.linear.app".to_string(),
                "api.openai.com".to_string(),
                "auth.openai.com".to_string(),
                "chatgpt.com".to_string(),
                "context7.com".to_string(),
                "github.com".to_string(),
                "mcp.cloudflare.com".to_string(),
                "mcp.mdn.mozilla.net".to_string(),
                "registry.npmjs.org".to_string(),
            ]
        );
        assert!(dry_run_command(&default).contains("--network=container:orbit-restricted-proxy"));
        let none = parse_and_plan_for_test([
            "--network",
            "none",
            "--allow-domain",
            "example.com",
            "--dry-run",
            "--",
            "echo",
            "hi",
        ])
        .unwrap();
        assert!(none.network.allowed_domains.is_empty());
        assert!(dry_run_command(&none).contains("--network=none"));
        let open = parse_and_plan_for_test([
            "--network",
            "open",
            "--allow-domain",
            "example.com",
            "--dry-run",
            "--",
            "echo",
            "hi",
        ])
        .unwrap();
        assert!(open.network.allowed_domains.is_empty());
        assert!(dry_run_command(&open).contains("--network=bridge"));
        let restricted = parse_and_plan_for_test([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--dry-run",
            "--",
            "curl",
            "https://example.com",
        ])
        .unwrap();
        assert_eq!(
            restricted.network.allowed_domains,
            vec![
                "api.linear.app".to_string(),
                "api.openai.com".to_string(),
                "auth.openai.com".to_string(),
                "chatgpt.com".to_string(),
                "context7.com".to_string(),
                "example.com".to_string(),
                "github.com".to_string(),
                "mcp.cloudflare.com".to_string(),
                "mcp.mdn.mozilla.net".to_string(),
                "registry.npmjs.org".to_string(),
            ]
        );
        let rendered = dry_run_command(&restricted);
        assert!(rendered.starts_with("docker run --detach --name=orbit-restricted-proxy"));
        assert!(rendered.contains("--network=container:orbit-restricted-proxy"));
    }

    #[test]
    fn raw_restricted_proxy_command_omits_proxy_without_secret_file() {
        let plan = parse_and_plan_for_test([
            "--network",
            "restricted",
            "--allow-domain",
            "example.com",
            "--proxy",
            "http://user:pass@proxy.example:8080",
            "--dry-run",
            "--",
            "curl",
            "https://example.com",
        ])
        .unwrap();
        let raw_args = restricted_proxy_command(&plan, false).unwrap();
        assert!(!shell_join(&raw_args).contains("user:pass"));
        assert!(
            !raw_args
                .iter()
                .any(|arg| arg.starts_with("ORBIT_UPSTREAM_PROXY="))
        );

        let redacted_args = restricted_proxy_command(&plan, true).unwrap();
        let redacted = shell_join(&redacted_args);
        assert!(!redacted.contains("user:pass"));
        assert!(redacted.contains("'ORBIT_UPSTREAM_PROXY=http://<redacted>@proxy.example:8080'"));
    }

    #[test]
    fn podman_golden_includes_rootless_userns() {
        let plan = parse_and_plan_for_test(["--engine", "podman", "--dry-run", "--", "echo", "hi"])
            .unwrap();
        let rendered = dry_run_command(&plan);
        assert!(rendered.starts_with("podman run"));
        assert!(rendered.contains("--userns=keep-id"));
        #[cfg(target_os = "linux")]
        assert!(rendered.contains("relabel=private"));
    }
}
