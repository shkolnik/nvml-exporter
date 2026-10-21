//! Config file (design §4.2): listen address, reader allow-list, the `collect_pcie` switch.

use ipnet::IpNet;
use serde::Deserialize;
use std::fmt;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

const DEFAULT_LISTEN: &str = "127.0.0.1:9835";

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub listen: SocketAddr,
    pub readers: AllowList,
    pub collect_pcie: bool,
}

/// Peers allowed to scrape besides loopback; entries are addresses or CIDRs, never hostnames.
#[derive(Debug, Clone, PartialEq)]
pub struct AllowList(Vec<IpNet>);

impl AllowList {
    pub fn parse(entries: &[String]) -> Result<Self, ConfigError> {
        entries
            .iter()
            .map(|entry| {
                entry
                    .parse::<IpAddr>()
                    .map(IpNet::from)
                    .or_else(|_| entry.parse::<IpNet>())
                    .map_err(|_| ConfigError::Reader {
                        entry: entry.clone(),
                        reason: "reader must be an IP address or CIDR".into(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    pub fn allows(&self, peer: IpAddr) -> bool {
        let peer = peer.to_canonical();
        peer.is_loopback() || self.0.iter().any(|net| net.contains(&peer))
    }

    pub fn entries(&self) -> &[IpNet] {
        &self.0
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io { path: PathBuf, source: io::Error },
    Toml(toml::de::Error),
    Listen { value: String, reason: String },
    Reader { entry: String, reason: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io { path, source } => write!(f, "{}: {source}", path.display()),
            ConfigError::Toml(e) => write!(f, "invalid config: {e}"),
            ConfigError::Listen { value, reason } => write!(f, "listen {value:?}: {reason}"),
            ConfigError::Reader { entry, reason } => write!(f, "readers entry {entry:?}: {reason}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            ConfigError::Toml(e) => Some(e),
            ConfigError::Listen { .. } | ConfigError::Reader { .. } => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    listen: Option<String>,
    readers: Option<Vec<String>>,
    collect_pcie: Option<bool>,
}

pub fn parse(text: &str) -> Result<Config, ConfigError> {
    let raw: Raw = toml::from_str(text).map_err(ConfigError::Toml)?;
    let listen_text = raw.listen.as_deref().unwrap_or(DEFAULT_LISTEN);
    let listen = listen_text.parse().map_err(|_| ConfigError::Listen {
        value: listen_text.to_owned(),
        reason: "listen must be address:port".into(),
    })?;
    Ok(Config {
        listen,
        readers: AllowList::parse(&raw.readers.unwrap_or_default())?,
        collect_pcie: raw.collect_pcie.unwrap_or(false),
    })
}

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_owned(),
        source,
    })?;
    parse(&text)
}
