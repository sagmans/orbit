---
title: fix: Resolve PR review thread hardening gaps
type: fix
status: completed
date: 2026-06-05
---

# fix: Resolve PR review thread hardening gaps

## Summary

Orbit will resolve the six unresolved PR #1 review threads by hardening socket and mount-path validation, fixing package-exclusion parsing across Rust and the container entrypoint, and removing a PATH-dependent Makefile build step. The plan also folds in one fresh-eyes scoped-package gap discovered during review: baked scoped npm packages must honor full scoped exclusions, not only package basenames.

---

## Problem Frame

The current PR has unresolved security and correctness review comments around host identity forwarding, dangerous mount guards, package filtering, and build repeatability. These are small, bounded fixes, but several sit on sensitive container boundary code where tests must prove both raw and canonical paths, both Rust and shell/Node filtering paths, and both dry-run/static and runtime entrypoint behavior.

---

## Requirements

- R1. Host-equivalent SSH/GPG behavior must use live agent sockets safely: Orbit may mount `.ssh`/`.gnupg` for config and key material, but forwarded SSH/GPG sockets must still reject container-control sockets even when the user-provided socket path is a symlink with a safe-looking filename.
- R2. Raw and canonical mount-policy checks must refuse macOS dangerous parent paths under `/private/var` and `/private/etc` with the same protection level as `/var` and `/etc`.
- R3. GitHub package-source parsing must recognize SSH-style `git@github.com:owner/repo` sources because Pi package sources can be GitHub repos, and repo-name exclusions must work for HTTPS, git-protocol, and SSH-style forms.
- R4. Pi package and extension filtering must match full scoped npm package identities such as `@linear/sdk`, not only unscoped basenames, because scoped npm packages are distinct first-class package names.
- R5. Runtime entrypoint filtering must apply the same scoped-package exclusion semantics to sanitized settings and baked package-store linking so packages excluded from config do not reappear from the baked package store.
- R6. `make build` must invoke the Orbit image build through Cargo, not a PATH-dependent `orbit` binary lookup.
- R7. Each review-thread fix must have targeted tests proving the regression and the intended unchanged behavior.

---

## Scope Boundaries

- Do not redesign the mount-policy model beyond the named dangerous macOS paths.
- Do not remove host identity mounts in this PR; `.ssh` and `.gnupg` remain read-only mounts for host-equivalent config/key availability.
- Do not replace socket forwarding with copied socket files; Unix sockets cannot be copied usefully, and forwarding/mounting the live socket is required for host agent-backed auth/signing.
- Do not broaden socket validation into positive SSH/GPG protocol detection; this plan only fixes the symlink bypass for known control socket filenames.
- Do not change package source validation rules beyond SSH GitHub parsing and scoped npm exclusion identity matching.
- Do not change default excluded extension values or add new product-level exclusions.
- Do not rewrite the entrypoint shell script structure beyond the scoped filtering paths needed for this PR.
- Do not change image build semantics other than invoking the existing image-build command through Cargo in `Makefile`.
- Do not resolve or dismiss GitHub review threads before the branch is pushed with passing validation.

### Deferred to Follow-Up Work

- Consolidate duplicated Rust and entrypoint JavaScript package-source parsing into a shared generated artifact or stricter parity test if future package-filtering changes continue to drift.

---

## Context & Research

### Relevant Code and Patterns

- `src/agents.rs`: `maybe_forward_ssh_socket()`, `maybe_forward_gpg_socket()`, `validate_socket_path()`, `reject_control_socket_path()`, `github_package_components()`, `npm_package_name()`, and `source_matches_exclusion()` are the Rust surfaces for identity forwarding and package exclusion.
- `src/mount_policy.rs`: `reject_raw_dangerous_path()` runs before canonicalization; `reject_canonical_dangerous_path()` runs after `canonicalize_source()` and must account for macOS symlink targets.
- `docker/orbit-agent-entrypoint`: `npmPackageName()`, `matchesExclusion()`, `is_excluded_entry()`, and `link_baked_npm_packages()` are the runtime filtering surfaces for copied settings and baked package stores.
- `Makefile`: `build` currently performs Cargo build/install first, then calls the Orbit image build.
- `tests/acceptance.rs`: entrypoint integration tests already exercise sanitized settings, excluded package names, baked npm/git symlink behavior, agent homes, and image build dry-run output.
- `src/agents.rs` unit tests already cover Unix socket acceptance/refusal and package exclusion identity matching.
- `src/mount_policy.rs` unit tests already cover broad dangerous paths, runtime socket parent paths, Docker socket refusal, and mount grammar rejection.

### SSH/GPG Identity Model Check

- Orbit currently makes `.ssh` and `.gnupg` available in agent containers through read-only host mounts, not copies, when running agent aliases.
- Those directory mounts provide host-equivalent config, trust stores, public keys, and keyring files; they do not replace live agent sockets.
- SSH/GPG agent sockets are special Unix sockets. Copying them would not provide a working agent connection; mounting/forwarding the live socket is the viable approach.
- Therefore the socket approach is not removable if the product goal is “same auth/signing behavior in Orbit container as on host with coding agents like pi.” The fix is to keep socket forwarding and harden it against control-socket symlink bypasses.
- Follow-up option, not this PR: decide whether raw private key/keyring mounts should be narrowed in favor of socket-only auth/signing. That is a product/security trade-off broader than this review-thread fix.

### Institutional Learnings

- No `docs/solutions/` learnings were found for Orbit mount policy, identity forwarding, package exclusion, entrypoint filtering, or Makefile/Cargo workflow.

### External References

- None used. The required behavior comes directly from PR review threads and existing repo security patterns.

---

## Impact & Required Changes by Scope

| Scope | Impact | Required change | Primary validation |
|---|---|---|---|
| Socket symlink validation | Security boundary for host identity forwarding; unsafe symlink targets can masquerade as agent sockets | Canonicalize before control-socket name rejection and metadata inspection | Unix unit test: safe symlink -> `podman.sock` refused |
| Raw macOS dangerous paths | Explicit user mount sources can name `/private/var` or `/private/etc` directly | Add raw guards for `/private/var` and `/private/etc` exact/descendant paths | Mount-policy unit/acceptance tests for raw paths |
| Canonical macOS dangerous paths | `/var` and `/etc` may canonicalize to `/private/var` and `/private/etc` | Add canonical guards for `/private/var` and `/private/etc` exact/descendant paths | Mount-policy unit tests for canonical-equivalent paths |
| SSH GitHub URL parsing | Excluded GitHub packages from SSH URLs are not recognized | Parse `github.com:` and `github.com/` as alternate host separators | `src/agents.rs` exclusion unit tests |
| Scoped npm settings filtering | Full scoped exclusions such as `@linear/sdk` fail in entrypoint settings sanitization | Return full scoped npm identity from `npmPackageName()` | Entrypoint acceptance test with `@linear/sdk` |
| Scoped baked npm filtering | Fresh-eyes issue: scoped baked packages are filtered by basename only inside scope dirs | Check combined `@scope/package` identity while linking scoped baked packages | Entrypoint acceptance test verifies excluded scoped package is not linked |
| Makefile image build | CI/non-interactive shells can miss Cargo bin dir after install | Replace direct `orbit image build` with Cargo-driven image build | Static Makefile assertion or `make build` when environment supports image build |

---

## Key Technical Decisions

- Canonicalize forwarded socket paths before control-socket rejection: the dangerous filename belongs to the real target, not necessarily to the user-provided symlink path.
- Keep live SSH/GPG socket forwarding for host-equivalent agent behavior: mounted `.ssh`/`.gnupg` dirs provide files; sockets provide live agent operations such as passphrase-mediated SSH auth and GPG signing.
- Keep socket rejection based on known control socket filenames: this preserves the existing conservative model and avoids overfitting to SSH/GPG implementation details.
- Guard both raw and canonical dangerous paths: raw checks catch direct user intent before filesystem resolution, while canonical checks catch symlink aliases such as macOS `/var` -> `/private/var`.
- Treat scoped npm package identity as first-class: `@scope/name` and `name` are not equivalent for exclusion purposes; full scoped exclusions must match full scoped packages.
- Keep Rust and entrypoint parsing behavior intentionally parallel: both plan-time filtering and runtime filtering must agree on GitHub repo and npm package identities.
- Add a targeted baked scoped-package fix with the settings sanitizer fix: otherwise settings may filter `@scope/name` while the baked package store still links it back into runtime state.
- Keep Makefile build steps otherwise unchanged: the review only requires the image-build invocation to avoid PATH dependence after build/install.

---

## Open Questions

### Resolved During Planning

- Should the fresh-eyes scoped baked npm link issue be in scope? Resolved: yes. It is the same package-exclusion contract as the scoped npm review thread and would leave the runtime partially unfixed if omitted.
- Should external research be used? Resolved: no. Existing code and tests define the local contract; no framework or third-party API decision is needed.
- Should implementation rewrite package parsing broadly? Resolved: no. Use minimal targeted changes and tests; defer broader parser consolidation.

### Deferred to Implementation

- Exact test factoring in `tests/acceptance.rs`: choose the smallest test additions that preserve current readability after seeing the final diff.
- Whether `make build` is feasible in the current environment: run it only if container image build dependencies are available; otherwise validate with targeted Makefile/static checks and Cargo tests.

---

## Implementation Units

### U1. Harden forwarded socket path validation

**Goal:** Prevent symlink bypasses that point a safe-looking SSH/GPG socket path at a container-control socket.

**Requirements:** R1, R7

**Dependencies:** None

**Files:**
- Modify: `src/agents.rs`
- Test: `src/agents.rs`

**Approach:**
- Confirm existing agent-alias behavior keeps `.ssh` and `.gnupg` mounted read-only for file-based identity material.
- Keep forwarding live SSH/GPG agent sockets because copied socket files cannot preserve host agent behavior.
- Change `validate_socket_path()` so it canonicalizes the provided path first.
- Run `reject_control_socket_path()` against the canonical target path.
- Inspect metadata on the canonical target path.
- Preserve existing error categories for missing/non-socket/control-socket failures where practical.
- Preserve auto-forward behavior: optional sockets that fail validation are skipped; explicitly requested forwarding still errors.

**Execution note:** Add the symlink-regression unit test before changing validation behavior.

**Patterns to follow:**
- Existing `rejects_regular_files_as_forwarded_sockets`, `accepts_unix_socket_paths`, and `rejects_container_control_sockets_as_forwarded_agent_sockets` unit tests in `src/agents.rs`.

**Test scenarios:**
- Happy path: direct Unix socket named `agent.sock` still validates.
- Error path: regular file still fails as not a Unix socket.
- Error path: direct Unix socket named `podman.sock` still fails as a container control socket.
- Error path: symlink named `agent.sock` pointing to a Unix socket named `podman.sock` fails as a container control socket.
- Error path: dangling symlink or otherwise non-canonicalizable socket path produces a missing/canonicalization refusal for explicit forwarding.

**Verification:**
- Socket unit tests prove canonical target validation and unchanged direct-socket behavior.

---

### U2. Close macOS dangerous path aliases in mount policy

**Goal:** Refuse `/private/var` and `/private/etc` through both raw and canonical mount validation.

**Requirements:** R2, R7

**Dependencies:** None

**Files:**
- Modify: `src/mount_policy.rs`
- Test: `src/mount_policy.rs`
- Test: `tests/acceptance.rs`

**Approach:**
- Extend `reject_raw_dangerous_path()` with exact and descendant checks for `/private/var` and `/private/etc`.
- Extend `reject_canonical_dangerous_path()` with the same exact and descendant checks.
- Preserve the existing distinction that specific runtime agent sockets can be allowed as `MountCategory::Socket` while non-socket categories cannot mount runtime socket parents/descendants.
- Avoid making all `/private/*` descendants illegal beyond the already-refused `/private` parent and the named dangerous aliases unless tests reveal the current policy already intends broader refusal.

**Execution note:** Add tests for raw and canonical guard helpers before changing the condition.

**Patterns to follow:**
- Existing `refuses_root_home_and_socket_before_canonicalization`, `refuses_socket_parents_and_broad_config_paths`, and `socket_policy_allows_specific_runtime_sockets_only` tests in `src/mount_policy.rs`.
- Existing `home_mount_and_dangerous_parent_mounts_are_refused` acceptance test.

**Test scenarios:**
- Error path: raw `/private/var` mount source is refused.
- Error path: raw `/private/var/log` mount source is refused.
- Error path: raw `/private/etc` mount source is refused.
- Error path: raw `/private/etc/hosts` mount source is refused.
- Error path: canonical path resolving to `/private/var` or `/private/etc` is refused.
- Regression: direct runtime SSH/GPG socket paths allowed as `MountCategory::Socket` remain allowed when not container-control sockets.
- Regression: Docker socket and runtime socket parent refusal behavior remains unchanged.

**Verification:**
- Mount-policy unit and acceptance tests demonstrate equivalent protection for `/var`/`/etc` and macOS `/private/*` aliases.

---

### U3. Align package-exclusion identity parsing across Rust and entrypoint

**Goal:** Make GitHub SSH sources and full scoped npm package names match exclusions consistently in plan-time and runtime filtering.

**Requirements:** R3, R4, R5, R7

**Dependencies:** None

**Files:**
- Modify: `src/agents.rs`
- Modify: `docker/orbit-agent-entrypoint`
- Test: `src/agents.rs`
- Test: `tests/acceptance.rs`

**Approach:**
- Update `github_package_components()` so `github.com:` and `github.com/` are alternate accepted separators after protocol/user prefixes are stripped.
- Extend Rust package exclusion tests to include `git@github.com:owner/repo.git` and reject similar-but-not-equal repo names.
- Update entrypoint `npmPackageName()` so scoped npm specs return `@scope/name` after removing any version suffix from the package segment.
- Keep unscoped npm behavior unchanged: `package@version` still produces `package`.
- Update entrypoint scoped baked package linking so exclusions can match the combined `@scope/name` identity, not only the basename inside a scope directory.
- Preserve existing path segment and GitHub repo matching so extension path exclusions such as `./extensions/pi-sandbox` keep working.

**Execution note:** Use characterization tests from current settings and baked package behavior first, then tighten assertions for full scoped names.

**Patterns to follow:**
- `source_exclusion_matches_exact_package_identity` in `src/agents.rs`.
- `orbit_agent_entrypoint_sanitizes_settings_and_mcp_copies` in `tests/acceptance.rs`.
- `config_excluded_extensions_filter_pi_packages` in `tests/acceptance.rs`.

**Test scenarios:**
- Happy path: `git:github.com/badlogic/pi-telegram` still matches `pi-telegram` exclusion.
- Happy path: `git@github.com:badlogic/pi-telegram.git` matches `pi-telegram` exclusion.
- Happy path: `npm:@linear/sdk@1.2.3` matches `@linear/sdk` exclusion.
- Edge case: `npm:@linear/sdk-helper` does not match `@linear/sdk` exclusion.
- Edge case: malformed scoped npm package values remain invalid or non-matching according to current parser rules.
- Integration: entrypoint sanitized settings removes `npm:@linear/sdk@1.2.3` when `ORBIT_EXCLUDED_EXTENSIONS` includes `@linear/sdk`.
- Integration: baked `npm/node_modules/@linear/sdk` is not symlinked into runtime when `@linear/sdk` is excluded.
- Regression: existing `pi-sandbox`, `pi-telegram`, `pi-telegram2`, path-based extension filtering, and baked unscoped package linking behavior remain unchanged.

**Verification:**
- Rust unit tests and entrypoint acceptance tests prove package identity matching is exact, scoped-aware, and consistent enough for current exclusion flows.

---

### U4. Remove PATH-dependent Makefile image build

**Goal:** Make `make build` run the image build through Cargo instead of relying on the installed `orbit` being present on `PATH`.

**Requirements:** R6, R7

**Dependencies:** None

**Files:**
- Modify: `Makefile`
- Test: `tests/acceptance.rs`

**Approach:**
- Keep `cargo build --locked` and `cargo install --path . --locked` unchanged.
- Replace the direct `orbit image build --no-host-mise-tools` invocation with a Cargo-driven Orbit invocation.
- Prefer the same lockfile discipline as the surrounding build steps.
- Avoid adding shell wrappers, environment assumptions, or comments that duplicate the command.

**Patterns to follow:**
- Existing Makefile simplicity.
- Existing static file checks in `tests/acceptance.rs` such as Dockerfile/entrypoint assertions.

**Test scenarios:**
- Static regression: `Makefile` `build` target contains the Cargo-driven image-build invocation.
- Static regression: `Makefile` `build` target no longer contains a standalone `orbit image build --no-host-mise-tools` command.
- Integration, when environment supports it: `make build` reaches the image-build command without requiring Cargo bin dir on `PATH`.

**Verification:**
- Makefile assertion or integration run proves build target is no longer PATH-dependent.

---

### U5. Validate, review, and resolve PR thread state

**Goal:** Prove all fixes work together and prepare accurate replies/resolution for PR review threads.

**Requirements:** R7

**Dependencies:** U1, U2, U3, U4

**Files:**
- Modify: no source files expected beyond U1-U4
- Test: `src/agents.rs`
- Test: `src/mount_policy.rs`
- Test: `tests/acceptance.rs`

**Approach:**
- Run targeted unit/acceptance coverage for the changed surfaces.
- Run the full Rust test suite if practical in the environment.
- Use semantic/PR review tools where available to confirm no unintended high-risk blast radius.
- Prepare concise GitHub thread replies that say what changed and what validation ran.
- Resolve threads only after the fix commit is pushed to the PR branch and CI or local validation evidence is available.

**Patterns to follow:**
- Existing PR-review workflow: verify externally reported feedback before applying and replying.
- Existing test organization: unit tests for private helper behavior; acceptance tests for CLI/entrypoint/runtime command surfaces.

**Test scenarios:**
- Integration: all socket, mount-policy, package-exclusion, entrypoint, and Makefile regression tests pass together.
- Regression: existing acceptance tests for Pi settings sanitization, agent home mounts, restricted network, and image build dry-run still pass.
- Review: changed files are limited to expected surfaces unless implementation discovers a justified dependency.

**Verification:**
- Validation output covers changed security and parser behavior.
- PR thread replies can cite specific tests and changed files without overstating CI state.

---

## System-Wide Impact

- **Interaction graph:** `manifest_mounts()` and socket forwarding feed `plan_mount()`, Docker run args, and agent env. Socket validation changes affect both explicit `--forward-ssh/--forward-gpg` and automatic identity forwarding for agent aliases.
- **Mount boundary:** `reject_raw_dangerous_path()` and `reject_canonical_dangerous_path()` are central policy gates for workspaces, explicit mounts, agent home mounts, secret mounts, and socket mounts. The macOS additions must not accidentally block valid specific agent sockets.
- **Package filtering parity:** Package exclusions are applied in Rust while planning Pi agent copies/image builds and in entrypoint JavaScript while sanitizing runtime settings and linking baked package stores. Drift between these surfaces can leave excluded packages partially available.
- **Build workflow:** `Makefile` affects developer/CI build entry points, not runtime behavior. The change should preserve the existing build/install/image-build sequence.
- **Error propagation:** Explicit socket forwarding and explicit dangerous mounts should fail loudly; auto-forwarded invalid sockets should continue to skip forwarding rather than breaking generic agent runs.
- **Unchanged invariants:** Whole-host mounts, Docker/control sockets, secret-like paths, read-only defaults, and default package exclusions remain governed by existing policies except where this plan tightens named bypasses.

---

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Canonical socket paths change error text or redacted path expectations | Assert behavior-level errors instead of brittle full strings; preserve existing error codes where practical |
| `/private/var` check blocks valid forwarded runtime sockets on macOS | Keep socket-category runtime socket rules explicit and test specific socket allowance where applicable |
| Scoped npm fix breaks existing unscoped/path extension exclusions | Add regression tests for `pi-sandbox`, `pi-telegram`, `pi-telegram2`, and path-based extension filters |
| Rust and entrypoint parsing drift again | Add tests for the same representative specs in both Rust and entrypoint acceptance coverage |
| Makefile command becomes slower because it rebuilds before image build | Acceptable for build target correctness; existing build/install steps are unchanged and Cargo should reuse artifacts |
| Full `make build` cannot run in current environment | Use targeted tests plus static Makefile assertion locally; leave image-build integration to CI or an environment with container build support |

---

## Documentation / Operational Notes

- No user-facing docs are required unless implementation changes command output, error wording, or exclusion configuration semantics beyond bug fixes.
- PR thread replies should be factual and specific: changed behavior, relevant file, validation run.
- If local validation cannot run image build, state that limitation in PR notes instead of claiming full Makefile integration coverage.

---

## Sources & References

- Related PR: https://github.com/sagmans/orbit/pull/1
- Related issue: ASG-276
- Related code: `src/agents.rs`
- Related code: `src/mount_policy.rs`
- Related code: `docker/orbit-agent-entrypoint`
- Related code: `Makefile`
- Related tests: `tests/acceptance.rs`
