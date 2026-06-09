# Orbit Structural Refactor Plan

## Solution approach

Refactor Orbit by preserving the existing `RunPlan` contract and public CLI behavior while splitting large mixed-responsibility modules into domain-focused Rust modules and splitting the monolithic acceptance suite into behavior-focused files. Move code first without logic changes, keep façade modules stable, add characterization coverage around risky seams, then simplify duplicate helpers/tests only after equivalent coverage exists.

## Ordered steps

### 1. Establish baseline and characterization lock

Files/systems:
- `src/cli.rs`
- `src/agents.rs`
- `src/runner.rs`
- `tests/acceptance.rs`
- `src/docker.rs`, `src/network.rs`, `src/mount_policy.rs`, `src/config.rs`, `src/explain.rs`, `src/plan.rs`

Work:
- Run baseline verification before code movement.
- Capture current public contracts for representative dry-run, explain JSON, alias, network, mount, image build, Pi agent state, Git metadata, and cleanup behavior.
- Add focused characterization tests only where current behavior is undercovered or too implicit.

Verification:
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --locked`
- `git diff --check`
- `./target/debug/orbit --dry-run -- echo hi`
- `./target/debug/orbit --dry-run pi`
- `./target/debug/orbit explain -- pi --version`

### 2. Split CLI responsibilities behind a stable façade

Files/systems:
- `src/cli.rs` -> `src/cli/mod.rs`
- New candidates: `src/cli/action.rs`, `src/cli/parse.rs`, `src/cli/render.rs`, `src/cli/plan_builder.rs`, `src/cli/git_metadata.rs`, `src/cli/image_build.rs`, `src/cli/product_commands.rs`
- `src/lib.rs`
- Tests under `src/cli/**` and acceptance tests

Work:
- Keep `orbit::cli::main_entry` and `parse_and_plan_for_test` available.
- Move command/action enums and render dispatch into small modules.
- Move parse-only functions into `parse.rs`.
- Move run-plan construction into `plan_builder.rs` while preserving `RunPlan` output.
- Move Git metadata/workspace target logic into `git_metadata.rs`.
- Move mise/Pi package/image-build helpers into `image_build.rs`.
- Move `doctor`, `cleanup`, and help text into `product_commands.rs`.

Verification:
- Unit tests for parser outcomes, image-build argument construction, Git metadata mapping, profile/config merge paths, and product command availability.
- Acceptance tests for exact dry-run/explain behavior still pass.
- `cargo test --locked cli` plus full `cargo test --locked`.

### 3. Split agent policy responsibilities behind `agents` façade

Files/systems:
- `src/agents.rs` -> `src/agents/mod.rs`
- New candidates: `src/agents/aliases.rs`, `src/agents/targets.rs`, `src/agents/identity.rs`, `src/agents/sockets.rs`, `src/agents/home_state.rs`, `src/agents/pi_state.rs`, `src/agents/pi_sessions.rs`, `src/agents/package_exclusions.rs`
- `src/cli/plan_builder.rs`
- `src/runner/**`

Work:
- Preserve public functions/constants used by plan building and runner prep, especially `manifest_mounts`, target constants, and package exclusion helpers.
- Extract alias mapping first because it is low risk.
- Extract package exclusion parsing before image-build callers move further.
- Extract SSH/GPG socket validation and identity mounts while preserving audit/redaction strings.
- Extract non-Pi agent home mounts/auth overlays.
- Extract Pi materialization/session helpers last because they carry most security and session compatibility risk.

Verification:
- Current `agents` unit tests remain or move next to extracted modules unchanged first.
- Add façade tests proving exported functions produce identical mount/env/audit outputs for representative aliases.
- Acceptance tests for Pi alias, Pi session dir, Linear token handling, SSH/GPG forwarding, Git identity mounts, and auth read-only behavior pass.

### 4. Split runner execution/prep responsibilities behind `runner` façade

Files/systems:
- `src/runner.rs` -> `src/runner/mod.rs`
- New candidates: `src/runner/execute.rs`, `src/runner/prep/git_metadata.rs`, `src/runner/prep/pi_agent_snapshot.rs`, `src/runner/prep/gh_auth.rs`, `src/runner/prep/proxy_secret.rs`, `src/runner/proxy_runtime.rs`, `src/runner/process.rs`, `src/runner/cleanup.rs`, `src/runner/fake.rs`
- `src/plan.rs`
- `src/agents/**`

Work:
- Keep `runner::execute` behavior stable.
- Extract cleanup/process execution first and preserve primary-error vs cleanup-error semantics.
- Extract Git metadata rewrite prep and GH auth/proxy secret staging.
- Extract restricted proxy readiness/log helpers.
- Extract Pi agent snapshot/copy/symlink filtering last; keep symlink/auth/package-store safety behavior identical.

Verification:
- Unit tests cover cleanup success/failure masking, fake runner cleanup, Git metadata staging cleanup, GH token staging, proxy secret staging, and Pi snapshot symlink safety.
- Acceptance tests for restricted proxy startup failure, signal cleanup, Pi copy entrypoint, and runtime cleanup pass.

### 5. Split acceptance tests into behavior matrix

Files/systems:
- `tests/acceptance.rs` -> behavior files
- New candidates: `tests/support/mod.rs`, `tests/acceptance_cli.rs`, `tests/acceptance_mounts.rs`, `tests/acceptance_network.rs`, `tests/acceptance_agents_pi.rs`, `tests/acceptance_identity.rs`, `tests/acceptance_config.rs`, `tests/acceptance_git.rs`, `tests/acceptance_runner_cleanup.rs`, `tests/acceptance_image_build.rs`

Work:
- Move shared helpers to `tests/support/mod.rs` first.
- Move tests domain-by-domain with assertions unchanged.
- Split tests that currently assert multiple independent behaviors into clearer cases.
- Remove redundant/stale tests only after equivalent behavior is covered by focused unit or acceptance tests.
- Build a matrix covering happy paths, edge cases, corner cases, security refusals, and integrated chain behavior.

Verification:
- Count/list current acceptance test names before split and map each to a new file or documented replacement.
- `cargo test --locked --test acceptance_cli` etc. as files are introduced.
- Full `cargo test --locked` after each batch.

### 6. Add missing coverage and simplify extracted code

Files/systems:
- `src/config.rs`
- `src/explain.rs`
- `src/plan.rs`
- Extracted `src/cli/**`, `src/agents/**`, `src/runner/**`
- `tests/**`

Work:
- Add unit tests for config profile merge/validation, engine auto detection, excluded extension normalization, malformed config cases, and unknown profile behavior.
- Add `explain` unit tests for human sections/redaction independent from acceptance smoke.
- Lock critical `RunPlan` serde shape when it is part of user-visible explain JSON.
- Remove duplicated logic introduced or exposed during moves.
- Keep abstractions concrete; avoid plugin/framework/generalized trait systems unless multiple real consumers require them.

Verification:
- New unit tests fail before missing behavior is implemented/isolated where practical, then pass.
- No public CLI or JSON contract changes unless explicitly documented as non-feature internal cleanup.

### 7. Update docs and final verification

Files/systems:
- `ARCHITECTURE.md`
- `DEVELOPMENT.md`
- `README.md` or `SCOPE.md` only if current public docs become stale

Work:
- Update module ownership table and development rules for new module/test layout.
- Document behavior matrix and where to add new tests.
- Ensure docs do not claim new product capabilities.

Verification:
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --locked`
- `git diff --check`
- `./target/debug/orbit --dry-run -- echo hi`
- `./target/debug/orbit --dry-run pi`
- `./target/debug/orbit explain -- pi --version`
- Optional pre-ship image gate when Docker is available: `./target/debug/orbit image build --no-host-mise-tools --tag orbit-agent:structural-refactor-test` plus documented image smoke from `DEVELOPMENT.md`.

## Risks and controls

| Risk | Control |
|---|---|
| Behavior drift during file moves | Move code first with minimal edits, run full tests after each domain. |
| Dry-run/explain output changes | Add/keep exact output and JSON-shape tests before moving render/plan code. |
| Rust privacy churn creates over-public APIs | Prefer `pub(crate)`/`pub(super)` and façade modules; avoid exporting internals. |
| Security regression in mount/socket/Pi symlink policy | Keep security tests close to extracted modules and preserve audit/refusal strings. |
| Cleanup behavior masks primary errors | Extract runner cleanup with current unit tests unchanged, then add focused tests. |
| Test split loses scenarios | Create scenario mapping from current test names to new files before deletion. |
| Over-abstraction violates project style | Use domain modules and simple functions; no plugin framework or deep traits. |
| Stale duplicated acceptance helpers | Replace only when equivalent independent tests or public JSON/dry-run assertions cover behavior. |

## Open questions

None blocking. User selected all-module consistent structure, strict public compatibility, atomic responsibilities, broad test matrix, stale/redundant test removal only when safely covered, and no new product features.
