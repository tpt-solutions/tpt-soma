use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use rand::RngCore;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SigningError {
    #[error("signing failed: {0}")]
    SignFailed(String),
    #[error("key generation failed: {0}")]
    KeyGenerationFailed(String),
    #[error("verification failed")]
    VerificationFailed,
    #[error("KMS error: {0}")]
    KmsError(String),
}

pub trait SigningBackend: Send + Sync {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SigningError>;
    fn verifying_key(&self) -> VerifyingKey;
    fn generate_key(&mut self) -> Result<(), SigningError>;
}

pub struct LocalSigningBackend {
    signing_key: ed25519_dalek::SigningKey,
}

impl LocalSigningBackend {
    pub fn new(signing_key: ed25519_dalek::SigningKey) -> Self {
        Self { signing_key }
    }

    pub fn from_bytes(key_bytes: &[u8; 32]) -> Self {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(key_bytes);
        Self::new(signing_key)
    }

    pub fn generate() -> Self {
        let mut csprng = rand::thread_rng();
        let mut key_bytes = [0u8; 32];
        csprng.fill_bytes(&mut key_bytes);
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&key_bytes);
        Self::new(signing_key)
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
}

impl SigningBackend for LocalSigningBackend {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SigningError> {
        let signature = self.signing_key.sign(payload);
        Ok(signature.to_bytes().to_vec())
    }

    fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    fn generate_key(&mut self) -> Result<(), SigningError> {
        let mut csprng = rand::thread_rng();
        let mut key_bytes = [0u8; 32];
        csprng.fill_bytes(&mut key_bytes);
        self.signing_key = ed25519_dalek::SigningKey::from_bytes(&key_bytes);
        Ok(())
    }
}

/// KMS/HSM signing backend trait for future integration with external key management systems.
/// Implement this trait to use AWS KMS, HashiCorp Vault, Azure Key Vault, or hardware security modules.
pub trait KmsSigningBackend: SigningBackend {
    /// Returns the KMS key identifier (ARN, key ID, etc.)
    fn key_id(&self) -> String;

    /// Rotates the signing key in the KMS
    fn rotate_key(&mut self) -> Result<(), SigningError>;
}

/// Placeholder for KMS backend - to be implemented when integrating with a specific KMS provider
pub struct KmsSigningBackendStub {
    key_id: String,
    verifying_key: VerifyingKey,
}

impl KmsSigningBackendStub {
    pub fn new(key_id: String, verifying_key: VerifyingKey) -> Self {
        Self {
            key_id,
            verifying_key,
        }
    }
}

impl SigningBackend for KmsSigningBackendStub {
    fn sign(&self, _payload: &[u8]) -> Result<Vec<u8>, SigningError> {
        Err(SigningError::KmsError(
            "KMS backend not implemented - use LocalSigningBackend for development".to_string(),
        ))
    }

    fn verifying_key(&self) -> VerifyingKey {
        self.verifying_key
    }

    fn generate_key(&mut self) -> Result<(), SigningError> {
        Err(SigningError::KmsError(
            "Key generation not supported in stub".to_string(),
        ))
    }
}

impl KmsSigningBackend for KmsSigningBackendStub {
    fn key_id(&self) -> String {
        self.key_id.clone()
    }

    fn rotate_key(&mut self) -> Result<(), SigningError> {
        Err(SigningError::KmsError(
            "Key rotation not implemented in stub".to_string(),
        ))
    }
}

pub fn verify_signature(
    verifying_key: &VerifyingKey,
    payload: &[u8],
    signature_bytes: &[u8],
) -> Result<(), SigningError> {
    let sig =
        Signature::from_slice(signature_bytes).map_err(|_| SigningError::VerificationFailed)?;
    verifying_key
        .verify(payload, &sig)
        .map_err(|_| SigningError::VerificationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_verifies_for_signed_payload() {
        let backend = LocalSigningBackend::generate();
        let signature = backend.sign(b"payload").unwrap();
        verify_signature(&backend.verifying_key(), b"payload", &signature).unwrap();
    }

    #[test]
    fn signature_rejects_tampered_payload() {
        let backend = LocalSigningBackend::generate();
        let signature = backend.sign(b"payload").unwrap();
        let err = verify_signature(&backend.verifying_key(), b"tampered", &signature).unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed));
    }

    #[test]
    fn malformed_signature_bytes_fail_verification() {
        let backend = LocalSigningBackend::generate();
        let err = verify_signature(&backend.verifying_key(), b"payload", &[1, 2, 3]).unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed));
    }

    #[test]
    fn key_bytes_round_trip_and_sign_deterministically() {
        let key_bytes = [7u8; 32];
        let backend = LocalSigningBackend::from_bytes(&key_bytes);
        assert_eq!(backend.to_bytes(), key_bytes);
        let again = LocalSigningBackend::new(ed25519_dalek::SigningKey::from_bytes(&key_bytes));
        assert_eq!(backend.sign(b"m").unwrap(), again.sign(b"m").unwrap());
    }

    #[test]
    fn generate_key_replaces_the_signing_key() {
        let mut backend = LocalSigningBackend::from_bytes(&[7u8; 32]);
        let before = backend.verifying_key();
        backend.generate_key().unwrap();
        assert_ne!(backend.verifying_key(), before);
    }

    #[test]
    fn kms_stub_reports_its_key_and_refuses_operations() {
        let verifying_key = LocalSigningBackend::generate().verifying_key();
        let mut stub = KmsSigningBackendStub::new("arn:kms:key".to_string(), verifying_key);

        assert_eq!(stub.key_id(), "arn:kms:key");
        assert_eq!(stub.verifying_key(), verifying_key);
        assert!(matches!(stub.sign(b"m"), Err(SigningError::KmsError(_))));
        assert!(matches!(
            stub.generate_key(),
            Err(SigningError::KmsError(_))
        ));
        assert!(matches!(stub.rotate_key(), Err(SigningError::KmsError(_))));
    }
}
