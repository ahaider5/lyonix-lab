use crate::domain::{ModelArtifact, ModelCapabilities, ModelDefinition, ModelRuntimeHints, ModelSource, Quantization};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub mod resolver;
pub use resolver::{ModelResolveError, ModelResolver, ResolvedModel};

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("failed to read catalog: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse catalog: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LegacyModelCapabilities {
    reasoning: Option<bool>,
    #[serde(rename = "toolCalling")]
    tool_calling: Option<bool>,
    coding: Option<bool>,
    vision: Option<bool>,
    uncensored: Option<bool>,
    streaming: Option<bool>,
    embeddings: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LegacyRuntimeHints {
    layers: Option<u32>,
    #[serde(rename = "fullAttentionLayers")]
    full_attention_layers: Option<u32>,
    #[serde(rename = "kvHeads")]
    kv_heads: Option<u32>,
    #[serde(rename = "headDim")]
    head_dim: Option<u32>,
    #[serde(rename = "kvCacheBytesPerTokenF16")]
    kv_cache_bytes_per_token_f16: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyModelRecord {
    id: String,
    #[serde(rename = "displayName")]
    display_name: String,
    path: String,
    #[serde(rename = "mmprojPath")]
    mmproj_path: Option<String>,
    family: String,
    #[serde(default)]
    architecture: Option<String>,
    #[serde(rename = "parametersB")]
    parameters_b: f64,
    quantization: String,
    #[serde(rename = "contextLimit")]
    context_limit: u32,
    #[serde(default)]
    #[serde(rename = "recommendedContext")]
    recommended_context: Option<u32>,
    #[serde(default)]
    vision: bool,
    #[serde(default)]
    reasoning: bool,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    #[serde(rename = "defaultProfile")]
    default_profile: Option<String>,
    #[serde(default)]
    #[serde(rename = "supportedProfiles")]
    supported_profiles: Vec<String>,
    #[serde(default)]
    capabilities: Option<LegacyModelCapabilities>,
    #[serde(default)]
    #[serde(rename = "runtimeHints")]
    runtime_hints: Option<LegacyRuntimeHints>,
    #[serde(rename = "defaultTemperature")]
    default_temperature: f64,
    #[serde(rename = "defaultTopP")]
    default_top_p: f64,
    #[serde(rename = "defaultTopK")]
    default_top_k: i32,
    #[serde(rename = "defaultMinP")]
    default_min_p: f64,
    #[serde(rename = "defaultRepeatPenalty")]
    default_repeat_penalty: f64,
    #[serde(rename = "defaultPresencePenalty")]
    default_presence_penalty: f64,
    #[serde(default)]
    source: Option<ModelSource>,
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub definition: ModelDefinition,
    pub artifact: ModelArtifact,
    pub relative_model_path: PathBuf,
    pub relative_mmproj_path: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct ModelCatalog {
    pub entries: Vec<CatalogEntry>,
}

impl ModelCatalog {
    pub fn from_legacy_json(path: impl AsRef<Path>) -> Result<Self, CatalogError> {
        let bytes = std::fs::read(path)?;
        Self::from_legacy_bytes(&bytes)
    }

    pub fn from_legacy_bytes(bytes: &[u8]) -> Result<Self, CatalogError> {
        let records: Vec<LegacyModelRecord> = serde_json::from_slice(bytes)?;
        let entries = records.into_iter().map(|record| {
            let supplied_capabilities = record.capabilities.unwrap_or_default();
            let coding = supplied_capabilities.coding.unwrap_or(matches!(record.family.as_str(), "Qwen3" | "Qwen3.5" | "Qwen2.5 Coder"));
            let reasoning = supplied_capabilities.reasoning.unwrap_or(record.reasoning);
            let vision = supplied_capabilities.vision.unwrap_or(record.vision);
            let capabilities = ModelCapabilities {
                coding,
                reasoning,
                vision,
                tools: supplied_capabilities.tool_calling.unwrap_or(false),
                embeddings: supplied_capabilities.embeddings.unwrap_or(false),
                streaming: supplied_capabilities.streaming.unwrap_or(true),
                tool_calling: supplied_capabilities.tool_calling.unwrap_or(false),
                uncensored: supplied_capabilities.uncensored.unwrap_or(false),
            };

            let runtime_hints = record.runtime_hints.unwrap_or_default();
            let def = ModelDefinition {
                id: crate::domain::ModelId(record.id.clone()),
                display_name: record.display_name,
                family: record.family,
                architecture: record.architecture,
                parameters_b: record.parameters_b,
                context_limit: record.context_limit,
                recommended_context: record.recommended_context,
                roles: record.roles,
                default_profile: record.default_profile,
                supported_profiles: record.supported_profiles,
                capabilities,
                runtime_hints: ModelRuntimeHints {
                    layers: runtime_hints.layers,
                    full_attention_layers: runtime_hints.full_attention_layers,
                    kv_heads: runtime_hints.kv_heads,
                    head_dim: runtime_hints.head_dim,
                    kv_cache_bytes_per_token_f16: runtime_hints.kv_cache_bytes_per_token_f16,
                },
                default_temperature: record.default_temperature,
                default_top_p: record.default_top_p,
                default_top_k: record.default_top_k,
                default_min_p: record.default_min_p,
                default_repeat_penalty: record.default_repeat_penalty,
                default_presence_penalty: record.default_presence_penalty,
                source: record.source.clone(),
            };
            let artifact = ModelArtifact {
                id: format!("{}:{}", record.id, record.quantization.to_lowercase()),
                model_definition_id: def.id.clone(),
                quantization: parse_quantization(&record.quantization),
                filename: Path::new(&record.path).file_name().unwrap_or_default().to_string_lossy().into_owned(),
                size_bytes: None,
                sha256: None,
                source: record.source,
                runtime_compatibility: vec!["llama.cpp".into()],
                mmproj_filename: record.mmproj_path.as_ref().and_then(|p| Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned())),
            };
            CatalogEntry {
                definition: def,
                artifact,
                relative_model_path: PathBuf::from(record.path),
                relative_mmproj_path: record.mmproj_path.map(PathBuf::from),
            }
        }).collect();
        Ok(Self { entries })
    }

    pub fn installed_models(&self, roots: &[PathBuf]) -> Vec<crate::domain::InstalledModel> {
        let mut installed = Vec::new();
        for entry in &self.entries {
            for root in roots {
                if !is_safe_relative_path(&entry.relative_model_path) { continue; }
                let model_path = root.join(&entry.relative_model_path);
                if model_path.is_file() {
                    let mmproj = entry.relative_mmproj_path.as_ref().filter(|p| is_safe_relative_path(p)).map(|p| root.join(p)).filter(|p| p.is_file());
                    let verified = entry.artifact.sha256.as_deref().map(|expected| sha256_matches(&model_path, expected)).unwrap_or(false);
                    installed.push(crate::domain::InstalledModel {
                        artifact_id: entry.artifact.id.clone(),
                        model_definition_id: entry.definition.id.clone(),
                        local_path: model_path,
                        mmproj_path: mmproj,
                        verified,
                        installed_at_epoch_seconds: 0,
                        last_used_at_epoch_seconds: None,
                    });
                    break;
                }
            }
        }
        installed
    }
}

fn parse_quantization(value: &str) -> Quantization {
    let upper = value.to_ascii_uppercase();
    let search = upper.strip_prefix("I1-").unwrap_or(&upper);
    if search.contains("BF16") { Quantization::Bf16 }
    else if search.contains("F16") { Quantization::F16 }
    else if search.contains("Q8") { Quantization::Q8 }
    else if search.contains("Q6") { Quantization::Q6 }
    else if search.contains("Q5") { Quantization::Q5 }
    else if search.contains("Q4") { Quantization::Q4 }
    else if search.contains("Q3") { Quantization::Q3 }
    else if search.contains("Q2") { Quantization::Q2 }
    else { Quantization::Unknown }
}

fn is_safe_relative_path(path: &Path) -> bool {
    if path.is_absolute() { return false; }
    !path.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir | std::path::Component::Prefix(_)))
}

fn sha256_matches(path: &Path, expected: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else { return false; };
    let actual = format!("{:x}", Sha256::digest(&bytes));
    actual.eq_ignore_ascii_case(expected)
}

pub fn legacy_catalog_supports_llama_cpp(entry: &CatalogEntry) -> bool {
    entry.artifact.runtime_compatibility.iter().any(|r| r == "llama.cpp")
}
