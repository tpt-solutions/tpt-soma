use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct RevocationList {
    revoked_nonces: Arc<RwLock<HashSet<Vec<u8>>>>,
}

impl RevocationList {
    pub fn new() -> Self {
        Self {
            revoked_nonces: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub async fn revoke(&self, nonce: Vec<u8>) {
        let mut list = self.revoked_nonces.write().await;
        list.insert(nonce);
    }

    pub async fn contains(&self, nonce: &[u8]) -> bool {
        let list = self.revoked_nonces.read().await;
        list.contains(nonce)
    }

    pub async fn is_empty(&self) -> bool {
        let list = self.revoked_nonces.read().await;
        list.is_empty()
    }
}

impl Default for RevocationList {
    fn default() -> Self {
        Self::new()
    }
}
