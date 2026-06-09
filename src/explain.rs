use crate::plan::RunPlan;

pub fn human(plan: &RunPlan) -> String {
    let mut out = String::new();
    out.push_str("Orbit run plan\n");
    out.push_str(&format!(
        "Engine: {}\n",
        format!("{:?}", plan.engine).to_ascii_lowercase()
    ));
    out.push_str(&format!("Agent: {}\n", plan.agent));
    out.push_str(&format!("Command: {}\n", plan.command.join(" ")));
    out.push_str(&format!("Cwd: {}\n", plan.cwd));
    out.push_str(&format!("Interactive: {}\n", plan.interactive));

    out.push_str("Mounts:\n");
    for mount in &plan.mounts {
        out.push_str(&format!(
            "- {} -> {} ({}, {:?}): {}\n",
            mount.source,
            mount.target,
            mount.mode.as_str(),
            mount.category,
            mount.reason
        ));
    }

    out.push_str("Env:\n");
    for env in &plan.env {
        out.push_str(&format!(
            "- {}={} ({:?}): {}\n",
            env.name, env.value_or_redacted, env.category, env.reason
        ));
    }

    out.push_str(&format!(
        "Network: {}\n",
        format!("{:?}", plan.network.mode).to_ascii_lowercase()
    ));
    if !plan.network.allowed_domains.is_empty() {
        out.push_str(&format!(
            "- allowed domains: {}\n",
            plan.network.allowed_domains.join(", ")
        ));
    }
    if let Some(proxy) = &plan.network.proxy {
        out.push_str(&format!("- proxy: {proxy}\n"));
    }
    for refusal in &plan.network.bypass_refusals {
        out.push_str(&format!("- bypass refusal: {refusal}\n"));
    }

    out.push_str("Hardening:\n");
    out.push_str(&format!(
        "- read-only rootfs: {}\n",
        plan.hardening.read_only_rootfs
    ));
    out.push_str(&format!(
        "- cap-drop all: {}\n",
        plan.hardening.cap_drop_all
    ));
    out.push_str(&format!(
        "- no-new-privileges: {}\n",
        plan.hardening.no_new_privileges
    ));
    out.push_str(&format!("- user: {}\n", plan.hardening.user));
    out.push_str(&format!("- tmpfs: {}\n", plan.hardening.tmpfs.join(", ")));
    out.push_str(&format!(
        "- denied mounts: {}\n",
        plan.hardening.denied_mounts.join(", ")
    ));

    out.push_str("Cleanup:\n");
    if plan.cleanup.is_empty() {
        out.push_str("- none\n");
    } else {
        for step in &plan.cleanup {
            out.push_str(&format!(
                "- {} {:?} {}\n",
                step.id, step.action, step.target
            ));
        }
    }

    out.push_str("Audit:\n");
    for entry in &plan.audit {
        out.push_str(&format!(
            "- {:?} {}: {}\n",
            entry.level, entry.code, entry.message
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::NetworkMode;
    use crate::plan::{
        AuditEntry, AuditLevel, CleanupAction, CleanupStep, Engine, EnvCategory, EnvPlan,
        HardeningPlan, MountCategory, MountMode, MountPlan, NetworkPlan,
    };

    fn example_plan() -> RunPlan {
        RunPlan {
            engine: Engine::Docker,
            agent: "pi".to_string(),
            image: "orbit-agent:latest".to_string(),
            command: vec!["pi".to_string(), "--version".to_string()],
            cwd: "/home/orbit/source".to_string(),
            interactive: false,
            mounts: vec![MountPlan {
                source: "/host/project".to_string(),
                host_source: "/host/project".into(),
                target: "/home/orbit/source".to_string(),
                mode: MountMode::Ro,
                category: MountCategory::Workspace,
                reason: "workspace".to_string(),
                redacted: false,
            }],
            env: vec![EnvPlan {
                name: "TOKEN".to_string(),
                value_or_redacted: "<redacted>".to_string(),
                value: "secret".to_string(),
                category: EnvCategory::Agent,
                reason: "agent token".to_string(),
                redacted: true,
            }],
            network: NetworkPlan {
                mode: NetworkMode::Restricted,
                allowed_domains: vec!["example.com".to_string()],
                proxy: Some("http://proxy.example:8080".to_string()),
                proxy_image: "orbit-agent:latest".to_string(),
                upstream_proxy: Some("<redacted>".to_string()),
                proxy_container: Some("orbit-proxy".to_string()),
                raw_upstream_proxy: Some("http://user:pass@proxy.example".to_string()),
                upstream_proxy_file: None,
                bypass_refusals: vec!["denied.test".to_string()],
            },
            hardening: HardeningPlan {
                read_only_rootfs: true,
                cap_drop_all: true,
                no_new_privileges: true,
                user: "1000:1000".to_string(),
                tmpfs: vec!["/tmp".to_string()],
                denied_mounts: vec!["/".to_string()],
            },
            cleanup: vec![CleanupStep {
                id: "tmp".to_string(),
                action: CleanupAction::RemovePath,
                target: "/tmp/orbit".to_string(),
            }],
            audit: vec![AuditEntry {
                level: AuditLevel::Warn,
                code: "test".to_string(),
                message: "watch this".to_string(),
            }],
            git_metadata_rewrites: Vec::new(),
        }
    }

    #[test]
    fn human_explain_lists_sections_and_redacted_values() {
        let output = human(&example_plan());

        assert!(output.contains("Orbit run plan"));
        assert!(output.contains("Engine: docker"));
        assert!(output.contains("Command: pi --version"));
        assert!(output.contains("Mounts:"));
        assert!(output.contains("/host/project -> /home/orbit/source (ro, Workspace): workspace"));
        assert!(output.contains("TOKEN=<redacted> (Agent): agent token"));
        assert!(output.contains("Network: restricted"));
        assert!(output.contains("- allowed domains: example.com"));
        assert!(output.contains("- bypass refusal: denied.test"));
        assert!(output.contains("Hardening:"));
        assert!(output.contains("Cleanup:"));
        assert!(output.contains("Audit:"));
        assert!(!output.contains("user:pass"));
    }
}
