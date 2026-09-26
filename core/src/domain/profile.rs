use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningMode {
    Off,
    On,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Chat,
    Coding,
    Reasoning,
    Vision,
    LongContext,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProfile {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub task: TaskKind,
    pub target_context: u32,
    pub preferred_reasoning: ReasoningMode,
    pub preferred_parallelism: u32,
    pub low_latency: bool,
    pub deterministic: bool,
    pub allow_reasoning: bool,
    pub cpu_only: bool,
}
