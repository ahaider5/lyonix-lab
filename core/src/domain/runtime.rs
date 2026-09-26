use crate::domain::{InstalledModel, ModelDefinition, RuntimeDevice};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeCapabilities {
    pub openai_compatible_api: bool,
    pub streaming: bool,
    pub tools: bool,
    pub vision: bool,
    pub reasoning: bool,
    pub model_download: bool,
    pub model_load_unload: bool,
    pub metrics: bool,
    pub embeddings: bool,
    pub multiple_models: bool,
    pub gpu_offload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProbe {
    pub id: String,
    pub display_name: String,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub capabilities: RuntimeCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOwnership {
    ManagedProcess,
    ExternalService,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Stopped,
    Starting,
    Loading,
    Ready,
    Stopping,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    pub host: String,
    pub port: u16,
}

impl Default for NetworkPolicy {
    fn default() -> Self { Self { host: "127.0.0.1".into(), port: 8081 } }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GpuOffloadPolicy {
    Auto,
    CpuOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPolicy {
    pub fit: bool,
    pub fit_target_mib: Option<u64>,
    pub cache_ram_mib: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvCachePolicy {
    pub key_type: String,
    pub value_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningPolicy {
    pub mode: String,
    pub budget: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingPolicy {
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: i32,
    pub min_p: f64,
    pub repeat_penalty: f64,
    pub presence_penalty: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfiguration {
    pub context: u32,
    pub batch: Option<u32>,
    pub micro_batch: Option<u32>,
    pub parallelism: u32,
    pub device_policy: Option<String>,
    pub gpu_offload: GpuOffloadPolicy,
    pub memory: MemoryPolicy,
    pub kv_cache: KvCachePolicy,
    pub reasoning: ReasoningPolicy,
    pub sampling: SamplingPolicy,
    pub network: NetworkPolicy,
    pub advanced: AdvancedSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdvancedSettings {
    pub flash_attention: Option<String>,
    pub numa: Option<String>,
    pub mlock: Option<bool>,
    pub no_mmap: Option<bool>,
    pub raw_runtime_arguments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchPlan {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub endpoint: String,
    pub model_id: String,
    pub artifact_id: String,
    pub ownership: RuntimeOwnership,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeHandle {
    pub pid: u32,
    pub executable: PathBuf,
    pub fingerprint: String,
    pub started_at_epoch_seconds: u64,
    pub stdout_log: Option<PathBuf>,
    pub stderr_log: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub runtime_id: String,
    pub state: RuntimeState,
    pub pid: Option<u32>,
    pub endpoint: Option<String>,
    pub model_id: Option<String>,
    pub artifact_id: Option<String>,
    pub effective_configuration: Option<RuntimeConfiguration>,
    pub devices: Vec<RuntimeDevice>,
    pub message: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeAdapterError {
    #[error("runtime error: {0}")]
    Message(String),
}

pub trait RuntimeAdapter: Send + Sync {
    fn id(&self) -> &str;
    fn probe(&self) -> Result<RuntimeProbe, RuntimeAdapterError>;
    fn capabilities(&self) -> RuntimeCapabilities;
    fn discover_models(&self) -> Result<Vec<String>, RuntimeAdapterError>;
    fn validate(&self, model: &ModelDefinition, artifact: &InstalledModel, config: &RuntimeConfiguration) -> Result<(), RuntimeAdapterError>;
    fn build_launch_plan(&self, model: &ModelDefinition, artifact: &InstalledModel, config: &RuntimeConfiguration) -> Result<LaunchPlan, RuntimeAdapterError>;
    fn inspect(&self) -> Result<RuntimeStatus, RuntimeAdapterError>;
}
