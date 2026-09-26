use super::CatalogEntry;
use crate::domain::{InstalledModel, ModelArtifact, ModelDefinition};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub definition: ModelDefinition,
    pub artifact: ModelArtifact,
    pub installed: InstalledModel,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelResolveError {
    #[error("model definition '{0}' was not found")]
    UnknownModel(String),
    #[error("no installed artifact for model '{0}' is compatible with runtime '{1}'")]
    NoCompatibleArtifact(String, String),
}

#[derive(Debug, Default)]
pub struct ModelResolver;

impl ModelResolver {
    pub fn resolve(
        &self,
        model_id: &str,
        runtime_id: &str,
        entries: &[CatalogEntry],
        installed: &[InstalledModel],
    ) -> Result<ResolvedModel, ModelResolveError> {
        let candidates: Vec<_> = entries.iter().filter(|e| e.definition.id.0 == model_id).collect();
        if candidates.is_empty() {
            return Err(ModelResolveError::UnknownModel(model_id.into()));
        }

        // Prefer the highest quantization quality among locally installed
        // compatible artifacts. Exact scoring remains the recommendation
        // engine's responsibility; the resolver only resolves identity and
        // availability.
        let rank = |artifact: &ModelArtifact| match artifact.quantization {
            crate::domain::Quantization::F16 | crate::domain::Quantization::Bf16 => 80,
            crate::domain::Quantization::Q8 => 70,
            crate::domain::Quantization::Q6 => 60,
            crate::domain::Quantization::Q5 => 50,
            crate::domain::Quantization::Q4 => 40,
            crate::domain::Quantization::Q3 => 30,
            crate::domain::Quantization::Q2 => 20,
            crate::domain::Quantization::Unknown => 0,
        };

        let mut matches = candidates.into_iter().filter_map(|entry| {
            if !entry.artifact.runtime_compatibility.iter().any(|r| r == runtime_id) {
                return None;
            }
            let installed_model = installed.iter().find(|i| i.artifact_id == entry.artifact.id)?.clone();
            Some((rank(&entry.artifact), entry, installed_model))
        }).collect::<Vec<_>>();
        matches.sort_by_key(|(score, _, _)| std::cmp::Reverse(*score));

        matches.into_iter().next()
            .map(|(_, entry, installed)| ResolvedModel { definition: entry.definition.clone(), artifact: entry.artifact.clone(), installed })
            .ok_or_else(|| ModelResolveError::NoCompatibleArtifact(model_id.into(), runtime_id.into()))
    }
}

#[allow(dead_code)]
fn _keep_path_import(_path: Option<PathBuf>) {}
