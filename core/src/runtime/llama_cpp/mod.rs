use crate::domain::*;
use crate::runtime::supervisor::ProcessSupervisor;
use crate::runtime::{list_devices, probe_version, resolve_executable};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum LlamaCppError {
    #[error("{0}")]
    Message(String),
}

pub struct LlamaCppAdapter {
    pub executable: PathBuf,
}

impl LlamaCppAdapter {
    pub fn discover(configured: Option<&Path>) -> Result<Self, LlamaCppError> {
        Ok(Self { executable: resolve_executable(configured, "llama-server").map_err(|e| LlamaCppError::Message(e.to_string()))? })
    }

    pub fn list_devices(&self) -> Result<Vec<RuntimeDevice>, LlamaCppError> {
        list_devices(&self.executable).map_err(|e| LlamaCppError::Message(e.to_string()))
    }

    pub fn build_launch_plan(&self, model: &InstalledModel, config: &RuntimeConfiguration) -> Result<LaunchPlan, LlamaCppError> {
        if config.network.host != "127.0.0.1" && config.network.host != "localhost" {
            return Err(LlamaCppError::Message("Non-localhost binding is not enabled by the initial core.".into()));
        }
        let mut args = Vec::new();
        args.extend(["-m".into(), model.local_path.display().to_string()]);
        if let Some(mmproj) = &model.mmproj_path { args.extend(["--mmproj".into(), mmproj.display().to_string()]); }
        args.extend(["-c".into(), config.context.to_string()]);
        if matches!(config.gpu_offload, GpuOffloadPolicy::CpuOnly) {
            args.extend(["-ngl".into(), "0".into()]);
        } else {
            args.extend(["-ngl".into(), "auto".into(), "--fit".into(), "on".into()]);
            if let Some(target) = config.memory.fit_target_mib.filter(|v| *v > 0) { args.extend(["--fit-target".into(), target.to_string()]); }
        }
        if let Some(v) = config.batch { args.extend(["-b".into(), v.to_string()]); }
        if let Some(v) = config.micro_batch { args.extend(["-ub".into(), v.to_string()]); }
        args.extend(["--parallel".into(), config.parallelism.to_string(), "--host".into(), config.network.host.clone(), "--port".into(), config.network.port.to_string()]);
        if let Some(device) = &config.device_policy && let Some(runtime_device) = device.strip_prefix("runtime:") { args.extend(["--device".into(), runtime_device.into()]); }
        if let Some(v) = &config.advanced.flash_attention { args.extend(["--flash-attn".into(), v.clone()]); }
        if let Some(v) = &config.advanced.numa { args.extend(["--numa".into(), v.clone()]); }
        if config.advanced.mlock == Some(true) { args.push("--mlock".into()); }
        if config.advanced.no_mmap == Some(true) { args.push("--no-mmap".into()); }
        args.extend(["--cache-type-k".into(), config.kv_cache.key_type.clone(), "--cache-type-v".into(), config.kv_cache.value_type.clone()]);
        if let Some(v) = config.memory.cache_ram_mib { args.extend(["--cache-ram".into(), v.to_string()]); }
        args.extend(["--reasoning".into(), config.reasoning.mode.clone()]);
        if let Some(v) = config.reasoning.budget { args.extend(["--reasoning-budget".into(), v.to_string()]); }
        args.extend([
            "--temp".into(), config.sampling.temperature.to_string(),
            "--top-p".into(), config.sampling.top_p.to_string(),
            "--top-k".into(), config.sampling.top_k.to_string(),
            "--min-p".into(), config.sampling.min_p.to_string(),
            "--repeat-penalty".into(), config.sampling.repeat_penalty.to_string(),
            "--presence-penalty".into(), config.sampling.presence_penalty.to_string(),
        ]);
        validate_raw_args(&config.advanced.raw_runtime_arguments)?;
        args.extend(config.advanced.raw_runtime_arguments.clone());
        Ok(LaunchPlan {
            executable: self.executable.clone(),
            arguments: args,
            endpoint: format!("http://{}:{}/v1", config.network.host, config.network.port),
            model_id: model.model_definition_id.0.clone(),
            artifact_id: model.artifact_id.clone(),
            ownership: RuntimeOwnership::ManagedProcess,
        })
    }

    pub fn validate_config(&self, config: &RuntimeConfiguration) -> Result<(), LlamaCppError> {
        if config.context < 2048 { return Err(LlamaCppError::Message("context must be at least 2048".into())); }
        if config.parallelism == 0 { return Err(LlamaCppError::Message("parallelism must be >= 1".into())); }
        if config.network.port == 0 { return Err(LlamaCppError::Message("port must be non-zero".into())); }
        validate_raw_args(&config.advanced.raw_runtime_arguments)
    }

    pub fn process_supervisor(&self) -> ProcessSupervisor { ProcessSupervisor }
}

impl RuntimeAdapter for LlamaCppAdapter {
    fn id(&self) -> &str { "llama.cpp" }

    fn probe(&self) -> Result<RuntimeProbe, RuntimeAdapterError> {
        Ok(RuntimeProbe {
            id: self.id().into(),
            display_name: "llama.cpp".into(),
            executable: Some(self.executable.clone()),
            version: probe_version(&self.executable).ok(),
            capabilities: self.capabilities(),
        })
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            openai_compatible_api: true,
            streaming: true,
            tools: true,
            vision: true,
            reasoning: true,
            model_download: false,
            model_load_unload: true,
            metrics: true,
            embeddings: true,
            multiple_models: true,
            gpu_offload: true,
        }
    }

    fn discover_models(&self) -> Result<Vec<String>, RuntimeAdapterError> { Ok(Vec::new()) }

    fn validate(&self, model: &ModelDefinition, artifact: &InstalledModel, config: &RuntimeConfiguration) -> Result<(), RuntimeAdapterError> {
        if model.context_limit < config.context { return Err(RuntimeAdapterError::Message(format!("context {} exceeds model limit {}", config.context, model.context_limit))); }
        if !artifact.local_path.extension().and_then(|v| v.to_str()).map(|v| v.eq_ignore_ascii_case("gguf")).unwrap_or(false) {
            return Err(RuntimeAdapterError::Message("llama.cpp artifacts must be GGUF files in the initial adapter".into()));
        }
        self.validate_config(config).map_err(|e| RuntimeAdapterError::Message(e.to_string()))
    }

    fn build_launch_plan(&self, model: &ModelDefinition, artifact: &InstalledModel, config: &RuntimeConfiguration) -> Result<LaunchPlan, RuntimeAdapterError> {
        self.validate(model, artifact, config)?;
        self.build_launch_plan(artifact, config).map_err(|e| RuntimeAdapterError::Message(e.to_string()))
    }

    fn inspect(&self) -> Result<RuntimeStatus, RuntimeAdapterError> {
        Ok(RuntimeStatus { runtime_id: self.id().into(), state: RuntimeState::Unknown, pid: None, endpoint: None, model_id: None, artifact_id: None, effective_configuration: None, devices: self.list_devices().unwrap_or_default(), message: Some("Process state is owned by ProcessSupervisor in this initial slice.".into()) })
    }
}

fn validate_raw_args(args: &[String]) -> Result<(), LlamaCppError> {
    const BLOCKED: &[&str] = &["--host", "--port", "--device", "--ctx-size", "-c", "--n-gpu-layers", "--gpu-layers", "-ngl", "--model", "-m", "--tools"];
    for arg in args {
        let key = arg.split('=').next().unwrap_or(arg);
        if BLOCKED.iter().any(|blocked| blocked.eq_ignore_ascii_case(key)) {
            return Err(LlamaCppError::Message(format!("raw runtime argument '{arg}' would override a managed security/process setting")));
        }
    }
    Ok(())
}
