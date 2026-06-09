# Facts

- Every production module is reviewed and either decomposed into smaller responsibility-focused modules/files or explicitly left intact because it is already small and cohesive.
- The refactor preserves strict public behavior compatibility: CLI arguments, aliases, dry-run output contracts, explain JSON shape, security refusals, image build behavior, runtime behavior, and documented capabilities do not intentionally change.
- Large mixed-responsibility hotspots are split into atomic domains: CLI parsing/planning/product commands/git metadata, agent aliases/Pi state/identity/socket mounts, runner execution/copy/auth/proxy/cleanup, and acceptance-test behavior domains.
- Tests are reorganized into a behavior matrix covering happy paths, edge cases, corner cases, security refusals, and integrated chain behaviors, without losing any meaningful existing scenario.
- Redundant or stale tests are removed only when their behavior is covered by clearer equivalent tests or when they no longer assert real behavior.
- Missing tests are added for extracted boundaries and behavior matrices, including unit coverage for reusable helpers and acceptance coverage for user-visible behavior.
- The refactor improves maintainability and reuse by making responsibilities discoverable through module names, minimizing duplicated logic, and keeping shared helpers reusable without introducing unnecessary framework or plugin abstractions.
- No new product features are added as part of this goal.
- Project architecture/development docs are updated when module ownership or test organization changes.
- Completion requires passing formatting, clippy with warnings denied, the full locked test suite, diff whitespace checks, and representative dry-run/explain smoke commands.
