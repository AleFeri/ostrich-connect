use std::fs;
use std::path::{Path, PathBuf};

use oc_core::error::OcError;
use oc_core::types::AppConfig;

pub fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var("OSTRICH_CONNECT_CONFIG_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let base = if let Ok(path) = std::env::var("XDG_CONFIG_HOME") {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            default_config_root()
        } else {
            PathBuf::from(trimmed)
        }
    } else {
        default_config_root()
    };

    base.join("ostrich-connect").join("config.json")
}

pub fn load_or_create(path: &Path) -> Result<AppConfig, OcError> {
    if path.exists() {
        let raw = fs::read_to_string(path)
            .map_err(|error| OcError::Io(format!("could not read {}: {error}", path.display())))?;
        let parsed = serde_json::from_str::<AppConfig>(&raw).map_err(|error| {
            OcError::InvalidCommand(format!(
                "invalid config json at {}: {error}",
                path.display()
            ))
        })?;
        let normalized = parsed.normalize();
        save(path, &normalized)?;
        return Ok(normalized);
    }

    let config = AppConfig::default();
    save(path, &config)?;
    Ok(config)
}

pub fn save(path: &Path, config: &AppConfig) -> Result<(), OcError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            OcError::Io(format!(
                "could not create config directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    let json = serde_json::to_string_pretty(config)
        .map_err(|error| OcError::Internal(format!("could not serialize config json: {error}")))?;
    fs::write(path, json)
        .map_err(|error| OcError::Io(format!("could not write {}: {error}", path.display())))
}

fn default_config_root() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(home) = home_dir() {
            return home.join("Library").join("Application Support");
        }
    }

    if let Some(home) = home_dir() {
        return home.join(".config");
    }

    PathBuf::from(".")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|home| home.trim().to_owned())
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}
