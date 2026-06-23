use crate::docker;
use crate::error::{OrbitError, Result};
use crate::image_build;
use crate::mount;
use crate::plan::{Engine, ExplicitMount, MountMode, NetworkMode, RunPlan};
use crate::runner;
use std::path::PathBuf;

pub fn main_entry() -> i32 {
    match run(std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn run(args: Vec<String>) -> Result<i32> {
    if args.is_empty() {
        print!("{}", help());
        return Ok(0);
    }

    match args[0].as_str() {
        "--help" | "-h" => {
            print!("{}", help());
            Ok(0)
        }
        "doctor" => {
            let json = args[1..].iter().any(|a| a == "--json");
            doctor(json);
            Ok(0)
        }
        "cleanup" => {
            let dry_run = args[1..].iter().any(|a| a == "--dry-run");
            cleanup(dry_run);
            Ok(0)
        }
        "image" => {
            if args.get(1).map(String::as_str) == Some("build") {
                handle_image_build(&args[2..])
            } else {
                Err(OrbitError::Usage(
                    "expected `orbit image build`".to_string(),
                ))
            }
        }
        "explain" => {
            let plan = build_plan_from_args(&args[1..])?;
            explain(&plan);
            Ok(0)
        }
        _ => {
            // Run path: orbit [flags] <agent> [args] or orbit [flags] -- <cmd>
            let dry_run = args.iter().any(|a| a == "--dry-run");
            let plan = build_plan_from_args(&args)?;
            if dry_run {
                println!("{}", docker::dry_run_command(&plan));
                Ok(0)
            } else {
                runner::execute(&plan)
            }
        }
    }
}

struct CliOptions {
    engine: Engine,
    network: NetworkMode,
    image: String,
    workspace: Option<PathBuf>,
    interactive: bool,
    explicit_mounts: Vec<ExplicitMount>,
    dry_run: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            engine: Engine::detect(),
            network: NetworkMode::Bridge,
            image: "orbit-agent:latest".to_string(),
            workspace: None,
            interactive: false,
            explicit_mounts: Vec::new(),
            dry_run: false,
        }
    }
}

fn build_plan_from_args(args: &[String]) -> Result<RunPlan> {
    let mut opts = CliOptions::default();
    let mut command = Vec::new();
    let mut agent = "generic".to_string();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                command = args[i + 1..].to_vec();
                break;
            }
            "--dry-run" => opts.dry_run = true,
            "--interactive" | "-i" => opts.interactive = true,
            "--help" | "-h" => {
                print!("{}", help());
                std::process::exit(0);
            }
            "--engine" => {
                i += 1;
                opts.engine = parse_engine(&require_value(args, i, "--engine")?)?;
            }
            "--network" => {
                i += 1;
                opts.network = NetworkMode::parse(&require_value(args, i, "--network")?)?;
            }
            "--image" => {
                i += 1;
                opts.image = require_value(args, i, "--image")?.to_string();
            }
            "--workspace" => {
                i += 1;
                opts.workspace = Some(PathBuf::from(require_value(args, i, "--workspace")?));
            }
            "--mount" => {
                i += 1;
                opts.explicit_mounts
                    .push(parse_mount(&require_value(args, i, "--mount")?)?);
            }
            token if mount::is_alias(token) => {
                agent = token.to_string();
                command.push(token.to_string());
                command.extend(args[i + 1..].iter().cloned());
                break;
            }
            token if token.starts_with('-') => {
                return Err(OrbitError::Usage(format!("unknown option `{token}`")));
            }
            token => {
                return Err(OrbitError::Usage(format!(
                    "command `{token}` must follow `--` or be a known agent alias"
                )));
            }
        }
        i += 1;
    }

    if command.is_empty() {
        return Err(OrbitError::Usage(
            "missing command; use `orbit -- <cmd...>` or `orbit <agent> <args...>`".to_string(),
        ));
    }

    // No-arg agent aliases default to interactive (TUI mode)
    if agent != "generic" && command.len() == 1 {
        opts.interactive = true;
    }

    // Pi's pi-sandbox extension uses bubblewrap which can't work inside the
    // container (cap-drop=ALL, no-new-privileges). The container is already
    // sandboxed, so inject --no-sandbox unless the user passed it explicitly.
    if agent == "pi" && !command.iter().any(|arg| arg == "--no-sandbox") {
        command.insert(1, "--no-sandbox".to_string());
    }

    let cwd = std::env::current_dir()?;
    let workspace_raw = opts
        .workspace
        .unwrap_or_else(|| mount::find_git_root(&cwd).unwrap_or_else(|_| cwd.clone()));
    let workspace = mount::validate_workspace(&workspace_raw)?;
    let mounts = mount::build_mounts(&agent, &workspace, &opts.explicit_mounts)?;
    let cwd_target = mount::container_target(&workspace)?;

    let mut env = vec![
        ("HOME".to_string(), "/home/orbit".to_string()),
        (
            "PATH".to_string(),
            "/home/orbit/.local/share/mise/shims:/usr/local/mise/shims:/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin"
                .to_string(),
        ),
        ("TMPDIR".to_string(), "/home/orbit".to_string()),
        // mise: pre-installed tools in system dir (read-only);
        // user installs go to tmpfs under /home/orbit
        (
            "MISE_DATA_DIR".to_string(),
            "/home/orbit/.local/share/mise".to_string(),
        ),
        // Config dir points to build-time global config (has tool versions).
        // Read-only at runtime; users can use project-level mise.toml for overrides.
        (
            "MISE_CONFIG_DIR".to_string(),
            "/usr/local/mise".to_string(),
        ),
        (
            "MISE_CACHE_DIR".to_string(),
            "/home/orbit/.cache/mise".to_string(),
        ),
        (
            "MISE_SYSTEM_DATA_DIR".to_string(),
            "/usr/local/mise".to_string(),
        ),
        // Preserve Rust env vars from the image
        ("CARGO_HOME".to_string(), "/usr/local/cargo".to_string()),
        ("RUSTUP_HOME".to_string(), "/usr/local/rustup".to_string()),
    ];

    // Inject GH_TOKEN from host's gh auth (keyring not available in container)
    if let Ok(token) = std::process::Command::new("gh")
        .arg("auth")
        .arg("token")
        .output()
    {
        if token.status.success() {
            let token_str = String::from_utf8_lossy(&token.stdout).trim().to_string();
            if !token_str.is_empty() {
                env.push(("GH_TOKEN".to_string(), token_str));
            }
        }
    }

    Ok(RunPlan {
        engine: opts.engine,
        image: opts.image,
        agent,
        command,
        cwd: cwd_target,
        interactive: opts.interactive,
        mounts,
        env,
        network: opts.network,
    })
}

fn handle_image_build(args: &[String]) -> Result<i32> {
    let mut dry_run = false;
    let mut engine = Engine::detect();
    let mut tag = "orbit-agent:latest".to_string();
    let mut dockerfile = "docker/Dockerfile".to_string();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" => dry_run = true,
            "--engine" => {
                i += 1;
                engine = parse_engine(&require_value(args, i, "--engine")?)?;
            }
            "--tag" => {
                i += 1;
                tag = require_value(args, i, "--tag")?.to_string();
            }
            "--dockerfile" | "-f" => {
                i += 1;
                dockerfile = require_value(args, i, "--dockerfile")?.to_string();
            }
            "--help" | "-h" => {
                println!(
                    "Usage: orbit image build [--dry-run] [--engine docker|orbstack|podman] [--tag TAG] [-f DOCKERFILE]"
                );
                return Ok(0);
            }
            other => {
                return Err(OrbitError::Usage(format!(
                    "unknown image build option `{other}`"
                )));
            }
        }
        i += 1;
    }

    image_build::build_image(engine, &tag, &dockerfile, dry_run)
}

fn parse_engine(value: &str) -> Result<Engine> {
    match value {
        "docker" => Ok(Engine::Docker),
        "orbstack" => Ok(Engine::OrbStack),
        "podman" => Ok(Engine::Podman),
        "auto" => Ok(Engine::detect()),
        other => Err(OrbitError::Usage(format!("unknown engine `{other}`"))),
    }
}

fn parse_mount(value: &str) -> Result<ExplicitMount> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err(OrbitError::Usage(format!(
            "mount `{value}` must be source:target[:ro|rw]"
        )));
    }
    let mode = match parts.get(2).copied().unwrap_or("ro") {
        "ro" => MountMode::Ro,
        "rw" => MountMode::Rw,
        other => return Err(OrbitError::Usage(format!("unknown mount mode `{other}`"))),
    };
    Ok(ExplicitMount {
        source: PathBuf::from(parts[0]),
        target: parts[1].to_string(),
        mode,
    })
}

fn require_value(args: &[String], index: usize, flag: &str) -> Result<String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| OrbitError::Usage(format!("{flag} requires a value")))
}

fn doctor(json: bool) {
    let docker_ok = runner::command_available("docker");
    let podman_ok = runner::command_available("podman");
    let engine = Engine::detect();
    if json {
        let report = serde_json::json!({
            "platform": std::env::consts::OS,
            "docker": docker_ok,
            "podman": podman_ok,
            "default_engine": format!("{:?}", engine).to_ascii_lowercase(),
        });
        println!("{report}");
    } else {
        println!("Platform: {}", std::env::consts::OS);
        println!(
            "Docker: {}",
            if docker_ok { "available" } else { "missing" }
        );
        println!(
            "Podman: {}",
            if podman_ok { "available" } else { "missing" }
        );
        println!("Default engine: {:?}", engine);
    }
}

fn cleanup(dry_run: bool) {
    let engine = Engine::detect();
    let containers = runner::cleanup_stale_containers(engine.binary());
    if dry_run {
        println!(
            "Found {} stale orbit container(s) (dry-run, not removed)",
            containers.len()
        );
    } else {
        println!("Removed {} stale orbit container(s)", containers.len());
    }
}

fn explain(plan: &RunPlan) {
    println!("Orbit run plan");
    println!("Engine: {:?}", plan.engine);
    println!("Agent: {}", plan.agent);
    println!("Command: {}", plan.command.join(" "));
    println!("Cwd: {}", plan.cwd);
    println!("Interactive: {}", plan.interactive);
    println!("Image: {}", plan.image);
    println!("Network: {:?}", plan.network);
    println!();
    println!("Mounts:");
    for mount in &plan.mounts {
        println!(
            "  {} -> {} ({}) - {}",
            mount.source,
            mount.target,
            mount.mode.as_str(),
            mount.reason
        );
    }
    println!();
    println!("Env:");
    for (name, value) in &plan.env {
        println!("  {name}={value}");
    }
    println!();
    println!("Hardening: read-only rootfs, cap-drop=ALL, no-new-privileges, user=1000:1000");
}

fn help() -> String {
    "\
Orbit - run coding agents inside hardened containers

USAGE:
  orbit [flags] <agent> [args...]    Run a known agent (pi, opencode, codex, ...)
  orbit [flags] -- <cmd...>          Run an arbitrary command
  orbit explain [flags] ...          Show the container plan without running
  orbit image build [flags]          Build the base container image
  orbit doctor                       Check engine availability
  orbit cleanup [--dry-run]          Remove stale orbit containers

AGENT ALIASES:
  pi, opencode, codex, claude, amp, cursor-agent, agy, gemini

FLAGS:
  --dry-run              Print the docker command without executing
  -i, --interactive      Attach stdin and TTY (auto for no-arg agents)
  --engine ENGINE        docker (default), orbstack, podman, auto
  --network MODE         bridge (default) or none
  --image TAG            Container image to use
  --workspace PATH       Git worktree root (default: auto-detected)
  --mount SRC:TGT[:MODE] Additional bind mount (ro or rw, default ro)

SAFETY:
  - Read-only rootfs, cap-drop=ALL, no-new-privileges, user 1000:1000
  - No Docker socket, no root/system paths, no whole $HOME
  - Agent $HOME dirs mounted read-write; git/ssh/gpg identity read-only
  - Workspace (git root) mounted read-write

EXAMPLES:
  orbit pi                    # interactive Pi TUI
  orbit pi \"say hi\"           # headless Pi prompt
  orbit opencode              # interactive OpenCode TUI
  orbit -- echo hello         # run arbitrary command
  orbit --dry-run -- pi       # show the docker command
  orbit explain -- pi         # show the mount/network plan
  orbit image build           # build the base image
"
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arg_alias_defaults_to_interactive() {
        let plan = build_plan_from_args(&["pi".to_string()]).unwrap();
        assert_eq!(plan.agent, "pi");
        assert!(plan.interactive);
    }

    #[test]
    fn pi_injects_no_sandbox() {
        let plan = build_plan_from_args(&["pi".to_string(), "say hi".to_string()]).unwrap();
        assert_eq!(plan.command, vec!["pi", "--no-sandbox", "say hi"]);
    }

    #[test]
    fn pi_respects_explicit_no_sandbox() {
        let plan = build_plan_from_args(&["pi".to_string(), "--no-sandbox".to_string()]).unwrap();
        // Should not duplicate --no-sandbox
        let count = plan.command.iter().filter(|c| *c == "--no-sandbox").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn generic_command_does_not_inject_no_sandbox() {
        let plan = build_plan_from_args(&["--".to_string(), "pi".to_string()]).unwrap();
        assert_eq!(plan.command, vec!["pi"]);
    }

    #[test]
    fn alias_with_args_not_interactive() {
        let plan = build_plan_from_args(&["pi".to_string(), "say hi".to_string()]).unwrap();
        assert_eq!(plan.agent, "pi");
        assert!(!plan.interactive);
    }

    #[test]
    fn generic_command_via_dash_dash() {
        let plan = build_plan_from_args(&["--".to_string(), "echo".to_string(), "hi".to_string()])
            .unwrap();
        assert_eq!(plan.agent, "generic");
        assert_eq!(plan.command, vec!["echo", "hi"]);
    }

    #[test]
    fn unknown_flag_errors() {
        assert!(
            build_plan_from_args(&["--bogus".to_string(), "--".to_string(), "echo".to_string()])
                .is_err()
        );
    }

    #[test]
    fn missing_command_errors() {
        assert!(build_plan_from_args(&["--dry-run".to_string()]).is_err());
    }

    #[test]
    fn help_returns_zero() {
        assert_eq!(run(vec!["--help".to_string()]).unwrap(), 0);
        assert_eq!(run(vec![]).unwrap(), 0);
    }

    #[test]
    fn parse_mount_valid() {
        let mount = parse_mount("/host/data:/data:rw").unwrap();
        assert_eq!(mount.source, PathBuf::from("/host/data"));
        assert_eq!(mount.target, "/data");
        assert_eq!(mount.mode, MountMode::Rw);
    }

    #[test]
    fn parse_mount_default_ro() {
        let mount = parse_mount("/host/data:/data").unwrap();
        assert_eq!(mount.mode, MountMode::Ro);
    }

    #[test]
    fn parse_mount_invalid() {
        assert!(parse_mount("just-a-path").is_err());
        assert!(parse_mount("a:b:c:d").is_err());
        assert!(parse_mount("a:b:bad").is_err());
    }
}
