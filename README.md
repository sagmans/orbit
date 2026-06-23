# Orbit

Run coding-agent CLIs inside hardened Docker containers with host-mount isolation.

## What it does

Orbit wraps agents like `pi`, `opencode`, `codex`, `claude`, `amp`, `cursor-agent`, `agy`, and `gemini` in a Docker container that:

- Mounts the current git workspace **read-write** so the agent can edit code
- Mounts agent-specific `$HOME` directories (e.g. `~/.pi`, `~/.codex`, `~/.claude`) **read-write** so state persists
- Mounts git/ssh/gpg identity (`~/.gitconfig`, `~/.ssh`, `~/.gnupg`) **read-only** for auth
- Hardens the container: read-only rootfs, `cap-drop=ALL`, `no-new-privileges`, user `1000:1000`
- Refuses to mount Docker sockets, root filesystem, `/etc`, `/proc`, `/sys`, or whole `$HOME`

## Quick start

```sh
cargo build
./target/debug/orbit image build          # build the base image
./target/debug/orbit pi                   # interactive Pi TUI
./target/debug/orbit pi "say hi"          # headless Pi prompt
./target/debug/orbit opencode             # interactive OpenCode
./target/debug/orbit -- echo hello        # run any command
```

## Usage

```
orbit [flags] <agent> [args...]    Run a known agent
orbit [flags] -- <cmd...>          Run an arbitrary command
orbit explain [flags] ...          Show the container plan without running
orbit image build [flags]          Build the base container image
orbit doctor                       Check engine availability
orbit cleanup [--dry-run]          Remove stale orbit containers
```

**Agent aliases:** `pi`, `opencode`, `codex`, `claude`, `amp`, `cursor-agent`, `agy`, `gemini`

**Flags:**
- `--dry-run` - print the docker command without executing
- `-i, --interactive` - attach stdin and TTY (auto for no-arg agents)
- `--engine docker|orbstack|podman|auto` - container engine (default: auto-detect)
- `--network bridge|none` - network mode (default: bridge)
- `--image TAG` - custom container image
- `--workspace PATH` - git worktree root (default: auto-detected)
- `--mount SRC:TGT[:ro|rw]` - additional bind mount

## Safety model

| Concern | Policy |
|---|---|
| Docker socket | Never mounted (prevents container escape) |
| Root filesystem | Never mounted |
| `/etc`, `/proc`, `/sys`, `/dev` | Never mounted |
| Whole `$HOME` | Never mounted (only specific agent dirs) |
| Container rootfs | Read-only with tmpfs for `/tmp`, `/home/orbit` |
| Capabilities | All dropped (`cap-drop=ALL`) |
| Privileges | `no-new-privileges` set |
| Container user | `1000:1000` (non-root) |
| Mount grammar injection | Commas and control chars in paths refused |
| Symlink escapes | Canonical paths checked against blocklist |

Agent `$HOME` directories are mounted read-write so agents can read/write their state (settings, sessions, tokens). Git/SSH/GPG identity is mounted read-only.

## Project structure

```
src/
  main.rs          - entry point
  lib.rs           - module declarations
  error.rs         - error types
  plan.rs          - RunPlan and mount types
  mount.rs         - mount policy, agent home mapping, workspace validation
  docker.rs        - docker command generation
  runner.rs        - container execution and signal handling
  cli.rs           - CLI parsing, dispatch, explain, help
  image_build.rs   - `orbit image build` command
docker/
  Dockerfile       - base image with Node.js and agent CLIs
  entrypoint       - pass-through entrypoint
tests/
  integration.rs   - end-to-end CLI tests
```
