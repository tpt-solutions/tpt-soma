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
            graph_scope: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parent(action: &str, cohort: &[&str]) -> CapabilityToken {
        CapabilityToken {
            subject: "researcher".to_string(),
            resource_class: "genomic".to_string(),
            cohort_scope: cohort.iter().map(|s| s.to_string()).collect(),
            graph_scope: None,
            action: action.to_string(),
            expiry: 4_102_444_800,
            nonce: b"parent-nonce".to_vec(),
            signature: Vec::new(),
        }
    }

    #[test]
    fn child_narrows_cohort_and_keeps_identity() {
        let parent = parent("read", &["a", "b", "c"]);
        let child =
            AttenuatedToken::derive_child(&parent, "read", vec!["a".to_string()], b"key").unwrap();

        assert_eq!(child.token.subject, parent.subject);
        assert_eq!(child.token.resource_class, parent.resource_class);
        assert_eq!(child.token.expiry, parent.expiry);
        assert_eq!(child.token.cohort_scope, vec!["a".to_string()]);
        assert_eq!(child.parent_nonce, parent.nonce);
        assert!(child.token.signature.is_empty());
    }

    #[test]
    fn child_nonce_is_deterministic_and_depends_on_key_and_scope() {
        let parent = parent("read", &["a", "b"]);
        let scope = vec!["a".to_string()];
        let first = AttenuatedToken::derive_child(&parent, "read", scope.clone(), b"key").unwrap();
        let again = AttenuatedToken::derive_child(&parent, "read", scope.clone(), b"key").unwrap();
        let other_key = AttenuatedToken::derive_child(&parent, "read", scope, b"other").unwrap();
        let other_scope =
            AttenuatedToken::derive_child(&parent, "read", vec!["b".to_string()], b"key").unwrap();

        assert_eq!(first.token.nonce, again.token.nonce);
        assert_ne!(first.token.nonce, other_key.token.nonce);
        assert_ne!(first.token.nonce, other_scope.token.nonce);
    }

    #[test]
    fn non_admin_child_cannot_change_action() {
        let parent = parent("read", &["a"]);
        let err = AttenuatedToken::derive_child(&parent, "write", vec![], b"key").unwrap_err();
        assert!(matches!(err, AttenuationError::ActionExceedsParent));
    }

    #[test]
    fn admin_parent_allows_a_different_child_action() {
        let parent = parent("admin:cohort", &["a"]);
        let child = AttenuatedToken::derive_child(&parent, "read", vec![], b"key").unwrap();
        assert_eq!(child.token.action, "read");
    }

    #[test]
    fn child_cannot_widen_cohort_scope() {
        let parent = parent("read", &["a"]);
        let wider = vec!["a".to_string(), "b".to_string()];
        let err = AttenuatedToken::derive_child(&parent, "read", wider, b"key").unwrap_err();
        assert!(matches!(err, AttenuationError::ScopeExceedsParent));
    }
}
