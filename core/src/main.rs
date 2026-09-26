use lyonix_core::catalog::{ModelCatalog, ModelResolver};
use lyonix_core::domain::{ReasoningMode, TaskKind, TaskProfile};
use lyonix_core::hardware::{HardwareProvider, SystemHardwareProvider};
use lyonix_core::runtime::llama_cpp::LlamaCppAdapter;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let models_path = root.join("models.json");
    let catalog = match ModelCatalog::from_legacy_json(&models_path) {
        Ok(c) => c,
        Err(e) => { eprintln!("catalog error: {e}"); std::process::exit(1); }
    };

    let runtime = LlamaCppAdapter::discover(None).ok();
    let devices = runtime.as_ref().and_then(|r| r.list_devices().ok()).unwrap_or_default();
    let hardware = match SystemHardwareProvider.snapshot(devices) {
        Ok(h) => h,
        Err(e) => { eprintln!("hardware error: {e}"); std::process::exit(1); }
    };

    println!("LYONIX-LAB core");
    println!("OS: {}", hardware.os);
    println!("CPU: {} ({} logical / {} physical)", hardware.cpu.model, hardware.cpu.logical_cores, hardware.cpu.physical_cores);
    println!("RAM: {:.1} GiB total / {:.1} GiB available", hardware.total_ram_gib(), hardware.available_ram_gib());
    println!("Tier metadata: {}", hardware.hardware_tier());
    for device in &hardware.runtime_devices {
        println!("Device: {} / {}", device.device_id, device.name);
    }

    let profile = TaskProfile {
        id: "chat".into(), display_name: "Chat / Fast".into(), description: "Fast general assistant".into(), task: TaskKind::Chat,
        target_context: 8192, preferred_reasoning: ReasoningMode::Off, preferred_parallelism: 1,
        low_latency: true, deterministic: true, allow_reasoning: false, cpu_only: false,
    };
    let default_model_root = root.join("models");
    // Discovery uses the existing user-configuration mechanism: model_roots
    // from config.toml when present, the repository ./models dir otherwise.
    let model_roots = match lyonix_core::persistence::effective_model_roots(&default_model_root) {
        Ok(roots) => roots,
        Err(e) => {
            eprintln!("model roots config error: {e}; falling back to default root");
            vec![default_model_root]
        }
    };
    println!("Model roots:");
    for r in &model_roots { println!("  {}", r.display()); }
    let installed = catalog.installed_models(&model_roots);
    for m in &installed {
        println!("Installed artifact: {} -> {}", m.artifact_id, m.local_path.display());
    }
    let candidates = catalog.entries.iter().map(|entry| {
        let i = installed.iter().find(|i| i.artifact_id == entry.artifact.id);
        (&entry.definition, &entry.artifact, i)
    }).collect::<Vec<_>>();
    let recommendation = lyonix_core::application::build_recommendation(&hardware, &candidates, &profile, "llama.cpp");
    if let Some(model) = recommendation.recommended_model.as_ref() {
        let resolver = ModelResolver;
        let _resolved = resolver.resolve(&model.id.0, "llama.cpp", &catalog.entries, &installed);
    }
    println!("Recommended: {}", recommendation.recommended_model.map(|m| m.display_name).unwrap_or_else(|| "none".into()));
}
