#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lyonix_core::application::build_recommendation;
use lyonix_core::catalog::ModelCatalog;
use lyonix_core::diagnostics::{self, DiagnosticReport};
use lyonix_core::domain::{InstalledModel, ModelArtifact, ModelDefinition, ReasoningMode, RuntimeConfiguration, RuntimeProbe, RuntimeState, RuntimeStatus, TaskKind, TaskProfile, RuntimeAdapter, UserConfiguration};
use lyonix_core::hardware::{HardwareProvider, SystemHardwareProvider};
use lyonix_core::runtime::llama_cpp::LlamaCppAdapter;
use lyonix_core::runtime::supervisor::{ManagedProcess, ProcessSupervisor};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug)]
struct AppStateInner {
    managed_process: Option<ManagedProcess>,
    last_status: RuntimeStatus,
}

struct AppState(Mutex<AppStateInner>);

const MODELS_JSON: &str = include_str!("../../../models.json");
const PROFILES_JSON: &str = include_str!("../../../profiles.json");

#[derive(Debug, Clone, serde::Serialize)]
struct Snapshot {
    hardware: lyonix_core::domain::HardwareSnapshot,
    runtimes: Vec<RuntimeProbe>,
    models: Vec<InstalledModelView>,
    runtime: RuntimeStatus,
}

#[derive(Debug, Clone, serde::Serialize)]
struct InstalledModelView {
    artifact_id: String,
    model_definition_id: lyonix_core::domain::ModelId,
    local_path: PathBuf,
    verified: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct LegacyProfile {
    id: String,
    #[serde(rename = "displayName")]
    display_name: String,
    description: String,
    #[serde(default, rename = "task")]
    task: Option<String>,
    #[serde(rename = "targetContext")]
    target_context: u32,
    #[serde(default)]
    parallel: u32,
    #[serde(default)]
    reasoning: String,
    #[serde(default, rename = "lowLatency")]
    low_latency: Option<bool>,
    #[serde(default)]
    deterministic: Option<bool>,
    #[serde(default, rename = "allowReasoning")]
    allow_reasoning: Option<bool>,
    #[serde(default, rename = "cpuOnly")]
    cpu_only: bool,
}

fn task_kind(id: &str, task: Option<&str>) -> TaskKind {
    match task.unwrap_or(id) {
        "coding" => TaskKind::Coding,
        "reasoning" => TaskKind::Reasoning,
        "vision" => TaskKind::Vision,
        "long_context" | "long-context" => TaskKind::LongContext,
        "agent" => TaskKind::Agent,
        _ => TaskKind::Chat,
    }
}

fn profile_from_id(id: &str) -> Result<TaskProfile, String> {
    let records: Vec<LegacyProfile> = serde_json::from_str(PROFILES_JSON).map_err(|e| e.to_string())?;
    let Some(profile) = records.into_iter().find(|p| p.id == id) else {
        return Err(format!("Unknown profile '{id}'."));
    };
    let preferred_reasoning = match profile.reasoning.as_str() {
        "on" => ReasoningMode::On,
        "auto" => ReasoningMode::Auto,
        _ => ReasoningMode::Off,
    };
    Ok(TaskProfile {
        id: profile.id.clone(),
        display_name: profile.display_name,
        description: profile.description,
        task: task_kind(&profile.id, profile.task.as_deref()),
        target_context: profile.target_context,
        preferred_reasoning: preferred_reasoning.clone(),
        preferred_parallelism: profile.parallel.max(1),
        low_latency: profile.low_latency.unwrap_or(profile.id == "chat" || profile.id == "coding"),
        deterministic: profile.deterministic.unwrap_or(profile.id == "coding"),
        allow_reasoning: profile.allow_reasoning.unwrap_or(!matches!(preferred_reasoning, ReasoningMode::Off)),
        cpu_only: profile.cpu_only,
    })
}

fn runtime_adapter() -> Result<LlamaCppAdapter, String> {
    let configured = std::env::var_os("LYONIX_LLAMA_SERVER").map(PathBuf::from);
    LlamaCppAdapter::discover(configured.as_deref()).map_err(|e| e.to_string())
}

fn hardware_and_runtime() -> Result<(lyonix_core::domain::HardwareSnapshot, LlamaCppAdapter), String> {
    let runtime = runtime_adapter()?;
    let devices = runtime.list_devices().unwrap_or_default();
    let hardware = SystemHardwareProvider::default().snapshot(devices).map_err(|e| e.to_string())?;
    Ok((hardware, runtime))
}

fn installed_models(catalog: &ModelCatalog) -> Vec<InstalledModel> {
    catalog.installed_models(&model_roots())
}

fn status_from_inner(inner: &mut AppStateInner) -> RuntimeStatus {
    if let Some(managed) = inner.managed_process.as_mut() {
        let endpoint = inner.last_status.endpoint.clone().unwrap_or_else(|| "http://127.0.0.1:8081/v1".into());
        if let Ok(Some(exit)) = managed.child_mut().try_wait() {
            inner.last_status.state = RuntimeState::Failed;
            inner.last_status.message = Some(format!("llama-server exited with {exit}"));
            inner.last_status.pid = None;
            inner.managed_process = None;
            return inner.last_status.clone();
        }
        if let Ok(health) = ProcessSupervisor::health(&endpoint.replace("/v1", "/health"), std::time::Duration::from_millis(750)) {
            inner.last_status.state = health.state;
            inner.last_status.message = Some(health.message);
        } else {
            inner.last_status.state = RuntimeState::Starting;
        }
    }
    inner.last_status.clone()
}

fn snapshot(state: &AppState) -> Result<Snapshot, String> {
    let (hardware, runtime) = hardware_and_runtime()?;
    let catalog = catalog()?;
    let installed = installed_models(&catalog);
    let models = installed.iter().map(|m| InstalledModelView { artifact_id: m.artifact_id.clone(), model_definition_id: m.model_definition_id.clone(), local_path: m.local_path.clone(), verified: m.verified }).collect();
    let runtime_probe = runtime.probe().map_err(|e| e.to_string())?;
    let mut inner = state.0.lock().map_err(|_| "state lock poisoned".to_string())?;
    let status = status_from_inner(&mut inner);
    Ok(Snapshot { hardware, runtimes: vec![runtime_probe], models, runtime: status })
}

#[tauri::command]
fn get_user_configuration() -> Result<UserConfiguration, String> {
    lyonix_core::persistence::load_user_configuration().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_user_configuration(config: UserConfiguration) -> Result<UserConfiguration, String> {
    if config.version == 0 { return Err("configuration version must be >= 1".into()); }
    if let Some(network) = &config.network {
        if network.host != "127.0.0.1" && network.host != "localhost" {
            return Err("non-localhost binding is not enabled in this milestone".into());
        }
    }
    lyonix_core::persistence::save_user_configuration(&config).map_err(|e| e.to_string())?;
    Ok(config)
}

#[tauri::command]
fn export_diagnostics(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let snapshot = snapshot(&state)?;
    lyonix_core::config::paths::ensure_dirs().map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let path = lyonix_core::config::paths::state_dir().join(format!("diagnostics-{stamp}.json"));
    let report = DiagnosticReport { app_version: env!("CARGO_PKG_VERSION").into(), os: snapshot.hardware.os.clone(), hardware: snapshot.hardware, runtimes: snapshot.runtimes, runtime_status: snapshot.runtime, recent_log: None };
    diagnostics::write_report(&path, &report).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn get_hardware() -> Result<lyonix_core::domain::HardwareSnapshot, String> {
    hardware_and_runtime().map(|(hardware, _)| hardware)
}

#[tauri::command]
fn get_runtimes() -> Result<Vec<RuntimeProbe>, String> {
    Ok(vec![runtime_adapter()?.probe().map_err(|e| e.to_string())?])
}

#[tauri::command]
fn get_installed_models() -> Result<Vec<InstalledModelView>, String> {
    let catalog = catalog()?;
    Ok(installed_models(&catalog).iter().map(|m| InstalledModelView { artifact_id: m.artifact_id.clone(), model_definition_id: m.model_definition_id.clone(), local_path: m.local_path.clone(), verified: m.verified }).collect())
}

#[tauri::command]
fn get_runtime_status(state: tauri::State<'_, AppState>) -> Result<RuntimeStatus, String> {
    let mut inner = state.0.lock().map_err(|_| "state lock poisoned".to_string())?;
    Ok(status_from_inner(&mut inner))
}

#[tauri::command]
fn refresh_snapshot(state: tauri::State<'_, AppState>) -> Result<Snapshot, String> {
    snapshot(&state)
}

#[tauri::command]
fn start_runtime(state: tauri::State<'_, AppState>) -> Result<RuntimeStatus, String> {
    let catalog = catalog()?;
    let (hardware, runtime) = hardware_and_runtime()?;
    let installed = installed_models(&catalog);
    if installed.is_empty() {
        return Err("No installed model artifacts were found. Configure LYONIX_MODEL_ROOTS or place models under ./models.".into());
    }
    let user_config = lyonix_core::persistence::load_user_configuration().map_err(|e| e.to_string())?;
    let selected_model_default_profile = user_config.selected_model_id.as_deref()
        .and_then(|id| catalog.entries.iter().find(|entry| entry.definition.id.0 == id))
        .and_then(|entry| entry.definition.default_profile.clone());
    let profile_id = user_config.selected_profile_id.clone()
        .or(selected_model_default_profile)
        .unwrap_or_else(|| "chat".into());
    let profile = profile_from_id(&profile_id)?;
    let candidates: Vec<(&ModelDefinition, &ModelArtifact, Option<&InstalledModel>)> = catalog.entries.iter().filter(|entry| {
        user_config.selected_model_id.as_deref().map(|id| entry.definition.id.0 == id).unwrap_or(true)
    }).map(|e| {
        let installed = installed.iter().find(|i| i.artifact_id == e.artifact.id);
        (&e.definition, &e.artifact, installed)
    }).collect();
    if candidates.is_empty() {
        return Err("The selected model is not present in the catalog.".into());
    }
    let recommendation = build_recommendation(&hardware, &candidates, &profile, "llama.cpp");
    let model = recommendation.recommended_model.ok_or_else(|| "No feasible installed model is available for the selected profile/model.".to_string())?;
    let artifact = recommendation.recommended_artifact.ok_or_else(|| "Recommendation did not resolve an artifact.".to_string())?;
    let resolver = lyonix_core::catalog::ModelResolver::default();
    let resolved = resolver.resolve(&model.id.0, "llama.cpp", &catalog.entries, &installed).map_err(|e| e.to_string())?;
    let installed_model = &resolved.installed;
    let config = recommendation.configuration.ok_or_else(|| "Recommendation did not produce runtime configuration.".to_string())?;
    runtime.validate(&model, installed_model, &config).map_err(|e| e.to_string())?;
    lyonix_core::application::configuration::validate_runtime_configuration(&config).map_err(|e| e.to_string())?;
    lyonix_core::application::configuration::assert_port_available(&config).map_err(|e| e.to_string())?;

    let mut inner = state.0.lock().map_err(|_| "state lock poisoned".to_string())?;
    if inner.managed_process.is_some() {
        return Err("A LYONIX-LAB managed runtime is already running.".into());
    }
    let plan = RuntimeAdapter::build_launch_plan(&runtime, &resolved.definition, installed_model, &config).map_err(|e| e.to_string())?;
    let process = runtime.process_supervisor().start(&plan).map_err(|e| e.to_string())?;
    let pid = process.handle.pid;
    inner.managed_process = Some(process);
    inner.last_status = RuntimeStatus {
        runtime_id: "llama.cpp".into(),
        state: RuntimeState::Starting,
        pid: Some(pid),
        endpoint: Some(plan.endpoint.clone()),
        model_id: Some(model.id.0),
        artifact_id: Some(artifact.id),
        effective_configuration: Some(config),
        devices: hardware.runtime_devices,
        message: Some("llama-server started; waiting for /health to become ready.".into()),
    };
    Ok(status_from_inner(&mut inner))
}

#[tauri::command]
fn stop_runtime(state: tauri::State<'_, AppState>) -> Result<RuntimeStatus, String> {
    let mut inner = state.0.lock().map_err(|_| "state lock poisoned".to_string())?;
    if let Some(process) = inner.managed_process.take() {
        inner.last_status.state = RuntimeState::Stopping;
        ProcessSupervisor::stop(process).map_err(|e| e.to_string())?;
        inner.last_status.state = RuntimeState::Stopped;
        inner.last_status.pid = None;
        inner.last_status.message = Some("Managed runtime stopped.".into());
    } else {
        inner.last_status.state = RuntimeState::Stopped;
        inner.last_status.message = Some("No LYONIX-LAB managed runtime is running.".into());
    }
    Ok(inner.last_status.clone())
}

pub fn run() {
    let state = AppState(Mutex::new(AppStateInner {
        managed_process: None,
        last_status: RuntimeStatus {
            runtime_id: "llama.cpp".into(),
            state: RuntimeState::Stopped,
            pid: None,
            endpoint: Some("http://127.0.0.1:8081/v1".into()),
            model_id: None,
            artifact_id: None,
            effective_configuration: None,
            devices: Vec::new(),
            message: Some("No managed runtime is running.".into()),
        },
    }));

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_hardware,
            get_runtimes,
            get_installed_models,
            get_runtime_status,
            refresh_snapshot,
            start_runtime,
            stop_runtime,
            get_user_configuration,
            save_user_configuration,
            export_diagnostics
        ])
        .run(tauri::generate_context!())
        .expect("error while running LYONIX-LAB");
}
