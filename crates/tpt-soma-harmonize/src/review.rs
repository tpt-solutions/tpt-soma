use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewQueue {
    pub pending: Vec<Unmapped>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unmapped {
    pub identifier: String,
    pub source: String,
}
