mod action;
mod git_metadata;
mod image_build;
mod parse;
mod plan_builder;
mod product_commands;
mod render;

use crate::error::{OrbitError, Result};
use crate::plan::RunPlan;
#[cfg(test)]
use action::PlanMode;
use action::{Action, Rendered};
#[cfg(test)]
use git_metadata::{container_workspace_target_with_home, git_metadata_mounts};
#[cfg(test)]
use image_build::parse_mise_tools_config;
use image_build::{mise_tools_build_arg, pi_version_build_arg};
use parse::parse;
use plan_builder::build_plan;
use product_commands::{cleanup, doctor, help};
use render::render;

pub fn main_entry() -> i32 {
    match render(std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(Rendered::Stdout(text)) => {
            print!("{text}");
            0
        }
        Ok(Rendered::Exit(code)) => code,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

pub fn parse_and_plan_for_test<I, S>(args: I) -> Result<RunPlan>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let strings = args
        .into_iter()
        .map(|s| s.as_ref().to_string())
        .collect::<Vec<_>>();
    let cwd = std::env::current_dir()?;
    match parse(strings)? {
        Action::Plan { request, .. } => build_plan(*request, &cwd),
        _ => Err(OrbitError::Usage(
            "test helper expected a run-plan action".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_keeps_alias_flags_as_command_args() {
        let action = parse(vec!["--dry-run".into(), "pi".into(), "--version".into()]).unwrap();
        match action {
            Action::Plan { request, mode } => {
                assert!(matches!(mode, PlanMode::DryRun));
                assert_eq!(request.agent, "pi");
                assert_eq!(request.command, vec!["pi", "--version"]);
            }
            _ => panic!("plan action expected"),
        }
    }

    #[test]
    fn no_arg_alias_defaults_to_interactive() {
        let action = parse(vec!["pi".into()]).unwrap();
        match action {
            Action::Plan { request, .. } => {
                assert_eq!(request.command, vec!["pi"]);
                assert!(request.options.interactive);
            }
            _ => panic!("plan action expected"),
        }
    }

    #[test]
    fn unknown_global_flag_errors() {
        assert!(parse(vec!["--bogus".into(), "--".into(), "echo".into()]).is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn home_child_workspace_maps_under_container_home() {
        let home = tempfile::tempdir().unwrap();
        let workspace = home.path().join("source/me/orbit/main");
        std::fs::create_dir_all(&workspace).unwrap();
        let target = container_workspace_target_with_home(&workspace, Some(home.path())).unwrap();
        assert_eq!(target, "/home/orbit/source/me/orbit/main");
    }

    #[cfg(not(windows))]
    #[test]
    fn non_home_workspace_keeps_absolute_target() {
        let home = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let target =
            container_workspace_target_with_home(workspace.path(), Some(home.path())).unwrap();
        assert_eq!(target, workspace.path().display().to_string());
    }

    #[cfg(not(windows))]
    #[test]
    fn generated_targets_cannot_overlap_agent_runtime_paths() {
        let home = tempfile::tempdir().unwrap();
        let workspace = home.path().join(".pi/agent/project");
        std::fs::create_dir_all(&workspace).unwrap();
        let err = container_workspace_target_with_home(&workspace, Some(home.path())).unwrap_err();
        assert!(err.to_string().contains("reserved container path"));
    }

    #[test]
    fn mise_tools_config_parses_tools_only() {
        let tools = parse_mise_tools_config(
            r#"
[tools]
bun = "1.3.14"
node = "24.11.1" # host global node
rust = '1.95.0'
make = "4.4.1"
python = ["3.14.2", "3.13.9"]
"npm:@scope/tool" = { version = "1.2.3" }

[settings]
experimental = true
"#,
        )
        .unwrap();

        assert_eq!(
            tools,
            vec![
                "bun@1.3.14",
                "make@4.4.1",
                "node@24.11.1",
                "npm:@scope/tool@1.2.3",
                "python@3.14.2",
                "python@3.13.9",
                "rust@1.95.0",
            ]
        );
    }

    #[test]
    fn mise_tools_config_rejects_node_before_24() {
        let err = parse_mise_tools_config(
            r#"
[tools]
node = "22.21.1"
"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("node` must be 24.x"));
    }

    #[test]
    fn mise_tools_config_rejects_secret_like_specs() {
        let err = parse_mise_tools_config(
            r#"
[tools]
"aqua:https://user:pass@example.com/tool" = "1.0.0"
"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("appears to contain credentials"));
        assert!(!err.to_string().contains("user:pass"));
    }

    #[cfg(not(windows))]
    #[test]
    fn absolute_gitdir_gets_runtime_metadata_rewrite() {
        let home = crate::mount_policy::home_dir()
            .and_then(|home| home.canonicalize().ok())
            .expect("HOME");
        let writable_home_child = std::env::current_dir().unwrap().canonicalize().unwrap();
        assert!(writable_home_child.starts_with(&home));
        let root = tempfile::Builder::new()
            .prefix("orbit-git-metadata-")
            .tempdir_in(&writable_home_child)
            .unwrap();
        let relative_root = root.path().strip_prefix(&home).unwrap();
        let workspace = root.path().join("source/worktree");
        let git_dir = root.path().join("source/main/.git/worktrees/worktree");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(
            workspace.join(".git"),
            format!("gitdir: {}\n", git_dir.display()),
        )
        .unwrap();

        let git = git_metadata_mounts(&workspace).unwrap();

        assert!(git.rewrites.iter().any(|rewrite| {
            rewrite.target
                == format!(
                    "/home/orbit/{}/source/worktree/.git",
                    relative_root.display()
                )
                && rewrite.content
                    == format!(
                        "gitdir: /home/orbit/{}/source/main/.git/worktrees/worktree\n",
                        relative_root.display()
                    )
        }));
    }
}
