use crate::domain::{CpuInfo, GpuInfo, GpuVendor, HardwareSnapshot, MemoryInfo, MemoryKind, RuntimeDevice};
use sysinfo::System;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")] mod windows;
#[cfg(target_os = "linux")] mod linux;
#[cfg(target_os = "macos")] mod macos;

#[derive(Debug, thiserror::Error)]
pub enum HardwareError { #[error("hardware discovery failed: {0}")] Failed(String) }

pub trait HardwareProvider: Send + Sync { fn snapshot(&self, runtime_devices: Vec<RuntimeDevice>) -> Result<HardwareSnapshot, HardwareError>; }

#[derive(Default)]
pub struct SystemHardwareProvider;

impl HardwareProvider for SystemHardwareProvider {
    fn snapshot(&self, runtime_devices: Vec<RuntimeDevice>) -> Result<HardwareSnapshot, HardwareError> {
        let mut system=System::new_all(); system.refresh_all(); let cpus=system.cpus(); let first=cpus.first();
        let cpu=CpuInfo { vendor:first.map(|c| c.vendor_id().to_string()), model:first.map(|c| c.brand().to_string()).unwrap_or_else(|| "Unknown CPU".into()), architecture:std::env::consts::ARCH.into(), physical_cores:System::physical_core_count().unwrap_or(0) as u32, logical_cores:cpus.len() as u32 };
        let total=system.total_memory(); let available=system.available_memory(); let used=total.saturating_sub(available);
        let memory=MemoryInfo { total_bytes:total, available_bytes:available, used_bytes:used, pressure_percent:if total>0 {Some((used as f32/total as f32)*100.0)} else {None} };
        let mut gpus=native_gpus(); merge_runtime_devices(&mut gpus, &runtime_devices);
        Ok(HardwareSnapshot { os:std::env::consts::OS.into(), cpu, memory, gpus, runtime_devices, collected_at_epoch_seconds:SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() })
    }
}

fn native_gpus()->Vec<GpuInfo>{
    #[cfg(target_os="windows")] { return windows::discover_gpus(); }
    #[cfg(target_os="linux")] { return linux::discover_gpus(); }
    #[cfg(target_os="macos")] { return macos::discover_gpus(); }
    #[allow(unreachable_code)] Vec::new()
}

fn merge_runtime_devices(gpus:&mut Vec<GpuInfo>, devices:&[RuntimeDevice]){
    for device in devices { if let Some(gpu)=gpus.iter_mut().find(|g| normalize_name(&g.model)==normalize_name(&device.name)) { gpu.runtime_devices.push(device.clone()); if gpu.dedicated_memory_bytes.is_none(){gpu.dedicated_memory_bytes=device.total_memory_bytes;} } else { let vendor=infer_vendor(&device.name); let kind=if vendor==GpuVendor::Apple {MemoryKind::Unified} else {MemoryKind::Unknown}; gpus.push(GpuInfo{vendor,model:device.name.clone(),hardware_id:None,dedicated_memory_bytes:device.total_memory_bytes,shared_memory_bytes:None,memory_kind:kind,backend:Some(device.backend.clone()),driver:None,runtime_devices:vec![device.clone()]}); } }
}
fn normalize_name(s:&str)->String{s.to_ascii_lowercase().replace([' ','_','-'],"")}

pub fn infer_vendor(name:&str)->GpuVendor { let n=name.to_ascii_lowercase(); if n.contains("nvidia")||n.contains("geforce")||n.contains("rtx")||n.contains("gtx"){GpuVendor::Nvidia}else if n.contains("amd")||n.contains("radeon"){GpuVendor::Amd}else if n.contains("intel")||n.contains("arc")||n.contains("uhd")||n.contains("iris"){GpuVendor::Intel}else if n.contains("apple")||n.contains("m1")||n.contains("m2")||n.contains("m3")||n.contains("m4"){GpuVendor::Apple}else{GpuVendor::Unknown}}
