# ADR-006: Versioning Convention

## Status

Accepted

## Context

The `tpt-soma` monorepo contains 8 crates plus a frontend package. We need a versioning strategy that:
- Allows independent evolution of domain crates
- Keeps the workspace coherent
- Supports both internal and external consumers
- Works with Cargo's published crate model

## Decision

We use **workspace-pinned versions with per-crate minor bumps**.

### Rules

1. The workspace root `Cargo.toml` defines `workspace.package.version = "0.1.0"`.
2. Each crate inherits this version unless it needs an independent bump.
3. When a crate makes a breaking change, only that crate's version bumps its minor (e.g., `tpt-soma-genomica` goes from `0.1.0` to `0.2.0`).
4. The workspace version tracks the overall project milestone, not individual crate changes.
5. Breaking changes to the public API of `tpt-soma-core` or `tpt-soma-capability` bump the workspace version.

### Rationale

- **Per-crate minor bumps** allow domain modules (`genomica`, `cytos`, `organon`, etc.) to evolve independently without forcing a full workspace version bump.
- **Workspace version as milestone tracker** keeps the overall project coherent — when `tpt-soma-core` breaks, everyone knows.
- This avoids the complexity of full semver per crate (which Cargo doesn't enforce anyway) while still giving consumers signal about breaking changes.

## Consequences

- CI must ensure all workspace members compile against the current workspace version.
- Release notes should call out which crates had breaking changes.
- Consumers depending on `tpt-soma-core` must accept the workspace version bump when core breaks.
