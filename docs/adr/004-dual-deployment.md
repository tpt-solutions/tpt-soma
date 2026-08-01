# ADR-004: Dual Docker Compose + Kubernetes/Helm Deployment

## Status

Accepted

## Context

The project needs to support two deployment contexts from day one:

1. **Local development** — a single developer running `docker compose up` on a laptop.
2. **Staging/production** — a Kubernetes cluster using Helm charts.

Many projects treat Helm as a "bolt-on later" concern, but Phase 4's federated compute feature literally requires running the same Helm chart at a partner site. If Helm parity is not maintained from Phase 0, the Phase 4 federated deployment becomes a separate re-implementation effort.

## Decision

Maintain two deployment manifests from Phase 0, kept in genuine parity:

- **`deploy/docker-compose.yml`**: Defines `keystone`, `minio`, `api`, and `frontend` services, plus named volumes for Keystone's `tpt-data/` and MinIO. Uses a pinned image for Keystone once one is published.
- **`deploy/docker-compose.override.yml`**: Local dev conveniences (hot reload, debug ports).
- **`deploy/helm/tpt-soma/`**: Chart skeleton with `Chart.yaml`, `values.yaml`, and templates mirroring the Compose services 1:1.

Secrets strategy:
- **Compose**: `.env.example` checked in; actual values in `.env` (gitignored).
- **Helm**: `values-secrets.yaml.example` checked in; actual values injected via K8s `Secret` or sealed-secrets/SOPS (noted as future hardening).

Dockerfiles use a shared image family consumed by both paths.

## Consequences

- **Positive**: Phase 4 federated compute is "run the same Helm chart at a partner site" with zero re-platforming.
- **Positive**: Developers can spin up the full stack with one command.
- **Trade-off**: Maintaining two manifests doubles the surface area for config drift. Mitigated by treating Helm values as the source of truth and generating Compose env from them (or vice versa) via CI validation.
- **Trade-off**: `tpt-keystone-db` image availability is a dependency for Compose in early phases. We will either build from the sibling repo's Dockerfile or wait for a published image.
