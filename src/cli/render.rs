use crate::docker;
use crate::error::Result;
use crate::explain;
use crate::runner;

use super::action::{Action, PlanMode, Rendered};
use super::{build_plan, cleanup, doctor, help, mise_tools_build_arg, parse, pi_version_build_arg};

pub(super) fn render(args: Vec<String>) -> Result<Rendered> {
    match parse(args)? {
        Action::Plan { request, mode } => {
            let cwd = std::env::current_dir()?;
            let plan = build_plan(*request, &cwd)?;
            match mode {
                PlanMode::Run => Ok(Rendered::Exit(runner::execute(&plan)?)),
                PlanMode::DryRun => Ok(Rendered::Stdout(format!(
                    "{}\n",
                    docker::dry_run_command(&plan)
                ))),
                PlanMode::ExplainHuman => Ok(Rendered::Stdout(explain::human(&plan))),
                PlanMode::ExplainJson => Ok(Rendered::Stdout(format!(
                    "{}\n",
                    serde_json::to_string_pretty(&plan)?
                ))),
            }
        }
        Action::Doctor { json } => Ok(Rendered::Stdout(doctor(json)?)),
        Action::Cleanup { dry_run, json } => Ok(Rendered::Stdout(cleanup(dry_run, json)?)),
        Action::ImageBuild {
            dry_run,
            engine,
            tag,
            host_mise_tools,
            host_pi_version,
            pi_version,
        } => {
            let mise_tools = mise_tools_build_arg(host_mise_tools)?;
            let pi_version = pi_version_build_arg(host_pi_version, pi_version.as_deref())?;
            let mut args = vec![
                engine.binary().to_string(),
                "build".to_string(),
                "-f".to_string(),
                "docker/orbit-agent.Dockerfile".to_string(),
                "--build-arg".to_string(),
                format!("ORBIT_MISE_TOOLS={mise_tools}"),
            ];
            if let Some(pi_version) = pi_version {
                args.push("--build-arg".to_string());
                args.push(format!("PI_VERSION={pi_version}"));
            }
            args.extend(["-t".to_string(), tag, ".".to_string()]);
            let rendered = docker::shell_join(&args);
            if dry_run {
                Ok(Rendered::Stdout(format!("{rendered}\n")))
            } else {
                let status = std::process::Command::new(&args[0])
                    .args(&args[1..])
                    .status()?;
                Ok(Rendered::Exit(status.code().unwrap_or(1)))
            }
        }
        Action::Help => Ok(Rendered::Stdout(help())),
    }
}
