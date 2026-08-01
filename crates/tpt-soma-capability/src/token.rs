use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    pub subject: String,
    pub resource_class: String,
    pub cohort_scope: Vec<String>,
    pub action: String,
    pub expiry: u64,
    pub nonce: Vec<u8>,
    pub signature: Vec<u8>,
}

impl CapabilityToken {
    pub fn sign(signing_key: &SigningKey, mut token: Self) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs();
        token.expiry = now + 3600;
        let payload = Payload {
            subject: token.subject.clone(),
            resource_class: token.resource_class.clone(),
            cohort_scope: token.cohort_scope.clone(),
            action: token.action.clone(),
            expiry: token.expiry,
            nonce: token.nonce.clone(),
        };
        let payload_bytes = serde_json::to_vec(&payload).expect("serialize");
        let signature = signing_key.sign(&payload_bytes);
        token.signature = signature.to_bytes().to_vec();
        token
    }

    pub fn verify(&self, verifying_key: &VerifyingKey) -> bool {
        let Ok(payload) = serde_json::from_slice::<Payload>(&self.payload_bytes()) else {
            return false;
        };
        let Ok(sig) = Signature::from_slice(&self.signature) else {
            return false;
        };
        verifying_key.verify(&serde_json::to_vec(&payload).expect("serialize"), &sig).is_ok()
    }

    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs();
        now > self.expiry
    }

    fn payload_bytes(&self) -> Vec<u8> {
        let payload = Payload {
            subject: self.subject.clone(),
            resource_class: self.resource_class.clone(),
            cohort_scope: self.cohort_scope.clone(),
            action: self.action.clone(),
            expiry: self.expiry,
            nonce: self.nonce.clone(),
        };
        serde_json::to_vec(&payload).expect("serialize")
    }
}

#[derive(Serialize, Deserialize)]
struct Payload {
    subject: String,
    resource_class: String,
    cohort_scope: Vec<String>,
    action: String,
    expiry: u64,
    nonce: Vec<u8>,
}
