# Implementation summary

Orbit implementation in `feat/full-plan-implementation` completes the planned hardened container runner.

## Completed milestones

- M0 generic Docker wrapper.
- M1 policy and lifecycle hardening.
- M2 Pi adapter and container-local agent state materialization policy.
- M3 restricted network mode.
- M4 OrbStack-compatible mode and Podman command surface.
- M5 aliases, `doctor`, `cleanup`, image build flow, and interactive TTY mode.

## Main outputs

- Rust CLI crate in `src/`.
- Base image in `docker/orbit-agent.Dockerfile` with pinned mise-managed Node 24.16.0/Rust 1.95.0, host Pi CLI version by default, `rg`, `fd`, `bubblewrap`/`bwrap`, `socat`, `sem`, `inspect-mcp`, and npm-backed agent CLIs.
- Interactive agent wrapper behavior with `-i`/`--interactive` and no-arg alias auto-TTY.
- Pi sandbox handoff: Orbit does not inject extension-owned Pi flags; the base image supplies `bubblewrap`/`bwrap` and `socat` for Pi sandbox-capable packages while container hardening remains Orbit-owned.
- Behavior-focused acceptance matrix in `tests/acceptance_*.rs` with shared helpers in `tests/support/mod.rs`.
- User/developer docs at repo root.

## Validation

Use root docs for exact commands:

- `README.md` for overview.
- `INSTALLATION.md` for setup.
- `USAGE.md` for CLI usage.
- `ARCHITECTURE.md` for structure and decisions.
- `DEVELOPMENT.md` for validation gates.
- `SCOPE.md` for branch/worktree completion state.
