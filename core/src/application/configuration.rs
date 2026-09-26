use crate::domain::RuntimeConfiguration;
use std::net::{IpAddr, TcpListener};
use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
pub enum ConfigurationError {
    #[error("invalid host '{0}'")]
    InvalidHost(String),
    #[error("port {0} is already in use")]
    PortInUse(u16),
    #[error("context must be at least 2048")]
    InvalidContext,
    #[error("parallelism must be at least 1")]
    InvalidParallelism,
}

pub fn validate_runtime_configuration(config: &RuntimeConfiguration) -> Result<(), ConfigurationError> {
    if config.context < 2048 { return Err(ConfigurationError::InvalidContext); }
    if config.parallelism == 0 { return Err(ConfigurationError::InvalidParallelism); }
    let ip = IpAddr::from_str(&config.network.host).map_err(|_| ConfigurationError::InvalidHost(config.network.host.clone()))?;
    // Initial milestone is deliberately localhost-only. LAN exposure will be
    // added only with an explicit security workflow and authenticated binding.
    if !ip.is_loopback() { return Err(ConfigurationError::InvalidHost(config.network.host.clone())); }
    Ok(())
}

pub fn port_available(host: &str, port: u16) -> bool {
    TcpListener::bind((host, port)).is_ok()
}

pub fn assert_port_available(config: &RuntimeConfiguration) -> Result<(), ConfigurationError> {
    if port_available(&config.network.host, config.network.port) { Ok(()) } else { Err(ConfigurationError::PortInUse(config.network.port)) }
}
