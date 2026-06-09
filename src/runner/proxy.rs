use super::process::run_cleanup_once;
use crate::network::NetworkMode;
use crate::plan::{CleanupAction, CleanupStep, RunPlan};
use crate::{OrbitError, Result};
use std::process::Command;
use std::sync::atomic::AtomicBool;

pub(super) fn prepare_runtime_proxy_secret(plan: &mut RunPlan) -> Result<()> {
    if plan.network.mode != NetworkMode::Restricted {
        return Ok(());
    }
    let Some(proxy) = plan.network.raw_upstream_proxy.take() else {
        return Ok(());
    };

    let dir = std::env::temp_dir().join(format!(
        "orbit-proxy-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }

    let file = dir.join("upstream-proxy");
    let stage_result: Result<()> = (|| {
        std::fs::write(&file, proxy)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
        }
        plan.network.upstream_proxy_file = Some(file);
        Ok(())
    })();
    if let Err(err) = stage_result {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(err);
    }
    plan.cleanup.push(CleanupStep {
        id: "upstream-proxy-secret".to_string(),
        action: CleanupAction::RemovePath,
        target: dir.display().to_string(),
    });
    Ok(())
}

pub(super) fn ensure_restricted_proxy_ready(
    plan: &RunPlan,
    engine: &str,
    cleanup: &[CleanupStep],
    cleanup_done: &AtomicBool,
) -> Result<()> {
    let Some(proxy_container) = plan.network.proxy_container.as_deref() else {
        return Ok(());
    };

    for _ in 0..20 {
        if proxy_is_ready(engine, proxy_container)? {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let logs = proxy_logs(engine, proxy_container)
        .ok()
        .filter(|logs| !logs.trim().is_empty())
        .unwrap_or_else(|| "<no proxy logs available>".to_string());
    let _ = run_cleanup_once(engine, cleanup, cleanup_done);
    Err(OrbitError::Process {
        status: 1,
        command: format!(
            "restricted proxy `{proxy_container}` was not ready before workload start; logs: {}",
            logs.trim()
        ),
    })
}

fn proxy_is_ready(engine: &str, proxy_container: &str) -> Result<bool> {
    let inspect = Command::new(engine)
        .args(["inspect", "--format={{.State.Running}}", proxy_container])
        .output()?;
    if !inspect.status.success() || String::from_utf8_lossy(&inspect.stdout).trim() != "true" {
        return Ok(false);
    }

    Ok(Command::new(engine)
        .args(["exec", proxy_container, "orbit-restricted-proxy-ready"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false))
}

fn proxy_logs(engine: &str, proxy_container: &str) -> Result<String> {
    let output = Command::new(engine)
        .args(["logs", proxy_container])
        .output()?;
    let mut logs = String::new();
    logs.push_str(&String::from_utf8_lossy(&output.stdout));
    logs.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(logs)
}
