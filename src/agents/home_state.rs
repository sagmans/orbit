use super::pi_state::materialized_pi_home_mounts_with_mode;
use super::targets::GH_CONFIG_TARGET;
use crate::Result;
use crate::mount_policy::plan_mount;
use crate::plan::{MountCategory, MountMode, MountPlan};
use std::path::Path;

const AGENT_HOME_PATHS: &[(&str, &str)] = &[
    (".config/pi", "/home/orbit/.config/pi"),
    (".opencode", "/home/orbit/.opencode"),
    (".config/opencode", "/home/orbit/.config/opencode"),
    (".config/gh", GH_CONFIG_TARGET),
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

pub(super) fn agent_home_mounts(home: &Path, workspace: &Path) -> Result<Vec<MountPlan>> {
    let mut mounts = Vec::new();
    mounts.extend(materialized_pi_home_mounts_with_mode(home, workspace)?);

    for (relative, target) in AGENT_HOME_PATHS {
        let source = home.join(relative);
        if source.is_dir() || source.is_file() {
            mounts.push(plan_mount(
                &source,
                target,
                MountMode::Rw,
                MountCategory::State,
                "coding agent top-level state mounted read-write by default",
                Some(workspace),
                true,
            )?);
        }
    }
    Ok(mounts)
}
