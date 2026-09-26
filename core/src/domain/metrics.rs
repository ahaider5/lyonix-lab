use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceMetric {
    pub timestamp_epoch_seconds: u64,
    pub runtime: String,
    pub runtime_version: Option<String>,
    pub model: String,
    pub artifact: Option<String>,
    pub hardware_fingerprint: Option<String>,
    pub device: Option<String>,
    pub context_size: Option<u32>,
    pub quantization: Option<String>,
    pub prompt_tokens: Option<u64>,
    pub generated_tokens: Option<u64>,
    pub prompt_tokens_per_second: Option<f64>,
    pub generation_tokens_per_second: Option<f64>,
    pub time_to_first_token_ms: Option<f64>,
    pub total_latency_ms: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub vram_bytes: Option<u64>,
}
