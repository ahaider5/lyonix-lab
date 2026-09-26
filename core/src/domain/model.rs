use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ModelId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCapabilities {
    pub coding: bool,
    pub reasoning: bool,
    pub vision: bool,
    pub tools: bool,
    pub embeddings: bool,
    pub streaming: bool,
    pub tool_calling: bool,
    pub uncensored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelRuntimeHints {
    pub layers: Option<u32>,
    pub full_attention_layers: Option<u32>,
    pub kv_heads: Option<u32>,
    pub head_dim: Option<u32>,
    pub kv_cache_bytes_per_token_f16: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSource {
    pub provider: String,
    pub repository: Option<String>,
    pub revision: Option<String>,
    pub filename: Option<String>,
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    pub id: ModelId,
    pub display_name: String,
    pub family: String,
    pub architecture: Option<String>,
    pub parameters_b: f64,
    pub context_limit: u32,
    pub recommended_context: Option<u32>,
    pub roles: Vec<String>,
    pub default_profile: Option<String>,
    pub supported_profiles: Vec<String>,
    pub capabilities: ModelCapabilities,
    pub runtime_hints: ModelRuntimeHints,
    pub default_temperature: f64,
    pub default_top_p: f64,
    pub default_top_k: i32,
    pub default_min_p: f64,
    pub default_repeat_penalty: f64,
    pub default_presence_penalty: f64,
    pub source: Option<ModelSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Quantization {
    Q2,
    Q3,
    Q4,
    Q5,
    Q6,
    Q8,
    F16,
    Bf16,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelArtifact {
    pub id: String,
    pub model_definition_id: ModelId,
    pub quantization: Quantization,
    pub filename: String,
    pub size_bytes: Option<u64>,
    pub sha256: Option<String>,
    pub source: Option<ModelSource>,
    pub runtime_compatibility: Vec<String>,
    pub mmproj_filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledModel {
    pub artifact_id: String,
    pub model_definition_id: ModelId,
    pub local_path: PathBuf,
    pub mmproj_path: Option<PathBuf>,
    pub verified: bool,
    pub installed_at_epoch_seconds: u64,
    pub last_used_at_epoch_seconds: Option<u64>,
}
