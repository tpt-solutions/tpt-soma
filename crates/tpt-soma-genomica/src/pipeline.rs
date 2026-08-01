pub struct Pipeline {
    pub name: String,
    pub steps: Vec<&'static str>,
}

impl Pipeline {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            steps: Vec::new(),
        }
    }

    pub fn add_step(mut self, step: &'static str) -> Self {
        self.steps.push(step);
        self
    }
}
