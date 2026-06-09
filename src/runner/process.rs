use crate::docker;
#[cfg(test)]
use crate::plan::RunPlan;
use crate::plan::{CleanupAction, CleanupStep};
use crate::{OrbitError, Result};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) fn run_command_with_cleanup(
    args: &[String],
    display: &str,
    engine: &str,
    cleanup: &[CleanupStep],
    cleanup_done: &AtomicBool,
) -> Result<i32> {
    let result = run_command(args, display, engine, cleanup, cleanup_done);
    if let Err(err) = run_cleanup_once(engine, cleanup, cleanup_done) {
        warn_cleanup_failure(engine, display, &err);
    }
    result
}

pub(super) fn run_command(
    args: &[String],
    display: &str,
    engine: &str,
    cleanup: &[CleanupStep],
    cleanup_done: &AtomicBool,
) -> Result<i32> {
    run_command_with_stdout(args, display, engine, cleanup, cleanup_done, false)
}

pub(super) fn run_command_quiet_stdout(
    args: &[String],
    display: &str,
    engine: &str,
    cleanup: &[CleanupStep],
    cleanup_done: &AtomicBool,
) -> Result<i32> {
    run_command_with_stdout(args, display, engine, cleanup, cleanup_done, true)
}

fn run_command_with_stdout(
    args: &[String],
    display: &str,
    engine: &str,
    cleanup: &[CleanupStep],
    cleanup_done: &AtomicBool,
    quiet_stdout: bool,
) -> Result<i32> {
    let Some((program, rest)) = args.split_first() else {
        return Err(OrbitError::Usage("empty command".to_string()));
    };

    let mut command = Command::new(program);
    command.args(rest);
    if quiet_stdout {
        command.stdout(Stdio::null());
    }
    let status = command.status();
    match status {
        Ok(status) if status.success() => Ok(status.code().unwrap_or(0)),
        Ok(status) => {
            let err = OrbitError::Process {
                status: status.code().unwrap_or(1),
                command: display.to_string(),
            };
            if let Err(cleanup_err) = run_cleanup_once(engine, cleanup, cleanup_done) {
                warn_cleanup_failure(engine, display, &cleanup_err);
            }
            Err(err)
        }
        Err(err) => {
            let err = OrbitError::Io(err);
            if let Err(cleanup_err) = run_cleanup_once(engine, cleanup, cleanup_done) {
                warn_cleanup_failure(engine, display, &cleanup_err);
            }
            Err(err)
        }
    }
}

pub(super) fn warn_cleanup_failure(engine: &str, display: &str, err: &OrbitError) {
    eprintln!("warning: cleanup failed after `{display}` using {engine}: {err}");
}

pub(super) fn install_signal_cleanup(
    engine: String,
    cleanup: Vec<CleanupStep>,
    cleanup_done: Arc<AtomicBool>,
) {
    #[cfg(unix)]
    {
        use signal_hook::consts::signal::{SIGINT, SIGTERM};
        use signal_hook::iterator::Signals;

        if let Ok(mut signals) = Signals::new([SIGINT, SIGTERM]) {
            std::thread::spawn(move || {
                if let Some(signal) = signals.forever().next() {
                    let _ = run_cleanup_once(&engine, &cleanup, &cleanup_done);
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
    #[cfg(not(unix))]
    let _ = (engine, cleanup, cleanup_done);
}

pub(crate) fn run_cleanup_once(
    engine: &str,
    steps: &[CleanupStep],
    cleanup_done: &AtomicBool,
) -> Result<()> {
    if cleanup_done.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    run_cleanup_steps(engine, steps)
}

pub fn run_cleanup_steps(engine: &str, steps: &[CleanupStep]) -> Result<()> {
    let mut failures = Vec::new();
    for step in steps.iter().rev() {
        match step.action {
            CleanupAction::RemovePath => {
                let path = std::path::Path::new(&step.target);
                let result = if path.is_dir() {
                    std::fs::remove_dir_all(path)
                } else if path.exists() {
                    std::fs::remove_file(path)
                } else {
                    Ok(())
                };
                if let Err(err) = result {
                    failures.push(format!("{}: {err}", step.id));
                }
            }
            CleanupAction::StopProxy => {
                if let Err(err) = run_cleanup_command(engine, &["rm", "-f", &step.target]) {
                    failures.push(format!("{}: {err}", step.id));
                }
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(OrbitError::Process {
            status: 1,
            command: format!("cleanup failed: {}", failures.join("; ")),
        })
    }
}

pub(super) fn run_cleanup_command(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        let command = std::iter::once(program.to_string())
            .chain(args.iter().map(|arg| (*arg).to_string()))
            .collect::<Vec<_>>();
        Err(OrbitError::Process {
            status: status.code().unwrap_or(1),
            command: docker::shell_join(&command),
        })
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct FakeRunner {
    pub commands: Vec<Vec<String>>,
    pub cleanup: Vec<String>,
    pub fail: bool,
}

#[cfg(test)]
impl FakeRunner {
    pub fn run(&mut self, plan: &RunPlan) -> Result<()> {
        self.commands.push(docker::command_args(plan, false));
        if self.fail {
            self.cleanup(plan);
            return Err(OrbitError::Process {
                status: 1,
                command: "fake".to_string(),
            });
        }
        self.cleanup(plan);
        Ok(())
    }

    pub fn cleanup(&mut self, plan: &RunPlan) {
        for step in plan.cleanup.iter().rev() {
            self.cleanup
                .push(format!("{:?}:{}", step.action, step.target));
        }
    }
}
