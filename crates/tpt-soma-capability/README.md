# tpt-soma-capability

Custom Ed25519-signed capability-token cryptosystem (capability-based access control).

Part of the `tpt-soma` multi-scale computational physiology platform.

`tpt-soma-capability` is the single authorization primitive for the platform. A researcher must present an unforgeable, signed token that names a data class, a cohort scope and an action before any query runs. Tokens can be narrowed by the holder without the root key, and revoked by nonce.

## Features

- **Signed tokens** (`CapabilityToken`): an Ed25519 signature over `subject`, `resource_class`, `cohort_scope`, `action`, `expiry`, `nonce` and an optional `graph_scope`. `sign` and `verify` / `verify_signature` check it.
- **Attenuation** (`attenuation`): `derive_child` produces a narrower `AttenuatedToken` by adding HMAC-chained caveats. The holder can shrink scope without contacting the signer. A child cannot widen its parent, and the HMAC chain makes that check fail closed (`AttenuationError`).
- **Revocation** (`revocation`): `RevocationList` keys revoked tokens by nonce. Short expiry is the second line of defence.
- **Data-class registry** (`registry`): `DataClassRegistry` holds `DataClass` entries. Each class has a `Sensitivity` (`Public`, `Internal`, `Confidential` or `Restricted`). `seed_phase0` through `seed_phase4` register the classes each platform phase introduced.
- **Signing backends** (`signing`): the `SigningBackend` trait keeps the root key behind an interface. `LocalSigningBackend` loads a key from disk for development. `KmsSigningBackend` is the trait for production KMS or HSM signing, and `KmsSigningBackendStub` is a placeholder whose `sign` returns an error.
- **CLI** (`tpt-soma-capability`): `gen-key`, `issue` and `revoke` for operator token management.

## Token lifecycle

1. **Generate a root key.** `tpt-soma-capability gen-key` writes an Ed25519 key. Development keys default to `./dev-keys`.
2. **Issue.** `issue` signs a token for a subject, resource class, cohort scope, action and lifetime (default 3600 seconds).
3. **Attenuate** (optional). The holder derives a child token with a narrower scope.
4. **Verify** on every request. The API checks the signature, expiry, scope and revocation list.
5. **Revoke** by nonce if a token must stop working before it expires.

The API loads the root key through `load_signing_key`, so production deployments supply the path via `CAPABILITY_ROOT_KEY_PATH`.

## Scope and limits

- The **action** field is a string, typically `read`, `export` or `admin`. The sandbox gate treats `admin` as allowing any action, so issue admin tokens sparingly.
- Only `LocalSigningBackend` is implemented. Production should not hold the root key on disk until the KMS backend is written. Track this under the KMS item in [ADR 007](../../docs/adr/007-deferred-items-status.md).
- Tokens carry no encryption. Do not put secrets in the subject or cohort fields.

See [ADR 002](../../docs/adr/002-custom-capability-tokens.md) for the design rationale.

## Crate layout

| Module | Responsibility |
|---|---|
| `token` | `CapabilityToken`, signing and verification |
| `attenuation` | `AttenuatedToken`, `derive_child`, `AttenuationError` |
| `revocation` | `RevocationList` |
| `registry` | `DataClassRegistry`, `DataClass`, `Sensitivity`, phase seeding |
| `signing` | `SigningBackend`, `LocalSigningBackend`, `KmsSigningBackend`, `KmsSigningBackendStub`, `SigningError` |

## Running the tests

```sh
cargo test -p tpt-soma-capability
```

`tests/token_tests.rs` covers forged signatures, expired tokens, attenuation that tries to widen scope, and revocation. This is security-critical code with an 85% coverage floor (see [coverage targets](../../docs/testing/coverage-targets.md)).

## Related crates

- [`tpt-soma-api`](../tpt-soma-api/README.md): enforces tokens on every route
- [`tpt-soma-audit`](../tpt-soma-audit/README.md): records each decision
- [`tpt-soma-sandbox`](../tpt-soma-sandbox/README.md): gates researcher compute with the same tokens

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of MIT or Apache-2.0 at your option.
