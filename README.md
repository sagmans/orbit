# Orbit

Orbit is a Rust CLI that runs coding-agent commands inside constrained containers with an auditable host-mount, network, and runtime-hardening policy.

This branch (`feat/full-plan-implementation`) implements the containerized agent runner summarized in [SCOPE.md](SCOPE.md).

## What is done

- Generic command wrapper plus aliases: `pi`, `opencode`, `codex`, `claude`, `amp`, `cursor-agent`, `agy`, `gemini`.
- Serializable run plan used by dry-run, explain, tests, and execution.
- Docker command generation, OrbStack-compatible Docker mode, and Podman command surface.
- Default-deny mount policy: no `$HOME`, root, Docker socket, broad parent paths, duplicate targets, symlink escapes, or mount grammar injection.
- Runtime hardening: read-only rootfs, dropped caps, `no-new-privileges`, non-root app user, tmpfs runtime dirs.
- Network modes: `none`, `open`, and `restricted` with domain allowlist and proxy-bypass checks.
- Default agent home policy for supported agents: generic commands mount no agent state; agent aliases mount supported coding-agent top-level state dirs/files read-write with no subgroup mounts; shared Git/SSH/GPG identity stays read-only.
- Pi adapter and audited SSH/GPG socket forwarding.
- Product commands: `doctor`, `cleanup`, and `image build`.
- Regression tests for mount, network, proxy redaction, signal cleanup, profiles, aliases, and explain output.

## Quick start

```sh
cargo build
./target/debug/orbit image build --dry-run
./target/debug/orbit --dry-run -- echo hi
./target/debug/orbit explain -- pi --version
./target/debug/orbit pi "say hi"
```

Build the base image before real container execution:

```sh
./target/debug/orbit image build --no-host-mise-tools
./target/debug/orbit -- echo hi
./target/debug/orbit -- pi --version
./target/debug/orbit pi
```

`orbit-agent:latest` installs pinned `mise`, all tools declared in host `~/.config/mise/config.toml` `[tools]` plus `gh` when absent (falling back to Node `24.16.0`, Rust `1.95.0`, and `gh` `2.93.0` when no host mise config exists or `--no-host-mise-tools` is set; host `node` entries must be `24.x`), `rg`, `fd`, `bubblewrap`/`bwrap`, `socat`, `sem`, `inspect-mcp`, and npm-backed agent CLIs (`opencode`, `codex`, `claude`, `amp`, `gemini`, plus `pi` from host `pi --version` by default). Use `--pi-version VERSION` to override or `--no-host-pi-version` to keep the Dockerfile fallback. Image builds do not read host Pi settings and do not bake host Pi packages. At runtime, agent aliases mount supported coding-agent top-level state dirs/files from the host read-write, including full `~/.pi` at `/home/orbit/.pi`; Orbit does not snapshot, sanitize, or subgroup those agent homes. Agent aliases with no args attach stdin/TTY automatically for TUI use. Orbit never adds extension-owned Pi flags automatically; pass Pi flags explicitly only when the container has that extension. `cursor-agent` and `agy` require vendor installers or a custom-derived image.

Transparent local alias example:

```sh
alias pi='orbit pi'
pi          # interactive TUI
pi "say hi" # headless prompt
```

Install the local binary when ready:

```sh
cargo install --path .
orbit doctor
```

## Safety defaults

- Engine: Docker by default.
- Network: `restricted` by default with `registry.npmjs.org`, GitHub (`github.com` including API/SSH hosts), Linear API (`api.linear.app`), plus OpenAI Codex endpoints (`chatgpt.com`, `auth.openai.com`, `api.openai.com`) allowed for default agent/GitHub dogfooding; use `--network none` for fully offline runs or `--allow-domain` to extend the allowlist. The restricted proxy uses trusted `orbit-agent:latest` unless `--proxy-image` explicitly selects another trusted Orbit-derived image.
- Workspace: current git worktree source root mounted read-write by default under `/home/orbit/<home-relative-suffix>` when it is a strict descendant of host `$HOME` (for example `/Users/sercans/source/me/orbit/main` -> `/home/orbit/source/me/orbit/main`), with non-HOME paths falling back to their absolute target. Git metadata (`.git`, linked-worktree git dirs, and common metadata dirs) follows the same mapping and is mounted writable so `git fetch`, upstream tracking, and PR workflows can update `FETCH_HEAD`, refs, and config.
- Whole `$HOME`: never mounted.
- Supported agent homes: generic commands mount no agent state; agent aliases mount supported coding-agent top-level state dirs/files read-write by default, including full `~/.pi`, `~/.codex`, `~/.claude`, `~/.config/gh`, and other listed agent homes. Orbit does not create managed subgroup mounts inside those homes; Pi extension entries that are absolute symlinks under `~/.pi/agent/extensions` get exact read-only target mounts so linked local extensions resolve. Credentials/config inside mounted homes are host-writable from the container. Agent aliases also mount Git identity/config (`~/.gitconfig`, `~/.config/git`, `~/.git-hooks`) plus full SSH/GPG homes (`~/.ssh`, `~/.gnupg`) read-only/redacted when present so host `IdentityFile`, `known_hosts`, and signing config resolve inside the container; valid SSH/GPG agent sockets are forwarded automatically for host-equivalent Git auth/signing, and Git SSH remotes route through the restricted proxy with `GIT_SSH_COMMAND`. When host `gh` stores a token in OS keyring, runtime stages an ephemeral writable/redacted `/home/orbit/.config/gh` with `gh auth token -h github.com`, then removes it in cleanup.
- Writable paths: container tmpfs (`/tmp`, `/var/tmp`, `/home/orbit`), current source workspace, and audited Git metadata mounts.
- App container hardening: read-only rootfs, `--cap-drop=ALL`, `no-new-privileges`, user `1000:1000`.
- Base image starts from `debian:bookworm-slim`, installs pinned `mise`, `rg`, `fd`, `bubblewrap`/`bwrap`, `socat`, installs all host global mise `[tools]` from `~/.config/mise/config.toml` plus `gh` when absent under `/usr/local/etc/mise/config.toml` so `/home/orbit` tmpfs does not hide active tool versions, requires any host `node` entry to be 24.x, falls back to Node 24.16.0, Rust 1.95.0, and gh 2.93.0 when no host mise config exists or `--no-host-mise-tools` is set, passes host `pi --version` as the Pi npm package version by default, installs `sem`/`inspect-mcp` with cargo, and installs npm-backed agent CLIs. It never reads host Pi settings or preinstalls host Pi packages.

## Documentation

- [SCOPE.md](SCOPE.md) — current branch/worktree scope and completed work.
- [ARCHITECTURE.md](ARCHITECTURE.md) — structure, data flow, and policy decisions.
- [INSTALLATION.md](INSTALLATION.md) — requirements and install/build commands.
- [USAGE.md](USAGE.md) — exact CLI commands, flags, and examples.
- [DEVELOPMENT.md](DEVELOPMENT.md) — dev workflow, tests, and validation gates.
