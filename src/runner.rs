use crate::docker;
use crate::error::{OrbitError, Result};
use crate::plan::RunPlan;
use std::process::{Command, Stdio};
use std::sync::{Arc, atomic::AtomicBool};

pub fn execute(plan: &RunPlan) -> Result<i32> {
    let engine = plan.engine.binary().to_string();
    let cleanup_done = Arc::new(AtomicBool::new(false));

    #[cfg(unix)]
    install_signal_handler(engine.clone(), cleanup_done.clone());

    let args = docker::command_args(plan);
    let display = docker::dry_run_command(plan);

    let result = run_command(&args, &display);
    let _ = run_cleanup_if_needed(&cleanup_done);
    result
}

fn run_command(args: &[String], display: &str) -> Result<i32> {
    let Some((program, rest)) = args.split_first() else {
        return Err(OrbitError::Usage("empty command".to_string()));
    };

    let mut command = Command::new(program);
    command.args(rest);

    match command.status() {
        Ok(status) if status.success() => Ok(status.code().unwrap_or(0)),
        Ok(status) => Err(OrbitError::Process {
            status: status.code().unwrap_or(1),
            command: display.to_string(),
        }),
        Err(err) => Err(OrbitError::Io(err)),
    }
}

fn run_cleanup_if_needed(_cleanup_done: &AtomicBool) -> Result<()> {
    // No proxy containers or staged files to clean up in the simplified version.
    // This is a hook for future cleanup needs.
    Ok(())
}

#[cfg(unix)]
fn install_signal_handler(_engine: String, _cleanup_done: Arc<AtomicBool>) {
    use signal_hook::consts::signal::{SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;

    if let Ok(mut signals) = Signals::new([SIGINT, SIGTERM]) {
        std::thread::spawn(move || {
            if let Some(signal) = signals.forever().next() {
                let code = match signal {
                    SIGINT => 130,
                    SIGTERM => 143,
                    other => 128 + other,
                };
                std::process::exit(code);
            }
        });
    }
}

/// Check if a container engine binary is available.
pub fn command_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Find and remove stale orbit proxy/temp containers (for `orbit cleanup`).
pub fn cleanup_stale_containers(engine: &str) -> Vec<String> {
    let output = Command::new(engine)
        .args([
            "ps",
            "-a",
            "--filter",
            "label=dev.orbit.engine",
            "--format",
            "{{.Names}}",
        ])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let containers = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    for container in &containers {
        let _ = Command::new(engine)
            .args(["rm", "-f", container])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    containers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_command_reports_failure() {
        let args = vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "exit 7".to_string(),
        ];
        let err = run_command(&args, "test").unwrap_err();
        assert!(err.to_string().contains("status 7"));
    }

    #[test]
    fn run_command_succeeds() {
        let args = vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "exit 0".to_string(),
        ];
        let status = run_command(&args, "test").unwrap();
        assert_eq!(status, 0);
    }
}
