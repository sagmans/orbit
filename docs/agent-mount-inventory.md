# Agent mount inventory

## Purpose

Current Orbit mount contract after the direct-wrapper refactor. Orbit mounts top-level coding-agent state directly for agent aliases, avoids image-time host Pi package installs, and removes runtime Pi copy/sanitize/link behavior.

## Build failure root cause

`make build` runs `orbit image build --no-host-mise-tools`. The old image build still read host `~/.pi/agent/settings.json`, sent host Pi packages as `ORBIT_PI_PACKAGES_JSON`, and asked Docker to install them. One host package (`npm:@plannotator/pi-extension`) depended on unavailable `@pierre/theming@0.0.1`, causing npm `E404`.

Fix: image builds now read host mise `[tools]` plus host `pi --version` only. They do not read host Pi settings and do not bake host Pi packages.

## Current mount policy

### Always writable

| Host source | Container target | Mode | Why |
|---|---:|---:|---|
| Current workspace | `/home/orbit/<home-relative-workspace>` or absolute fallback | RW | Let agents edit repo files. |
| Git metadata roots/common dirs | Same container path mapping as workspace | RW | Let Git update refs, `FETCH_HEAD`, config, and worktree metadata. |

Some non-Git child worktrees under a writable Git common root are over-mounted RO to preserve source read-only defaults outside the active worktree.

### Shared identity and sockets

| Host source | Container target | Mode | Notes |
|---|---:|---:|---|
| `~/.gitconfig` | `/home/orbit/.gitconfig` | RO | Secret category; enables identity, signing, aliases, includes. |
| `~/.config/git` | `/home/orbit/.config/git` | RO | Secret category; enables global Git include dirs. |
| `~/.git-hooks` | `/home/orbit/.git-hooks` | RO | Secret category; enables `core.hooksPath`. |
| `~/.ssh` | `/home/orbit/.ssh` | RO | Secret category; Git SSH config, trust, identity files. |
| `~/.gnupg` | `/home/orbit/.gnupg` | RO | Secret category; Git signing config/keyrings. |
| `$SSH_AUTH_SOCK` | `/run/host-ssh-agent.sock` | RO socket | Auto-forwarded for agent aliases when valid. |
| `$GPG_AGENT_SOCK` or `~/.gnupg/S.gpg-agent` | `/run/host-gpg-agent.sock` | RO socket | Auto-forwarded for agent aliases when valid. |

### Coding-agent state dirs/files

Generated for every non-generic agent alias, not only the selected alias. Generic commands mount none of these.

| Host source | Container target | Mode | Notes |
|---|---:|---:|---|
| `~/.pi` | `/home/orbit/.pi` | RW | Full Pi state: settings, packages, extensions, auth, sessions, and related files. No snapshot, sanitizer, package-store linking, or session subgroup. |
| absolute symlink targets from `~/.pi/agent/extensions/*` | original absolute target path | RO | Lets linked local Pi extensions resolve inside the container without mounting extension subgroups under `/home/orbit/.pi`. |
| `~/.config/pi` | `/home/orbit/.config/pi` | RW | Pi-related top-level config. |
| `~/.opencode` | `/home/orbit/.opencode` | RW | Agent state/config. |
| `~/.config/opencode` | `/home/orbit/.config/opencode` | RW | Agent state/config. |
| `~/.config/gh` | `/home/orbit/.config/gh` | RW | GitHub CLI config; runtime may replace with ephemeral staged config when host keyring token exists. |
| `~/.codex` | `/home/orbit/.codex` | RW | Agent state/config. |
| `~/.claude` | `/home/orbit/.claude` | RW | Agent state/config. |
| `~/.claude.json` | `/home/orbit/.claude.json` | RW | Top-level Claude config file. |
| `~/.cursor` | `/home/orbit/.cursor` | RW | Agent state/config. |
| `~/.config/cursor` | `/home/orbit/.config/cursor` | RW | Agent state/config. |
| `~/.gemini` | `/home/orbit/.gemini` | RW | Agent state/config. |
| `~/.config/gemini` | `/home/orbit/.config/gemini` | RW | Agent state/config. |
| `~/.antigravity` | `/home/orbit/.antigravity` | RW | Vendor-reserved agent state/config. |
| `~/.agy` | `/home/orbit/.agy` | RW | Vendor-reserved agent state/config. |
| `~/.config/antigravity` | `/home/orbit/.config/antigravity` | RW | Vendor-reserved agent state/config. |
| `~/.amp` | `/home/orbit/.amp` | RW | Agent state/config. |
| `~/.config/amp` | `/home/orbit/.config/amp` | RW | Agent state/config. |

## Not mounted or not generated

| Old behavior | Current behavior |
|---|---|
| Image build reads `~/.pi/agent/settings.json` packages | Removed; runtime mounts host Pi state instead. |
| `ORBIT_PI_PACKAGES_JSON` build arg | Removed. |
| Dockerfile Pi package resolver / `/opt/orbit/pi-agent` package store | Removed. |
| Runner temp Pi snapshot at `/run/orbit-pi-agent-copy-source` | Removed. |
| `ORBIT_PI_SETTINGS_SOURCE`, `ORBIT_PI_MCP_SOURCE`, `ORBIT_EXCLUDED_EXTENSIONS`, `PI_CODING_AGENT_SESSION_DIR` runtime env | Removed. |
| Entrypoint settings/MCP sanitizer, pi-linear token copy, baked package links, extension `node_modules` links | Removed. |
| Managed auth-like subgroup mounts inside agent homes | Removed by direct top-level policy. |

## Security tradeoff

Direct RW top-level agent mounts make Orbit a thin wrapper and preserve host-equivalent agent behavior. Tradeoff: credentials/config inside those top-level dirs are writable by the container. Shared Git/SSH/GPG identity homes remain read-only/redacted, and whole `$HOME` remains refused.
