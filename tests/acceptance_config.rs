mod support;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use support::*;
use tempfile::tempdir;

#[test]
fn config_profiles_and_auto_orbstack_detection_are_supported() {
    let dir = tempdir().expect("tempdir");
    let config = dir.path().join("orbit.json");
    std::fs::write(
        &config,
        r#"{"profiles":{"dev":{"engine":"podman","network":"open","image":"example/orbit:test"}}}"#,
    )
    .expect("config");

    orbit()
        .args([
            "--config",
            config.to_str().unwrap(),
            "--profile",
            "dev",
            "--dry-run",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("podman run"))
        .stdout(predicate::str::contains("example/orbit:test"))
        .stdout(predicate::str::contains("--network=bridge"));

    orbit()
        .args([
            "image",
            "build",
            "--dry-run",
            "--config",
            config.to_str().unwrap(),
            "--profile",
            "dev",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("podman build"));

    orbit()
        .args([
            "image",
            "build",
            "--dry-run",
            "--engine",
            "docker",
            "--config",
            config.to_str().unwrap(),
            "--profile",
            "dev",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("docker build"));

    orbit()
        .args([
            "--engine",
            "docker",
            "--network",
            "none",
            "--image",
            "cli-image:test",
            "--config",
            config.to_str().unwrap(),
            "--profile",
            "dev",
            "--dry-run",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("docker run"))
        .stdout(predicate::str::contains("cli-image:test"))
        .stdout(predicate::str::contains("--network=none"));

    orbit()
        .env("ORBSTACK", "true")
        .args(["--engine", "auto", "--dry-run", "--", "echo", "hi"])
        .assert()
        .success()
        .stdout(predicate::str::contains("label=dev.orbit.engine=orbstack"));
}

#[test]
fn persistent_user_config_loads_before_project_config() {
    let home = tempdir().expect("home");
    let user_config_dir = home.path().join(".config/orbit");
    std::fs::create_dir_all(&user_config_dir).expect("user config dir");
    std::fs::write(
        user_config_dir.join("config.json"),
        r#"{"excluded_extensions":["pi-telegram"],"profiles":{"dev":{"network":"none","image":"user/orbit:test"}}}"#,
    )
    .expect("user config");
    let project_dir = tempdir().expect("project config dir");
    let project_config = project_dir.path().join("orbit.json");
    std::fs::write(
        &project_config,
        r#"{"profiles":{"dev":{"network":"open","image":"project/orbit:test","excluded_extensions":["pi-simplify"]}}}"#,
    )
    .expect("project config");

    let output = orbit()
        .env("HOME", home.path())
        .args([
            "explain",
            "--json",
            "--config",
            project_config.to_str().unwrap(),
            "--profile",
            "dev",
            "--",
            "pi",
            "--version",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("json");
    assert_eq!(json["image"], "project/orbit:test");
    assert_eq!(json["network"]["mode"], "open");
    assert!(
        json["env"]
            .as_array()
            .expect("env")
            .iter()
            .all(|entry| { entry["name"] != "ORBIT_EXCLUDED_EXTENSIONS" })
    );
}

#[test]
fn persistent_user_config_does_not_require_project_profile() {
    let home = tempdir().expect("home");
    let user_config_dir = home.path().join(".config/orbit");
    std::fs::create_dir_all(&user_config_dir).expect("user config dir");
    std::fs::write(
        user_config_dir.join("config.json"),
        r#"{"excluded_extensions":["pi-telegram"]}"#,
    )
    .expect("user config");
    let project_dir = tempdir().expect("project config dir");
    let project_config = project_dir.path().join("orbit.json");
    std::fs::write(
        &project_config,
        r#"{"profiles":{"dev":{"excluded_extensions":["pi-simplify"]}}}"#,
    )
    .expect("project config");

    orbit()
        .env("HOME", home.path())
        .args([
            "image",
            "build",
            "--dry-run",
            "--config",
            project_config.to_str().unwrap(),
            "--profile",
            "dev",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("pi-sandbox").not())
        .stdout(predicate::str::contains("pi-telegram").not())
        .stdout(predicate::str::contains("pi-simplify").not());
}

#[test]
fn config_excluded_extensions_validate_but_do_not_filter_runtime_state() {
    let home = tempdir().expect("home");
    let settings_dir = home.path().join(".pi/agent");
    std::fs::create_dir_all(&settings_dir).expect("settings dir");
    std::fs::write(
        settings_dir.join("settings.json"),
        r#"{"packages":["npm:pi-subagents","npm:pi-sandbox",{"source":"git:github.com/badlogic/pi-telegram"},"npm:@linear/sdk@1.2.3"],"extensions":["./extensions/qna","./extensions/pi-telegram/index.ts"]}"#,
    )
    .expect("settings");
    let dir = tempdir().expect("config dir");
    let config = dir.path().join("orbit.json");
    std::fs::write(
        &config,
        r#"{"excluded_extensions":["pi-telegram","@linear/sdk"],"profiles":{"dev":{"excluded_extensions":["pi-simplify"]}}}"#,
    )
    .expect("config");

    let output = orbit()
        .env("HOME", home.path())
        .args([
            "explain",
            "--json",
            "--config",
            config.to_str().unwrap(),
            "--profile",
            "dev",
            "--",
            "pi",
            "--version",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("json");
    assert!(
        json["env"]
            .as_array()
            .expect("env")
            .iter()
            .all(|entry| { entry["name"] != "ORBIT_EXCLUDED_EXTENSIONS" })
    );

    orbit()
        .env("HOME", home.path())
        .args([
            "image",
            "build",
            "--dry-run",
            "--config",
            config.to_str().unwrap(),
            "--profile",
            "dev",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("pi-subagents").not())
        .stdout(predicate::str::contains("pi-sandbox").not())
        .stdout(predicate::str::contains("pi-telegram").not())
        .stdout(predicate::str::contains("@linear/sdk").not());

    for value in ["bad/name", "@bad/name/extra"] {
        let bad_config = dir
            .path()
            .join(format!("bad-orbit-{value}.json").replace('/', "-"));
        std::fs::write(
            &bad_config,
            format!(r#"{{"excluded_extensions":["{value}"]}}"#),
        )
        .expect("bad config");
        orbit()
            .args([
                "--config",
                bad_config.to_str().unwrap(),
                "--dry-run",
                "--",
                "echo",
                "hi",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid excluded extension"));
    }
}
