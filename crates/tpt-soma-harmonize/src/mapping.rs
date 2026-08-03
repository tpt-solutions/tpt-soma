use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MappingTable {
    #[serde(flatten)]
    pub mappings: HashMap<String, String>,
}

impl MappingTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.mappings.insert(key.into(), value.into());
    }

    pub fn resolve(&self, key: &str) -> Option<&str> {
        self.mappings.get(key).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.mappings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.mappings.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_resolve_len() {
        let mut table = MappingTable::new();
        assert!(table.is_empty());
        table.insert("1:100:A:T", "rs123");
        table.insert("BRCA1", "HGNC:1100");
        assert_eq!(table.len(), 2);
        assert_eq!(table.resolve("1:100:A:T"), Some("rs123"));
        assert_eq!(table.resolve("BRCA1"), Some("HGNC:1100"));
        assert_eq!(table.resolve("missing"), None);
    }

    #[test]
    fn insert_overwrites() {
        let mut table = MappingTable::new();
        table.insert("1:100:A:T", "rs123");
        table.insert("1:100:A:T", "rs999");
        assert_eq!(table.len(), 1);
        assert_eq!(table.resolve("1:100:A:T"), Some("rs999"));
    }

    #[test]
    fn serde_round_trip() {
        let mut table = MappingTable::new();
        table.insert("1:100:A:T", "rs123");
        let json = serde_json::to_string(&table).unwrap();
        let decoded: MappingTable = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.resolve("1:100:A:T"), Some("rs123"));
    }

    #[test]
    fn resolve_is_readonly_to_map() {
        let table = MappingTable::new();
        let _ = table.resolve("nope");
        assert!(table.is_empty());
    }
}
