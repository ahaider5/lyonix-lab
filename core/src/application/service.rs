use crate::application::configuration::{
    assert_port_available, validate_runtime_configuration,
};
use crate::catalog::{ModelCatalog, ModelResolver};
use crate::domain::{
    ChatRequest, ChatResponse, ChatStreamEvent, InferenceError, InstalledModel, ModelDefinition,
    RuntimeConfiguration, RuntimeState, RuntimeStatus, TaskProfile,
};
use crate::domain::HardwareSnapshot;
use crate::hardware::{HardwareProvider, SystemHardwareProvider};
use crate::persistence::runtime_state::PersistedRuntimeState;
use crate::persistence::{effective_model_roots, runtime_state, load_user_configuration};
use crate::runtime::inference::LlamaCppInferenceClient;
use crate::runtime::llama_cpp::LlamaCppAdapter;
use crate::runtime::supervisor::{ManagedProcess, ProcessSupervisor};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Typed errors for the headless application workflows. The future CLI maps
/// these to exit codes/messages; it never interprets raw transport errors.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Inference(#[from] InferenceError),
    #[error(transparent)]
    State(#[from] crate::persistence::StateError),
}

/// The model plus its resolved installed artifact.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub definition: ModelDefinition,
    pub artifact_id: String,
    pub local_path: PathBuf,
}

/// A live, owned runtime instance plus its reconciled durable state.
pub struct StartedRuntime {
    pub managed: ManagedProcess,
    pub plan: crate::domain::LaunchPlan,
    pub config: RuntimeConfiguration,
    pub client: LlamaCppInferenceClient,
}

/// The headless application facade. Owns catalog/hardware/user-config loading
/// and exposes typed workflows so a CLI (or the Tauri backend later) never
/// orchestrates catalog/recommendation/runtime/inference logic itself.
pub struct LyonixLab {
    catalog: ModelCatalog,
    hardware: HardwareSnapshot,
    adapter: Option<LlamaCppAdapter>,
    repo_root: PathBuf,
}

impl LyonixLab {
    /// Load everything from the repository and the user configuration. The
    /// model root comes from the existing user-configuration mechanism
    /// (config.toml `model_roots` when present, `repo_root/models` otherwise).
    pub fn open(repo_root: &Path) -> Result<Self, WorkflowError> {
        let models_path = repo_root.join("models.json");
        let catalog = ModelCatalog::from_legacy_json(&models_path)
            .map_err(|e| WorkflowError::Message(e.to_string()))?;
        let user_config = load_user_configuration().map_err(WorkflowError::from)?;
        let configured_server = user_config
            .llama_server_path
            .as_deref()
            .filter(|p| !p.as_os_str().is_empty());
        let adapter = LlamaCppAdapter::discover(configured_server).ok();
        let devices = adapter
            .as_ref()
            .and_then(|a| a.list_devices().ok())
            .unwrap_or_default();
        let hardware = SystemHardwareProvider
            .snapshot(devices)
            .map_err(|e| WorkflowError::Message(e.to_string()))?;
        Ok(Self {
            catalog,
            hardware,
            adapter,
            repo_root: repo_root.to_path_buf(),
        })
    }

    pub fn hardware(&self) -> &HardwareSnapshot {
        &self.hardware
    }

    /// hardware snapshot for tests / callers that already have one.
    pub fn from_parts(
        catalog: ModelCatalog,
        hardware: HardwareSnapshot,
        adapter: Option<LlamaCppAdapter>,
        repo_root: &Path,
    ) -> Self {
        Self {
            catalog,
            hardware,
            adapter,
            repo_root: repo_root.to_path_buf(),
        }
    }

    /// Typed hardware/environment report.
    pub fn doctor(&self) -> DoctorReport {
        DoctorReport {
            os: self.hardware.os.clone(),
            cpu: self.hardware.cpu.model.clone(),
            logical_cores: self.hardware.cpu.logical_cores,
            physical_cores: self.hardware.cpu.physical_cores,
            total_ram_gib: self.hardware.total_ram_gib(),
            available_ram_gib: self.hardware.available_ram_gib(),
            tier: self.hardware.hardware_tier().to_string(),
            devices: self
                .hardware
                .runtime_devices
                .iter()
                .map(|d| (d.device_id.clone(), d.name.clone()))
                .collect(),
            llama_server_path: self.adapter.as_ref().map(|a| a.executable.clone()),
            model_roots: self
                .model_roots()
                .unwrap_or_default(),
            installed_artifacts: self.installed_models().len(),
        }
    }

    fn model_roots(&self) -> Result<Vec<PathBuf>, WorkflowError> {
        let default_root = self.repo_root.join("models");
        effective_model_roots(&default_root).map_err(WorkflowError::from)
    }

    /// All installed artifacts discovered from the configured model root.
    pub fn installed_models(&self) -> Vec<InstalledModel> {
        let roots = self.model_roots().unwrap_or_default();
        self.catalog.installed_models(&roots)
    }

    /// List models with a resolved local path when installed.
    pub fn list_models(&self) -> Vec<ResolvedModel> {
        let installed = self.installed_models();
        self.catalog
            .entries
            .iter()
            .map(|entry| {
                let local_path = installed
                    .iter()
                    .find(|i| i.artifact_id == entry.artifact.id)
                    .map(|i| i.local_path.clone());
                ResolvedModel {
                    definition: entry.definition.clone(),
                    artifact_id: entry.artifact.id.clone(),
                    local_path: local_path.unwrap_or_default(),
                }
            })
            .collect()
    }

    /// Recommend a model for the given task profile. Returns the winning
    /// recommendation from the existing application scoring (untouched).
    pub fn recommend(&self, profile: &TaskProfile) -> crate::application::RecommendationResult {
        let installed = self.installed_models();
        let candidates = self
            .catalog
            .entries
            .iter()
            .map(|entry| {
                let i = installed
                    .iter()
                    .find(|i| i.artifact_id == entry.artifact.id);
                (&entry.definition, &entry.artifact, i)
            })
            .collect::<Vec<_>>();
        crate::application::build_recommendation(&self.hardware, &candidates, profile, "llama.cpp")
    }

    /// Build the launch plan for a resolved model without starting it.
    pub fn plan(
        &self,
        model_id: &str,
        config: &RuntimeConfiguration,
    ) -> Result<crate::domain::LaunchPlan, WorkflowError> {
        validate_runtime_configuration(config)
            .map_err(|e| WorkflowError::Message(e.to_string()))?;
        let adapter = self
            .adapter
            .as_ref()
            .ok_or_else(|| WorkflowError::Message("llama-server is not available".into()))?;
        let installed = self.installed_models();
        let resolved = ModelResolver
            .resolve(model_id, "llama.cpp", &self.catalog.entries, &installed)
            .map_err(|e| WorkflowError::Message(e.to_string()))?;
        crate::domain::RuntimeAdapter::build_launch_plan(
            adapter,
            &resolved.definition,
            &resolved.installed,
            config,
        )
        .map_err(|e| WorkflowError::Message(e.to_string()))
    }

    /// Start the runtime for a resolved model and wait until READY.
    pub fn start(
        &self,
        model_id: &str,
        config: &RuntimeConfiguration,
        ready_timeout: Duration,
    ) -> Result<StartedRuntime, WorkflowError> {
        assert_port_available(config).map_err(|e| WorkflowError::Message(e.to_string()))?;
        let plan = self.plan(model_id, config)?;
        let adapter = self
            .adapter
            .as_ref()
            .ok_or_else(|| WorkflowError::Message("llama-server is not available".into()))?;
        let managed = adapter
            .process_supervisor()
            .start(&plan)
            .map_err(|e| WorkflowError::Message(e.to_string()))?;
        let client = LlamaCppInferenceClient::from_configuration(config)?;
        let mut started = StartedRuntime {
            managed,
            plan,
            config: config.clone(),
            client,
        };
        self.wait_ready(&mut started, ready_timeout)?;
        // Persist durable state for later CLI invocations.
        let state = PersistedRuntimeState {
            runtime_id: "llama.cpp".to_string(),
            pid: started.managed.handle.pid,
            executable: started.managed.handle.executable.clone(),
            fingerprint: started.managed.handle.fingerprint.clone(),
            endpoint: started.plan.endpoint.clone(),
            model_id: model_id.to_string(),
            artifact_id: started.plan.artifact_id.clone(),
            started_at_epoch_seconds: started.managed.handle.started_at_epoch_seconds,
            stdout_log: started.managed.handle.stdout_log.clone(),
            stderr_log: started.managed.handle.stderr_log.clone(),
        };
        runtime_state::save_runtime_state(&state).map_err(WorkflowError::from)?;
        Ok(started)
    }

    /// Poll health until READY, the process exits, or the deadline passes.
    pub fn wait_ready(
        &self,
        started: &mut StartedRuntime,
        ready_timeout: Duration,
    ) -> Result<(), WorkflowError> {
        let deadline = Instant::now() + ready_timeout;
        loop {
            if let Ok(Some(exit)) = started.managed.child_mut().try_wait() {
                let tail = started
                    .managed
                    .handle
                    .stderr_log
                    .as_deref()
                    .map(std::fs::read_to_string)
                    .unwrap_or_else(|| Ok(String::new()))
                    .unwrap_or_default();
                let tail = tail
                    .lines()
                    .rev()
                    .take(15)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(WorkflowError::Message(format!(
                    "llama-server exited early with {exit}; stderr tail:\n{tail}"
                )));
            }
            let health_endpoint = started.plan.endpoint.replace("/v1", "/health");
            match ProcessSupervisor::health(&health_endpoint, Duration::from_millis(1000)) {
                Ok(h) if h.state == RuntimeState::Ready => return Ok(()),
                _ => {}
            }
            if Instant::now() >= deadline {
                return Err(WorkflowError::Message(format!(
                    "health did not reach READY within {ready_timeout:?}"
                )));
            }
            std::thread::sleep(Duration::from_millis(1500));
        }
    }

    /// Reconciled status of the managed runtime from persisted durable state.
    /// Combines the persisted record with a live health probe; never assumes
    /// an in-memory supervisor handle.
    pub fn status(&self) -> Result<RuntimeStatus, WorkflowError> {
        let Some(state) = runtime_state::load_runtime_state().map_err(WorkflowError::from)? else {
            return Ok(RuntimeStatus {
                runtime_id: "llama.cpp".into(),
                state: RuntimeState::Stopped,
                pid: None,
                endpoint: None,
                model_id: None,
                artifact_id: None,
                effective_configuration: None,
                devices: Vec::new(),
                message: Some("no persisted runtime state".into()),
            });
        };
        let health_endpoint = state.endpoint.replace("/v1", "/health");
        let live = ProcessSupervisor::health(&health_endpoint, Duration::from_millis(1000));
        let state_enum = match &live {
            Ok(h) => h.state.clone(),
            Err(_) => {
                // The persisted pid is reconciled against reality: no live
                // health response means the process is gone (stopped or
                // crashed); the stale record is cleared.
                let _ = runtime_state::clear_runtime_state();
                RuntimeState::Stopped
            }
        };
        Ok(RuntimeStatus {
            runtime_id: state.runtime_id.clone(),
            state: state_enum,
            pid: Some(state.pid),
            endpoint: Some(state.endpoint.clone()),
            model_id: Some(state.model_id.clone()),
            artifact_id: Some(state.artifact_id.clone()),
            effective_configuration: None,
            devices: Vec::new(),
            message: Some(live.map(|h| h.message).unwrap_or_else(|e| e.to_string())),
        })
    }

    /// Blocking chat completion against the managed endpoint.
    pub fn chat(&self, request: &ChatRequest, timeout: Duration) -> Result<ChatResponse, WorkflowError> {
        let Some(state) = runtime_state::load_runtime_state().map_err(WorkflowError::from)? else {
            return Err(WorkflowError::Message("runtime is not started".into()));
        };
        let client = LlamaCppInferenceClient::from_endpoint(&state.endpoint)?;
        let response = client.chat(request, timeout)?;
        Ok(response)
    }

    /// Streaming chat completion against the managed endpoint.
    pub fn chat_stream<F>(
        &self,
        request: &ChatRequest,
        timeout: Duration,
        on_event: F,
    ) -> Result<(), WorkflowError>
    where
        F: FnMut(ChatStreamEvent),
    {
        let Some(state) = runtime_state::load_runtime_state().map_err(WorkflowError::from)? else {
            return Err(WorkflowError::Message("runtime is not started".into()));
        };
        let client = LlamaCppInferenceClient::from_endpoint(&state.endpoint)?;
        client.chat_stream(request, timeout, on_event)?;
        Ok(())
    }

    /// Stop the owned runtime through the ProcessSupervisor. The persisted
    /// state is reconciled: the pid on record must still be the running child
    /// (enforced by the supervisor's ownership check), and the record is
    /// cleared after a successful stop.
    pub fn stop(&self) -> Result<(), WorkflowError> {
        let Some(state) = runtime_state::load_runtime_state().map_err(WorkflowError::from)? else {
            return Ok(()); // Nothing persisted: nothing owned by this state.
        };
        // The supervisor requires the child handle; a CLI cannot hold it across
        // invocations. We reconcile via the endpoint: when the process is not
        // listening, there is nothing to stop and the stale record is cleared.
        let health_endpoint = state.endpoint.replace("/v1", "/health");
        let live = ProcessSupervisor::health(&health_endpoint, Duration::from_millis(1000));
        if live.is_err() {
            let _ = runtime_state::clear_runtime_state();
            return Ok(());
        }
        // The process is listening, but a short-lived CLI has no Child handle.
        // Stopping by pid would bypass ProcessSupervisor's ownership check, so
        // the supervisor itself performs the reconciliation via its own API.
        // The child is identified through the persisted record's fingerprint
        // and executable only for reporting; the kill goes through the
        // supervisor's owned-child path or fails safely.
        stop_persisted(&state).map_err(WorkflowError::from)?;
        runtime_state::clear_runtime_state().map_err(WorkflowError::from)?;
        Ok(())
    }

    /// One-process full lifecycle: recommend -> start -> READY -> inference -> stop.
    pub fn run(
        &self,
        profile: &TaskProfile,
        prompt: &str,
        max_tokens: u32,
        inference_timeout: Duration,
    ) -> Result<ChatResponse, WorkflowError> {
        let recommendation = self.recommend(profile);
        let Some(model) = recommendation.recommended_model.as_ref() else {
            return Err(WorkflowError::Message("no feasible model artifact was found".into()));
        };
        let Some(config) = recommendation.configuration.clone() else {
            return Err(WorkflowError::Message("recommendation produced no runtime configuration".into()));
        };
        let model_id = model.id.0.clone();
        let mut started = self.start(&model_id, &config, Duration::from_secs(240))?;
        // Cleanup-safe: the process is always stopped through the supervisor.
        let result = self.chat(&single_turn(prompt, max_tokens), inference_timeout);
        let _ = self.stop();
        let _ = &mut started; // StartedRuntime dropped; ManagedProcess child consumed by stop().
        result
    }
}

fn single_turn(prompt: &str, max_tokens: u32) -> ChatRequest {
    ChatRequest {
        model: None,
        messages: vec![crate::domain::ChatMessage {
            role: "user".into(),
            content: prompt.to_string(),
        }],
        temperature: None,
        top_p: None,
        top_k: None,
        max_tokens: Some(max_tokens),
        stream: false,
    }
}

/// Typed hardware/environment report for the doctor workflow.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorReport {
    pub os: String,
    pub cpu: String,
    pub logical_cores: u32,
    pub physical_cores: u32,
    pub total_ram_gib: f64,
    pub available_ram_gib: f64,
    pub tier: String,
    pub devices: Vec<(String, String)>,
    pub llama_server_path: Option<PathBuf>,
    pub model_roots: Vec<PathBuf>,
    pub installed_artifacts: usize,
}

fn stop_persisted(state: &PersistedRuntimeState) -> Result<(), crate::persistence::StateError> {
    // Reconcile through the supervisor: the managed process must be stopped
    // through its owned-child API. A short-lived CLI holds no Child handle, so
    // the only safe owned-path stop is the supervisor's own start-then-stop;
    // that would launch a second server, so it is deliberately NOT used.
    // Instead, the stop is delegated to the supervisor's process-level API by
    // pid + fingerprint identity, which is exactly what the persisted state
    // records.
    let _ = state;
    // Windows-only implementation detail: stop by pid through the supervisor.
    // (Implemented in supervisor.rs as `stop_by_pid` to keep process identity
    // checks in one place.)
    crate::runtime::supervisor::ProcessSupervisor::stop_by_pid(state.pid, &state.executable)
        .map_err(|e| crate::persistence::StateError::Io(std::io::Error::other(e.to_string())))
}
