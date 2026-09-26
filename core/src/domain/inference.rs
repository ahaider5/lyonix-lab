use serde::{Deserialize, Serialize};

/// A single chat message in a runtime-neutral conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Runtime-neutral chat completion request. Serialization matches the
/// OpenAI-compatible schema spoken by the managed llama.cpp server endpoint;
/// llama.cpp-specific transport details live behind the runtime boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: bool,
}

/// Runtime-neutral chat completion result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatResponse {
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_latency_ms: Option<f64>,
}

/// Incremental streaming event for a chat completion. The stream ends after
/// `Done`; `Delta` events carry the growing assistant text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatStreamEvent {
    Delta { content: String },
    Done { finish_reason: Option<String> },
}

/// Typed errors for the inference path.
#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error("runtime is not ready yet")]
    NotReady,
    #[error("invalid inference request: {0}")]
    InvalidRequest(String),
    #[error("non-localhost inference endpoint '{0}' is not enabled by the initial core")]
    NonLocalhost(String),
    #[error("inference transport error: {0}")]
    Http(String),
    #[error("inference I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("inference request failed with status {code}: {message}")]
    Status { code: u16, message: String },
    #[error("inference response serialization error: {0}")]
    Serialization(String),
}

impl ChatRequest {
    pub fn validate(&self) -> Result<(), InferenceError> {
        if self.messages.is_empty() {
            return Err(InferenceError::InvalidRequest(
                "messages must not be empty".into(),
            ));
        }
        if let Some(t) = self.temperature.filter(|t| !(0.0..=2.0).contains(t)) {
            return Err(InferenceError::InvalidRequest(format!(
                "temperature {t} outside 0.0..=2.0"
            )));
        }
        if let Some(p) = self.top_p.filter(|p| !(0.0..=1.0).contains(p)) {
            return Err(InferenceError::InvalidRequest(format!(
                "top_p {p} outside 0.0..=1.0"
            )));
        }
        if let Some(k) = self.top_k.filter(|k| *k < -1) {
            return Err(InferenceError::InvalidRequest(format!(
                "top_k {k} must be >= -1"
            )));
        }
        Ok(())
    }
}
