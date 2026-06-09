# Architecture

Orbit builds a reviewed `RunPlan`, then renders or executes it. The plan is the single source for dry-run output, explain output, tests, and container execution.

## Flow

```text
main -> cli parse -> build RunPlan -> dry-run | explain | runner -> container engine
```

Restricted networking adds one proxy sidecar before the app container and a cleanup step after execution or signal handling.

## Project structure

| Path | Purpose |
|---|---|
| `src/main.rs` | Process entry; exits with CLI result code. |
| `src/cli/mod.rs` + `src/cli/*.rs` | CLI façade plus action dispatch, parsing, rendering, plan building, Git metadata, image-build helpers, and product commands. |
| `src/plan.rs` | Serializable plan types: mounts, env, network, interactive mode, hardening, cleanup, audit. |
| `src/docker.rs` | Docker/Podman argument rendering, dry-run shell output, TTY flags, proxy sidecar command. |
| `src/mount_policy.rs` | Workspace validation, bind-mount refusal rules, redaction. |
| `src/network.rs` | Network modes, allowlist checks, URL host parsing, proxy redaction. |
| `src/agents/mod.rs` + `src/agents/*.rs` | Agent façade plus aliases, target constants, direct Pi/state mounts, identity mounts, top-level home-state mounts, and SSH/GPG socket policy. |
| `src/runner/mod.rs` + `src/runner/*.rs` | Runner façade plus process cleanup, Git metadata staging, GitHub auth staging, and restricted proxy runtime. |
| `src/config.rs` | JSON profile loading and engine detection. |
| `src/explain.rs` | Human-readable run-plan report. |
| `src/error.rs` | Shared error and refusal types. |
| `docker/orbit-agent.Dockerfile` | Base image with pinned mise, host global mise `[tools]` plus `gh` when absent installed via mise unless `--no-host-mise-tools` is set, host Pi CLI version unless `--no-host-pi-version` or `--pi-version` overrides it, active config under `/usr/local/etc/mise/config.toml`, fallback Node 24.16.0/Rust 1.95.0/gh 2.93.0, `rg`, `fd`, `bubblewrap`/`bwrap`, `socat`, `sem`, `inspect-mcp`, npm-backed agent CLIs, and restricted proxy script. |
| `tests/acceptance_*.rs` + `tests/support/mod.rs` | Behavior-focused acceptance matrix with shared CLI/workspace/Git/Pi helpers. |

## Core decisions

- Start flat: modules over deep traits or plugin systems.
- Keep cohesive single-purpose modules as files: `src/docker.rs`, `src/network.rs`, `src/mount_policy.rs`, `src/config.rs`, `src/explain.rs`, `src/plan.rs`, `src/error.rs`, `src/main.rs`, and `src/lib.rs`; split only mixed-responsibility hotspots.
- Use `RunPlan` as the reviewable contract before execution.
- Deny broad `$HOME` access by default; generic commands mount no agent state, while agent aliases mount supported coding-agent top-level state dirs/files read-write as deliberate wrapper behavior.
- Mount current git worktree read-write by default; strict host-`$HOME` descendants target `/home/orbit/<home-relative-suffix>` while non-HOME paths keep absolute targets.
- Map Git root/common metadata with the same target rule and stage rewrites for absolute linked-worktree refs so Git resolves container paths without exposing whole `$HOME`.
- Keep app container hardened; proxy sidecar uses extra privileges only for restricted egress enforcement.
- Redact secret-like paths, sockets, and proxy credentials in review surfaces.
- Treat OrbStack as Docker-compatible; treat Podman as a separate command surface.

## Runtime model

- App container runs as `1000:1000` with read-only rootfs, dropped caps, `no-new-privileges`, and tmpfs runtime paths.
- Interactive plans add `--interactive --tty`; no-arg agent aliases enable this automatically for TUI parity.
- Default network is `restricted` with `registry.npmjs.org` and OpenAI Codex endpoints allowed for default agent dogfooding.
- `restricted` starts `orbit-restricted-proxy-*`, joins app container to that network namespace, injects local proxy env vars, then cleans up.
- `none` fully disables egress; `open` uses engine bridge networking.

## Agent model

- Generic commands run as `orbit -- <cmd...>`.
- Known aliases become the agent name and first command token.
- No-arg aliases are interactive automatically; `-i`/`--interactive` forces TTY for other commands.
- Orbit never injects extension-owned Pi flags; users pass Pi flags explicitly only when the container has that extension.
- Aliases: `pi`, `opencode`, `codex`, `claude`, `amp`, `cursor-agent`, `agy`, `gemini`.
- For agent aliases, existing supported coding-agent top-level dirs/files under `$HOME` are mounted into `/home/orbit` read-write, including full `~/.pi` at `/home/orbit/.pi`. Orbit does not manage subgroup mounts inside those homes; absolute symlink targets from `~/.pi/agent/extensions/*` are mounted read-only at their original target paths so linked local Pi extensions resolve.
- `orbit image build` reads host `~/.config/mise/config.toml` `[tools]` and host `pi --version` only. It does not read host Pi settings, does not pass Pi package build args, and does not install host Pi packages into `/opt/orbit/pi-agent`.
- Runner prepares only Git metadata, GitHub auth staging, and restricted-proxy secrets. It does not prepare Pi snapshots or Pi copy sources.
- `orbit-agent-entrypoint` is pass-through: it execs the requested command and performs no Pi copy, sanitization, or package-store linking.
- `excluded_extensions` is accepted for config compatibility but direct-wrapper mode no longer uses it to filter Pi settings, mounts, or image builds.
- Broad read-write agent homes expose credentials/config inside those top-level dirs to container writes. Shared Git/SSH/GPG identity mounts remain read-only/redacted.
- SSH/GPG forwarding is opt-in, Unix-socket-only, redacted, and warned.

## Important limits

- Base image uses `debian:bookworm-slim`, installs `ripgrep`/`rg`, `fd-find` with an `fd` shim, `bubblewrap`/`bwrap`, `socat`, pinned `mise`, installs every host global `~/.config/mise/config.toml` `[tools]` entry with mise plus `gh` when absent unless `--no-host-mise-tools` is set, falls back to `node@24.16.0`, `rust@1.95.0`, and `gh@2.93.0` when no host mise config exists or host tools are disabled, installs `sem`/`inspect-mcp` with cargo into `/usr/local/share/mise/installs/cargo-tools`, and installs npm-backed agent CLIs (`pi` from host `pi --version` by default; pinned `opencode`, `codex`, `claude`, `amp`, `gemini`). Mise data lives under `/usr/local/share/mise`, active global mise config under `/usr/local/etc/mise/config.toml`, and Rust/cargo state under `/usr/local/share/mise`. Cursor Agent and Antigravity need vendor installers or a derived image.
- `--engine auto` detects OrbStack env only; it does not probe all engines.
- `--image` changes the app container image, not the restricted proxy sidecar image. Use `--proxy-image` only for trusted Orbit-derived proxy images because the proxy runs as root with `NET_ADMIN`.
- Auth-file detection is allowlist/name-based; unknown vendor auth layouts may need a new path entry.
- Restricted URL/bypass checks are defensive static checks, not full command interpretation.
