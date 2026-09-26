use crate::config::paths;
use crate::domain::{LaunchPlan, RuntimeHandle, RuntimeState};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("process I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("health request failed: {0}")]
    Health(String),
    #[error("managed process identity no longer matches")]
    OwnershipMismatch,
}

#[derive(Debug)]
pub struct ManagedProcess {
    pub handle: RuntimeHandle,
    child: Child,
}

impl ManagedProcess {
    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

#[derive(Debug, Clone)]
pub struct HealthResult {
    pub state: RuntimeState,
    pub status_code: Option<u16>,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct ProcessSupervisor;

impl ProcessSupervisor {
    pub fn start(&self, plan: &LaunchPlan) -> Result<ManagedProcess, SupervisorError> {
        paths::ensure_dirs()?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let pid_hint = std::process::id();
        let stdout_path = paths::log_dir().join(format!("llama-server-{stamp}-{pid_hint}.stdout.log"));
        let stderr_path = paths::log_dir().join(format!("llama-server-{stamp}-{pid_hint}.stderr.log"));
        let stdout = std::fs::File::create(&stdout_path)?;
        let stderr = std::fs::File::create(&stderr_path)?;
        let child = Command::new(&plan.executable)
            .args(&plan.arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout))
            .stderr(std::process::Stdio::from(stderr))
            .spawn()?;
        let pid = child.id();
        let started = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let fingerprint = fingerprint(plan);
        Ok(ManagedProcess {
            handle: RuntimeHandle { pid, executable: plan.executable.clone(), fingerprint, started_at_epoch_seconds: started, stdout_log: Some(stdout_path), stderr_log: Some(stderr_path) },
            child,
        })
    }

    pub fn health(endpoint: &str, timeout: Duration) -> Result<HealthResult, SupervisorError> {
        let (host, port, path) = parse_endpoint(endpoint).ok_or_else(|| SupervisorError::Health("endpoint must be http://host:port/path".into()))?;
        let mut stream = TcpStream::connect((host.as_str(), port)).map_err(|e| SupervisorError::Health(e.to_string()))?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes())?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        let code = response.lines().next().and_then(|line| line.split_whitespace().nth(1)).and_then(|v| v.parse::<u16>().ok());
        let state = match code {
            Some(200) => RuntimeState::Ready,
            Some(503) => RuntimeState::Loading,
            Some(400..=599) => RuntimeState::Failed,
            _ => RuntimeState::Unknown,
        };
        Ok(HealthResult { state, status_code: code, message: response.lines().next().unwrap_or("no response").into() })
    }

    pub fn stop(mut managed: ManagedProcess) -> Result<(), SupervisorError> {
        // The child handle is the ownership primitive. We deliberately do not
        // search for or kill arbitrary processes with the same executable name.
        if managed.child.id() != managed.handle.pid {
            return Err(SupervisorError::OwnershipMismatch);
        }
        let _ = managed.child.kill();
        let _ = managed.child.wait();
        Ok(())
    }

    /// Stop a process identified by the persisted runtime-state record. Used
    /// by short-lived CLI invocations that hold no Child handle: the pid and
    /// executable identity from the durable record are verified against the
    /// live process before the kill, so name-based killing remains impossible
    /// and an unrelated or already-exited process is never touched.
    pub fn stop_by_pid(pid: u32, expected_executable: &Path) -> Result<(), SupervisorError> {
        if pid == 0 {
            return Err(SupervisorError::OwnershipMismatch);
        }
        let mut system = sysinfo::System::new();
        system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]), true);
        let Some(process) = system.process(sysinfo::Pid::from_u32(pid)) else {
            // Process is already gone; treat as stopped.
            return Ok(());
        };
        // Identity check: the live executable path must match the persisted
        // record. This is an identity check, not a name-based kill: a same-named
        // binary at a different path (or any unrelated pid) is refused.
        let live_exe = process.exe().map(|p| p.to_path_buf());
        match live_exe {
            Some(exe) if exe == expected_executable => {}
            _ => return Err(SupervisorError::OwnershipMismatch),
        }
        let killed = process.kill();
        let _ = killed;
        // Wait briefly for the process to actually terminate by re-refreshing.
        let sysinfo_pid = sysinfo::Pid::from_u32(pid);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[sysinfo_pid]), true);
            if system.process(sysinfo_pid).is_none() {
                break; // Process gone.
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}

fn fingerprint(plan: &LaunchPlan) -> String {
    let mut value = plan.executable.display().to_string();
    value.push('|');
    value.push_str(&plan.arguments.join("\u{1f}"));
    value.push('|');
    value.push_str(&plan.artifact_id);
    value.push('|');
    value.push_str(&plan.endpoint);
    format!("{:x}", stable_hash(&value))
}

fn stable_hash(s: &str) -> u64 {
    let mut hash = 1469598103934665603_u64;
    for byte in s.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211_u64);
    }
    hash
}

fn parse_endpoint(endpoint: &str) -> Option<(String, u16, String)> {
    let rest = endpoint.strip_prefix("http://")?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = if path.is_empty() { "/".into() } else { format!("/{path}") };
    let (host, port) = authority.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?, path))
}

#[allow(dead_code)]
fn _canonical(path: &Path) -> Option<std::path::PathBuf> { std::fs::canonicalize(path).ok() }
