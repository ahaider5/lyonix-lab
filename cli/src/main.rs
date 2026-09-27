mod args;

use args::{Command, parse};
use lyonix_core::application::service::{DoctorReport, LyonixLab, WorkflowError};
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
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
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
