use crate::domain::{GpuInfo, GpuVendor, MemoryKind};
use std::process::Command;

pub fn discover_gpus() -> Vec<GpuInfo> {
    let Ok(output) = Command::new("nvidia-smi").args(["--query-gpu=name,memory.total,driver_version", "--format=csv,noheader,nounits"]).output() else { return Vec::new(); };
    if !output.status.success() { return Vec::new(); }
    String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| {
        let mut parts=line.split(',').map(|s| s.trim()); let model=parts.next()?.to_string(); let mib:u64=parts.next()?.parse().ok()?; let driver=parts.next().map(str::to_string);
        Some(GpuInfo { vendor: GpuVendor::Nvidia, model, hardware_id: None, dedicated_memory_bytes: Some(mib*1024*1024), shared_memory_bytes: None, memory_kind: MemoryKind::Dedicated, backend: None, driver, runtime_devices: Vec::new() })
    }).collect()
}
