use crate::domain::{GpuInfo, GpuVendor, MemoryKind};
use std::process::Command;

pub fn discover_gpus() -> Vec<GpuInfo> {
    let Ok(output) = Command::new("system_profiler").args(["SPDisplaysDataType", "-json"]).output() else { return Vec::new(); };
    if !output.status.success() { return Vec::new(); }
    let text=String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if text.contains("apple") { vec![GpuInfo { vendor: GpuVendor::Apple, model: "Apple GPU".into(), hardware_id: None, dedicated_memory_bytes: None, shared_memory_bytes: None, memory_kind: MemoryKind::Unified, backend: Some("metal".into()), driver: None, runtime_devices: Vec::new() }] } else { Vec::new() }
}
