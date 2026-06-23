FROM debian:bookworm-slim

ARG TARGETARCH
ARG NODE_VERSION=24.3.0
ARG RUST_VERSION=1.85.0
ARG PI_VERSION=0.79.10
ARG OPENCODE_VERSION=1.15.13
ARG CODEX_VERSION=0.136.0
ARG CLAUDE_VERSION=2.1.160
ARG GEMINI_VERSION=0.44.1
ARG AMP_VERSION=0.0.1780391988-g4f09f3

ENV DEBIAN_FRONTEND=noninteractive

# Install system packages and create orbit user
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        build-essential \
        ca-certificates \
        curl \
        git \
        fd-find \
        jq \
        openssh-client \
        ripgrep \
        xz-utils \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 1000 -s /bin/bash orbit

# Install Node.js to /usr/local (system-wide, survives tmpfs on /home/orbit)
RUN set -eux; \
    arch="${TARGETARCH:-$(dpkg --print-architecture)}"; \
    case "$arch" in \
        amd64|x86_64) node_arch="x64" ;; \
        arm64|aarch64) node_arch="arm64" ;; \
        *) echo "unsupported arch: $arch"; exit 1 ;; \
    esac; \
    curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${node_arch}.tar.xz" \
        | tar -xJ --strip-components=1 -C /usr/local; \
    node --version; \
    npm --version

# Install Rust toolchain (system-wide, survives tmpfs on /home/orbit)
ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
RUN set -eux; \
    arch="${TARGETARCH:-$(dpkg --print-architecture)}"; \
    case "$arch" in \
        amd64|x86_64) rust_arch="x86_64-unknown-linux-gnu" ;; \
        arm64|aarch64) rust_arch="aarch64-unknown-linux-gnu" ;; \
        *) echo "unsupported arch: $arch"; exit 1 ;; \
    esac; \
    curl -fsSL "https://static.rust-lang.org/rustup/dist/${rust_arch}/rustup-init" \
        -o /tmp/rustup-init \
        && chmod +x /tmp/rustup-init \
        && /tmp/rustup-init -y --no-modify-path --profile minimal \
            --default-toolchain "${RUST_VERSION}" \
        && rm /tmp/rustup-init; \
    /usr/local/cargo/bin/rustc --version; \
    /usr/local/cargo/bin/cargo --version
ENV PATH="/usr/local/cargo/bin:${PATH}"

# Install mise (polyglot tool version manager) with pre-installed tools.
# MISE_DATA_DIR points to /usr/local/mise (system-wide, survives tmpfs on /home/orbit).
# At runtime, mise reads from here (read-only). Users can set MISE_DATA_DIR to a
# writable tmpfs path to install additional tools.
ENV MISE_DATA_DIR=/usr/local/mise
ENV MISE_CONFIG_DIR=/usr/local/mise
ENV MISE_CACHE_DIR=/usr/local/mise/cache
ENV MISE_INSTALL_PATH=/usr/local/bin/mise
ENV PATH="/usr/local/mise/shims:${PATH}"
RUN curl -fsSL https://mise.run | sh \
    && mise install uv python go \
    && mise use -g uv python go \
    && mise --version \
    && uv --version \
    && python --version \
    && go version

# Install GitHub CLI (gh) from official apt repo
RUN set -eux; \
    arch="${TARGETARCH:-$(dpkg --print-architecture)}"; \
    case "$arch" in \
        amd64|x86_64) gh_arch="amd64" ;; \
        arm64|aarch64) gh_arch="arm64" ;; \
        *) echo "unsupported arch: $arch"; exit 1 ;; \
    esac; \
    curl -fsSL "https://cli.github.com/packages/githubcli-archive-keyring.gpg" \
        -o /usr/share/keyrings/githubcli-archive-keyring.gpg; \
    chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg; \
    echo "deb [arch=${gh_arch} signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
        > /etc/apt/sources.list.d/github-cli.list; \
    apt-get update; \
    apt-get install -y gh; \
    rm -rf /var/lib/apt/lists/*; \
    gh --version

# Install agent CLIs globally (binaries land in /usr/local/bin)
RUN set -eux; \
    npm install -g \
        "@earendil-works/pi-coding-agent@${PI_VERSION}" \
        "opencode-ai@${OPENCODE_VERSION}" \
        "@openai/codex@${CODEX_VERSION}" \
        "@anthropic-ai/claude-code@${CLAUDE_VERSION}" \
        "@google/gemini-cli@${GEMINI_VERSION}" \
        "@ampcode/cli@${AMP_VERSION}"; \
    command -v pi; \
    command -v opencode; \
    command -v codex; \
    command -v claude; \
    command -v amp; \
    command -v gemini

COPY docker/entrypoint /usr/local/bin/orbit-entrypoint
RUN chmod +x /usr/local/bin/orbit-entrypoint

USER orbit
WORKDIR /workspace
