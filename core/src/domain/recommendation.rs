use crate::domain::{ModelArtifact, ModelDefinition, RuntimeConfiguration};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RejectionCode {
    MissingArtifact,
    UnsupportedCapability,
    RuntimeIncompatible,
    InsufficientRam,
    InsufficientVram,
    ContextExceedsModelLimit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectionReason {
    pub code: RejectionCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeasibilityResult {
    pub feasible: bool,
    pub reasons: Vec<RejectionReason>,
}

/// Capability gates a requested task imposes on a candidate model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapabilityRequirements {
    pub vision: bool,
    pub reasoning: bool,
    pub tools: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateScore {
    pub artifact_id: String,
    pub score: f64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationResult {
    pub recommended_model: Option<ModelDefinition>,
    pub recommended_artifact: Option<ModelArtifact>,
    pub runtime_id: Option<String>,
    pub configuration: Option<RuntimeConfiguration>,
    pub confidence: f64,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub alternatives: Vec<CandidateScore>,
    pub rejected: Vec<RejectionReason>,
}
