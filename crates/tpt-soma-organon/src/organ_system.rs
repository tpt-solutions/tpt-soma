//! Cross-organ coupling analysis and organ system graph

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Cross-organ coupling edge types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CouplingType {
    /// Hormonal/endocrine signaling
    Endocrine,
    /// Neural signaling
    Neural,
    /// Metabolic substrate exchange
    Metabolic,
    /// Immune cell trafficking
    Immune,
    /// Mechanical/hemodynamic coupling
    Mechanical,
    /// Inflammatory mediator spread
    Inflammatory,
}

/// Cross-organ coupling edge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossOrganCoupling {
    pub source_organ: String, // UBERON code
    pub target_organ: String, // UBERON code
    pub coupling_type: CouplingType,
    pub strength: f64,          // 0.0 to 1.0
    pub mediators: Vec<String>, // e.g., hormone names, cytokines
    pub evidence_level: EvidenceLevel,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceLevel {
    Computational,
    AnimalModel,
    HumanObservational,
    HumanInterventional,
    ConsensusGuideline,
}

/// Organ system graph for cross-talk analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganSystemGraph {
    pub nodes: HashMap<String, OrganNode>,
    pub edges: Vec<CrossOrganCoupling>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganNode {
    pub uberon_id: String,
    pub name: String,
    pub system: String, // e.g., "cardiovascular", "renal"
    pub functions: Vec<String>,
    pub biomarkers: Vec<String>, // LOINC codes
}

impl OrganSystemGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_node(&mut self, node: OrganNode) {
        self.nodes.insert(node.uberon_id.clone(), node);
    }

    pub fn add_edge(&mut self, edge: CrossOrganCoupling) {
        self.edges.push(edge);
    }

    pub fn get_neighbors(
        &self,
        organ: &str,
        coupling_type: Option<CouplingType>,
    ) -> Vec<&CrossOrganCoupling> {
        self.edges
            .iter()
            .filter(|e| {
                (e.source_organ == organ || e.target_organ == organ)
                    && coupling_type
                        .as_ref()
                        .is_none_or(|ct| &e.coupling_type == ct)
            })
            .collect()
    }

    pub fn find_paths(
        &self,
        source: &str,
        target: &str,
        max_depth: usize,
    ) -> Vec<Vec<&CrossOrganCoupling>> {
        let mut paths = Vec::new();
        let mut visited = HashSet::new();
        let mut current_path = Vec::new();

        self.dfs_find_paths(
            source,
            target,
            max_depth,
            &mut visited,
            &mut current_path,
            &mut paths,
        );
        paths
    }

    fn dfs_find_paths<'a>(
        &'a self,
        current: &str,
        target: &str,
        max_depth: usize,
        visited: &mut HashSet<String>,
        current_path: &mut Vec<&'a CrossOrganCoupling>,
        paths: &mut Vec<Vec<&'a CrossOrganCoupling>>,
    ) {
        if current_path.len() >= max_depth {
            return;
        }

        if current == target && !current_path.is_empty() {
            paths.push(current_path.clone());
            return;
        }

        visited.insert(current.to_string());

        for edge in self.get_neighbors(current, None) {
            let next = if edge.source_organ == current {
                &edge.target_organ
            } else {
                &edge.source_organ
            };

            if !visited.contains(next) {
                current_path.push(edge);
                self.dfs_find_paths(next, target, max_depth, visited, current_path, paths);
                current_path.pop();
            }
        }

        visited.remove(current);
    }

    /// Calculate cascade risk score for a dysfunction starting at an organ
    pub fn cascade_risk(&self, source_organ: &str, max_hops: usize) -> f64 {
        let mut total_risk = 0.0;
        let mut visited = HashSet::new();
        let mut queue = vec![(source_organ.to_string(), 1.0, 0)];

        while let Some((organ, risk, depth)) = queue.pop() {
            if depth >= max_hops || visited.contains(&organ) {
                continue;
            }
            visited.insert(organ.clone());
            total_risk += risk;

            for edge in self.get_neighbors(&organ, None) {
                let next = if edge.source_organ == organ {
                    &edge.target_organ
                } else {
                    &edge.source_organ
                };
                let propagated_risk = risk * edge.strength * 0.8; // Decay factor
                if propagated_risk > 0.01 {
                    queue.push((next.clone(), propagated_risk, depth + 1));
                }
            }
        }

        total_risk
    }
}

impl Default for OrganSystemGraph {
    fn default() -> Self {
        let mut graph = Self::new();

        // Add major organ nodes
        graph.add_node(OrganNode {
            uberon_id: "UBERON:0002107".to_string(), // liver
            name: "Liver".to_string(),
            system: "hepatic".to_string(),
            functions: vec![
                "detoxification".to_string(),
                "protein_synthesis".to_string(),
                "metabolism".to_string(),
            ],
            biomarkers: vec![
                "1742-6".to_string(),
                "1920-8".to_string(),
                "6768-6".to_string(),
            ], // ALT, AST, ALP
        });

        graph.add_node(OrganNode {
            uberon_id: "UBERON:0002113".to_string(), // kidney
            name: "Kidney".to_string(),
            system: "renal".to_string(),
            functions: vec![
                "filtration".to_string(),
                "electrolyte_balance".to_string(),
                "blood_pressure".to_string(),
            ],
            biomarkers: vec!["2160-0".to_string(), "62238-1".to_string()], // Creatinine, eGFR
        });

        graph.add_node(OrganNode {
            uberon_id: "UBERON:0004535".to_string(), // heart
            name: "Heart".to_string(),
            system: "cardiovascular".to_string(),
            functions: vec!["pump".to_string(), "circulation".to_string()],
            biomarkers: vec!["18043-0".to_string(), "10839-9".to_string()], // EF, Troponin
        });

        graph.add_node(OrganNode {
            uberon_id: "UBERON:0001004".to_string(), // lung
            name: "Lung".to_string(),
            system: "respiratory".to_string(),
            functions: vec!["gas_exchange".to_string()],
            biomarkers: vec!["19914-1".to_string(), "19915-8".to_string()], // FEV1, FVC
        });

        graph.add_node(OrganNode {
            uberon_id: "UBERON:0000949".to_string(), // pancreas
            name: "Pancreas".to_string(),
            system: "endocrine".to_string(),
            functions: vec![
                "insulin_secretion".to_string(),
                "glucagon_secretion".to_string(),
            ],
            biomarkers: vec!["4548-4".to_string(), "1558-6".to_string()], // HbA1c, Glucose
        });

        graph.add_node(OrganNode {
            uberon_id: "UBERON:0001016".to_string(), // brain
            name: "Brain".to_string(),
            system: "nervous".to_string(),
            functions: vec!["neural_control".to_string(), "neuroendocrine".to_string()],
            biomarkers: vec![],
        });

        graph.add_node(OrganNode {
            uberon_id: "UBERON:0001007".to_string(), // intestine
            name: "Intestine".to_string(),
            system: "gastrointestinal".to_string(),
            functions: vec!["absorption".to_string(), "immune_barrier".to_string()],
            biomarkers: vec![],
        });

        // Add cross-organ coupling edges (simplified examples)
        graph.add_edge(CrossOrganCoupling {
            source_organ: "UBERON:0000949".to_string(), // pancreas
            target_organ: "UBERON:0002107".to_string(), // liver
            coupling_type: CouplingType::Endocrine,
            strength: 0.9,
            mediators: vec!["insulin".to_string(), "glucagon".to_string()],
            evidence_level: EvidenceLevel::HumanInterventional,
            description: "Insulin/glucagon regulate hepatic glucose production".to_string(),
        });

        graph.add_edge(CrossOrganCoupling {
            source_organ: "UBERON:0002113".to_string(), // kidney
            target_organ: "UBERON:0004535".to_string(), // heart
            coupling_type: CouplingType::Mechanical,
            strength: 0.7,
            mediators: vec![
                "renin".to_string(),
                "angiotensin".to_string(),
                "aldosterone".to_string(),
            ],
            evidence_level: EvidenceLevel::HumanInterventional,
            description: "RAAS system links renal perfusion to cardiac load".to_string(),
        });

        graph.add_edge(CrossOrganCoupling {
            source_organ: "UBERON:0002107".to_string(), // liver
            target_organ: "UBERON:0002113".to_string(), // kidney
            coupling_type: CouplingType::Metabolic,
            strength: 0.6,
            mediators: vec!["urea".to_string(), "creatinine".to_string()],
            evidence_level: EvidenceLevel::HumanObservational,
            description: "Hepatic urea production affects renal clearance".to_string(),
        });

        graph.add_edge(CrossOrganCoupling {
            source_organ: "UBERON:0001004".to_string(), // lung
            target_organ: "UBERON:0004535".to_string(), // heart
            coupling_type: CouplingType::Mechanical,
            strength: 0.8,
            mediators: vec!["intrathoracic_pressure".to_string()],
            evidence_level: EvidenceLevel::HumanObservational,
            description: "Pulmonary pressure affects right ventricular afterload".to_string(),
        });

        graph.add_edge(CrossOrganCoupling {
            source_organ: "UBERON:0001007".to_string(), // intestine
            target_organ: "UBERON:0002107".to_string(), // liver
            coupling_type: CouplingType::Metabolic,
            strength: 0.7,
            mediators: vec![
                "portal_venous_nutrients".to_string(),
                "bile_acids".to_string(),
            ],
            evidence_level: EvidenceLevel::HumanObservational,
            description: "Gut-liver axis via portal circulation".to_string(),
        });

        graph.add_edge(CrossOrganCoupling {
            source_organ: "UBERON:0000949".to_string(), // pancreas
            target_organ: "UBERON:0001007".to_string(), // intestine
            coupling_type: CouplingType::Endocrine,
            strength: 0.5,
            mediators: vec![
                "insulin".to_string(),
                "glucagon".to_string(),
                "somatostatin".to_string(),
            ],
            evidence_level: EvidenceLevel::AnimalModel,
            description: "Pancreatic hormones affect intestinal motility and absorption"
                .to_string(),
        });

        graph.add_edge(CrossOrganCoupling {
            source_organ: "UBERON:0001016".to_string(), // brain
            target_organ: "UBERON:0004535".to_string(), // heart
            coupling_type: CouplingType::Neural,
            strength: 0.9,
            mediators: vec!["autonomic_nervous_system".to_string()],
            evidence_level: EvidenceLevel::HumanInterventional,
            description: "Autonomic control of heart rate and contractility".to_string(),
        });

        graph
    }
}

/// Dysfunction cascade simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DysfunctionCascade {
    pub initial_organ: String,
    pub initial_dysfunction: String,
    pub affected_organs: HashMap<String, OrganDysfunction>,
    pub total_risk_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganDysfunction {
    pub organ: String,
    pub dysfunction_type: String,
    pub severity: f64,        // 0.0 to 1.0
    pub pathway: Vec<String>, // Organ UBERON IDs in path
    pub mediators: Vec<String>,
}

impl OrganSystemGraph {
    pub fn simulate_cascade(
        &self,
        initial_organ: &str,
        initial_dysfunction: &str,
        severity: f64,
    ) -> DysfunctionCascade {
        let mut affected = HashMap::new();
        let mut queue = vec![(
            initial_organ.to_string(),
            severity,
            vec![initial_organ.to_string()],
            Vec::new(),
        )];
        let mut visited = HashMap::new();

        while let Some((organ, sev, path, mediators)) = queue.pop() {
            if sev < 0.05 {
                continue;
            }

            let existing_sev = visited.get(&organ).copied().unwrap_or(0.0);
            if sev <= existing_sev {
                continue;
            }
            visited.insert(organ.clone(), sev);

            affected.insert(
                organ.clone(),
                OrganDysfunction {
                    organ: organ.clone(),
                    dysfunction_type: if organ == initial_organ {
                        initial_dysfunction.to_string()
                    } else {
                        format!("secondary_{}", initial_dysfunction)
                    },
                    severity: sev,
                    pathway: path.clone(),
                    mediators: mediators.clone(),
                },
            );

            for edge in self.get_neighbors(&organ, None) {
                let next = if edge.source_organ == organ {
                    &edge.target_organ
                } else {
                    &edge.source_organ
                };

                let propagated_sev = sev * edge.strength * 0.7;
                if propagated_sev > 0.05 {
                    let mut new_path = path.clone();
                    new_path.push(next.clone());
                    let mut new_mediators = mediators.clone();
                    new_mediators.extend(edge.mediators.clone());
                    queue.push((next.clone(), propagated_sev, new_path, new_mediators));
                }
            }
        }

        let total_risk = affected.values().map(|d| d.severity).sum();

        DysfunctionCascade {
            initial_organ: initial_organ.to_string(),
            initial_dysfunction: initial_dysfunction.to_string(),
            affected_organs: affected,
            total_risk_score: total_risk,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_organ_system_graph_default() {
        let graph = OrganSystemGraph::default();
        assert!(!graph.nodes.is_empty());
        assert!(!graph.edges.is_empty());
    }

    #[test]
    fn test_find_paths() {
        let graph = OrganSystemGraph::default();
        let paths = graph.find_paths("UBERON:0000949", "UBERON:0002113", 3); // pancreas -> kidney
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_cascade_risk() {
        let graph = OrganSystemGraph::default();
        let risk = graph.cascade_risk("UBERON:0000949", 3); // pancreas
        assert!(risk > 0.0);
    }

    #[test]
    fn test_simulate_cascade() {
        let graph = OrganSystemGraph::default();
        let cascade = graph.simulate_cascade("UBERON:0000949", "insulin_deficiency", 1.0);
        assert!(cascade.affected_organs.contains_key("UBERON:0000949"));
        assert!(cascade.total_risk_score > 0.0);
    }
}
