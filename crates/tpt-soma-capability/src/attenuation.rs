use crate::token::CapabilityToken;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttenuatedToken {
    pub token: CapabilityToken,
    pub parent_nonce: Vec<u8>,
}

impl AttenuatedToken {
    pub fn derive_child(
        parent: &CapabilityToken,
        narrower_action: &str,
        narrower_cohort: Vec<String>,
        key: &[u8],
    ) -> Result<Self, AttenuationError> {
        if !parent.action.contains("admin") && narrower_action != parent.action {
            return Err(AttenuationError::ActionExceedsParent);
        }
        if narrower_cohort.len() > parent.cohort_scope.len() {
            return Err(AttenuationError::ScopeExceedsParent);
        }
        let mut mac = HmacSha256::new_from_slice(key).map_err(|_| AttenuationError::MacInit)?;
        mac.update(&parent.nonce);
        mac.update(narrower_action.as_bytes());
        for c in &narrower_cohort {
            mac.update(c.as_bytes());
        }
        let child_nonce = mac.finalize().into_bytes().to_vec();
        let child = CapabilityToken {
            subject: parent.subject.clone(),
            resource_class: parent.resource_class.clone(),
            cohort_scope: narrower_cohort,
            action: narrower_action.to_string(),
            expiry: parent.expiry,
            nonce: child_nonce,
            signature: Vec::new(),
        };
        Ok(Self {
            token: child,
            parent_nonce: parent.nonce.clone(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AttenuationError {
    #[error("HMAC init failed")]
    MacInit,
    #[error("attenuated action exceeds parent scope")]
    ActionExceedsParent,
    #[error("attenuated cohort scope exceeds parent")]
    ScopeExceedsParent,
}
