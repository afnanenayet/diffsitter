//! Utilities and definitions for config handling

use crate::formatting::DiffWriter;
use anyhow::{Context, Result};
use json5 as json;
use log::info;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[cfg(target_os = "windows")]
use directories_next::ProjectDirs;

/// The expected filename for the config file
const CFG_FILE_NAME: &str = "config.json5";

/// The config struct for the application
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    /// Custom file extension mappings between a file extension and a language
    ///
    /// These will be merged with the existing defaults, with the user-defined mappings taking
    /// precedence. The existing mappings are available at: `parse::FILE_EXTS` and the user can
    /// list all available langauges with `diffsitter --cmd list`
    pub file_associations: Option<HashMap<String, String>>,

    /// Formatting options for display
    pub formatting: DiffWriter,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            file_associations: None,
            formatting: DiffWriter::default(),
        }
    }
}

/// The possible errors that can arise when attempting to read a config
#[derive(Error, Debug)]
pub enum ConfigReadError {
    #[error("The file failed to deserialize")]
    DeserializationFailure(#[from] anyhow::Error),
    #[error("Failed to read the config file")]
    ReadFileFailure(#[from] io::Error),
    #[error("Unable to compute the default config file path")]
    NoDefault,
}

impl Config {
    /// Read a config from a given filepath, or fall back to the default file paths
    pub fn try_from_file<P: AsRef<Path>>(path: Option<&P>) -> Result<Self, ConfigReadError> {
        // We Have to store the default config here so we can use it as a reference later
        // (otherwise we would have a dangling reference to a temporary)
        let default_config_fp =
            default_config_file_path().map_err(|_| ConfigReadError::NoDefault)?;
        // If the user provided a path, we can just unwrap that, otherwise we fall back to the
        // default file path
        let config_fp: &Path = path.map(|x| x.as_ref()).unwrap_or(&default_config_fp);
        info!("Reading config at {}", config_fp.to_string_lossy());
        let config_contents = fs::read_to_string(config_fp)?;
        let config = json::from_str(&config_contents)
            .with_context(|| format!("Failed to parse config at {}", config_fp.to_string_lossy()))
            .map_err(ConfigReadError::DeserializationFailure)?;
        Ok(config)
    }
}

/// Return the default location for the config file (for *nix, Linux and MacOS), this will use
/// $XDG_CONFIG/.config, where `$XDG_CONFIG` is `$HOME/.config` by default.
#[cfg(not(target_os = "windows"))]
fn default_config_file_path() -> Result<PathBuf> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("diffsitter")?;
    let file_path = xdg_dirs.place_config_file(CFG_FILE_NAME)?;
    Ok(file_path)
}

/// Return the default location for the config file (for windows), this will use
/// $XDG_CONFIG_HOME/.config, where `$XDG_CONFIG_HOME` is `$HOME/.config` by default.
#[cfg(target_os = "windows")]
fn default_config_file_path() -> Result<PathBuf> {
    use anyhow::ensure;

    let proj_dirs = ProjectDirs::from("io", "afnan", "diffsitter");
    ensure!(proj_dirs.is_some(), "Was not able to retrieve config path");
    let proj_dirs = proj_dirs.unwrap();
    let mut config_file: PathBuf = proj_dirs.config_dir().into();
    config_file.push(CFG_FILE_NAME);
    Ok(config_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_config() {
        let mut sample_config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        sample_config_path.push("assets/sample_config.json5");
        Config::try_from_file(Some(sample_config_path).as_ref()).unwrap();
    }
}
