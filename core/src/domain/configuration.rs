use crate::domain::NetworkPolicy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryDefaults {
    pub default_runtime: String,
    pub default_model_id: Option<String>,
    pub default_profile_id: String,
    pub network: NetworkPolicy,
}

impl Default for RepositoryDefaults {
    fn default() -> Self {
        Self {
            default_runtime: "llama.cpp".into(),
            default_model_id: None,
            default_profile_id: "chat".into(),
            network: NetworkPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfiguration {
    pub model_roots: Vec<PathBuf>,
    pub version: u32,
    pub llama_server_path: Option<PathBuf>,
    pub selected_model_id: Option<String>,
    pub selected_profile_id: Option<String>,
    pub network: Option<NetworkPolicy>,
}

impl Default for UserConfiguration {
    fn default() -> Self {
        Self { version: 1, model_roots: Vec::new(), llama_server_path: None, selected_model_id: None, selected_profile_id: None, network: None }
    }
}

