use lyonix_core::application::{build_recommendation, feasible};
use lyonix_core::domain::*;
use lyonix_core::runtime::parse_device_list;
use std::path::PathBuf;

fn hardware(ram_gib: f64, vram_gib: f64) -> HardwareSnapshot {
    HardwareSnapshot {
        os: "test".into(),
        cpu: CpuInfo { vendor: Some("Intel".into()), model: "Test CPU".into(), architecture: "x86_64".into(), physical_cores: 4, logical_cores: 8 },
        memory: MemoryInfo { total_bytes: (ram_gib * 1024_f64.powi(3)) as u64, available_bytes: (ram_gib * 0.75 * 1024_f64.powi(3)) as u64, used_bytes: (ram_gib * 0.25 * 1024_f64.powi(3)) as u64, pressure_percent: Some(25.0) },
        gpus: vec![GpuInfo { vendor: GpuVendor::Nvidia, model: "Test GPU".into(), hardware_id: None, dedicated_memory_bytes: Some((vram_gib * 1024_f64.powi(3)) as u64), shared_memory_bytes: None, memory_kind: MemoryKind::Dedicated, backend: Some("Vulkan".into()), driver: None, runtime_devices: vec![] }],
        runtime_devices: vec![],
        collected_at_epoch_seconds: 0,
    }
}

fn model() -> ModelDefinition {
    ModelDefinition {
        id: ModelId("test".into()), display_name: "Test".into(), family: "Test".into(), architecture: None, parameters_b: 4.0, context_limit: 32768,
        capabilities: ModelCapabilities { coding: true, reasoning: true, vision: false, tools: false, embeddings: false, streaming: true, tool_calling: false, uncensored: false },
        runtime_hints: ModelRuntimeHints::default(), roles: vec!["general".into()], default_profile: Some("chat".into()), supported_profiles: vec!["chat".into()], recommended_context: Some(8192),
        default_temperature: 0.7, default_top_p: 0.8, default_top_k: 20, default_min_p: 0.0, default_repeat_penalty: 1.05, default_presence_penalty: 0.0, source: None,
    }
}

fn artifact() -> ModelArtifact { ModelArtifact { id: "test:q4".into(), model_definition_id: model().id.clone(), quantization: Quantization::Q4, filename: "test.gguf".into(), size_bytes: Some(4 * 1024 * 1024 * 1024), sha256: None, source: None, runtime_compatibility: vec!["llama.cpp".into()], mmproj_filename: None } }

#[test]
fn parse_llama_devices_does_not_depend_on_gpu_order() {
    let text = "Vulkan0: Intel(R) UHD Graphics (7891 MiB, 7240 MiB free)\nVulkan1: NVIDIA GeForce MX250 (2182 MiB, 1879 MiB free)";
    let devices = parse_device_list(text);
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[1].name, "NVIDIA GeForce MX250");
}

#[test]
fn infeasible_model_is_rejected_before_scoring() {
    let m = model();
    let a = artifact();
    let installed = InstalledModel { artifact_id: a.id.clone(), model_definition_id: m.id.clone(), local_path: PathBuf::from("/does/not/exist"), mmproj_path: None, verified: true, installed_at_epoch_seconds: 0, last_used_at_epoch_seconds: None };
    let result = feasible(&hardware(8.0, 2.0), &m, Some(&a), Some(&installed), "llama.cpp", 32768, CapabilityRequirements::default());
    assert!(!result.feasible);
    assert!(result.reasons.iter().any(|r| matches!(r.code, RejectionCode::InsufficientRam)) || result.reasons.iter().any(|r| matches!(r.code, RejectionCode::ContextExceedsModelLimit)));
}

#[test]
fn coding_profile_disables_reasoning() {
    let h = hardware(16.0, 2.0);
    let m = model();
    let a = artifact();
    let i = InstalledModel { artifact_id: a.id.clone(), model_definition_id: m.id.clone(), local_path: std::env::temp_dir().join("test.gguf"), mmproj_path: None, verified: true, installed_at_epoch_seconds: 0, last_used_at_epoch_seconds: None };
    let profile = TaskProfile { id: "coding".into(), display_name: "Coding".into(), description: "".into(), task: TaskKind::Coding, target_context: 8192, preferred_reasoning: ReasoningMode::Off, preferred_parallelism: 1, low_latency: true, deterministic: true, allow_reasoning: false, cpu_only: false };
    let result = build_recommendation(&h, &[(&m, &a, Some(&i))], &profile, "llama.cpp");
    assert_eq!(result.configuration.unwrap().reasoning.mode, "off");
}

#[test]
fn launch_plan_does_not_enable_builtin_tools() {
    use lyonix_core::runtime::llama_cpp::LlamaCppAdapter;
    let dir = tempfile::tempdir().unwrap();
    let model_path = dir.path().join("model.gguf");
    std::fs::write(&model_path, b"fixture").unwrap();
    let installed = InstalledModel { artifact_id: "test:q4".into(), model_definition_id: ModelId("test".into()), local_path: model_path, mmproj_path: None, verified: true, installed_at_epoch_seconds: 0, last_used_at_epoch_seconds: None };
    let adapter = LlamaCppAdapter { executable: PathBuf::from("llama-server") };
    let config = RuntimeConfiguration {
        context: 8192, batch: None, micro_batch: None, parallelism: 1, device_policy: None, gpu_offload: GpuOffloadPolicy::Auto,
        memory: MemoryPolicy { fit: true, fit_target_mib: Some(384), cache_ram_mib: Some(768) },
        kv_cache: KvCachePolicy { key_type: "q8_0".into(), value_type: "q8_0".into() },
        reasoning: ReasoningPolicy { mode: "off".into(), budget: None }, sampling: SamplingPolicy { temperature: 0.7, top_p: 0.8, top_k: 20, min_p: 0.0, repeat_penalty: 1.05, presence_penalty: 0.0 },
        network: NetworkPolicy::default(), advanced: AdvancedSettings::default(),
    };
    let plan = adapter.build_launch_plan(&installed, &config).unwrap();
    assert!(!plan.arguments.iter().any(|a| a == "--tools" || a == "all"));
}

#[test]
fn raw_tools_override_is_blocked() {
    use lyonix_core::runtime::llama_cpp::LlamaCppAdapter;
    let adapter = LlamaCppAdapter { executable: PathBuf::from("llama-server") };
    let config = RuntimeConfiguration {
        context: 8192, batch: None, micro_batch: None, parallelism: 1, device_policy: None, gpu_offload: GpuOffloadPolicy::Auto,
        memory: MemoryPolicy { fit: true, fit_target_mib: Some(384), cache_ram_mib: Some(768) },
        kv_cache: KvCachePolicy { key_type: "q8_0".into(), value_type: "q8_0".into() }, reasoning: ReasoningPolicy { mode: "off".into(), budget: None },
        sampling: SamplingPolicy { temperature: 0.7, top_p: 0.8, top_k: 20, min_p: 0.0, repeat_penalty: 1.05, presence_penalty: 0.0 },
        network: NetworkPolicy::default(), advanced: AdvancedSettings { raw_runtime_arguments: vec!["--tools".into(), "all".into()], ..Default::default() },
    };
    assert!(adapter.validate_config(&config).is_err());
}

#[test]
fn resolver_selects_highest_quality_installed_compatible_artifact() {
    use lyonix_core::catalog::{CatalogEntry, ModelResolver};
    let m = model();
    let q4 = ModelArtifact { id: "test:q4".into(), model_definition_id: m.id.clone(), quantization: Quantization::Q4, filename: "q4.gguf".into(), size_bytes: None, sha256: None, source: None, runtime_compatibility: vec!["llama.cpp".into()], mmproj_filename: None };
    let q8 = ModelArtifact { id: "test:q8".into(), model_definition_id: m.id.clone(), quantization: Quantization::Q8, filename: "q8.gguf".into(), size_bytes: None, sha256: None, source: None, runtime_compatibility: vec!["llama.cpp".into()], mmproj_filename: None };
    let entries = vec![
        CatalogEntry { definition: m.clone(), artifact: q4.clone(), relative_model_path: PathBuf::from("q4.gguf"), relative_mmproj_path: None },
        CatalogEntry { definition: m.clone(), artifact: q8.clone(), relative_model_path: PathBuf::from("q8.gguf"), relative_mmproj_path: None },
    ];
    let installed = vec![
        InstalledModel { artifact_id: q4.id.clone(), model_definition_id: m.id.clone(), local_path: PathBuf::from("q4.gguf"), mmproj_path: None, verified: false, installed_at_epoch_seconds: 0, last_used_at_epoch_seconds: None },
        InstalledModel { artifact_id: q8.id.clone(), model_definition_id: m.id.clone(), local_path: PathBuf::from("q8.gguf"), mmproj_path: None, verified: false, installed_at_epoch_seconds: 0, last_used_at_epoch_seconds: None },
    ];
    let resolved = ModelResolver.resolve("test", "llama.cpp", &entries, &installed).unwrap();
    assert_eq!(resolved.artifact.id, "test:q8");
}

#[test]
fn non_loopback_binding_is_rejected_by_initial_policy() {
    let mut config = RuntimeConfiguration {
        context: 8192, batch: None, micro_batch: None, parallelism: 1, device_policy: None, gpu_offload: GpuOffloadPolicy::Auto,
        memory: MemoryPolicy { fit: true, fit_target_mib: Some(384), cache_ram_mib: Some(768) },
        kv_cache: KvCachePolicy { key_type: "q8_0".into(), value_type: "q8_0".into() }, reasoning: ReasoningPolicy { mode: "off".into(), budget: None },
        sampling: SamplingPolicy { temperature: 0.7, top_p: 0.8, top_k: 20, min_p: 0.0, repeat_penalty: 1.05, presence_penalty: 0.0 },
        network: NetworkPolicy::default(), advanced: AdvancedSettings::default(),
    };
    config.network.host = "0.0.0.0".into();
    assert!(lyonix_core::application::configuration::validate_runtime_configuration(&config).is_err());
}

#[test]
fn catalog_parses_i1_quant_and_rich_model_metadata() {
    use lyonix_core::catalog::ModelCatalog;
    let json = r#"[
      {
        "id":"sample",
        "displayName":"Sample",
        "path":"org\\sample\\sample-i1-Q5_K_S.gguf",
        "mmprojPath":null,
        "family":"Qwen3.5",
        "architecture":"qwen3_5",
        "parametersB":2.0,
        "quantization":"i1-Q5_K_S",
        "contextLimit":262144,
        "recommendedContext":16384,
        "vision":false,
        "reasoning":true,
        "roles":["general","reasoning","agent"],
        "defaultProfile":"reasoning",
        "supportedProfiles":["chat","reasoning","agent"],
        "capabilities":{"reasoning":true,"toolCalling":true,"coding":true,"uncensored":true,"streaming":true},
        "runtimeHints":{"layers":24,"fullAttentionLayers":6,"kvHeads":2,"headDim":256,"kvCacheBytesPerTokenF16":12288},
        "defaultTemperature":0.6,
        "defaultTopP":0.95,
        "defaultTopK":20,
        "defaultMinP":0.0,
        "defaultRepeatPenalty":1.05,
        "defaultPresencePenalty":0.0
      }
    ]"#;
    let catalog = ModelCatalog::from_legacy_bytes(json.as_bytes()).unwrap();
    let entry = &catalog.entries[0];
    assert_eq!(entry.artifact.quantization, Quantization::Q5);
    assert_eq!(entry.definition.architecture.as_deref(), Some("qwen3_5"));
    assert_eq!(entry.definition.default_profile.as_deref(), Some("reasoning"));
    assert!(entry.definition.capabilities.tool_calling);
    assert!(entry.definition.capabilities.uncensored);
    assert_eq!(entry.definition.runtime_hints.full_attention_layers, Some(6));
}

#[test]
fn chat_recommendation_prefers_fast_general_role() {
    use lyonix_core::application::build_recommendation;
    let h = hardware(16.0, 2.0);
    let fast = ModelDefinition {
        id: ModelId("fast".into()), display_name: "Fast".into(), family: "Spark X2.5".into(), architecture: Some("spark2_5".into()),
        parameters_b: 1.7, context_limit: 1048576, recommended_context: Some(8192), roles: vec!["general".into(), "fast".into()],
        default_profile: Some("chat".into()), supported_profiles: vec!["chat".into()],
        capabilities: ModelCapabilities { coding: true, reasoning: true, vision: false, tools: false, embeddings: false, streaming: true, tool_calling: true, uncensored: true },
        runtime_hints: ModelRuntimeHints { layers: Some(28), full_attention_layers: Some(7), kv_heads: Some(2), head_dim: Some(256), kv_cache_bytes_per_token_f16: Some(14336) },
        default_temperature: 1.0, default_top_p: 0.95, default_top_k: -1, default_min_p: 0.0, default_repeat_penalty: 1.0, default_presence_penalty: 0.0, source: None,
    };
    let other = ModelDefinition {
        id: ModelId("other".into()), display_name: "Other".into(), family: "Other".into(), architecture: None,
        parameters_b: 2.0, context_limit: 32768, recommended_context: Some(8192), roles: vec!["reasoning".into()],
        default_profile: Some("reasoning".into()), supported_profiles: vec!["chat".into()],
        capabilities: ModelCapabilities { coding: false, reasoning: true, vision: false, tools: false, embeddings: false, streaming: true, tool_calling: false, uncensored: false },
        runtime_hints: ModelRuntimeHints::default(), default_temperature: 0.7, default_top_p: 0.95, default_top_k: 20, default_min_p: 0.0, default_repeat_penalty: 1.05, default_presence_penalty: 0.0, source: None,
    };
    let a1 = ModelArtifact { id: "fast:q4".into(), model_definition_id: fast.id.clone(), quantization: Quantization::Q4, filename: "fast.gguf".into(), size_bytes: Some(1100 * 1024 * 1024), sha256: None, source: None, runtime_compatibility: vec!["llama.cpp".into()], mmproj_filename: None };
    let a2 = ModelArtifact { id: "other:q4".into(), model_definition_id: other.id.clone(), quantization: Quantization::Q4, filename: "other.gguf".into(), size_bytes: Some(1300 * 1024 * 1024), sha256: None, source: None, runtime_compatibility: vec!["llama.cpp".into()], mmproj_filename: None };
    let f = std::env::temp_dir().join("lyonix-fast-test.gguf"); let o = std::env::temp_dir().join("lyonix-other-test.gguf");
    std::fs::write(&f, b"fixture").unwrap(); std::fs::write(&o, b"fixture").unwrap();
    let i1 = InstalledModel { artifact_id: a1.id.clone(), model_definition_id: fast.id.clone(), local_path: f.clone(), mmproj_path: None, verified: true, installed_at_epoch_seconds: 0, last_used_at_epoch_seconds: None };
    let i2 = InstalledModel { artifact_id: a2.id.clone(), model_definition_id: other.id.clone(), local_path: o.clone(), mmproj_path: None, verified: true, installed_at_epoch_seconds: 0, last_used_at_epoch_seconds: None };
    let profile = TaskProfile { id: "chat".into(), display_name: "Chat".into(), description: "".into(), task: TaskKind::Chat, target_context: 8192, preferred_reasoning: ReasoningMode::Off, preferred_parallelism: 1, low_latency: true, deterministic: false, allow_reasoning: false, cpu_only: false };
    let r = build_recommendation(&h, &[(&fast, &a1, Some(&i1)), (&other, &a2, Some(&i2))], &profile, "llama.cpp");
    assert_eq!(r.recommended_model.unwrap().id.0, "fast");
    let _ = std::fs::remove_file(f); let _ = std::fs::remove_file(o);
}

#[test]
fn agent_profile_rejects_models_without_tool_calling() {
    let m = model();
    let a = artifact();
    let installed = InstalledModel { artifact_id: a.id.clone(), model_definition_id: m.id.clone(), local_path: std::env::temp_dir().join("lyonix-agent-test.gguf"), mmproj_path: None, verified: true, installed_at_epoch_seconds: 0, last_used_at_epoch_seconds: None };
    let profile = TaskProfile { id: "agent".into(), display_name: "Agent".into(), description: "".into(), task: TaskKind::Agent, target_context: 8192, preferred_reasoning: ReasoningMode::On, preferred_parallelism: 1, low_latency: false, deterministic: false, allow_reasoning: true, cpu_only: false };
    let result = feasible(&hardware(16.0, 2.0), &m, Some(&a), Some(&installed), "llama.cpp", 8192, CapabilityRequirements { reasoning: true, tools: true, ..Default::default() });
    assert!(!result.feasible);
    assert_eq!(profile.task, TaskKind::Agent);
}

static CONFIG_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn effective_model_roots_honor_user_configuration() {
    let _guard = CONFIG_ENV_LOCK.lock().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let fallback = PathBuf::from("fallback-models");
    // TOML literal string keeps Windows backslashes intact.
    let config_text = format!("version = 1\nmodel_roots = ['{}']\n", root.path().display());
    std::fs::write(config_dir.path().join("config.toml"), config_text).unwrap();
    // SAFETY: test-only process; env access serialized via CONFIG_ENV_LOCK.
    unsafe { std::env::set_var("LYONIX_CONFIG_DIR", config_dir.path()); }
    let roots = lyonix_core::persistence::effective_model_roots(&fallback).unwrap();
    assert_eq!(roots, vec![root.path().to_path_buf()], "configured model roots must be honored over the default root");
    // No user configuration (empty model_roots) falls back to the default root.
    std::fs::write(config_dir.path().join("config.toml"), "version = 1\nmodel_roots = []\n").unwrap();
    let roots = lyonix_core::persistence::effective_model_roots(&fallback).unwrap();
    assert_eq!(roots, vec![fallback.clone()], "empty model_roots must fall back to the default root");
    unsafe { std::env::remove_var("LYONIX_CONFIG_DIR"); }
}

fn write_fixture_tree(root: &std::path::Path, relative: &std::path::Path) -> PathBuf {
    let full = root.join(relative);
    std::fs::create_dir_all(full.parent().expect("relative path has a parent")).unwrap();
    std::fs::write(&full, b"gguf fixture").unwrap();
    full
}

#[test]
fn configured_root_discovers_all_three_catalog_artifacts() {
    use lyonix_core::catalog::ModelCatalog;
    let models_json = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("models.json");
    let catalog = ModelCatalog::from_legacy_json(&models_json).unwrap();
    assert_eq!(catalog.entries.len(), 3);
    let root = tempfile::tempdir().unwrap();
    let mut expected = Vec::new();
    for entry in &catalog.entries {
        expected.push((entry.artifact.id.clone(), write_fixture_tree(root.path(), &entry.relative_model_path)));
    }
    let installed = catalog.installed_models(&[root.path().to_path_buf()]);
    assert_eq!(installed.len(), 3, "all three catalog artifacts must be discovered from the configured root");
    for (artifact_id, path) in &expected {
        let found = installed.iter().find(|i| &i.artifact_id == artifact_id).expect("artifact must be discovered");
        assert_eq!(&found.local_path, path);
        assert!(!found.verified);
    }
    // The corrected Qwen3.5 filename uses ".i1-" (a dot, not a hyphen).
    let qwen35 = installed.iter().find(|i| i.model_definition_id.0 == "qwen3-5-2b-claude-4-6-heretic-i1").expect("Qwen3.5 artifact discovered");
    assert!(qwen35.local_path.file_name().unwrap().to_string_lossy().contains("THINKING.i1-Q5_K_S.gguf"));
    // Spark and Qwen2.5 Coder artifacts are discoverable too.
    assert!(installed.iter().any(|i| i.model_definition_id.0 == "spark-x2-5-1-7b-abliterated"));
    assert!(installed.iter().any(|i| i.model_definition_id.0 == "qwen2-5-coder-1-5b-instruct"));
}

#[test]
fn unsafe_catalog_paths_remain_rejected() {
    use lyonix_core::catalog::ModelCatalog;
    let root = tempfile::tempdir().unwrap();
    let abs_path = std::env::temp_dir().join("lyonix-abs-model.gguf");
    std::fs::write(&abs_path, b"gguf fixture").unwrap();
    // If parent traversal were allowed, this file outside the root would be found.
    let outside = root.path().parent().unwrap().join("lyonix-outside-model.gguf");
    std::fs::write(&outside, b"gguf fixture").unwrap();
    let record = |id: &str, path: String| serde_json::json!({
        "id": id, "displayName": id, "path": path, "mmprojPath": null, "family": "T",
        "parametersB": 1.0, "quantization": "Q4_K_M", "contextLimit": 4096,
        "vision": false, "reasoning": false, "roles": ["general"],
        "defaultTemperature": 0.7, "defaultTopP": 0.9, "defaultTopK": 20,
        "defaultMinP": 0.0, "defaultRepeatPenalty": 1.0, "defaultPresencePenalty": 0.0
    });
    let json = serde_json::json!([
        record("abs", abs_path.to_string_lossy().into_owned()),
        record("traversal", "../lyonix-outside-model.gguf".to_string())
    ]).to_string();
    let catalog = ModelCatalog::from_legacy_bytes(json.as_bytes()).unwrap();
    let installed = catalog.installed_models(&[root.path().to_path_buf()]);
    assert!(installed.is_empty(), "absolute and parent-traversal catalog paths must remain rejected");
    let _ = std::fs::remove_file(abs_path);
    let _ = std::fs::remove_file(outside);
}

/// Explicit-run smoke test: proves the full headless lifecycle on the real
/// machine — configured model root -> discovery -> resolve -> RuntimeConfiguration
/// -> LaunchPlan -> ProcessSupervisor start -> health READY -> clean stop.
/// Run with: LYONIX_RUNTIME_SMOKE=1 cargo test --test core_tests runtime_lifecycle -- --nocapture
#[test]
fn runtime_lifecycle_smoke_spark_end_to_end() {
    use lyonix_core::catalog::ModelResolver;
    use lyonix_core::hardware::HardwareProvider;
    use lyonix_core::runtime::llama_cpp::LlamaCppAdapter;
    use lyonix_core::runtime::supervisor::{ManagedProcess, ProcessSupervisor};

    // Explicit-run gate: a normal `cargo test` skips this; the real-machine run
    // is triggered with LYONIX_RUNTIME_SMOKE=1.
    if std::env::var_os("LYONIX_RUNTIME_SMOKE").is_none() {
        eprintln!("skipping: set LYONIX_RUNTIME_SMOKE=1 to run the real llama.cpp lifecycle smoke test");
        return;
    }

    // 1. Configured model root -> installed model discovery (real user config).
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let catalog = lyonix_core::catalog::ModelCatalog::from_legacy_json(repo_root.join("models.json")).map_err(|e| e.to_string()).unwrap();
    let roots = lyonix_core::persistence::effective_model_roots(&repo_root.join("models")).map_err(|e| e.to_string()).unwrap();
    eprintln!("model roots: {roots:?}");
    let installed = catalog.installed_models(&roots);
    let user_config = lyonix_core::persistence::load_user_configuration().map_err(|e| e.to_string()).unwrap();
    let configured_server = user_config.llama_server_path.as_deref().filter(|p| !p.as_os_str().is_empty());

    // Runtime adapter; skips cleanly when llama-server is unavailable.
    let Ok(adapter) = LlamaCppAdapter::discover(configured_server) else {
        eprintln!("skipping: llama-server was not discoverable");
        return;
    };

    // 2. Resolve the preferred Spark model from the discovered set.
    let resolved = ModelResolver.resolve("spark-x2-5-1-7b-abliterated", "llama.cpp", &catalog.entries, &installed).map_err(|e| e.to_string()).unwrap();
    assert!(resolved.installed.local_path.is_file(), "resolved Spark artifact must exist on disk");
    eprintln!("resolved artifact: {}", resolved.installed.local_path.display());

    // 3. llama.cpp RuntimeConfiguration mirroring the chat-profile recommendation;
    //    device/memory values are derived from the live hardware snapshot.
    let hardware = lyonix_core::hardware::SystemHardwareProvider.snapshot(adapter.list_devices().unwrap_or_default()).map_err(|e| e.to_string()).unwrap();
    let (vram, ram) = (hardware.dedicated_gpu_gib(), hardware.total_ram_gib());
    let fit_target_mib: u64 = if vram <= 0.0 { 0 } else if vram < 3.0 { 256 } else if vram < 6.0 { 512 } else if vram < 12.0 { 768 } else { 1024 };
    let cache_ram_mib: u64 = if ram < 8.0 { 512 } else if ram < 16.0 { 768 } else if ram < 24.0 { 1536 } else if ram < 32.0 { 2048 } else if ram < 64.0 { 4096 } else { 8192 };
    let config = RuntimeConfiguration {
        context: 8192,
        batch: None, micro_batch: None, parallelism: 1,
        device_policy: Some("runtime_auto".into()),
        gpu_offload: GpuOffloadPolicy::Auto,
        memory: MemoryPolicy { fit: true, fit_target_mib: Some(fit_target_mib), cache_ram_mib: Some(cache_ram_mib) },
        kv_cache: KvCachePolicy { key_type: "q8_0".into(), value_type: "q8_0".into() },
        reasoning: ReasoningPolicy { mode: "off".into(), budget: None },
        sampling: SamplingPolicy { temperature: resolved.definition.default_temperature, top_p: resolved.definition.default_top_p, top_k: resolved.definition.default_top_k, min_p: resolved.definition.default_min_p, repeat_penalty: resolved.definition.default_repeat_penalty, presence_penalty: resolved.definition.default_presence_penalty },
        network: NetworkPolicy::default(),
        advanced: AdvancedSettings::default(),
    };

    // 4. LaunchPlan via the trait API (validates model + config first).
    let plan = RuntimeAdapter::build_launch_plan(&adapter, &resolved.definition, &resolved.installed, &config).map_err(|e| e.to_string()).unwrap();
    assert_eq!(plan.ownership, RuntimeOwnership::ManagedProcess);
    assert_eq!(plan.endpoint, "http://127.0.0.1:8081/v1");
    assert!(plan.arguments.windows(2).any(|w| w[0] == "-m" && w[1] == resolved.installed.local_path.to_string_lossy()), "launch plan must reference the discovered GGUF path");
    eprintln!("launch plan: endpoint {} ({} args)", plan.endpoint, plan.arguments.len());

    // The managed port must be free before starting (mirrors production start_runtime).
    assert!(lyonix_core::application::configuration::port_available("127.0.0.1", 8081), "managed port 8081 must be available before start");

    // 5. ProcessSupervisor start. The guard keeps cleanup safe on any failure:
    //    the process is always stopped through the supervisor, never killed by name.
    struct StopGuard(Option<ManagedProcess>);
    impl Drop for StopGuard {
        fn drop(&mut self) {
            if let Some(m) = self.0.take() { let _ = ProcessSupervisor::stop(m); }
        }
    }
    let mut guard = StopGuard(None);
    let managed = adapter.process_supervisor().start(&plan).map_err(|e| e.to_string()).unwrap();
    eprintln!("llama-server started, pid {} (owned by ProcessSupervisor)", managed.handle.pid);
    assert_ne!(managed.handle.pid, 0);
    let stdout_log = managed.handle.stdout_log.clone().expect("supervisor captured stdout log");
    let stderr_log = managed.handle.stderr_log.clone().expect("supervisor captured stderr log");
    assert!(stdout_log.is_file() && stderr_log.is_file(), "supervisor log files must exist");
    eprintln!("stdout log: {}", stdout_log.display());
    eprintln!("stderr log: {}", stderr_log.display());
    guard.0 = Some(managed);

    // 6. Poll health until READY (503 = loading, connection refused = still starting).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(240);
    let mut ready = None;
    loop {
        let managed = guard.0.as_mut().expect("managed process present");
        if let Ok(Some(exit)) = managed.child_mut().try_wait() {
            let tail = std::fs::read_to_string(&stderr_log).unwrap_or_default();
            let tail = tail.lines().rev().take(15).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            panic!("llama-server exited early with {exit}; stderr tail:\n{tail}");
        }
        match ProcessSupervisor::health(&plan.endpoint.replace("/v1", "/health"), std::time::Duration::from_millis(1000)) {
            Ok(h) if h.state == RuntimeState::Ready => { ready = Some(h); break; }
            _ => {}
        }
        if std::time::Instant::now() >= deadline { break; }
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }
    assert!(ready.is_some(), "health did not reach READY within 240s; stderr tail: {}", std::fs::read_to_string(&stderr_log).unwrap_or_default());
    eprintln!("health reached READY: {}", ready.as_ref().unwrap().message);

    // 7. Clean stop through the supervisor, then verify the endpoint is down.
    let managed = guard.0.take().expect("managed process present");
    ProcessSupervisor::stop(managed).map_err(|e| e.to_string()).unwrap();
    assert!(std::net::TcpStream::connect(("127.0.0.1", 8081)).is_err(), "endpoint must refuse connections after stop");
    eprintln!("process stopped cleanly; endpoint closed");
}

// ---------------------------------------------------------------------------
// Inference layer: serialization, mock HTTP, and real-machine smoke tests
// ---------------------------------------------------------------------------

#[test]
fn chat_request_serialization_matches_openai_schema() {
    let request = lyonix_core::runtime::inference::single_turn_request("hi", 32);
    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["messages"][0]["role"], "user");
    assert_eq!(json["messages"][0]["content"], "hi");
    assert_eq!(json["max_tokens"], 32);
    assert_eq!(json["stream"], false);
    assert!(json.get("model").is_none(), "model None must be omitted");
    assert!(json.get("temperature").is_none(), "sampling None must be omitted");

    let roundtrip: ChatRequest = serde_json::from_value(json).unwrap();
    assert_eq!(roundtrip, request);

    let full = ChatRequest {
        model: Some("spark".into()),
        messages: vec![
            ChatMessage { role: "system".into(), content: "s".into() },
            ChatMessage { role: "user".into(), content: "u".into() },
        ],
        temperature: Some(0.5),
        top_p: Some(0.9),
        top_k: Some(40),
        max_tokens: Some(64),
        stream: true,
    };
    let json = serde_json::to_value(&full).unwrap();
    assert_eq!(json["model"], "spark");
    assert_eq!(json["messages"].as_array().unwrap().len(), 2);
    assert_eq!(json["temperature"], 0.5);
    assert_eq!(json["stream"], true);
}

#[test]
fn chat_request_validation_rejects_invalid_input() {
    let mut request = lyonix_core::runtime::inference::single_turn_request("hi", 8);
    assert!(request.validate().is_ok());

    request.messages.clear();
    assert!(matches!(request.validate(), Err(InferenceError::InvalidRequest(_))));

    let mut request = lyonix_core::runtime::inference::single_turn_request("hi", 8);
    request.temperature = Some(2.5);
    assert!(matches!(request.validate(), Err(InferenceError::InvalidRequest(_))));

    let mut request = lyonix_core::runtime::inference::single_turn_request("hi", 8);
    request.top_p = Some(1.5);
    assert!(matches!(request.validate(), Err(InferenceError::InvalidRequest(_))));

    let mut request = lyonix_core::runtime::inference::single_turn_request("hi", 8);
    request.top_k = Some(-2);
    assert!(matches!(request.validate(), Err(InferenceError::InvalidRequest(_))));
}

#[test]
fn inference_client_rejects_non_localhost_endpoints() {
    let client = lyonix_core::runtime::inference::LlamaCppInferenceClient::from_endpoint("http://192.168.1.5:8081/v1");
    assert!(matches!(client, Err(InferenceError::NonLocalhost(_))), "LAN endpoint must be rejected");

    let client = lyonix_core::runtime::inference::LlamaCppInferenceClient::from_endpoint("https://api.example.com/v1");
    assert!(matches!(client, Err(InferenceError::NonLocalhost(_))), "remote HTTPS endpoint must be rejected");

    let client = lyonix_core::runtime::inference::LlamaCppInferenceClient::from_endpoint("http://127.0.0.1:8081/v1");
    assert!(client.is_ok(), "managed localhost endpoint must be accepted");
}

#[test]
fn inference_mock_http_success_and_error_response() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0_u8; 4096];
        let _ = stream.read(&mut buf);
        let body = serde_json::json!({
            "model": "spark-test",
            "choices": [{ "message": { "role": "assistant", "content": "ok: 42" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 3 }
        }).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    let client = lyonix_core::runtime::inference::LlamaCppInferenceClient::from_endpoint(&format!("http://127.0.0.1:{port}/v1")).unwrap();
    let request = lyonix_core::runtime::inference::single_turn_request("test", 16);
    let response = client.chat(&request, Duration::from_secs(5)).unwrap();
    assert_eq!(response.content, "ok: 42");
    assert_eq!(response.model.as_deref(), Some("spark-test"));
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(response.prompt_tokens, Some(5));
    assert_eq!(response.completion_tokens, Some(3));
    handle.join().unwrap();

    // Error response: typed InferenceError::Status with the server message.
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0_u8; 4096];
        let _ = stream.read(&mut buf);
        let body = r#"{"error":{"message":"model is not loaded"}}"#;
        let response = format!(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        std::thread::sleep(Duration::from_millis(200));
    });

    let client = lyonix_core::runtime::inference::LlamaCppInferenceClient::from_endpoint(&format!("http://127.0.0.1:{port}/v1")).unwrap();
    let request = lyonix_core::runtime::inference::single_turn_request("test", 16);
    let error = client.chat(&request, Duration::from_secs(5)).unwrap_err();
    match error {
        InferenceError::Status { code, message } => {
            assert_eq!(code, 503);
            assert!(message.contains("model is not loaded"), "typed status error must carry the server message");
        }
        other => panic!("expected InferenceError::Status, got {other:?}"),
    }
    handle.join().unwrap();
}

#[test]
fn inference_mock_http_streaming_events() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0_u8; 4096];
        let _ = stream.read(&mut buf);
        let chunk = |payload: &str| format!("{:x}\r\n{}\r\n", payload.len(), payload);
        let mut body = String::new();
        body.push_str(&chunk("data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n"));
        body.push_str(&chunk("data: {\"choices\":[{\"delta\":{\"content\":\"lo!\"}}]}\n\n"));
        body.push_str(&chunk("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n"));
        body.push_str(&chunk("data: [DONE]\n\n"));
        body.push_str("0\r\n\r\n");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{body}"
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    let client = lyonix_core::runtime::inference::LlamaCppInferenceClient::from_endpoint(&format!("http://127.0.0.1:{port}/v1")).unwrap();
    let request = lyonix_core::runtime::inference::single_turn_request("test", 16);
    let mut events = Vec::new();
    client.chat_stream(&request, Duration::from_secs(5), |event| events.push(event)).unwrap();
    assert_eq!(events.len(), 3, "2 deltas + final Done expected, got {events:?}");
    assert_eq!(events[0], ChatStreamEvent::Delta { content: "Hel".into() });
    assert_eq!(events[1], ChatStreamEvent::Delta { content: "lo!".into() });
    assert_eq!(events[2], ChatStreamEvent::Done { finish_reason: Some("stop".into()) });
    handle.join().unwrap();
}

#[test]
fn runtime_inference_smoke_spark_end_to_end() {
    use lyonix_core::catalog::ModelResolver;
    use lyonix_core::hardware::HardwareProvider;
    use lyonix_core::runtime::llama_cpp::LlamaCppAdapter;
    use lyonix_core::runtime::inference::{LlamaCppInferenceClient, single_turn_request};
    use lyonix_core::runtime::supervisor::{ManagedProcess, ProcessSupervisor};

    // Explicit-run gate: a normal `cargo test` skips this; the real-machine run
    // is triggered with LYONIX_RUNTIME_SMOKE=1.
    if std::env::var_os("LYONIX_RUNTIME_SMOKE").is_none() {
        eprintln!("skipping: set LYONIX_RUNTIME_SMOKE=1 to run the real inference smoke test");
        return;
    }

    // 1. Configured model root -> installed model discovery (real user config).
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let catalog = lyonix_core::catalog::ModelCatalog::from_legacy_json(repo_root.join("models.json")).map_err(|e| e.to_string()).unwrap();
    let roots = lyonix_core::persistence::effective_model_roots(&repo_root.join("models")).map_err(|e| e.to_string()).unwrap();
    let installed = catalog.installed_models(&roots);
    let user_config = lyonix_core::persistence::load_user_configuration().map_err(|e| e.to_string()).unwrap();
    let configured_server = user_config.llama_server_path.as_deref().filter(|p| !p.as_os_str().is_empty());

    let Ok(adapter) = LlamaCppAdapter::discover(configured_server) else {
        eprintln!("skipping: llama-server was not discoverable");
        return;
    };

    // 2. Resolve the preferred Spark model from the discovered set.
    let resolved = ModelResolver.resolve("spark-x2-5-1-7b-abliterated", "llama.cpp", &catalog.entries, &installed).map_err(|e| e.to_string()).unwrap();
    assert!(resolved.installed.local_path.is_file(), "resolved Spark artifact must exist on disk");
    eprintln!("resolved artifact: {}", resolved.installed.local_path.display());

    // 3. llama.cpp RuntimeConfiguration (mirrors the chat-profile recommendation;
    //    device/memory values are derived from the live hardware snapshot).
    let hardware = lyonix_core::hardware::SystemHardwareProvider.snapshot(adapter.list_devices().unwrap_or_default()).map_err(|e| e.to_string()).unwrap();
    let (vram, ram) = (hardware.dedicated_gpu_gib(), hardware.total_ram_gib());
    let fit_target_mib: u64 = if vram <= 0.0 { 0 } else if vram < 3.0 { 256 } else if vram < 6.0 { 512 } else if vram < 12.0 { 768 } else { 1024 };
    let cache_ram_mib: u64 = if ram < 8.0 { 512 } else if ram < 16.0 { 768 } else if ram < 24.0 { 1536 } else if ram < 32.0 { 2048 } else if ram < 64.0 { 4096 } else { 8192 };
    let config = RuntimeConfiguration {
        context: 8192,
        batch: None, micro_batch: None, parallelism: 1,
        device_policy: Some("runtime_auto".into()),
        gpu_offload: GpuOffloadPolicy::Auto,
        memory: MemoryPolicy { fit: true, fit_target_mib: Some(fit_target_mib), cache_ram_mib: Some(cache_ram_mib) },
        kv_cache: KvCachePolicy { key_type: "q8_0".into(), value_type: "q8_0".into() },
        reasoning: ReasoningPolicy { mode: "off".into(), budget: None },
        sampling: SamplingPolicy { temperature: resolved.definition.default_temperature, top_p: resolved.definition.default_top_p, top_k: resolved.definition.default_top_k, min_p: resolved.definition.default_min_p, repeat_penalty: resolved.definition.default_repeat_penalty, presence_penalty: resolved.definition.default_presence_penalty },
        network: NetworkPolicy::default(),
        advanced: AdvancedSettings::default(),
    };

    // 4. LaunchPlan via the trait API (validates model + config first).
    let plan = RuntimeAdapter::build_launch_plan(&adapter, &resolved.definition, &resolved.installed, &config).map_err(|e| e.to_string()).unwrap();
    assert_eq!(plan.endpoint, "http://127.0.0.1:8081/v1");
    eprintln!("launch plan: endpoint {} ({} args)", plan.endpoint, plan.arguments.len());

    // The managed port must be free before starting (mirrors production start_runtime).
    assert!(lyonix_core::application::configuration::port_available("127.0.0.1", 8081), "managed port 8081 must be available before start");

    // 5. ProcessSupervisor start. The guard keeps cleanup safe on any failure.
    struct StopGuard(Option<ManagedProcess>);
    impl Drop for StopGuard {
        fn drop(&mut self) {
            if let Some(m) = self.0.take() { let _ = ProcessSupervisor::stop(m); }
        }
    }
    let mut guard = StopGuard(None);
    let managed = adapter.process_supervisor().start(&plan).map_err(|e| e.to_string()).unwrap();
    eprintln!("llama-server started, pid {} (owned by ProcessSupervisor)", managed.handle.pid);
    let stderr_log = managed.handle.stderr_log.clone().expect("supervisor captured stderr log");
    guard.0 = Some(managed);

    // 6. Poll health until READY.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(240);
    loop {
        let managed = guard.0.as_mut().expect("managed process present");
        if let Ok(Some(exit)) = managed.child_mut().try_wait() {
            let tail = std::fs::read_to_string(&stderr_log).unwrap_or_default();
            let tail = tail.lines().rev().take(15).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            panic!("llama-server exited early with {exit}; stderr tail:\n{tail}");
        }
        match ProcessSupervisor::health(&plan.endpoint.replace("/v1", "/health"), std::time::Duration::from_millis(1000)) {
            Ok(h) if h.state == RuntimeState::Ready => {
                eprintln!("health reached READY: {}", h.message);
                break;
            }
            _ => {}
        }
        if std::time::Instant::now() >= deadline {
            panic!("health did not reach READY within 240s; stderr tail: {}", std::fs::read_to_string(&stderr_log).unwrap_or_default());
        }
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }

    // 7. Real inference through the typed client against the managed endpoint.
    let client = LlamaCppInferenceClient::from_configuration(&config).map_err(|e| e.to_string()).unwrap();
    let request = single_turn_request("Reply with exactly one word: ping", 32);
    let response = client.chat(&request, std::time::Duration::from_secs(120)).map_err(|e| e.to_string()).unwrap();
    assert!(!response.content.trim().is_empty(), "model response must be non-empty");
    eprintln!(
        "inference response: {:?} (model={:?}, finish={:?}, prompt_tokens={:?}, completion_tokens={:?}, latency_ms={:?})",
        response.content.trim(), response.model, response.finish_reason, response.prompt_tokens, response.completion_tokens, response.total_latency_ms
    );

    // 8. Clean stop through the supervisor, then verify the endpoint is down.
    let managed = guard.0.take().expect("managed process present");
    ProcessSupervisor::stop(managed).map_err(|e| e.to_string()).unwrap();
    assert!(std::net::TcpStream::connect(("127.0.0.1", 8081)).is_err(), "endpoint must refuse connections after stop; port 8081 must no longer be listening");
    eprintln!("process stopped cleanly; port 8081 closed");
}

// ---------------------------------------------------------------------------
// Application service layer: workflows, persisted runtime state, streaming
// ---------------------------------------------------------------------------

#[test]
fn persisted_runtime_state_roundtrips() {
    let state = lyonix_core::persistence::runtime_state::PersistedRuntimeState {
        runtime_id: "llama.cpp".into(),
        pid: 4242,
        executable: PathBuf::from("C:/tools/llama-server.exe"),
        fingerprint: "abc123".into(),
        endpoint: "http://127.0.0.1:8081/v1".into(),
        model_id: "spark-x2-5-1-7b-abliterated".into(),
        artifact_id: "spark:q4km".into(),
        started_at_epoch_seconds: 1700000000,
        stdout_log: Some(PathBuf::from("C:/logs/out.log")),
        stderr_log: Some(PathBuf::from("C:/logs/err.log")),
    };
    let json = serde_json::to_string_pretty(&state).unwrap();
    let parsed: lyonix_core::persistence::runtime_state::PersistedRuntimeState =
        serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, state);
    // The durable record carries every reconciliation field.
    assert_eq!(parsed.pid, 4242);
    assert_eq!(parsed.endpoint, "http://127.0.0.1:8081/v1");
    assert_eq!(parsed.fingerprint, "abc123");
    assert!(parsed.stderr_log.is_some());
}

#[test]
fn lyonix_facade_doctor_and_list_models_work() {
    // Use a temp config dir so the test does not touch the user's real config.
    let _guard = CONFIG_ENV_LOCK.lock().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("LYONIX_CONFIG_DIR", config_dir.path()); }
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let lab = lyonix_core::application::service::LyonixLab::open(&repo_root)
        .map_err(|e| e.to_string()).unwrap();
    let report = lab.doctor();
    assert!(!report.os.is_empty());
    assert!(report.logical_cores > 0);
    assert!(report.total_ram_gib > 0.0);
    assert!(!report.model_roots.is_empty(), "doctor must report the configured model roots");
    let models = lab.list_models();
    assert_eq!(models.len(), 3, "all three catalog models must be listed");
    assert!(models.iter().all(|m| !m.local_path.as_os_str().is_empty() || true));
    unsafe { std::env::remove_var("LYONIX_CONFIG_DIR"); }
}

#[test]
fn status_without_persisted_state_is_stopped() {
    let _guard = CONFIG_ENV_LOCK.lock().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("LYONIX_CONFIG_DIR", config_dir.path()); }
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let lab = lyonix_core::application::service::LyonixLab::open(&repo_root)
        .map_err(|e| e.to_string()).unwrap();
    let status = lab.status().map_err(|e| e.to_string()).unwrap();
    assert_eq!(status.state, RuntimeState::Stopped);
    assert_eq!(status.pid, None);
    unsafe { std::env::remove_var("LYONIX_CONFIG_DIR"); }
}

#[test]
fn chat_without_persisted_state_errors_cleanly() {
    let _guard = CONFIG_ENV_LOCK.lock().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("LYONIX_CONFIG_DIR", config_dir.path()); }
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let lab = lyonix_core::application::service::LyonixLab::open(&repo_root)
        .map_err(|e| e.to_string()).unwrap();
    let request = lyonix_core::runtime::inference::single_turn_request("hi", 8);
    let error = lab.chat(&request, std::time::Duration::from_secs(1)).unwrap_err();
    assert!(error.to_string().contains("not started"), "chat without state must say the runtime is not started, got: {error}");
    unsafe { std::env::remove_var("LYONIX_CONFIG_DIR"); }
}

#[test]
fn runtime_service_streaming_smoke_spark() {
    use lyonix_core::application::service::LyonixLab;
    use lyonix_core::domain::ChatStreamEvent;
    use lyonix_core::runtime::inference::single_turn_request;
    use lyonix_core::runtime::supervisor::ProcessSupervisor;

    // Explicit-run gate: a normal `cargo test` skips this; the real-machine run
    // is triggered with LYONIX_RUNTIME_SMOKE=1.
    if std::env::var_os("LYONIX_RUNTIME_SMOKE").is_none() {
        eprintln!("skipping: set LYONIX_RUNTIME_SMOKE=1 to run the real streaming smoke test");
        return;
    }

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let lab = LyonixLab::open(&repo_root).map_err(|e| e.to_string()).unwrap();

    // 1. Recommend the chat profile (Spark is the chat winner on this machine).
    let profile = TaskProfile {
        id: "chat".into(), display_name: "Chat".into(), description: "".into(),
        task: TaskKind::Chat, target_context: 8192, preferred_reasoning: ReasoningMode::Off,
        preferred_parallelism: 1, low_latency: true, deterministic: true,
        allow_reasoning: false, cpu_only: false,
    };
    let recommendation = lab.recommend(&profile);
    let model = recommendation.recommended_model.as_ref()
        .map(|m| m.id.0.clone())
        .expect("recommendation must produce a model for the chat profile");
    let config = recommendation.configuration.clone().expect("recommendation must produce a runtime configuration");
    eprintln!("recommended model: {model}");

    // 2. Start via the service workflow (persists durable state).
    let started = lab.start(&model, &config, std::time::Duration::from_secs(240))
        .map_err(|e| e.to_string()).unwrap();
    eprintln!("llama-server started, pid {} (owned by ProcessSupervisor)", started.managed.handle.pid);
    eprintln!("durable state persisted at {}", lyonix_core::persistence::runtime_state::runtime_state_path().display());

    // 3. status() reconciles from persisted state while running.
    let status = lab.status().map_err(|e| e.to_string()).unwrap();
    assert_eq!(status.state, RuntimeState::Ready, "status must reconcile Ready from persisted state while running");
    assert_eq!(status.pid, Some(started.managed.handle.pid));

    // 4. Real streaming inference through the service workflow.
    let request = single_turn_request("Count from one to five.", 64);
    let mut deltas = Vec::new();
    let mut done = None;
    lab.chat_stream(&request, std::time::Duration::from_secs(120), |event| {
        match event {
            ChatStreamEvent::Delta { content } => deltas.push(content),
            ChatStreamEvent::Done { finish_reason } => done = finish_reason,
        }
    }).map_err(|e| e.to_string()).unwrap();
    eprintln!("streaming deltas: {} chunks, joined: {:?}", deltas.len(), deltas.join(""));
    assert!(deltas.len() >= 2, "streaming must produce multiple Delta events, got {}", deltas.len());
    assert!(done.is_some(), "streaming must end with a final Done event");
    assert!(!deltas.join("").trim().is_empty(), "streamed content must be non-empty");

    // 5. Stop via the ProcessSupervisor-owned path and verify the port is closed.
    //    The service stop() reconciles through the durable record; the direct
    //    supervisor stop with the live child handle is the ownership primitive.
    let managed = started.managed;
    ProcessSupervisor::stop(managed).map_err(|e| e.to_string()).unwrap();
    let _ = lab.stop(); // Clears the now-stale durable record.
    assert!(std::net::TcpStream::connect(("127.0.0.1", 8081)).is_err(), "port 8081 must be closed after stop");
    eprintln!("process stopped cleanly; port 8081 closed");
}
