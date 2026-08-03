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

#[cfg(test)]
mod tests {
    use super::*;

    fn queue() -> ReviewQueue {
        ReviewQueue {
            pending: vec![
                Unmapped {
                    identifier: "1:999:G:A".to_string(),
                    source: "dbSNP".to_string(),
                },
                Unmapped {
                    identifier: "BRCA2".to_string(),
                    source: "HGNC".to_string(),
                },
            ],
        }
    }

    #[test]
    fn serde_round_trip_preserves_pending() {
        let original = queue();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: ReviewQueue = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.pending.len(), 2);
        assert_eq!(decoded.pending[0].identifier, "1:999:G:A");
        assert_eq!(decoded.pending[1].source, "HGNC");
    }

    #[test]
    fn retain_removes_resolved_identifiers() {
        let mut review_queue = queue();
        let identifier = "1:999:G:A".to_string();
        review_queue.pending.retain(|u| u.identifier != identifier);
        assert_eq!(review_queue.pending.len(), 1);
        assert_eq!(review_queue.pending[0].identifier, "BRCA2");
    }
}
