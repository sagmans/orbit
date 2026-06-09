mod support;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use support::*;
use tempfile::tempdir;

#[test]
fn custom_image_keeps_trusted_proxy_image_by_default() {
    orbit()
        .args([
            "--dry-run",
            "--image",
            "custom/orbit:test",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "orbit-agent:latest orbit-restricted-proxy",
        ))
        .stdout(predicate::str::contains("custom/orbit:test echo hi"));

    let output = orbit()
        .args([
            "explain",
            "--json",
            "--image",
            "custom/orbit:test",
            "--proxy-image",
            "custom/proxy:test",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).expect("json");
    assert_eq!(json["image"], "custom/orbit:test");
    assert_eq!(json["network"]["proxy_image"], "custom/proxy:test");
    assert!(
        json["audit"]
            .as_array()
            .expect("audit")
            .iter()
            .any(|entry| { entry["code"] == "custom_proxy_image" && entry["level"] == "warn" })
    );

    orbit()
        .args([
            "--dry-run",
            "--image",
            "custom/orbit:test",
            "--proxy-image",
            "custom/proxy:test",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "custom/proxy:test orbit-restricted-proxy",
        ))
        .stdout(predicate::str::contains("custom/orbit:test echo hi"));

    orbit()
        .args(["--dry-run", "--image", "--privileged", "--", "echo", "hi"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("valid container image reference"));
    orbit()
        .args([
            "--dry-run",
            "--proxy-image",
            "--privileged",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("valid container image reference"));

    let dir = tempdir().expect("config dir");
    let config = dir.path().join("orbit.json");
    std::fs::write(
        &config,
        r#"{"profiles":{"bad":{"proxy_image":"--privileged"}}}"#,
    )
    .expect("config");
    orbit()
        .args([
            "--config",
            config.to_str().unwrap(),
            "--profile",
            "bad",
            "--dry-run",
            "--",
            "echo",
            "hi",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("valid container image reference"));
}

#[test]
fn base_image_installs_node_24_and_npm_agent_clis() {
    let dockerfile = std::fs::read_to_string(repo_root().join("docker/orbit-agent.Dockerfile"))
        .expect("dockerfile");
    assert!(dockerfile.contains("FROM debian:bookworm-slim"));
    assert!(dockerfile.contains("ARG MISE_VERSION=2026.5.18"));
    assert!(dockerfile.contains("ARG NODE_VERSION=24.16.0"));
    assert!(dockerfile.contains("ARG RUST_VERSION=1.95.0"));
    assert!(dockerfile.contains("ARG GH_VERSION=2.93.0"));
    assert!(dockerfile.contains("ARG PI_VERSION=0.79.1"));
    assert!(dockerfile.contains("ARG OPENCODE_VERSION=1.15.13"));
    assert!(dockerfile.contains("ARG CODEX_VERSION=0.136.0"));
    assert!(dockerfile.contains("ARG CLAUDE_VERSION=2.1.160"));
    assert!(dockerfile.contains("ARG GEMINI_VERSION=0.44.1"));
    assert!(dockerfile.contains("ARG AMP_VERSION=0.0.1780391988-g4f09f3"));
    assert!(dockerfile.contains("ARG SEM_REV=54df10ca18313471776bffcc1e040ceccdf7eced"));
    assert!(dockerfile.contains("ARG INSPECT_REV=90a8a5dd3a15c39e59062e2518171615fecff5c3"));
    assert!(dockerfile.contains("ARG ORBIT_MISE_TOOLS=\"\""));
    assert!(!dockerfile.contains("ARG ORBIT_PI_PACKAGES_JSON"));
    assert!(dockerfile.contains("https://github.com/jdx/mise/releases/download/v${MISE_VERSION}"));
    assert!(dockerfile.contains("sha256sum -c -"));
    assert!(
        dockerfile.contains("3c19d4861684e4ed5b8d020bcbdd99f478df43583fb767bd8a663983d4a4e209")
    );
    assert!(
        dockerfile.contains("cfac593469d028d7ae5fe36e37bd7c59118b5238e92d8a876209578464f24a84")
    );
    assert!(dockerfile.contains("MISE_DATA_DIR=\"/usr/local/share/mise\""));
    assert!(dockerfile.contains("MISE_CONFIG_DIR=\"/usr/local/etc/mise\""));
    assert!(dockerfile.contains("MISE_GLOBAL_CONFIG_FILE=\"/usr/local/etc/mise/config.toml\""));
    assert!(dockerfile.contains("MISE_STATE_DIR=\"/home/orbit/.local/state/mise\""));
    assert!(dockerfile.contains("RUSTUP_HOME=\"/usr/local/share/mise/rustup\""));
    assert!(dockerfile.contains("CARGO_HOME=\"/usr/local/share/mise/cargo\""));
    assert!(!dockerfile.contains("ORBIT_BAKED_PI_AGENT_DIR"));
    assert!(
        dockerfile.contains("ORBIT_CARGO_TOOLS_DIR=\"/usr/local/share/mise/installs/cargo-tools\"")
    );
    assert!(dockerfile.contains("TMPDIR=\"/home/orbit\""));
    assert!(dockerfile.contains("/usr/local/share/mise/installs/cargo-tools/bin"));
    assert!(dockerfile.contains("/usr/local/share/mise/cargo/bin"));
    assert!(dockerfile.contains("/usr/local/share/mise/shims"));
    assert!(dockerfile.contains("npm.package_manager = \"npm\""));
    assert!(
        dockerfile.contains(
            "mise_tools=\"${ORBIT_MISE_TOOLS:-node@${NODE_VERSION} rust@${RUST_VERSION} gh@${GH_VERSION}}\""
        )
    );
    assert!(dockerfile.contains("mise use -g ${mise_tools}"));
    assert!(dockerfile.contains("npm:@earendil-works/pi-coding-agent@${PI_VERSION}"));
    assert!(dockerfile.contains("npm:opencode-ai@${OPENCODE_VERSION}"));
    assert!(dockerfile.contains("npm:@openai/codex@${CODEX_VERSION}"));
    assert!(dockerfile.contains("npm:@anthropic-ai/claude-code@${CLAUDE_VERSION}"));
    assert!(dockerfile.contains("npm:@google/gemini-cli@${GEMINI_VERSION}"));
    assert!(dockerfile.contains("npm:@ampcode/cli@${AMP_VERSION}"));
    assert!(dockerfile.contains("build-essential"));
    assert!(dockerfile.contains("bubblewrap"));
    assert!(dockerfile.contains("cmake"));
    assert!(dockerfile.contains("fd-find"));
    assert!(dockerfile.contains("libffi-dev"));
    assert!(dockerfile.contains("libssl-dev"));
    assert!(dockerfile.contains("libyaml-dev"));
    assert!(dockerfile.contains("pkg-config"));
    assert!(dockerfile.contains("ripgrep"));
    assert!(dockerfile.contains("socat"));
    assert!(dockerfile.contains("zlib1g-dev"));
    assert!(dockerfile.contains("ln -sf /usr/bin/fdfind /usr/local/bin/fd"));
    assert!(dockerfile.contains("update-alternatives --set iptables /usr/sbin/iptables-legacy"));
    assert!(dockerfile.contains("update-alternatives --set ip6tables /usr/sbin/ip6tables-legacy"));
    assert!(
        dockerfile
            .contains("COPY docker/orbit-restricted-proxy /usr/local/bin/orbit-restricted-proxy")
    );
    assert!(dockerfile.contains(
        "COPY docker/orbit-restricted-proxy-ready /usr/local/bin/orbit-restricted-proxy-ready"
    ));
    assert!(
        dockerfile
            .contains("COPY docker/orbit-agent-entrypoint /usr/local/bin/orbit-agent-entrypoint")
    );
    let proxy_script = std::fs::read_to_string(repo_root().join("docker/orbit-restricted-proxy"))
        .expect("proxy script");
    let ready_script =
        std::fs::read_to_string(repo_root().join("docker/orbit-restricted-proxy-ready"))
            .expect("proxy ready script");
    let entrypoint_script =
        std::fs::read_to_string(repo_root().join("docker/orbit-agent-entrypoint"))
            .expect("entrypoint script");
    assert!(proxy_script.starts_with("#!/bin/sh\n"));
    assert!(ready_script.starts_with("#!/bin/sh\n"));
    assert!(entrypoint_script.starts_with("#!/bin/sh\n"));
    assert!(proxy_script.contains(".domain entries match exact hosts and subdomains"));
    assert!(proxy_script.contains("[ -n \"$domain\" ] || continue"));
    assert!(proxy_script.contains("case \"$domain\" in"));
    assert!(proxy_script.contains("*) printf '.%s\\n' \"$domain\""));
    assert!(proxy_script.contains("printf '%s\\n' \"$domain\""));
    assert!(proxy_script.contains("iptables -w -F OUTPUT"));
    assert!(proxy_script.contains("grep -vq ' lo$' /proc/net/if_inet6"));
    assert!(proxy_script.contains("ip6tables -w -F OUTPUT"));
    assert!(ready_script.contains("squid_uid=\"$(id -u squid)\""));
    assert!(ready_script.contains("--uid-owner (squid|${squid_uid}) -j ACCEPT"));
    assert!(ready_script.contains("[ \"$(cat /proc/1/comm)\" = \"squid\" ]"));
    assert!(entrypoint_script.contains("exec \"$@\""));
    assert!(!entrypoint_script.contains("/opt/orbit/pi-agent"));
    assert!(!entrypoint_script.contains("ORBIT_PI_SETTINGS_SOURCE"));
    assert!(!entrypoint_script.contains("ORBIT_PI_MCP_SOURCE"));
    assert!(!entrypoint_script.contains("pi-sandbox"));
    assert!(!entrypoint_script.contains("@steipete/peekaboo"));
    assert!(!entrypoint_script.contains("ORBIT_EXCLUDED_EXTENSIONS"));
    assert!(!entrypoint_script.contains("ORBIT_PI_AGENT_COPY_SOURCE_ROOT"));
    assert!(!entrypoint_script.contains("copy_tree"));
    assert!(!entrypoint_script.contains("link_baked_npm"));
    assert!(!entrypoint_script.contains("link_extension_node_modules"));
    assert!(dockerfile.contains("cargo install --locked --root \"$ORBIT_CARGO_TOOLS_DIR\" --git https://github.com/Ataraxy-Labs/sem --rev \"$SEM_REV\" sem-cli"));
    assert!(dockerfile.contains("cargo install --locked --root \"$ORBIT_CARGO_TOOLS_DIR\" --git https://github.com/Ataraxy-Labs/inspect --rev \"$INSPECT_REV\" inspect-mcp"));
    assert!(dockerfile.contains("ln -sf inspect-mcp \"$ORBIT_CARGO_TOOLS_DIR/bin/inspect\""));
    assert!(dockerfile.contains("command -v rustc"));
    assert!(dockerfile.contains("command -v cargo"));
    assert!(dockerfile.contains("command -v gh"));
    assert!(dockerfile.contains("command -v bwrap"));
    assert!(dockerfile.contains("command -v socat"));
    assert!(dockerfile.contains("command -v rg"));
    assert!(dockerfile.contains("command -v fd"));
    assert!(dockerfile.contains("command -v sem"));
    assert!(dockerfile.contains("command -v inspect-mcp"));
    assert!(dockerfile.contains("command -v inspect"));
    assert!(dockerfile.contains("command -v pi"));
    assert!(dockerfile.contains("postinstall.mjs"));
    assert!(dockerfile.contains("install.cjs"));
    assert!(dockerfile.contains("command -v opencode"));
    assert!(dockerfile.contains("command -v codex"));
    assert!(dockerfile.contains("command -v claude"));
    assert!(dockerfile.contains("command -v amp"));
    assert!(dockerfile.contains("command -v gemini"));
    assert!(!dockerfile.contains("DefaultPackageManager"));
    assert!(!dockerfile.contains("ORBIT_PI_PACKAGES_JSON"));
    assert!(!dockerfile.contains("await packageManager.resolve()"));
    assert!(proxy_script.contains("access_log none"));
    assert!(proxy_script.contains("ORBIT_UPSTREAM_PROXY_FILE"));
    assert!(dockerfile.contains("getent passwd 1000"));
    assert!(dockerfile.contains("usermod -l orbit -d /home/orbit -m"));
    assert!(!dockerfile.contains("FROM node:"));
    assert!(!dockerfile.contains("npm install -g"));
    assert!(!proxy_script.contains("access_log stdio:/dev/stdout"));
    assert!(!dockerfile.contains("https://mise.run"));
    assert!(!dockerfile.contains("@latest"));
    assert!(!dockerfile.contains("node@22"));
    assert!(!dockerfile.contains("cursor.com/install"));
    assert!(!dockerfile.contains("antigravity.google/cli/install.sh"));
}

fn fake_pi_on_path(version: &str) -> tempfile::TempDir {
    let dir = tempdir().expect("fake pi dir");
    let pi = dir.path().join("pi");
    std::fs::write(&pi, format!("#!/bin/sh\nprintf '%s\\n' '{version}'\n")).expect("fake pi");
    let mut perms = std::fs::metadata(&pi)
        .expect("fake pi metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&pi, perms).expect("fake pi executable");
    dir
}

#[test]
fn image_build_uses_host_pi_version_by_default() {
    let fake_pi = fake_pi_on_path("0.99.0-test.1");

    orbit()
        .env("PATH", fake_pi.path())
        .args(["image", "build", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PI_VERSION=0.99.0-test.1"));

    orbit()
        .env("PATH", fake_pi.path())
        .args(["image", "build", "--dry-run", "--no-host-pi-version"])
        .assert()
        .success()
        .stdout(predicate::str::contains("PI_VERSION=").not());

    orbit()
        .args([
            "image",
            "build",
            "--dry-run",
            "--pi-version",
            "1.2.3-custom.4",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("PI_VERSION=1.2.3-custom.4"));

    orbit()
        .args(["image", "build", "--dry-run", "--pi-version", "1.2.3;bad"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported Pi version"));
}

#[test]
fn make_build_invokes_image_build_through_cargo() {
    let makefile = std::fs::read_to_string(repo_root().join("Makefile")).expect("Makefile");
    assert!(makefile.contains("cargo run --locked -- image build --no-host-mise-tools"));
    assert!(!makefile.contains("\n\torbit image build --no-host-mise-tools"));
}

#[test]
fn doctor_cleanup_image_build_and_help_commands_are_available() {
    orbit()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Platform:"))
        .stdout(predicate::str::contains("Docker:"));

    orbit()
        .args(["cleanup", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cleanup plan"));

    let no_pi_home = tempdir().expect("home without pi config");
    orbit()
        .env("HOME", no_pi_home.path())
        .args(["image", "build", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ORBIT_MISE_TOOLS="))
        .stdout(predicate::str::contains("ORBIT_PI_PACKAGES_JSON").not());

    let mise_home = tempdir().expect("home with mise config");
    let mise_config_dir = mise_home.path().join(".config/mise");
    std::fs::create_dir_all(&mise_config_dir).expect("mise config dir");
    std::fs::write(
        mise_config_dir.join("config.toml"),
        r#"[tools]
bun = "1.3.14"
node = "24.11.1"
rust = "1.95.0"
make = "4.4.1"

[settings]
experimental = true
"#,
    )
    .expect("mise config");
    orbit()
        .env("HOME", mise_home.path())
        .args(["image", "build", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "'ORBIT_MISE_TOOLS=bun@1.3.14 make@4.4.1 node@24.11.1 rust@1.95.0 gh@2.93.0'",
        ));

    orbit()
        .env("HOME", mise_home.path())
        .args(["image", "build", "--dry-run", "--no-host-mise-tools"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ORBIT_MISE_TOOLS="))
        .stdout(predicate::str::contains("bun@1.3.14").not())
        .stdout(predicate::str::contains("make@4.4.1").not());

    std::fs::write(
        mise_config_dir.join("config.toml"),
        r#"[tools]
node = "22.21.1"
"#,
    )
    .expect("mise config with old node");
    orbit()
        .env("HOME", mise_home.path())
        .args(["image", "build", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("node` must be 24.x"));

    std::fs::write(
        mise_config_dir.join("config.toml"),
        r#"[tools]
"aqua:https://user:pass@example.com/tool" = "1.0.0"
"#,
    )
    .expect("mise config with secret");
    orbit()
        .env("HOME", mise_home.path())
        .args(["image", "build", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("appears to contain credentials"))
        .stderr(predicate::str::contains("user:pass").not());

    let home = tempdir().expect("home");
    let settings_dir = home.path().join(".pi/agent");
    std::fs::create_dir_all(&settings_dir).expect("settings dir");
    std::fs::write(
        settings_dir.join("settings.json"),
        r#"{"packages":["npm:pi-subagents","npm:pi-sandbox@1.2.3",{"source":"npm:pi-sandbox","extensions":["+index.ts"]},{"source":"pi-sandbox@2.0.0"}],"defaultModel":"secret-model"}"#,
    )
    .expect("settings");
    std::fs::create_dir_all(settings_dir.join("extensions/qna")).expect("qna extension dir");
    std::fs::create_dir_all(settings_dir.join("extensions/pi-sandbox"))
        .expect("excluded extension dir");
    std::fs::write(
        settings_dir.join("extensions/qna/package.json"),
        r#"{"dependencies":{"@linear/sdk":"^1.2.3","bad/name":"1.0.0","unsafe-spec":"file:../unsafe"},"optionalDependencies":{"typebox":"1.0.0"}}"#,
    )
    .expect("extension package");
    std::fs::write(
        settings_dir.join("extensions/pi-sandbox/package.json"),
        r#"{"dependencies":{"pi-sandbox-dep":"1.0.0"}}"#,
    )
    .expect("excluded extension package");

    orbit()
        .env("HOME", home.path())
        .args(["image", "build", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("docker build"))
        .stdout(predicate::str::contains("orbit-agent"))
        .stdout(predicate::str::contains("--build-arg"))
        .stdout(predicate::str::contains("ORBIT_PI_PACKAGES_JSON").not())
        .stdout(predicate::str::contains("npm:pi-subagents").not())
        .stdout(predicate::str::contains("npm:@linear/sdk@^1.2.3").not())
        .stdout(predicate::str::contains("secret-model").not());

    std::fs::write(
        settings_dir.join("settings.json"),
        r#"{"packages":["git:https://ghp_secret@github.com/acme/private","git:https://github.com/acme/private?token=secret"]}"#,
    )
    .expect("secret settings");
    orbit()
        .env("HOME", home.path())
        .args(["image", "build", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ghp_secret").not())
        .stdout(predicate::str::contains("token=secret").not());

    orbit()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Base image installs pinned mise, host ~/.config/mise/config.toml [tools] unless --no-host-mise-tools",
        ))
        .stdout(predicate::str::contains("--no-host-mise-tools"))
        .stdout(predicate::str::contains("--pi-version"))
        .stdout(predicate::str::contains("--no-host-pi-version"))
        .stdout(predicate::str::contains(
            "--proxy-image selects a trusted Orbit-derived image",
        ))
        .stdout(predicate::str::contains(
            "never adds extension-owned Pi flags",
        ))
        .stdout(predicate::str::contains("pinned npm-backed agent CLIs"))
        .stdout(predicate::str::contains(
            "Cursor Agent and Antigravity require vendor installers",
        ));
}
