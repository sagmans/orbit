use crate::docker;
use crate::error::Result;
use crate::plan::Engine;
use std::process::Command;

pub fn build_image(engine: Engine, tag: &str, dockerfile: &str, dry_run: bool) -> Result<i32> {
    let args = vec![
        engine.binary().to_string(),
        "build".to_string(),
        "-f".to_string(),
        dockerfile.to_string(),
        "-t".to_string(),
        tag.to_string(),
        ".".to_string(),
    ];

    let rendered = docker::shell_join(&args);

    if dry_run {
        println!("{rendered}");
        return Ok(0);
    }

    let status = Command::new(&args[0]).args(&args[1..]).status()?;
    Ok(status.code().unwrap_or(1))
}
