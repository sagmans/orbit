# Orbit implementation scope

## Branch and worktree

| Item | Value |
|---|---|
| Branch | `feat/full-plan-implementation` |
| Worktree | `full-plan-implementation` |
| Source plan | External handoff reference: `$HOME/source/me/orbit/docs-final-plan/PLAN.md` |
| Status | Implementation complete; base image installs pinned mise plus host global mise `[tools]` from `~/.config/mise/config.toml` and `gh` when absent unless `--no-host-mise-tools` is set (fallback Node 24.16.0/Rust 1.95.0/gh 2.93.0), host Pi CLI version by default, rg/fd, bubblewrap/socat, sem/inspect-mcp, and npm-backed agent tools |

Default `main` worktree was not part of this implementation pass.

## Completed in this branch

- Rust CLI crate for running commands inside hardened containers.
- Generic command path: `orbit -- <cmd...>`.
- Agent aliases: `pi`, `opencode`, `codex`, `claude`, `amp`, `cursor-agent`, `agy`, `gemini`.
- Run-plan model shared by execution, dry-run, explain, and tests.
- Dry-run output for exact container commands.
- Human and JSON explain output.
- Docker runner plus OrbStack-compatible Docker labeling and Podman command rendering.
- Mount policy refusals for root/home/socket parents, Docker socket, broad parents, symlink escapes, duplicate targets, mount grammar injection, and unknown outside-workspace paths.
- Runtime hardening for app containers.
- Read-write source workspace by default with audited writable Git metadata mounts so container commands and agents can edit source while Git fetch, tracking, and PR workflows update `FETCH_HEAD`, refs, and config.
- Agent home policy for supported aliases: supported coding-agent top-level dirs/files mount read-write by default with no managed subgroup mounts; generic commands mount no agent state; shared Git/SSH/GPG identity stays read-only/redacted.
- SSH/GPG socket forwarding with socket validation, redaction, and audit warnings.
- Network modes: default `restricted`, plus explicit `none` and `open`.
- Restricted network default npm registry allowlist, allowlist extension, proxy setup, bypass refusals, and cleanup.
- Product commands: `doctor`, `cleanup`, `image build`.
- Base image with pinned `mise`, host global mise `[tools]` from `~/.config/mise/config.toml` plus `gh` when absent installed via mise unless `--no-host-mise-tools` is set, active config under `/usr/local/etc/mise/config.toml`, fallback `node@24.16.0`/`rust@1.95.0`/`gh@2.93.0`, host Pi CLI version by default, `rg`, `fd`, `bubblewrap`/`bwrap`, `socat`, `sem`, `inspect-mcp`, and npm-backed agent CLIs installed via mise/cargo.
- Interactive TTY support via `-i`/`--interactive` and automatic no-arg agent aliases.
- Pi runs without Orbit-injected extension flags; host `~/.pi` mounts directly read-write at `/home/orbit/.pi`; absolute symlink targets from `~/.pi/agent/extensions/*` mount read-only so linked local extensions resolve; Orbit does not snapshot, sanitize, copy, or link Pi state, and image builds only read host Pi CLI version, not host Pi settings or packages.
- Unit and acceptance tests for policy, command generation, profiles, signals, interactive mode, and product commands.

## Key commits

- `bc877c2` `feat(cli): implement containerized agent runner`
- `c6f26be` `fix(policy): harden mount and lifecycle gates`
- `2559930` `fix(security): close policy bypass gaps`
- `5755983` `test(policy): cover final security gates`
- `2c04359` `fix(network): parse URL authorities safely`
- `e258b91` `fix(network): harden proxy URL parsing`

## Latest known validation

Latest validation after blocker fixes:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
git diff --check
./target/debug/orbit --dry-run pi
./target/debug/orbit --dry-run --image custom/orbit:test -- echo hi
./target/debug/orbit --dry-run --image custom/orbit:test --proxy-image custom/proxy:test -- echo hi
./target/debug/orbit --network restricted --allow-domain internal --dry-run -- curl https://internal # refused: dotted DNS domain required
./target/debug/orbit --dry-run --proxy-image --privileged -- echo hi # refused: invalid image reference
./target/debug/orbit image build --no-host-mise-tools
docker run --rm --read-only --tmpfs /tmp:rw,noexec,nosuid,nodev --tmpfs /var/tmp:rw,noexec,nosuid,nodev --tmpfs /home/orbit:rw,exec,nosuid,nodev,uid=1000,gid=1000,mode=755 --user=1000:1000 orbit-agent:latest sh -c 'node --version && npm --version && rustc --version && cargo --version && gh --version && bwrap --version && socat -V >/dev/null && rg --version && fd --version && sem --version && command -v inspect-mcp && command -v inspect && pi --version && opencode --version && codex --version && claude --version && amp --version && gemini --version'
./target/debug/orbit --network open -- pi --version
./target/debug/orbit --allow-domain github.com -- curl -sS -o /dev/null -w '%{http_code}\n' https://api.github.com/rate_limit
./target/debug/orbit --allow-domain github.com -- curl -sS -o /dev/null -w '%{http_code}\n' https://github.com/
```

Observed coverage includes unit tests, acceptance tests, doctests, Docker image build, direct read-only image smoke, and Orbit runtime smoke.

## Known remaining work

No known code/test work remains in this scope. Shipping still needs push, PR, CI/review, merge, then worktree cleanup if desired.
