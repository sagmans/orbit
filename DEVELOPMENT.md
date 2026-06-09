# Development

## Local workflow

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
git diff --check
```

Useful smoke checks:

```sh
cargo build
./target/debug/orbit --dry-run -- echo hi
./target/debug/orbit --dry-run pi
./target/debug/orbit explain -- pi --version
./target/debug/orbit doctor
```

## Test coverage

| Layer | Coverage |
|---|---|
| CLI | Parsing, aliases, unknown flags, product commands. |
| Plan/rendering | Stable Docker/Podman command generation and JSON explain. |
| Mount policy | Root/home/socket refusal, symlink escape, duplicate targets, grammar injection. |
| Network | default `restricted`, `none`, `open`, allowlist, IP refusal, proxy redaction. |
| Agents | Direct top-level coding-agent mounts, full Pi home state, Pi extension symlink target mounts, SSH/GPG socket validation. |
| Runner | Dry-run no-spawn, cleanup on success/failure/signals. |
| Profiles | JSON profile defaults and CLI override behavior. |

Current suite includes unit tests plus a behavior-focused acceptance matrix in `tests/acceptance_*.rs` with shared helpers in `tests/support/mod.rs`.

## Module ownership

- Add CLI flags in `src/cli/parse.rs`, plan construction in `src/cli/plan_builder.rs`, command output in `src/cli/render.rs`, image-build behavior in `src/cli/image_build.rs`, and product commands in `src/cli/product_commands.rs`; reflect execution-affecting fields in `src/plan.rs`.
- Add policy checks near the policy domain: mounts in `src/mount_policy.rs`, network in `src/network.rs`, and agent-specific state under `src/agents/*.rs`.
- Keep `src/docker.rs` as renderer only; it should consume `RunPlan` rather than decide policy.
- Keep runner execution in `src/runner/mod.rs` with runtime helpers under `src/runner/*.rs` for cleanup, Git metadata, GH auth, and proxy prep.
- Update the matching `tests/acceptance_*.rs` file for user-visible behavior; add reusable integration helpers to `tests/support/mod.rs`.

## Acceptance test matrix

| File | Behavior |
|---|---|
| `tests/acceptance_cli.rs` | Core dry-run, workspace targeting, CLI boundaries, explain output, aliases, engine surfaces. |
| `tests/acceptance_config.rs` | Config/profile loading, persistence precedence, compatible excluded-extension validation. |
| `tests/acceptance_git.rs` | Worktree and Git metadata mount behavior. |
| `tests/acceptance_mounts.rs` | Mount refusal, duplicate target, grammar injection, explicit write behavior. |
| `tests/acceptance_network.rs` | Restricted networking, allowlists, proxy redaction/secrets/readiness failures. |
| `tests/acceptance_identity.rs` | Socket forwarding, identity mounts, non-Pi agent home state. |
| `tests/acceptance_agents_pi.rs` | Pi alias, direct top-level Pi state mount behavior, and legacy auth exclusions. |
| `tests/acceptance_entrypoint.rs` | Minimal `orbit-agent-entrypoint` pass-through behavior. |
| `tests/acceptance_runner_cleanup.rs` | Dry-run no-spawn, signal cleanup, labeled proxy cleanup. |
| `tests/acceptance_image_build.rs` | Custom images, base image contract, image build/product commands/help. |

## Design rules

- Build a `RunPlan` before execution.
- Keep dry-run and explain output redacted and reviewable.
- Refuse unsafe input before command generation.
- Do not mount broad host paths.
- Agent home mounts are alias-only and top-level; broad RW within supported agent dirs is deliberate wrapper behavior.
- Add abstractions only when there are multiple real consumers.
- Base image should stay on `debian:bookworm-slim`, install `ripgrep`/`rg`, `fd-find`/`fd`, `bubblewrap`/`bwrap`, and `socat`, install pinned `mise`, install all host global `~/.config/mise/config.toml` `[tools]` plus `gh` when absent through mise with active config stored at `/usr/local/etc/mise/config.toml` (fallback `node@24.16.0`, `rust@1.95.0`, and `gh@2.93.0` when no host mise config exists or `--no-host-mise-tools` is set), pass host `pi --version` by default unless `--pi-version` or `--no-host-pi-version` says otherwise, install `sem`/`inspect-mcp` via cargo into `/usr/local/share/mise/installs/cargo-tools`, then install npm-backed agent CLIs through mise. Vendor-only CLIs belong in derived images.
- Interactive agent use should attach stdin/TTY only for `-i`/`--interactive` or no-arg aliases; headless prompts should stay non-TTY.
- Do not inject extension-owned Pi flags from host settings. Do not read host Pi settings during image build. Do not add Pi snapshots, settings sanitizers, package-store links, or subgroup mounts; mount supported top-level coding-agent dirs/files directly for agent aliases.

## Pre-ship gate

```sh
git status --short --branch
git diff --check main..HEAD
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
./target/debug/orbit --dry-run -- echo hi
./target/debug/orbit --dry-run pi
./target/debug/orbit explain -- pi --version
./target/debug/orbit image build --no-host-mise-tools --tag orbit-agent:node24-npm-tools-test
docker run --rm --read-only --tmpfs /tmp:rw,noexec,nosuid,nodev --tmpfs /var/tmp:rw,noexec,nosuid,nodev --tmpfs /home/orbit:rw,exec,nosuid,nodev,uid=1000,gid=1000,mode=755 --user=1000:1000 orbit-agent:node24-npm-tools-test sh -c 'node --version && npm --version && rustc --version && cargo --version && gh --version && bwrap --version && socat -V >/dev/null && rg --version && fd --version && sem --version && command -v inspect-mcp && command -v inspect && pi --version && opencode --version && codex --version && claude --version && amp --version && gemini --version'
```

Expected result after docs are committed: clean branch, unit tests, acceptance tests, doctests, and image smoke passing.
