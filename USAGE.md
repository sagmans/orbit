# Usage

Use `./target/debug/orbit` before install, or `orbit` after `cargo install --path .`.

## Core commands

```sh
orbit -- <cmd...>
orbit <alias> <args...>
orbit -i -- <cmd...>
orbit --dry-run -- <cmd...>
orbit explain [--json] -- <cmd...>
orbit doctor [--json]
orbit cleanup [--dry-run] [--json]
orbit image build [--dry-run] [--engine docker|orbstack|podman] [--tag TAG] [--pi-version VERSION] [--no-host-pi-version] [--no-host-mise-tools]
```

Aliases:

```text
pi, opencode, codex, claude, amp, cursor-agent, agy, gemini
```

Aliases select agent mount policy. The default image installs pinned `mise`, Node `24.16.0`, Rust `1.95.0`, `rg`, `fd`, `bubblewrap`/`bwrap`, `socat`, `sem`, `inspect-mcp`/`inspect`, then npm-backed tools: `pi` from host `pi --version` by default plus pinned `opencode`, `codex`, `claude`, `amp`, and `gemini`.

```sh
orbit -- pi --version
orbit opencode --version
orbit claude --version
```

Agent aliases with no args attach stdin/TTY automatically for TUI use. Use `-i`/`--interactive` to force TTY for generic commands. Orbit never adds extension-owned Pi flags automatically; pass Pi flags explicitly only when the container has that extension.

```sh
orbit pi
orbit -i -- bash
```

Alias pattern for transparent local use:

```sh
alias pi='orbit pi'
pi          # interactive TUI
pi "say hi" # headless prompt
```

`cursor-agent` and `agy` aliases are reserved for vendor-installed tools; use a custom image if you need them. Put derived-image mise installs under `/usr/local/share/mise`; `/home/orbit` is runtime tmpfs and would hide image-layer installs.

## Dry-run and explain

Show exact container command without spawning engine:

```sh
orbit --dry-run -- echo hi
orbit --dry-run pi --version
```

Review the plan:

```sh
orbit explain -- pi --version
orbit explain --json -- pi --version
```

## Engine selection

```sh
orbit --engine docker --dry-run -- echo hi
orbit --engine orbstack --dry-run -- echo hi
orbit --engine podman --dry-run -- echo hi
orbit --engine auto --dry-run -- echo hi
```

`orbstack` uses the Docker binary with an Orbit label. `auto` only checks Orbit/OrbStack env hints.

## Network modes

Default: restricted network. Orbit starts a restricted proxy and allowlists `registry.npmjs.org`, Linear API (`api.linear.app`), plus OpenAI Codex endpoints (`chatgpt.com`, `auth.openai.com`, `api.openai.com`) so image-time Pi package installs, Linear sync, and the default `pi` provider can work without mounting host npm state.

```sh
orbit --dry-run -- echo hi
```

Fully offline mode renders `--network=none` and blocks DNS/TCP egress:

```sh
orbit --network none --dry-run -- curl https://example.com
```

Open network uses Docker/Podman bridge networking with normal outbound internet:

```sh
orbit --network open --dry-run -- curl https://example.com
```

Restricted allowlist extension. Values must be dotted DNS domains; IP literals and single-label names are refused:

```sh
orbit --allow-domain example.com --dry-run -- curl https://example.com
```

Restricted with upstream proxy:

```sh
orbit explain --network restricted --allow-domain example.com --proxy http://user:pass@proxy.example:8080 -- curl https://example.com
```

If you tag a custom Orbit-derived image and need the restricted proxy from the same trusted image, set it explicitly. The proxy image runs as root with `NET_ADMIN`, so do not point it at untrusted app images:

```sh
orbit --image custom/orbit:test --proxy-image custom/orbit:test --dry-run -- echo hi
```

Use `explain --json` if machine-readable output is needed:

```sh
orbit explain --json --network restricted --allow-domain example.com -- curl https://example.com
```

## Workspace and mounts

Default workspace is current git worktree root, with source files mounted read-write under the container home when it is a strict descendant of host `$HOME`. Example: `/Users/sercans/source/me/orbit/main` mounts at `/home/orbit/source/me/orbit/main`, so `~/source/me/orbit/main` keeps the same home-relative shape inside the container. Non-HOME workspaces keep their absolute target. Git metadata (`.git`, linked-worktree git dirs, and common metadata dirs) follows the same mapping and is writable by default so `git fetch`, upstream tracking, and PR workflows can update `FETCH_HEAD`, refs, and config.

Choose workspace:

```sh
orbit --workspace /path/to/repo --dry-run -- echo hi
```

Source workspace writes are enabled by default for both generic commands and agent aliases:

```sh
orbit --dry-run -- sh -lc 'touch "$PWD/out.txt"'
orbit --dry-run pi "edit this repo"
```

Add explicit mount from inside workspace:

```sh
orbit --mount ./tool.conf:/tool.conf:ro --dry-run -- echo hi
```

Rules: source must exist, target must be absolute, duplicate targets are refused, outside-workspace mounts are refused unless covered by an audited agent policy.

## Agent home state and auth

For known agent aliases, Orbit mounts supported coding-agent top-level home/config dirs into `/home/orbit` read-write by default, with no managed subgroup mounts. This includes full Pi `~/.pi` mounted directly at `/home/orbit/.pi`; Orbit does not snapshot, sanitize, copy, or link Pi state at runtime. Pi extension entries that are absolute symlinks under `~/.pi/agent/extensions` get exact read-only target mounts so linked local extensions resolve inside the container. Generic commands mount no agent homes unless passed as explicit safe workspace mounts. GitHub CLI auth lives under `~/.config/gh`; when host `gh` stores its token in OS keyring, Orbit stages an ephemeral writable/redacted `/home/orbit/.config/gh` with `gh auth token -h github.com` and deletes it during cleanup. Image builds do not read host Pi settings and do not bake host Pi packages. Broad read-write agent homes are deliberate wrapper behavior: credentials/config inside those top-level dirs are mutable from the container.

```sh
orbit explain --json -- pi --version
orbit explain --json -- gemini --version
```

Whole `$HOME` is never mounted. Generic commands do not mount agent homes unless passed as explicit safe workspace mounts.

## SSH, GPG, and Git identity

Agent aliases mount Git identity/config files read-only by default: `~/.gitconfig`, `~/.config/git`, and `~/.git-hooks` when present. They also mount full SSH/GPG homes (`~/.ssh`, `~/.gnupg`) read-only so host `IdentityFile`, `known_hosts`, and signing config resolve inside the container. Valid SSH/GPG agent sockets are forwarded automatically so Git auth/signing works like the host. Explicit forwarding still fails closed if the socket is missing or not a Unix socket.

```sh
orbit --forward-ssh --dry-run -- ssh -T git@github.com
orbit --forward-gpg --dry-run -- gpg --list-keys
```

Restricted network mode also sets `GIT_SSH_COMMAND` to route Git SSH remotes through the local restricted proxy, so `git@github.com:owner/repo.git` can resolve via the allowlisted `github.com` proxy path instead of direct container DNS.

## Profiles

Create JSON config:

```json
{"excluded_extensions":["pi-sandbox"],"profiles":{"dev":{"engine":"podman","network":"open","image":"example/orbit:test","proxy_image":"example/orbit:test","excluded_extensions":["pi-telegram"]}}}
```

Use profile:

```sh
orbit --config orbit.json --profile dev --dry-run -- echo hi
```

CLI flags override profile defaults. `excluded_extensions` is accepted and validated for config compatibility, but direct-wrapper mode no longer uses it to filter runtime Pi settings, agent mounts, or image-build packages.

`orbit image build` reads host `~/.config/mise/config.toml` `[tools]` and host `pi --version` by default. Use `--no-host-mise-tools` for a stable minimal image that only installs fallback Node/Rust/GH plus required agent tooling; use `--pi-version VERSION` to pin Pi explicitly or `--no-host-pi-version` to keep the Dockerfile fallback. Image builds still do not read host Pi settings or bake host Pi packages.

## Product commands

```sh
orbit doctor
orbit doctor --json
orbit cleanup --dry-run
orbit cleanup --dry-run --json
orbit image build --dry-run
orbit image build --tag orbit-agent:dev
orbit image build --pi-version 0.79.1
orbit image build --no-host-pi-version
orbit image build --no-host-mise-tools
```
