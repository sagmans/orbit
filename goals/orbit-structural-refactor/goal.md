# Orbit Structural Refactor Goal

Refactor Orbit into smaller, responsibility-focused modules and tests while preserving all current public behavior and capabilities. The work should improve project structure, testability, maintainability, and reuse across production code and test code without adding product features.

Shared understanding: see `facts.md`.

Execution plan: see `plan.md`.

Done when every accepted fact in `facts.md` is satisfied, the approved plan in `plan.md` has been executed or explicitly superseded by an approved revision, docs reflect the new structure, and the full verification gate passes: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --locked`, `git diff --check`, `./target/debug/orbit --dry-run -- echo hi`, `./target/debug/orbit --dry-run pi`, and `./target/debug/orbit explain -- pi --version`.
