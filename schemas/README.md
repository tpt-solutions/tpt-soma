# tpt-soma Schema Registry

## Versioning Policy

- Schemas are additive-only within a major version.
- Removing or renaming a field requires a major version bump.
- Backward compatibility is enforced in CI (see `ci/compatibility.yml`).

## Contents

| Path | Description |
|------|-------------|
| `arrow/` | Apache Arrow schema definitions (Rust + Python) |
| `protobuf/` | Protobuf `.proto` files for service-to-service serialization |

## Naming Convention

- Files use `snake_case`.
- Record batch / message names use `PascalCase`.
- Field names use `snake_case`.
