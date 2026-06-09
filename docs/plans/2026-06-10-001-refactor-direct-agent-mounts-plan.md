---
title: refactor: Direct RW coding-agent mounts
status: active
date: 2026-06-10
origin: docs/agent-mount-inventory.md
---

# refactor: Direct RW coding-agent mounts

## Problem

`make build` failed because image builds still consumed host Pi package config and asked npm to install host-specific packages. That made the base image depend on mutable host Pi state and unpublished transitive npm packages.

Runtime also carried an obsolete Pi-specific synchronization model: host `~/.pi/agent` was copied into a temp source, sanitized by the entrypoint, linked to baked package stores, and exposed through scoped submounts.

## Decision

Orbit is a thin container wrapper, not a Pi package installer or Pi state synchronizer.

- Image builds read host mise `[tools]` and host `pi --version` only.
- Image builds never read host Pi settings and never bake host Pi packages.
- Agent aliases mount supported coding-agent top-level dirs/files directly, read-write.
- Generic commands mount no agent state.
- Orbit does not create managed subgroup mounts inside agent homes.
- Pi `~/.pi` mounts directly at `/home/orbit/.pi` read-write.
- Absolute symlink targets from `~/.pi/agent/extensions/*` mount read-only at their original target paths so linked local extensions resolve without Pi-home subgroup mounts.
- `orbit-agent-entrypoint` only execs the requested command.

## Requirements

- R1. Remove `ORBIT_PI_PACKAGES_JSON` and Dockerfile host Pi package resolver behavior.
- R2. Remove runner Pi snapshot/copy-source preparation.
- R3. Remove entrypoint copy/sanitize/link behavior.
- R4. Mount supported coding-agent top-level dirs/files RW for every non-generic agent alias.
- R5. Keep whole `$HOME`, root, Docker socket, broad parents, duplicate targets, symlink escapes, and mount grammar injection refused.
- R6. Keep shared Git/SSH/GPG identity mounts read-only/redacted.
- R7. Keep restricted network, proxy, GH auth staging, and Git metadata staging behavior.
- R8. Preserve `excluded_extensions` config validation for compatibility, but do not use it for runtime filtering or image package filtering.
- R9. Update tests and docs so no current path implies Pi snapshot, sanitization, or host Pi package baking.

## Implementation units

### U1. Image build no longer consumes host Pi packages

Files:

- `src/cli/image_build.rs`
- `src/cli/render.rs`
- `docker/orbit-agent.Dockerfile`
- `tests/acceptance_image_build.rs`

Checks:

- Dry-run image build contains `ORBIT_MISE_TOOLS` and `PI_VERSION` only.
- Dockerfile contains no `ORBIT_PI_PACKAGES_JSON`, `DefaultPackageManager`, or `/opt/orbit/pi-agent` package-store contract.
- Host Pi settings secrets/package names never appear in image-build dry-run.

### U2. Direct top-level agent mounts

Files:

- `src/agents/home_state.rs`
- `src/agents/pi_state.rs`
- `src/agents/mod.rs`
- `src/agents/targets.rs`
- `tests/acceptance_identity.rs`
- `tests/acceptance_agents_pi.rs`

Checks:

- Agent aliases mount supported top-level coding-agent dirs/files RW.
- Pi mounts full `~/.pi` as `/home/orbit/.pi` RW.
- No managed submount exists for Pi settings, MCP, sessions, package stores, tokens, or extension entries under `/home/orbit/.pi`.
- Absolute symlink targets from `~/.pi/agent/extensions/*` mount read-only at their original target paths.
- Generic commands mount no agent state.
- Symlinked `~/.pi` or `~/.pi/agent` is refused.

### U3. Delete runtime Pi sync surfaces

Files:

- `src/runner/mod.rs`
- `src/runner/pi_agent_snapshot.rs` (deleted)
- `src/agents/pi_sessions.rs` (deleted)
- `docker/orbit-agent-entrypoint`
- `tests/acceptance_entrypoint.rs`

Checks:

- Runner prepares Git metadata, GH auth, and proxy secret only.
- `RunPlan` has no Pi copy-source field.
- Entrypoint executes `exec "$@"` and creates no Pi state.
- Tests assert old env vars do not affect entrypoint behavior.

### U4. Config compatibility

Files:

- `src/config.rs`
- `tests/acceptance_config.rs`
- `USAGE.md`

Checks:

- `excluded_extensions` still parses, merges, dedupes, and validates names.
- Explain JSON emits no `ORBIT_EXCLUDED_EXTENSIONS` env.
- Image build dry-run emits no Pi package names, excluded or otherwise.

### U5. Docs

Files:

- `README.md`
- `USAGE.md`
- `INSTALLATION.md`
- `ARCHITECTURE.md`
- `DEVELOPMENT.md`
- `SCOPE.md`
- `docs/agent-mount-inventory.md`
- this plan

Checks:

- Current docs state direct RW top-level agent mounts.
- Current docs state no image-time host Pi package baking.
- Current docs state no runtime Pi snapshot/sanitize/link behavior.
- Security tradeoff is explicit: top-level agent credentials/config are writable from the container.

## 12-factor review

| Factor | Status | Finding | Action |
|---|---|---|---|
| Dependencies | risk | Image no longer depends on host Pi package settings or unpublished host package deps, but default Pi binary follows host `pi --version` for local parity. | Keep image build scoped to pinned tools, host mise `[tools]`, and validated host Pi version only. |
| Config | risk | Runtime behavior now depends on broad mounted host agent config/state. | Document direct-wrapper tradeoff and keep generic command no-state default. |
| Build/release/run | pass | Host Pi packages no longer install during build; runtime no longer mutates copied app state. | Validate with `make build` and image dry-run. |
| Processes | risk | Agent processes can mutate host agent dirs by design. | Keep explicit docs and audit entry for RW agent homes. |

## Validation plan

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --locked`
- `cargo build --locked`
- `git diff --check`
- `./target/debug/orbit --dry-run -- echo hi`
- `./target/debug/orbit --dry-run pi --version`
- `./target/debug/orbit explain --json -- pi --version`
- `cargo run --locked -- image build --dry-run --no-host-mise-tools`
- `make build` when local Docker/network can complete
