use ed25519_dalek::SigningKey;
use rand::RngCore;

const ENFORCE_SECRETS_ENV: &str = "TPT_ENFORCE_SECRETS";

/// Load the capability signing key.
///
/// - `Some(path)` reads the 32-byte Ed25519 key file (fails hard on any error).
/// - `None` with `TPT_ENFORCE_SECRETS=1` refuses to start (fail closed): an
///   ephemeral key would silently reject every token issued by `tpt-soma-admin`
///   and is a misconfiguration in any non-dev deployment.
/// - `None` otherwise generates an ephemeral dev key with a loud warning.
pub fn load_signing_key(
    path: Option<&str>,
) -> Result<SigningKey, Box<dyn std::error::Error + Send + Sync>> {
    match path {
        Some(path) => {
            let data = std::fs::read(path)
                .map_err(|e| format!("failed to read capability root key '{path}': {e}"))?;
            let key: [u8; 32] = data
                .try_into()
                .map_err(|_| format!("invalid key length (expected 32 bytes) at '{path}'"))?;
            Ok(SigningKey::from_bytes(&key))
        }
        None => {
            if std::env::var(ENFORCE_SECRETS_ENV).as_deref() == Ok("1") {
                return Err(format!(
                    "{ENFORCE_SECRETS_ENV}=1 set but CAPABILITY_ROOT_KEY_PATH is not configured"
                )
                .into());
            }
            eprintln!(
                "WARNING: CAPABILITY_ROOT_KEY_PATH unset; using an ephemeral signing key. \
                 Tokens issued by `tpt-soma-admin` with a persistent key will be rejected."
            );
            let mut csprng = rand::thread_rng();
            let mut key_bytes = [0u8; 32];
            csprng.fill_bytes(&mut key_bytes);
            Ok(SigningKey::from_bytes(&key_bytes))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env<F: FnOnce()>(value: Option<&str>, f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        match value {
            Some(v) => unsafe { std::env::set_var(ENFORCE_SECRETS_ENV, v) },
            None => unsafe { std::env::remove_var(ENFORCE_SECRETS_ENV) },
        }
        f();
    }

    #[test]
    fn enforce_secrets_refuses_missing_key_path() {
        with_env(Some("1"), || {
            let result = load_signing_key(None);
            assert!(
                result.is_err(),
                "TPT_ENFORCE_SECRETS=1 with no key path must fail closed"
            );
            let msg = format!("{}", result.unwrap_err());
            assert!(
                msg.contains("CAPABILITY_ROOT_KEY_PATH"),
                "unexpected error: {msg}"
            );
        });
    }

    #[test]
    fn missing_key_file_errors() {
        with_env(None, || {
            let result = load_signing_key(Some("C:/definitely/not/a/key/file.bin"));
            assert!(
                result.is_err(),
                "missing key file must error, not fall back"
            );
        });
    }

    #[test]
    fn dev_fallback_ephemeral_key_without_enforcement() {
        with_env(None, || {
            let result = load_signing_key(None);
            assert!(
                result.is_ok(),
                "dev fallback should produce an ephemeral key"
            );
            let key = result.unwrap();
            assert_eq!(key.to_bytes().len(), 32);
        });
    }
}
