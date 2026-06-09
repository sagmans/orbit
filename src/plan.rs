use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    Docker,
    OrbStack,
    Podman,
}

impl Engine {
    pub fn binary(self) -> &'static str {
        match self {
            Self::Docker | Self::OrbStack => "docker",
            Self::Podman => "podman",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineChoice {
    Auto,
    Explicit(Engine),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountMode {
    Ro,
    Rw,
}

impl MountMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ro => "ro",
            Self::Rw => "rw",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountCategory {
    Workspace,
    Config,
    Secret,
    State,
    Socket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvCategory {
    Runtime,
    Agent,
    Proxy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditLevel {
    Info,
    Warn,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MountPlan {
    pub source: String,
    #[serde(skip)]
    pub host_source: PathBuf,
    pub target: String,
    pub mode: MountMode,
    pub category: MountCategory,
    pub reason: String,
    pub redacted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnvPlan {
    pub name: String,
    pub value_or_redacted: String,
    #[serde(skip)]
    pub value: String,
    pub category: EnvCategory,
    pub reason: String,
    pub redacted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkPlan {
    pub mode: crate::network::NetworkMode,
    pub allowed_domains: Vec<String>,
    pub proxy: Option<String>,
    pub proxy_image: String,
    pub upstream_proxy: Option<String>,
    pub proxy_container: Option<String>,
    #[serde(skip)]
    pub raw_upstream_proxy: Option<String>,
    #[serde(skip)]
    pub upstream_proxy_file: Option<PathBuf>,
    pub bypass_refusals: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HardeningPlan {
    pub read_only_rootfs: bool,
    pub cap_drop_all: bool,
    pub no_new_privileges: bool,
    pub user: String,
    pub tmpfs: Vec<String>,
    pub denied_mounts: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CleanupStep {
    pub id: String,
    pub action: CleanupAction,
    pub target: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupAction {
    RemovePath,
    StopProxy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    pub level: AuditLevel,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitMetadataRewrite {
    pub target: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunPlan {
    pub engine: Engine,
    pub agent: String,
    pub image: String,
    pub command: Vec<String>,
    pub cwd: String,
    pub interactive: bool,
    pub mounts: Vec<MountPlan>,
    pub env: Vec<EnvPlan>,
    pub network: NetworkPlan,
    pub hardening: HardeningPlan,
    pub cleanup: Vec<CleanupStep>,
    pub audit: Vec<AuditEntry>,
    #[serde(skip)]
    pub git_metadata_rewrites: Vec<GitMetadataRewrite>,
}

#[derive(Clone, Debug)]
pub struct ExplicitMount {
    pub source: PathBuf,
    pub target: String,
    pub mode: MountMode,
}

#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub engine: EngineChoice,
    pub network_mode: crate::network::NetworkMode,
    pub allowed_domains: Vec<String>,
    pub proxy: Option<String>,
    pub explicit_mounts: Vec<ExplicitMount>,
    pub workspace: Option<PathBuf>,
    pub image: String,
    pub proxy_image: String,
    pub allow_agent_state: bool,
    pub forward_ssh: bool,
    pub forward_gpg: bool,
    pub interactive: bool,
    pub excluded_extensions: Vec<String>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            engine: EngineChoice::Explicit(Engine::Docker),
            network_mode: crate::network::NetworkMode::Restricted,
            allowed_domains: Vec::new(),
            proxy: None,
            explicit_mounts: Vec::new(),
            workspace: None,
            image: "orbit-agent:latest".to_string(),
            proxy_image: "orbit-agent:latest".to_string(),
            allow_agent_state: false,
            forward_ssh: false,
            forward_gpg: false,
            interactive: false,
            excluded_extensions: vec!["pi-sandbox".to_string()],
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunRequest {
    pub agent: String,
    pub command: Vec<String>,
    pub options: BuildOptions,
}

pub fn runtime_env(name: &str, value: &str, reason: &str) -> EnvPlan {
    EnvPlan {
        name: name.to_string(),
        value_or_redacted: value.to_string(),
        value: value.to_string(),
        category: EnvCategory::Runtime,
        reason: reason.to_string(),
        redacted: false,
    }
}

pub fn env_plan(
    name: &str,
    value: &str,
    category: EnvCategory,
    reason: &str,
    redacted: bool,
) -> EnvPlan {
    EnvPlan {
        name: name.to_string(),
        value_or_redacted: if redacted {
            "<redacted>".to_string()
        } else {
            value.to_string()
        },
        value: value.to_string(),
        category,
        reason: reason.to_string(),
        redacted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::NetworkMode;

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
                host_source: "/private/host/project".into(),
                target: "/home/orbit/source".to_string(),
                mode: MountMode::Ro,
                category: MountCategory::Workspace,
                reason: "workspace".to_string(),
                redacted: false,
            }],
            env: vec![env_plan(
                "TOKEN",
                "secret",
                EnvCategory::Agent,
                "agent token",
                true,
            )],
            network: NetworkPlan {
                mode: NetworkMode::Restricted,
                allowed_domains: vec!["example.com".to_string()],
                proxy: Some("<redacted>".to_string()),
                proxy_image: "orbit-agent:latest".to_string(),
                upstream_proxy: Some("<redacted>".to_string()),
                proxy_container: Some("orbit-proxy".to_string()),
                raw_upstream_proxy: Some("http://user:pass@proxy.example".to_string()),
                upstream_proxy_file: Some("/tmp/proxy-secret".into()),
                bypass_refusals: Vec::new(),
            },
            hardening: HardeningPlan {
                read_only_rootfs: true,
                cap_drop_all: true,
                no_new_privileges: true,
                user: "1000:1000".to_string(),
                tmpfs: vec!["/tmp".to_string()],
                denied_mounts: Vec::new(),
            },
            cleanup: vec![CleanupStep {
                id: "tmp".to_string(),
                action: CleanupAction::RemovePath,
                target: "/tmp/orbit".to_string(),
            }],
            audit: vec![AuditEntry {
                level: AuditLevel::Info,
                code: "test".to_string(),
                message: "covered".to_string(),
            }],
            git_metadata_rewrites: vec![GitMetadataRewrite {
                target: "/home/orbit/source/.git".to_string(),
                content: "gitdir: /tmp/worktree\n".to_string(),
            }],
        }
    }

    #[test]
    fn env_plan_redacts_display_value_only() {
        let env = env_plan("TOKEN", "secret", EnvCategory::Agent, "token", true);

        assert_eq!(env.value, "secret");
        assert_eq!(env.value_or_redacted, "<redacted>");
        assert!(env.redacted);
    }

    #[test]
    fn run_plan_json_omits_runtime_only_host_fields() {
        let value = serde_json::to_value(example_plan()).unwrap();

        assert_eq!(value["engine"], "docker");
        assert_eq!(value["mounts"][0]["source"], "/host/project");
        assert!(value["mounts"][0].get("host_source").is_none());
        assert_eq!(value["env"][0]["value_or_redacted"], "<redacted>");
        assert!(value["env"][0].get("value").is_none());
        assert!(value["network"].get("raw_upstream_proxy").is_none());
        assert!(value["network"].get("upstream_proxy_file").is_none());
        assert!(value.get("git_metadata_rewrites").is_none());
    }
}
