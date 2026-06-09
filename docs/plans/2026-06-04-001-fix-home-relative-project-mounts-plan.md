---
title: fix: Mount home-relative projects under container home
type: fix
status: completed
date: 2026-06-04
---

# fix: Mount home-relative projects under container home

## Summary

Orbit will mount project workspaces that are strict descendants of the host user home into the container home at `/home/orbit/<same-home-relative-suffix>`, so `~/source/me/orbit/main` keeps the same home-relative shape inside the container. The plan updates workspace targets, staged git metadata compatibility, process cwd, Pi session compatibility, reserved path guards, tests, and docs together so git and agent workflows keep working after the target-path change.

---

## Problem Frame

Orbit currently mounts workspaces at host-equivalent absolute paths on Unix, which leaks host HOME shape like `/Users/sercans/...` into a Linux container whose actual `HOME` is `/home/orbit`. That breaks the desired invariant: user projects should remain home-relative in the container by changing only the HOME prefix.

---

## Requirements

- R1. For any workspace/project path that is a strict descendant of host `$HOME`, Orbit must bind it inside the container at `/home/orbit/<host-home-relative-suffix>`.
- R2. The container `cwd`/`--workdir` must match the mapped project target so launched processes see the same `~/source/...` shape under the container HOME.
- R3. Git operations must continue to work for normal repos and linked worktrees by keeping required `.git`, gitdir, common-dir, and sibling-worktree paths reachable through mapped targets and safe staged metadata rewrites where absolute host refs would otherwise break.
- R4. Pi session storage must remain scoped to the host worktree and must not accidentally orphan existing sessions or expose the full sessions tree.
- R5. Explicit user mount targets must remain explicit; only Orbit-managed project/git targets should be HOME-mapped.
- R6. Non-HOME paths, host `$HOME` itself, reserved agent/runtime prefixes, and unsupported platforms must retain safe fallback/refusal behavior instead of fabricating misleading or dangerous `/home/orbit` targets.
- R7. Dry-run, explain JSON, docs, and tests must describe the new contract and stop promising host-equivalent absolute paths for home-relative projects.

---

## Scope Boundaries

- Do not mount the whole host `$HOME`, a workspace exactly equal to host `$HOME`, or broad `/Users`/`/home` parents.
- Do not place generated project/git targets under reserved agent/auth/runtime prefixes such as `/home/orbit/.pi`, `/home/orbit/.codex`, `/home/orbit/.claude`, `/home/orbit/.cursor`, `/home/orbit/.ssh`, `/home/orbit/.gnupg`, or auth-like `/home/orbit/.config/*` paths.
- Do not move agent home/runtime directories away from their existing `/home/orbit/.pi`, `/home/orbit/.codex`, `/home/orbit/.claude`, and related targets.
- Do not change Pi package/image/mise behavior from the base-image work.
- Do not infer or rewrite user-provided explicit `--mount` targets; target paths remain caller-owned and policy-validated.
- Do not attempt a Windows path redesign; keep the current Windows `/workspace` fallback.

### Deferred to Follow-Up Work

- Capture the final path-mapping decision in `docs/solutions/` after implementation: separate compound-learning follow-up, not part of this implementation plan.

---

## Context & Research

### Relevant Code and Patterns

- `src/cli.rs`: `finish_plan()` builds workspace mount, `cwd`, env, hardening, and calls `agents::manifest_mounts()`.
- `src/cli.rs`: `git_metadata_mounts()` mounts git common/root metadata outside the workspace so linked worktrees can resolve `.git` files.
- `src/cli.rs`: `parse_gitdir_file()` and `git_common_dir()` read git metadata that may contain absolute host paths; linked worktrees may need staged metadata copies with rewritten refs rather than only remapped bind destinations.
- `src/cli.rs`: `container_workspace_target()` is the current Unix target helper and returns the host absolute path.
- `src/agents.rs`: `scoped_pi_session_mount()`, `scoped_orbit_session_dir()`, and `scoped_session_components()` scope Pi sessions by host workspace path and mount only the current scoped dir at `/home/orbit/.pi/agent/sessions/orbit-current`.
- `src/agents.rs`: `copy_matching_legacy_sessions()` imports legacy sessions matching canonical host workspace cwd or old `/workspace` cwd.
- `src/mount_policy.rs`: `plan_mount()` validates sources/targets, rejects broad mounts, redacts secrets, and detects duplicate targets after targets are generated.
- `src/docker.rs`: `command_args()` renders `--workdir` and bind mount `dst` values directly from `RunPlan`.
- `tests/acceptance.rs`: helpers `workspace_target()`, `workspace_mount_arg()`, and `git_metadata_mount_arg()` encode the old host-equivalent target assumption.
- `USAGE.md`, `ARCHITECTURE.md`, and `README.md`: current docs say workspace paths stay host-equivalent and must be updated.

### Institutional Learnings

- No `docs/solutions/` tree or critical-patterns doc exists in this repo.

### External References

- None needed. This is a local contract change with strong repo patterns and no new external API or framework behavior.

---

## Key Technical Decisions

- Centralize target mapping in one helper: prevents workspace and git metadata from drifting into different path schemes.
- Use validated source paths plus component-aware HOME-relative suffix calculation: preserve the user-visible `~/...` contract while avoiding unsafe string-prefix matches and keeping symlink/source validation in `validate_workspace_root()` and `plan_mount()`.
- Treat host `$HOME` itself as out of scope: only strict descendants can map to `/home/orbit/<suffix>`, preventing a broad host-HOME bind at the container HOME.
- Add reserved-prefix guards for generated targets: workspace/git targets must not equal, contain, or be contained by managed agent/auth/runtime paths under `/home/orbit`.
- Preserve host-path-based Pi session scoping: session storage remains keyed to stable host worktree identity, while runtime `PI_CODING_AGENT_SESSION_DIR` continues to point at the mounted scoped dir.
- Use safe staged git metadata rewrites for absolute host refs: linked worktree `.git`/`commondir` files that contain absolute host paths should be over-mounted or otherwise staged with container-mapped targets, instead of relying on unreachable `/Users/...` paths or reintroducing broad host-equivalent project mounts.
- Leave explicit mount targets alone: users who pass `--mount ./tool.conf:/tool.conf:ro` asked for `/tool.conf`, not a HOME rewrite.
- Keep non-HOME fallback unchanged: paths outside host HOME are rare but should remain honest absolute targets rather than pretending they are under `/home/orbit`.

---

## Open Questions

### Resolved During Planning

- Should this be only a workspace mount change, or include process/git/Pi session effects? Resolved: include full process, git, and Pi session impact analysis in the plan.
- Should git metadata/common roots be considered? Resolved: yes, and absolute git metadata references need an explicit compatibility strategy. The plan chooses safe staged/over-mounted metadata rewrites for absolute `.git`/`commondir` refs rather than bringing host-shaped `/Users/...` paths back into the container.

### Deferred to Implementation

- Exact helper name and placement: defer to implementation, but keep it near `container_workspace_target()` in `src/cli.rs` unless extraction becomes clearer.
- Exact black-box assertion for Pi history visibility: defer until implementation can run the current Pi runtime, but the implementation must not claim session compatibility from file placement alone; it must prove host-cwd legacy sessions remain visible or add a narrow migration/matching fix.

---

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification. The implementing agent should treat it as context, not code to reproduce.*

```mermaid
flowchart TD
    HostWorkspace[Host workspace path] --> Validate[validate_workspace_root]
    Validate --> TargetMap[HOME-aware target mapping]
    TargetMap --> Cwd[RunPlan.cwd]
    TargetMap --> WorkspaceMount[Workspace bind dst]
    HostWorkspace --> GitRoots[git metadata/common roots]
    GitRoots --> TargetMap
    GitRoots --> StagedGit[staged .git/commondir rewrites]
    TargetMap --> StagedGit
    StagedGit --> WorkspaceMount
    HostWorkspace --> PiSessionScope[Pi session host scoped dir]
    PiSessionScope --> SessionMount[/home/orbit/.pi/agent/sessions/orbit-current]
    Cwd --> Process[Container process cwd]
    WorkspaceMount --> Process
    SessionMount --> Agent[Pi agent runtime]
```

---

## Implementation Units

### U1. Add HOME-aware Orbit-managed target mapping

**Goal:** Replace host-equivalent Unix workspace targets with container-HOME targets when the source is a strict descendant of host `$HOME` and does not map under a reserved runtime prefix.

**Requirements:** R1, R2, R5, R6

**Dependencies:** None

**Files:**
- Modify: `src/cli.rs`
- Test: `src/cli.rs`

**Approach:**
- Extend or replace `container_workspace_target()` with a helper that maps validated strict HOME descendants to `/home/orbit/<relative-components>`.
- Preserve the existing Windows `/workspace` behavior.
- Refuse or fall back for host `$HOME` itself rather than producing `/home/orbit` as a project target.
- Preserve existing Unix absolute fallback for paths outside host HOME or when HOME cannot be read/canonicalized.
- Build target suffix from path components rather than textual replacement so `/Users/me2` cannot match `/Users/me` accidentally.
- Add a generated-target guard that rejects targets equal to or nested under managed runtime/auth prefixes, and rejects generated targets that would be ancestors of those managed prefixes.
- Update mount reasons from “host-equivalent path” to “container-home-relative path” where the new mapping applies.

**Execution note:** Start with unit tests around the helper before updating acceptance helpers.

**Patterns to follow:**
- `src/cli.rs` small private helpers near `container_workspace_target()`.
- `src/mount_policy.rs` canonical-path safety model.

**Test scenarios:**
- Happy path: host HOME `/Users/sercans`, workspace `/Users/sercans/source/me/orbit/main` maps to `/home/orbit/source/me/orbit/main`.
- Happy path: workspace equal to a nested HOME child with spaces or dashes maps by components without losing or merging segments.
- Edge case: workspace exactly equal to host HOME is refused or falls back according to the chosen implementation policy, but never maps to `/home/orbit`.
- Edge case: path outside host HOME keeps existing absolute Unix target.
- Edge case: HOME unset or not canonicalizable keeps existing absolute Unix target.
- Error path: HOME child under `.pi`, `.ssh`, `.gnupg`, `.codex`, `.claude`, or reserved `.config/*` auth/runtime prefix is refused before render.
- Edge case: symlinked HOME child behavior is explicit: either safe lexical HOME suffix with canonical source validation or documented canonical fallback; test locks whichever behavior is implemented.
- Edge case: Windows build path still returns `/workspace`.

**Verification:**
- Helper tests prove HOME-child, non-HOME fallback, and platform fallback behavior.

---

### U2. Apply mapped targets to workspace mount, cwd, and git metadata

**Goal:** Ensure process cwd, workspace bind dst, git metadata bind dst, and required git metadata file contents all resolve inside the mapped container path scheme.

**Requirements:** R1, R2, R3, R6

**Dependencies:** U1

**Files:**
- Modify: `src/cli.rs`
- Test: `src/cli.rs`
- Test: `tests/acceptance.rs`

**Approach:**
- Keep `finish_plan()` using the centralized helper for the primary workspace target and `RunPlan.cwd`.
- Keep `git_metadata_mounts()` using the same helper for git common/root metadata targets.
- Add a git compatibility mechanism for absolute `gitdir:` and absolute `commondir` refs: stage safe rewritten metadata files and over-mount them at the mapped workspace/gitdir locations, or an equivalent approach that makes Git resolve container-mapped paths without restoring project mounts at `/Users/...`.
- Keep any staged git metadata sources alive for the whole container execution and add cleanup for their temp directory/path through the existing cleanup/lifetime mechanism, so dry-run/explain and real execution cannot reference files deleted too early or leak temp state.
- Validate resolved gitdir/common-dir sources before mounting or staging: they must look like git admin directories/files and must not resolve into secret/auth/runtime-like host paths.
- Preserve source validation through `validate_workspace_root()` and `plan_mount()`; this unit changes generated target/mount/staging behavior, not explicit mount source policy.
- Confirm duplicate and nested target-overlap detection runs after all target mapping and catches unexpected collisions.

**Patterns to follow:**
- Existing `RunPlan` as source-of-truth pattern in `ARCHITECTURE.md`.
- Existing `git_common_root_mounts_repository_root_for_non_bare_common_dir` unit test for git metadata behavior.

**Test scenarios:**
- Happy path: `parse_and_plan_for_test()` for a HOME-child repo produces `cwd` under `/home/orbit/source/...`.
- Happy path: workspace mount has `target` matching `cwd` and source still points at canonical host workspace.
- Integration: linked worktree with absolute `gitdir:` under host HOME runs Git resolution successfully from the mapped workdir, proving staged/rewritten metadata works rather than only matching dry-run strings.
- Integration: linked worktree with absolute `commondir` under host HOME resolves common dir through mapped/staged metadata.
- Regression: real execution does not fail with missing staged metadata files, and cleanup removes any created metadata temp dir/path after execution or signal cleanup.
- Edge case: git metadata outside HOME retains absolute fallback target and still renders a valid mount when it is a valid git admin path.
- Error path: malicious `.git`/`commondir` pointing at `.ssh`, `.gnupg`, `.pi`, token-like paths, or other auth/runtime paths is refused before render.
- Error path: if HOME mapping causes duplicate or ancestor/descendant target overlap, generated target refusal remains the failure path.

**Verification:**
- Dry-run and explain JSON render mapped `--workdir`/mount targets for HOME-child workspaces.
- Linked-worktree validation proves an actual Git command can resolve metadata from the mapped container cwd.

---

### U3. Preserve Pi session storage semantics across cwd rewrite

**Goal:** Keep Pi sessions scoped and persistent after runtime cwd changes from host absolute path to `/home/orbit/<suffix>`.

**Requirements:** R4, R7

**Dependencies:** U1, U2

**Files:**
- Modify: `src/agents.rs` if tests show legacy import needs new cwd matching
- Test: `src/agents.rs`
- Test: `tests/acceptance.rs`

**Approach:**
- Keep `scoped_session_components(workspace)` based on the canonical host workspace path so scoped session dirs remain stable across the mount target change.
- Keep `PI_CODING_AGENT_SESSION_DIR` mounted at `/home/orbit/.pi/agent/sessions/orbit-current`; do not expose full host session tree.
- Add coverage proving session host source path remains host-workspace scoped while container `cwd` is mapped under `/home/orbit`.
- Add or run a black-box check for Pi history/session visibility when a legacy host-cwd session exists and the new runtime cwd is `/home/orbit/<suffix>`.
- If Pi writes or filters session headers with the new container cwd, extend legacy import/matching narrowly so existing host-path sessions and new container-cwd sessions can be recognized without broadening session exposure, including when the scoped dir is already nonempty.

**Patterns to follow:**
- `src/agents.rs` tests around `scoped_session_dirs_copy_matching_legacy_sessions_only` and symlink refusal.
- `tests/acceptance.rs` `pi_alias_uses_sanitized_settings_copy_to_disable_pi_sandbox` and session-dir assertions.

**Test scenarios:**
- Happy path: Pi alias plan for HOME-child workspace still mounts exactly one scoped session dir at `/home/orbit/.pi/agent/sessions/orbit-current`.
- Integration: explain JSON shows `cwd` mapped to `/home/orbit/source/...` while `PI_CODING_AGENT_SESSION_DIR` remains `/home/orbit/.pi/agent/sessions/orbit-current`.
- Regression: scoped host session dir name/components continue to derive from host workspace path, not from `/home/orbit/...`.
- Migration: existing legacy host-path session jsonl files are still copied into the scoped dir when the scoped dir is empty.
- Edge case: new container-cwd session jsonl files do not cause broad legacy imports or cross-workspace leakage.
- Regression: nonempty scoped session dirs still receive any required migration/alias handling, so preexisting sessions are not skipped solely because one jsonl already exists.

**Verification:**
- Pi session tests demonstrate no full sessions tree mount and stable host scoped storage after cwd rewrite.

---

### U4. Update acceptance/golden coverage for process and engine surfaces

**Goal:** Update CLI-level assertions so Docker/Podman args, dry-run text, explain JSON, and helper expectations match the new path contract.

**Requirements:** R2, R3, R7

**Dependencies:** U1, U2, U3

**Files:**
- Modify: `tests/acceptance.rs`
- Modify: `src/docker.rs` only if unit golden expectations live there
- Test: `tests/acceptance.rs`
- Test: `src/docker.rs`

**Approach:**
- Update acceptance helpers that compute `workspace_target()`, `workspace_mount_arg()`, and `git_metadata_mount_arg()`.
- Make those helpers accept the test's intended host HOME (or use explicit fixture HOME paths) instead of blindly reading the process `HOME`; many Pi tests set `HOME` to a tempdir for agent state while the repo workspace is outside that temp HOME, which would otherwise make assertions accidentally validate the old host-absolute fallback.
- Add at least one fixture repo/workspace physically inside a synthetic HOME and pass `--workspace` to prove the HOME-child mapping independent of the developer machine's real home path.
- Update dry-run assertions to expect `--workdir=/home/orbit/source/...` and matching `dst=/home/orbit/source/...` for HOME-child workspaces.
- Update explain JSON assertions for `cwd` and mount target values.
- Keep source-side assertions anchored to host paths so bind source security is not weakened.
- Include Podman/OrbStack golden surfaces if they assert workdir or mount target strings.

**Patterns to follow:**
- Existing acceptance helpers near the top of `tests/acceptance.rs`.
- Existing exact command-surface tests in `src/docker.rs`.

**Test scenarios:**
- Happy path: generic dry-run command under a repo nested inside synthetic HOME emits mapped workdir and mapped workspace mount dst.
- Happy path: Pi alias dry-run emits mapped workdir while keeping Pi runtime mounts under `/home/orbit/.pi/...`, with agent-state `HOME` and workspace fixture chosen so mapping is actually exercised.
- Integration: explain JSON serializes mapped `cwd`, mapped workspace mount target, and unchanged host source.
- Integration: linked-worktree acceptance uses mapped common-root target plus staged metadata compatibility and validates Git behavior.
- Regression: explicit `--mount ./tool.conf:/tool.conf:ro` keeps `/tool.conf` target.
- Error path: generated workspace target under reserved agent/auth prefix is refused and never appears in Docker args.

**Verification:**
- Acceptance suite verifies all user-visible plan/render surfaces match the new contract.

---

### U5. Update documentation and review language

**Goal:** Align docs and audit/reason strings with the new HOME-relative project mount model.

**Requirements:** R7

**Dependencies:** U1, U2, U3, U4

**Files:**
- Modify: `ARCHITECTURE.md`
- Modify: `USAGE.md`
- Modify: `README.md`
- Modify: `src/cli.rs`

**Approach:**
- Replace “same absolute path” / “host-equivalent path” language with “same home-relative path under `/home/orbit` for host HOME children.”
- Document fallback for non-HOME paths.
- Mention that git metadata/common roots follow the same mapping when under HOME and that absolute git metadata refs are staged/reconciled safely for linked worktrees.
- Keep safety docs clear that whole HOME is still denied, reserved agent/auth prefixes are refused for generated targets, and only the project/git roots are mounted.

**Patterns to follow:**
- Current concise docs style in `USAGE.md` and `ARCHITECTURE.md`.

**Test scenarios:**
- Test expectation: none -- documentation and reason string update only, covered indirectly by dry-run/explain assertions that read plan strings where applicable.

**Verification:**
- Docs no longer promise host-equivalent absolute paths for HOME-child projects.
- Security/audit language still states workspace read-only default and broad HOME mount denial.

---

## System-Wide Impact

- **Interaction graph:** `build_plan()` → `finish_plan()` → `RunPlan.cwd`/mounts → `docker::command_args()`/`explain::human()`/JSON output. `agents::manifest_mounts()` still receives host workspace path for state/session decisions.
- **Git behavior:** `.git` files in linked worktrees may contain absolute host paths. Mapping git roots alone is insufficient; staged/over-mounted metadata rewrites or an equivalent compatibility layer must make those refs point at mapped container targets.
- **Process behavior:** Container processes see `PWD`/workdir under `/home/orbit/source/...`, matching `HOME=/home/orbit`; shell `~` expansion and relative project navigation become consistent.
- **Pi sessions:** Runtime session mount target stays `/home/orbit/.pi/agent/sessions/orbit-current`; host storage should remain keyed by host workspace identity to avoid session orphaning. Compatibility with legacy host-cwd, new mapped-cwd, and old `/workspace` headers must be proven.
- **Agent homes:** Existing agent home targets under `/home/orbit/.pi`, `.codex`, `.claude`, etc. must not overlap with generated project/git targets; reserved-prefix checks should reject unsafe target shapes before render.
- **Explicit mounts:** Explicit target semantics remain unchanged; only Orbit-managed workspace/git targets are rewritten.
- **Unchanged invariants:** Whole HOME remains denied, workspace remains read-only unless `--write-workspace`, Docker socket remains refused, and Pi full sessions tree remains unmounted.

---

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Git linked worktree paths break because `.git` references still point to host-shaped common dirs | Stage/rewrite absolute `.git` and `commondir` refs to mapped targets, validate actual Git behavior, and refuse suspicious git admin sources |
| Pi session history appears lost after cwd rewrite | Keep host workspace path as scoped session key; preserve legacy host-path import; black-box test mapped-cwd visibility and add narrow migration if needed |
| HOME symlink/canonical mismatch produces surprising suffix | Choose and document lexical-vs-canonical HOME suffix behavior, keep canonical source validation, and test symlink-under-HOME behavior |
| `/home/orbit/source/...` or other mapped suffix collides with agent/runtime dirs | Add generated-target reserved-prefix and ancestor/descendant overlap checks for workspace/git targets |
| Docs imply whole HOME mount because target is under `/home/orbit` | Explicitly document source remains project/git roots only and broad HOME remains denied |
| Non-HOME workspace target looks inconsistent | Preserve fallback and document it as honest non-HOME behavior rather than fabricating a home-relative path |

---

## Documentation / Operational Notes

- Update `USAGE.md` Workspace and mounts section with the exact example: `/Users/sercans/source/me/orbit/main` -> `/home/orbit/source/me/orbit/main`.
- Update `ARCHITECTURE.md` core/runtime model to say RunPlan targets use container HOME mapping for strict host HOME descendants.
- Document linked-worktree git compatibility at behavior level, not internal staging details unless user-visible.

---

## Sources & References

- Related code: `src/cli.rs`
- Related code: `src/agents.rs`
- Related code: `src/mount_policy.rs`
- Related code: `src/docker.rs`
- Related tests: `tests/acceptance.rs`
- Related docs: `ARCHITECTURE.md`
- Related docs: `USAGE.md`
- Related issue: ASG-231
