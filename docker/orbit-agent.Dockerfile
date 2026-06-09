FROM debian:bookworm-slim

ARG TARGETARCH
ARG MISE_VERSION=2026.5.18
ARG NODE_VERSION=24.16.0
ARG RUST_VERSION=1.95.0
ARG GH_VERSION=2.93.0
ARG PI_VERSION=0.79.1
ARG OPENCODE_VERSION=1.15.13
ARG CODEX_VERSION=0.136.0
ARG CLAUDE_VERSION=2.1.160
ARG GEMINI_VERSION=0.44.1
ARG AMP_VERSION=0.0.1780391988-g4f09f3
ARG SEM_REV=54df10ca18313471776bffcc1e040ceccdf7eced
ARG INSPECT_REV=90a8a5dd3a15c39e59062e2518171615fecff5c3
ARG ORBIT_MISE_TOOLS=""

ENV DEBIAN_FRONTEND=noninteractive \
    MISE_DATA_DIR="/usr/local/share/mise" \
    MISE_CONFIG_DIR="/usr/local/etc/mise" \
    MISE_GLOBAL_CONFIG_FILE="/usr/local/etc/mise/config.toml" \
    MISE_CACHE_DIR="/home/orbit/.cache/mise" \
    MISE_STATE_DIR="/home/orbit/.local/state/mise" \
    RUSTUP_HOME="/usr/local/share/mise/rustup" \
    CARGO_HOME="/usr/local/share/mise/cargo" \
    MISE_INSTALL_PATH="/usr/local/bin/mise" \
    MISE_NPM_PACKAGE_MANAGER="npm" \
    ORBIT_CARGO_TOOLS_DIR="/usr/local/share/mise/installs/cargo-tools" \
    PATH="/usr/local/share/mise/installs/cargo-tools/bin:/usr/local/share/mise/cargo/bin:/usr/local/share/mise/shims:/usr/local/bin:${PATH}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        build-essential \
        bubblewrap \
        ca-certificates \
        cmake \
        curl \
        git \
        fd-find \
        iptables \
        libffi-dev \
        libssl-dev \
        libyaml-dev \
        openssh-client \
        pkg-config \
        ripgrep \
        socat \
        squid \
        xz-utils \
        zlib1g-dev \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd \
    && if command -v iptables-legacy >/dev/null 2>&1; then update-alternatives --set iptables /usr/sbin/iptables-legacy; fi \
    && if command -v ip6tables-legacy >/dev/null 2>&1; then update-alternatives --set ip6tables /usr/sbin/ip6tables-legacy; fi \
    && rm -rf /var/lib/apt/lists/* \
    && { useradd -r -s /usr/sbin/nologin squid 2>/dev/null || true; } \
    && if id -u orbit >/dev/null 2>&1; then \
        :; \
    elif getent passwd 1000 >/dev/null; then \
        existing_user="$(getent passwd 1000 | cut -d: -f1)"; \
        usermod -l orbit -d /home/orbit -m "$existing_user"; \
    else \
        useradd -m -u 1000 -s /bin/bash orbit; \
    fi

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN set -eux; \
    case "${TARGETARCH:-$(dpkg --print-architecture)}" in \
        amd64|x86_64) \
            mise_arch="linux-x64"; \
            mise_sha="cfac593469d028d7ae5fe36e37bd7c59118b5238e92d8a876209578464f24a84"; \
            ;; \
        arm64|aarch64) \
            mise_arch="linux-arm64"; \
            mise_sha="3c19d4861684e4ed5b8d020bcbdd99f478df43583fb767bd8a663983d4a4e209"; \
            ;; \
        *) \
            echo "unsupported architecture: ${TARGETARCH:-$(dpkg --print-architecture)}"; \
            exit 1; \
            ;; \
    esac; \
    curl -fsSL "https://github.com/jdx/mise/releases/download/v${MISE_VERSION}/mise-v${MISE_VERSION}-${mise_arch}" -o "$MISE_INSTALL_PATH"; \
    echo "$mise_sha  $MISE_INSTALL_PATH" | sha256sum -c -; \
    chmod +x "$MISE_INSTALL_PATH"; \
    mkdir -p "$MISE_DATA_DIR" "$MISE_CONFIG_DIR" "$MISE_CACHE_DIR" "$MISE_STATE_DIR" "$RUSTUP_HOME" "$CARGO_HOME" "$ORBIT_CARGO_TOOLS_DIR"; \
    chown -R orbit:orbit /home/orbit "$MISE_DATA_DIR" "$MISE_CONFIG_DIR"

RUN set -eux; \
    { \
        printf '%s\n' '[settings]'; \
        printf '%s\n' 'npm.package_manager = "npm"'; \
    } > "$MISE_CONFIG_DIR/config.toml"; \
    chown -R orbit:orbit "$MISE_CONFIG_DIR"; \
    command -v mise; \
    mise --version

ENV TMPDIR="/home/orbit"

USER orbit
RUN set -eux; \
    command -v mise; \
    mise --version; \
    mise_tools="${ORBIT_MISE_TOOLS:-node@${NODE_VERSION} rust@${RUST_VERSION} gh@${GH_VERSION}}"; \
    mise use -g ${mise_tools}; \
    mise use -g \
        "npm:@earendil-works/pi-coding-agent@${PI_VERSION}" \
        "npm:opencode-ai@${OPENCODE_VERSION}" \
        "npm:@openai/codex@${CODEX_VERSION}" \
        "npm:@anthropic-ai/claude-code@${CLAUDE_VERSION}" \
        "npm:@google/gemini-cli@${GEMINI_VERSION}" \
        "npm:@ampcode/cli@${AMP_VERSION}"; \
    opencode_dir="$(find "$MISE_DATA_DIR/installs/npm-opencode-ai" -path '*/lib/node_modules/opencode-ai' -type d -print -quit)"; \
    test -n "$opencode_dir"; \
    (cd "$opencode_dir" && node postinstall.mjs); \
    claude_dir="$(find "$MISE_DATA_DIR/installs/npm-anthropic-ai-claude-code" -path '*/lib/node_modules/@anthropic-ai/claude-code' -type d -print -quit)"; \
    test -n "$claude_dir"; \
    (cd "$claude_dir" && node install.cjs); \
    amp_dir="$(find "$MISE_DATA_DIR/installs/npm-ampcode-cli" -path '*/lib/node_modules/@ampcode/cli' -type d -print -quit)"; \
    test -n "$amp_dir"; \
    (cd "$amp_dir" && node install.cjs); \
    cargo install --locked --root "$ORBIT_CARGO_TOOLS_DIR" --git https://github.com/Ataraxy-Labs/sem --rev "$SEM_REV" sem-cli; \
    cargo install --locked --root "$ORBIT_CARGO_TOOLS_DIR" --git https://github.com/Ataraxy-Labs/inspect --rev "$INSPECT_REV" inspect-mcp; \
    ln -sf inspect-mcp "$ORBIT_CARGO_TOOLS_DIR/bin/inspect"; \
    command -v node; \
    node --version | grep '^v24\.'; \
    command -v npm; \
    command -v rustc; \
    command -v cargo; \
    command -v gh; \
    command -v bwrap; \
    command -v socat; \
    command -v rg; \
    command -v fd; \
    command -v sem; \
    command -v inspect-mcp; \
    command -v inspect; \
    command -v pi; \
    command -v opencode; \
    command -v codex; \
    command -v claude; \
    command -v amp; \
    command -v gemini; \
    if command -v cursor-agent || command -v agy; then \
        echo 'cursor-agent and agy require vendor installers or custom images'; \
        exit 1; \
    fi

USER root
COPY docker/orbit-restricted-proxy /usr/local/bin/orbit-restricted-proxy
COPY docker/orbit-restricted-proxy-ready /usr/local/bin/orbit-restricted-proxy-ready
COPY docker/orbit-agent-entrypoint /usr/local/bin/orbit-agent-entrypoint
RUN chmod +x /usr/local/bin/orbit-restricted-proxy /usr/local/bin/orbit-restricted-proxy-ready /usr/local/bin/orbit-agent-entrypoint

USER orbit
WORKDIR /workspace
