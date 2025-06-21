//! Utilities and definitions for config handling

use crate::input_processing::TreeSitterProcessor;
use crate::{parse::GrammarConfig, render::RenderConfig};
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct Config {
    /// Custom file extension mappings between a file extension and a language
    ///
    /// These will be merged with the existing defaults, with the user-defined mappings taking
    /// precedence. The existing mappings are available at: `parse::FILE_EXTS` and the user can
    /// list all available langauges with `diffsitter --cmd list`
    pub file_associations: Option<HashMap<String, String>>,

    /// Formatting options for display
    pub formatting: RenderConfig,

    /// Options for loading
    pub grammar: GrammarConfig,

    /// Options for processing tree-sitter input.
    pub input_processing: TreeSitterProcessor,

    /// The program to invoke if the given files can not be parsed by the available tree-sitter
    /// parsers.
    ///
    /// This will invoke the program with with the old and new file as arguments, like so:
    ///
    /// ```sh
    /// ${FALLBACK_PROGRAM} ${OLD} ${NEW}
    /// ```
    pub fallback_cmd: Option<String>,
}

/// The possible errors that can arise when attempting to read a config
#[derive(Error, Debug)]
pub enum ReadError {
    #[error("The file failed to deserialize")]
    DeserializationFailure(#[from] anyhow::Error),
    #[error("Failed to read the config file")]
    ReadFileFailure(#[from] io::Error),
    #[error("Unable to compute the default config file path")]
    NoDefault,
}

impl Config {
    /// Read a config from a given filepath, or fall back to the default file paths
    ///
    /// If a path is supplied, this method will attempt to read the contents of that path and parse
    /// it to a string. If a path isn't supplied, the function will attempt to figure out what the
    /// default config file path is supposed to be (based on OS conventions, see
    /// [`default_config_file_path`]).
    ///
    /// # Errors
    ///
    /// This method will return an error if the config cannot be parsed or if no default config
    /// exists.
    pub fn try_from_file<P: AsRef<Path>>(path: Option<&P>) -> Result<Self, ReadError> {
        // rustc will emit an incorrect warning that this variable isn't used, which is untrue.
        // While the variable isn't read *directly*, it is used to store the owned PathBuf from
        // `default_config_file_path` so we can use the reference to the variable in `config_fp`.
        #[allow(unused_assignments)]
        let mut default_config_fp = PathBuf::new();

        let config_fp = if let Some(p) = path {
            p.as_ref()
        } else {
            default_config_fp = default_config_file_path().map_err(|_| ReadError::NoDefault)?;
            default_config_fp.as_ref()
        };
        info!("Reading config at {}", config_fp.to_string_lossy());
        let config_contents = fs::read_to_string(config_fp)?;
        let config = json::from_str(&config_contents)
            .with_context(|| format!("Failed to parse config at {}", config_fp.to_string_lossy()))
            .map_err(ReadError::DeserializationFailure)?;
        Ok(config)
    }
}

/// Return the default location for the config file (for *nix, Linux and `MacOS`), this will use
/// $`XDG_CONFIG/.config`, where `$XDG_CONFIG` is `$HOME/.config` by default.
#[cfg(not(target_os = "windows"))]
fn default_config_file_path() -> Result<PathBuf> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("diffsitter");
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
    use anyhow::Context;
    use std::{env, fs::read_dir};

    #[test]
    fn test_sample_config() {
        let repo_root =
            env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| env::var("BUILD_DIR").unwrap());
        assert!(!repo_root.is_empty());
        let sample_config_path = [repo_root, "assets".into(), "sample_config.json5".into()]
            .iter()
            .collect::<PathBuf>();
        assert!(sample_config_path.exists());
        Config::try_from_file(Some(sample_config_path).as_ref()).unwrap();
    }

    #[test]
    fn test_configs() {
        let mut test_config_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        test_config_dir.push("resources/test_configs");
        assert!(test_config_dir.is_dir());

        for config_file_path in read_dir(test_config_dir).unwrap() {
            let config_file_path = config_file_path.unwrap().path();
            let has_correct_ext = if let Some(ext) = config_file_path.extension() {
                ext == "json5"
            } else {
                false
            };
            if !config_file_path.is_file() || !has_correct_ext {
                continue;
            }
            // We add the context so if there is an error you'll see the actual deserialization
            // error from serde and which file it failed on, which makes for a much more
            // informative error message in the test logs.
            Config::try_from_file(Some(&config_file_path))
                .with_context(|| {
                    format!(
                        "Parsing file {}",
                        &config_file_path.file_name().unwrap().to_string_lossy()
                    )
                })
                .unwrap();
        }
    }
}
