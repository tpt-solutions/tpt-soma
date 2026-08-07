# Capability-Based Access Control (CBAC)

## Summary

`tpt-soma` uses a custom capability-token cryptosystem instead of user/password
sessions. Every request to the API must present a signed capability token that
declares exactly what the bearer may do. The token is the authorization
credential; there is no implicit "logged-in user can see everything" model.

## Token anatomy

A token (`tpt-soma-capability::CapabilityToken`) carries six claims, all
cryptographically bound by an Ed25519 signature:

| Field | Meaning |
|---|---|
| `subject` | The researcher/actor identity the token was issued to. |
| `resource_class` | The data class the token may touch (see Data Classification doc). |
| `cohort_scope` | The set of cohort IDs (or `*` for all) the token covers. |
| `action` | The permitted action, e.g. `read`, `write`, `export`. |
| `expiry` | Unix-seconds expiry. Tokens are short-lived. |
| `nonce` | Unique random value enabling targeted revocation. |

The signature is over a canonical JSON serialization of the claims using the
platform root signing key (Ed25519). Signature verification is performed on
every request.

## Issuance

Tokens are issued by an administrative path only:

- `tpt-soma-capability` CLI: `gen-key`, `issue`, `list-classes`, `revoke`.
- `tpt-soma-api` admin binary (auto-discovered `admin`).

There is no self-service issuance endpoint. Issuance is a manual, logged
operation performed by the platform administrator for a named researcher and a
narrow scope.

## Root signing key lifecycle

- **Dev path:** `issue_token gen-key` writes `signing_key.bin` /
  `verifying_key.bin` to `./dev-keys` (local keyfile, never committed).
- **Production path:** a `KmsSigningBackend` trait abstracts signing behind a
  KMS/HSM; a stub implementation exists and the trait is the swap point. The
  production deployment mounts the key via a secret volume
  (`CAPABILITY_ROOT_KEY_PATH`), never via the image or repo.

## Verification pipeline (per request)

1. Parse the `Authorization: Bearer <token>` header.
2. Verify the Ed25519 signature against the configured verifying key.
3. Reject if expired.
4. Reject if the nonce is on the revocation list.
5. Enforce the declared resource class / action / cohort scope against the
   route and requested resource.
6. Record an audit event (see Audit Ledger doc).

## Attenuation

A token can be attenuated (narrowed) by a downstream component: an attenuated
token may never grant a scope, resource class, or action broader than its
parent. This lets a pipeline stage hand a narrower capability to a sub-process
without re-issuance and without privilege growth.

## Revocation

Revocation is keyed by nonce via `RevocationList`. A revoked token is rejected
at step 4 above. Revocation is an in-memory list in the current implementation
(see Threat Model finding TM-04); persistence to Keystone is a planned
hardening item.

## Relevant code

- `crates/tpt-soma-capability/src/token.rs` — struct, `sign`, `verify`, `is_expired`
- `crates/tpt-soma-capability/src/signing.rs` — `SigningBackend`, `LocalSigningBackend`, `KmsSigningBackend`
- `crates/tpt-soma-capability/src/attenuation.rs` — attenuation rules
- `crates/tpt-soma-capability/src/revocation.rs` — `RevocationList`
- `crates/tpt-soma-capability/src/registry.rs` — data class taxonomy
- `crates/tpt-soma-api/src/auth.rs` — `capability_middleware` enforcement
