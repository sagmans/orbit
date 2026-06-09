# Installation

## Requirements

- macOS or Linux.
- Rust `1.95` or newer.
- Cargo.
- Docker for default execution, or Podman/OrbStack for alternate engine modes.

Check tools:

```sh
rustc --version
cargo --version
docker --version
podman --version
```

`podman` is optional unless using `--engine podman`.

## Build locally

Run from repo root:

```sh
cargo build
./target/debug/orbit doctor
```

## Install local binary

```sh
cargo install --path .
orbit doctor
```

## Build base container image

The default image starts from `debian:bookworm-slim`, installs `ripgrep`/`rg`, `fd-find`/`fd`, `bubblewrap`/`bwrap`, `socat`, pinned `mise`, reads host `~/.config/mise/config.toml` `[tools]`, installs every declared tool plus `gh` when absent through `mise` under `/usr/local/etc/mise/config.toml`, requires any host `node` entry to be `24.x`, falls back to Node `24.16.0`, Rust `1.95.0`, and gh `2.93.0` when no host mise config exists or `--no-host-mise-tools` is used, reads host `pi --version` unless disabled or overridden, installs cargo-backed tools, and installs npm-backed agent CLIs through `mise`:

- `gh` from `gh@2.93.0` when host global mise config does not provide a `gh` version
- `sem` from `cargo install --git https://github.com/Ataraxy-Labs/sem sem-cli`
- `inspect-mcp` from `cargo install --git https://github.com/Ataraxy-Labs/inspect inspect-mcp` (also symlinked as `inspect`)
- `pi` from host `pi --version` by default (`0.79.1` Dockerfile fallback; override with `--pi-version VERSION` or keep fallback with `--no-host-pi-version`)
- `opencode` from `npm:opencode-ai@1.15.13`
- `codex` from `npm:@openai/codex@0.136.0`
- `claude` from `npm:@anthropic-ai/claude-code@2.1.160`
- `gemini` from `npm:@google/gemini-cli@0.44.1`
- `amp` from `npm:@ampcode/cli@0.0.1780391988-g4f09f3`

The image keeps mise/cargo tool installs under `/usr/local/share/mise` and the active global mise config under `/usr/local/etc/mise/config.toml` so runtime `/home/orbit` tmpfs mounts do not hide image-layer tools or active tool versions. `orbit image build` reads host mise tools and host Pi CLI version only; it does not read host Pi settings, does not pass Pi package build args, and does not install host Pi packages into the image. At runtime, agent aliases mount supported top-level coding-agent state dirs/files directly from the host read-write, including full `~/.pi` at `/home/orbit/.pi`; `orbit-agent-entrypoint` only execs the requested command.

Cursor Agent and Antigravity do not have npm/mise registry packages; install them separately with their vendor installers in a derived image if needed.

Dry-run first:

```sh
orbit image build --dry-run
```

Build default image:

```sh
orbit image build
```

Build stable minimal image without host mise `[tools]` (useful for local smoke builds when optional host tool downloads are flaky):

```sh
orbit image build --no-host-mise-tools
```

Build with custom tag:

```sh
orbit image build --tag orbit-agent:dev
```

The image build command expects repo root because `docker/orbit-agent.Dockerfile` is referenced by relative path.

## Optional shell aliases

For transparent agent use, no extra network flag is required. No-arg aliases start interactive TTY; prompts with args stay headless:

```sh
alias pi='orbit pi'
pi
pi "say hi"
```

Orbit defaults to restricted networking and allowlists `registry.npmjs.org`, Linear API (`api.linear.app`), plus OpenAI Codex endpoints (`chatgpt.com`, `auth.openai.com`, `api.openai.com`) for image-time Pi package installs, Linear sync, and default Pi dogfooding. Use `--allow-domain` to extend the restricted allowlist, `--network open` for normal outbound bridge networking, or `--network none` for fully offline runs. Orbit does not add extension-owned Pi flags automatically; pass Pi flags explicitly only when the container has that extension.

## Use a custom app image

Use a custom image only when you need pinned versions, extra tools, or private agent builds:

```sh
orbit --image your-orbit-agent:tag --dry-run -- echo hi
```

Note: restricted-network proxy sidecar uses trusted `orbit-agent:latest` by default. If you intentionally need a different trusted Orbit-derived proxy image, pass `--proxy-image your-orbit-agent:tag`; do not use untrusted app images for the proxy because it runs as root with `NET_ADMIN`.
