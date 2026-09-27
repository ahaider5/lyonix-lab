mod args;

use args::{Command, parse};
use lyonix_core::application::service::{DoctorReport, LyonixLab, WorkflowError};
use lyonix_core::domain::{ReasoningMode, TaskKind, TaskProfile};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
lyonix - LYONIX-LAB local LLM manager

Usage: lyonix <command>

Commands:
  doctor      Print a JSON report of hardware, runtimes, and model availability
  models      List installed catalog models
  recommend   Recommend a feasible model and runtime configuration
  start       Start the managed runtime
  status      Show the reconciled runtime status
  chat        Send a blocking chat completion
  stop        Stop the managed runtime
  run         Recommend, start, infer, and stop in one process

Options:
  --help      Print this usage text
  --version   Print the CLI version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Command::Help => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("lyonix {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::Doctor => doctor(),
        Command::Models => models(),
        Command::Recommend { profile } => recommend(profile.as_deref()),
        Command::Start { profile } => start_runtime(profile.as_deref()),
        Command::Status => status(),
        Command::Stop => stop(),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Serializable view of the facade's ResolvedModel (presentation only).
#[derive(serde::Serialize)]
struct ModelOutput {
    definition: lyonix_core::domain::ModelDefinition,
    artifact_id: String,
    local_path: PathBuf,
}

fn doctor() -> ExitCode {
    let report = match doctor_report(&find_repo_root()) {
        Ok(report) => report,
        Err(error) => return print_error(error),
    };
    match serde_json::to_string_pretty(&report) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => print_error(error),
    }
}

fn doctor_report(repo_root: &Path) -> Result<DoctorReport, WorkflowError> {
    let lab = LyonixLab::open(repo_root)?;
    Ok(lab.doctor())
}

fn models() -> ExitCode {
    let lab = match LyonixLab::open(&find_repo_root()) {
        Ok(lab) => lab,
        Err(error) => return print_error(error),
    };
    let output: Vec<ModelOutput> = lab
        .list_models()
        .into_iter()
        .map(|resolved| ModelOutput {
            definition: resolved.definition,
            artifact_id: resolved.artifact_id,
            local_path: resolved.local_path,
        })
        .collect();
    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => print_error(error),
    }
}

fn recommend(profile_name: Option<&str>) -> ExitCode {
    let profile = task_profile(profile_name);
    let lab = match LyonixLab::open(&find_repo_root()) {
        Ok(lab) => lab,
        Err(error) => return print_error(error),
    };
    let recommendation = lab.recommend(&profile);
    match serde_json::to_string_pretty(&recommendation) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => print_error(error),
    }
}

#[derive(serde::Serialize)]
struct StartOutput {
    pid: u32,
    endpoint: String,
    model_id: String,
    artifact_id: String,
}

#[derive(serde::Serialize)]
struct StopOutput {
    stopped: bool,
}

fn start_runtime(profile_name: Option<&str>) -> ExitCode {
    let profile = task_profile(profile_name);
    let lab = match LyonixLab::open(&find_repo_root()) {
        Ok(lab) => lab,
        Err(error) => return print_error(error),
    };
    let recommendation = lab.recommend(&profile);
    let Some(model) = recommendation.recommended_model.as_ref() else {
        return print_error(WorkflowError::Message(
            "no feasible model artifact was found".into(),
        ));
    };
    let Some(config) = recommendation.configuration else {
        return print_error(WorkflowError::Message(
            "recommendation produced no runtime configuration".into(),
        ));
    };
    let model_id = model.id.0.clone();
    let started = match lab.start(&model_id, &config, std::time::Duration::from_secs(240)) {
        Ok(started) => started,
        Err(error) => return print_error(error),
    };
    let output = StartOutput {
        pid: started.managed.handle.pid,
        endpoint: started.plan.endpoint,
        model_id: started.plan.model_id,
        artifact_id: started.plan.artifact_id,
    };
    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => print_error(error),
    }
}

fn status() -> ExitCode {
    let lab = match LyonixLab::open(&find_repo_root()) {
        Ok(lab) => lab,
        Err(error) => return print_error(error),
    };
    let status = match lab.status() {
        Ok(status) => status,
        Err(error) => return print_error(error),
    };
    match serde_json::to_string_pretty(&status) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => print_error(error),
    }
}

fn stop() -> ExitCode {
    let lab = match LyonixLab::open(&find_repo_root()) {
        Ok(lab) => lab,
        Err(error) => return print_error(error),
    };
    let stopped = match lab.stop() {
        Ok(()) => StopOutput { stopped: true },
        Err(error) => return print_error(error),
    };
    match serde_json::to_string_pretty(&stopped) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => print_error(error),
    }
}

/// TaskProfile request parameter for the facade's recommendation workflow.
/// Values are the CLI's request inputs only; no scoring or thresholds here.
fn task_profile(profile_name: Option<&str>) -> TaskProfile {
    match profile_name.unwrap_or("chat") {
        "chat" => TaskProfile {
            id: "chat".into(),
            display_name: "Chat / Fast".into(),
            description: "Fast general assistant".into(),
            task: TaskKind::Chat,
            target_context: 8192,
            preferred_reasoning: ReasoningMode::Off,
            preferred_parallelism: 1,
            low_latency: true,
            deterministic: true,
            allow_reasoning: false,
            cpu_only: false,
        },
        "coding" => TaskProfile {
            id: "coding".into(),
            display_name: "Coding".into(),
            description: "Coding assistance".into(),
            task: TaskKind::Coding,
            target_context: 8192,
            preferred_reasoning: ReasoningMode::Off,
            preferred_parallelism: 1,
            low_latency: false,
            deterministic: false,
            allow_reasoning: false,
            cpu_only: false,
        },
        "reasoning" => TaskProfile {
            id: "reasoning".into(),
            display_name: "Reasoning".into(),
            description: "Deep reasoning".into(),
            task: TaskKind::Reasoning,
            target_context: 16384,
            preferred_reasoning: ReasoningMode::On,
            preferred_parallelism: 1,
            low_latency: false,
            deterministic: false,
            allow_reasoning: true,
            cpu_only: false,
        },
        "vision" => TaskProfile {
            id: "vision".into(),
            display_name: "Vision".into(),
            description: "Vision tasks".into(),
            task: TaskKind::Vision,
            target_context: 8192,
            preferred_reasoning: ReasoningMode::Off,
            preferred_parallelism: 1,
            low_latency: false,
            deterministic: false,
            allow_reasoning: false,
            cpu_only: false,
        },
        "agent" => TaskProfile {
            id: "agent".into(),
            display_name: "Agent".into(),
            description: "Agent workflows".into(),
            task: TaskKind::Agent,
            target_context: 8192,
            preferred_reasoning: ReasoningMode::Off,
            preferred_parallelism: 1,
            low_latency: false,
            deterministic: false,
            allow_reasoning: false,
            cpu_only: false,
        },
        _ => TaskProfile {
            id: "longcontext".into(),
            display_name: "Long Context".into(),
            description: "Long context tasks".into(),
            task: TaskKind::LongContext,
            target_context: 32768,
            preferred_reasoning: ReasoningMode::Off,
            preferred_parallelism: 1,
            low_latency: false,
            deterministic: false,
            allow_reasoning: false,
            cpu_only: false,
        },
    }
}

fn find_repo_root() -> PathBuf {
    let Ok(cwd) = std::env::current_dir() else {
        return PathBuf::from(".");
    };
    let mut dir = Some(cwd.as_path());
    while let Some(current) = dir {
        if current.join("models.json").is_file() {
            return current.to_path_buf();
        }
        dir = current.parent();
    }
    cwd
}

fn print_error(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
