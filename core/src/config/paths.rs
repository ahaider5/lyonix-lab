use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("LYONIX_CONFIG_DIR") { return PathBuf::from(v); }
    #[cfg(target_os = "windows")]
    {
        if let Some(v) = std::env::var_os("APPDATA") { return PathBuf::from(v).join("LYONIX-LAB"); }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(v) = std::env::var_os("HOME") { return PathBuf::from(v).join("Library/Application Support/LYONIX-LAB"); }
    }
    if let Some(v) = std::env::var_os("XDG_CONFIG_HOME") { return PathBuf::from(v).join("LYONIX-LAB"); }
    if let Some(v) = std::env::var_os("HOME") { return PathBuf::from(v).join(".config/LYONIX-LAB"); }
    PathBuf::from(".lyonix-lab")
}

pub fn state_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("LYONIX_STATE_DIR") { return PathBuf::from(v); }
    let mut base = config_dir();
    base.push("state");
    base
}

pub fn log_dir() -> PathBuf {
    let mut base = state_dir();
    base.push("logs");
    base
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    std::fs::create_dir_all(state_dir())?;
    std::fs::create_dir_all(log_dir())?;
    Ok(())
}
