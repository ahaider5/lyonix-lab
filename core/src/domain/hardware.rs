use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Other,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Dedicated,
    Shared,
    Unified,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeDevice {
    pub runtime_id: String,
    pub device_id: String,
    pub backend: String,
    pub name: String,
    pub total_memory_bytes: Option<u64>,
    pub available_memory_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuInfo {
    pub vendor: GpuVendor,
    pub model: String,
    pub hardware_id: Option<String>,
    pub dedicated_memory_bytes: Option<u64>,
    pub shared_memory_bytes: Option<u64>,
    pub memory_kind: MemoryKind,
    pub backend: Option<String>,
    pub driver: Option<String>,
    pub runtime_devices: Vec<RuntimeDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CpuInfo {
    pub vendor: Option<String>,
    pub model: String,
    pub architecture: String,
    pub physical_cores: u32,
    pub logical_cores: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub pressure_percent: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HardwareSnapshot {
    pub os: String,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub gpus: Vec<GpuInfo>,
    pub runtime_devices: Vec<RuntimeDevice>,
    pub collected_at_epoch_seconds: u64,
}

impl HardwareSnapshot {
    pub fn dedicated_gpu_bytes(&self) -> u64 {
        self.gpus
            .iter()
            .filter_map(|g| match g.memory_kind {
                MemoryKind::Dedicated => g.dedicated_memory_bytes,
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }

    pub fn dedicated_gpu_gib(&self) -> f64 {
        self.dedicated_gpu_bytes() as f64 / 1024_f64.powi(3)
    }

    pub fn total_ram_gib(&self) -> f64 {
        self.memory.total_bytes as f64 / 1024_f64.powi(3)
    }

    pub fn available_ram_gib(&self) -> f64 {
        self.memory.available_bytes as f64 / 1024_f64.powi(3)
    }

    pub fn hardware_tier(&self) -> &'static str {
        let ram = self.total_ram_gib();
        let vram = self.dedicated_gpu_gib();
        let threads = self.cpu.logical_cores;
        if ram < 8.0 || threads <= 4 {
            "entry"
        } else if ram < 12.0 {
            "light-laptop"
        } else if ram < 16.0 || vram < 2.0 {
            "laptop"
        } else if ram < 32.0 || vram < 6.0 {
            "standard"
        } else if ram < 64.0 || vram < 12.0 {
            "high"
        } else {
            "enthusiast"
        }
    }
}

