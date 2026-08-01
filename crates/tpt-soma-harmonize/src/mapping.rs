use std::collections::HashMap;

#[derive(Default)]
pub struct MappingTable {
    mappings: HashMap<String, String>,
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
}
