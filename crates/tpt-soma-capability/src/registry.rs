use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DataClass {
    pub id: String,
    pub description: String,
    pub sensitivity: Sensitivity,
}

#[derive(Debug, Clone)]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
}

#[derive(Default)]
pub struct DataClassRegistry {
    classes: HashMap<String, DataClass>,
}

impl DataClassRegistry {
    pub fn register(&mut self, class: DataClass) {
        self.classes.insert(class.id.clone(), class);
    }

    pub fn get(&self, id: &str) -> Option<&DataClass> {
        self.classes.get(id)
    }

    pub fn list(&self) -> Vec<&DataClass> {
        self.classes.values().collect()
    }

    pub fn seed_phase0(&mut self) {
        let seeds = [
            (
                "genomic_raw",
                "Raw sequencing reads",
                Sensitivity::Restricted,
            ),
            (
                "genomic_variant",
                "Variant calls (VCF)",
                Sensitivity::Confidential,
            ),
            (
                "transcriptomic_scrna",
                "Single-cell RNA-seq",
                Sensitivity::Confidential,
            ),
            (
                "phi_demographic",
                "PHI demographic data",
                Sensitivity::Restricted,
            ),
        ];
        for (id, desc, sens) in seeds {
            self.register(DataClass {
                id: id.to_string(),
                description: desc.to_string(),
                sensitivity: sens,
            });
        }
    }

    /// Phase 2 data classes: clinical observations (FHIR/CSV), continuous glucose
    /// monitoring, and organ imaging metadata.
    pub fn seed_phase2(&mut self) {
        let seeds = [
            (
                "clinical_observation",
                "Normalized clinical observations from FHIR/CSV ingestion",
                Sensitivity::Confidential,
            ),
            (
                "cgm_continuous",
                "Continuous glucose monitor readings",
                Sensitivity::Confidential,
            ),
            (
                "organ_imaging",
                "Organ imaging metadata (MRI/CT/ultrasound/PET)",
                Sensitivity::Restricted,
            ),
        ];
        for (id, desc, sens) in seeds {
            self.register(DataClass {
                id: id.to_string(),
                description: desc.to_string(),
                sensitivity: sens,
            });
        }
    }

    /// Phase 3 data class: simulation-derived outputs (digital-twin trajectories,
    /// parameter sets, calibration results).
    pub fn seed_phase3(&mut self) {
        self.register(DataClass {
            id: "simulation_output".to_string(),
            description: "Digital-twin simulation outputs (trajectories, parameter sets)"
                .to_string(),
            sensitivity: Sensitivity::Confidential,
        });
    }

    /// Phase 4 data classes: pathology findings, clinical-trial metadata, and
    /// biomarker-discovery results.
    pub fn seed_phase4(&mut self) {
        let seeds = [
            (
                "pathos_finding",
                "Computational pathology findings (e.g. insulin-resistance, tumor microenvironment)",
                Sensitivity::Confidential,
            ),
            (
                "clinical_trial",
                "Clinical trial design, cohort discovery, and adverse-event metadata",
                Sensitivity::Confidential,
            ),
            (
                "biomarker_discovery",
                "Biomarker discovery/validation statistical pipeline outputs",
                Sensitivity::Confidential,
            ),
        ];
        for (id, desc, sens) in seeds {
            self.register(DataClass {
                id: id.to_string(),
                description: desc.to_string(),
                sensitivity: sens,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seed_phase2_registers_all_classes() {
        let mut registry = DataClassRegistry::default();
        registry.seed_phase2();
        assert!(registry.get("clinical_observation").is_some());
        assert!(registry.get("cgm_continuous").is_some());
        assert!(registry.get("organ_imaging").is_some());
    }

    #[test]
    fn test_seed_phase3_registers_simulation_output() {
        let mut registry = DataClassRegistry::default();
        registry.seed_phase3();
        assert!(registry.get("simulation_output").is_some());
    }

    #[test]
    fn test_seed_phase4_registers_pathos_clinica_classes() {
        let mut registry = DataClassRegistry::default();
        registry.seed_phase4();
        assert!(registry.get("pathos_finding").is_some());
        assert!(registry.get("clinical_trial").is_some());
        assert!(registry.get("biomarker_discovery").is_some());
    }
}
