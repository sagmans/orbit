use serde::{Deserialize, Serialize};

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

    pub fn detect() -> Self {
        if std::env::var("ORBSTACK")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            Self::OrbStack
        } else {
            Self::Docker
        }
    }
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
pub enum NetworkMode {
    None,
    Bridge,
}

impl NetworkMode {
    pub fn parse(value: &str) -> crate::error::Result<Self> {
        match value {
            "none" => Ok(Self::None),
            "bridge" | "open" => Ok(Self::Bridge),
            other => Err(crate::error::OrbitError::Usage(format!(
                "unknown network mode `{other}` (use none or bridge)"
            ))),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MountPlan {
    pub source: String,
    #[serde(skip)]
    pub host_source: std::path::PathBuf,
    pub target: String,
    pub mode: MountMode,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct RunPlan {
    pub engine: Engine,
    pub image: String,
    pub agent: String,
    pub command: Vec<String>,
    pub cwd: String,
    pub interactive: bool,
    pub mounts: Vec<MountPlan>,
    pub env: Vec<(String, String)>,
    pub network: NetworkMode,
}

#[derive(Clone, Debug)]
pub struct ExplicitMount {
    pub source: std::path::PathBuf,
    pub target: String,
    pub mode: MountMode,
}
