//! Utilities and definitions for config handling

use crate::{
    cli::Args, figment_utils::JsonProvider, input_processing::TreeSitterProcessor,
    parse::GrammarConfig, render::RenderConfig,
};
use anyhow::Result;
use figment::{
    self, Figment,
    providers::{Format, Serialized},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[cfg(target_os = "windows")]
use directories_next::ProjectDirs;

/// The expected filename for the config file
const CFG_FILE_NAME: &str = "config.json5";

/// The app name used for configuration purposes.
pub const APP_NAME: &str = "diffsitter";

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
    /// # Args
    ///
    /// * path: An optional path. If `None` is provided, then try to find the default config file
    ///   path, unless `no_config` is set to true.
    /// * `no_config`: Corresponds to the `--no-config` CLI option.
    ///
    /// # Errors
    ///
    /// This method will return an error if the config cannot be parsed or if no default config
    /// exists.
    pub fn try_from_file<P: AsRef<Path>>(path: Option<&P>, no_config: bool) -> Result<Self> {
        let fig: Figment = {
            let mut fig = figment::Figment::from(Serialized::defaults(Config::default()));
            if let Some(cfg_path) = get_config_path_from_args(path, no_config) {
                fig = merge_fig_provider_from_ext(fig, &cfg_path)?;
            }
            fig
        };
        let config: Config = fig.extract()?;
        Ok(config)
    }

    /// Create a new config, parsed hierarchically.
    ///
    /// Config values are pulled from the following sources listed in order of precedence:
    ///
    /// - environment variables
    /// - command line flags
    /// - config files specified at the command line
    /// - the hardcoded defaults
    // TODO: check if we can incorporate clap or add the command line flags somehow
    pub fn new_from_args(cli_args: &Args) -> Result<Self> {
        Self::try_from_file(cli_args.config.as_ref(), cli_args.no_config)
    }
}

/// Select the file path for the diffsitter config.
///
/// This will return `None` if the `--no-config` flag is selected by the user. Otherwise it will
/// look for the first existing config path found in order of precedence.
///
/// The search list for config paths (unless `--no-config` is selected)
///
/// * path specified by `--config` at the CLI (if applicable)
/// * path specified by `DIFFSITTER_CONFIG` environment variable
/// * default config file path
fn get_config_path_from_args<P: AsRef<Path>>(
    supplied_path: Option<&P>,
    no_config: bool,
) -> Option<PathBuf> {
    if no_config {
        return None;
    }
    if let Some(path) = supplied_path {
        return Some(PathBuf::from(path.as_ref()));
    }
    default_config_file_path().ok()
}

/// Merge the given path's figment data.
///
/// This is a helper function that constructs the correct figment provider for a given config path
/// based on the file's path extension. We can't return the figment provider directly, which would
/// be nicer, because the provider has to be sized and there's no way to generally handle this.
///
/// # Supported extensions
///
/// * .json
/// * .json5
/// * .toml
///
/// # Errors
///
/// This will return an error the extension is not one of the known extensions.
fn merge_fig_provider_from_ext(fig: figment::Figment, path: &Path) -> Result<figment::Figment> {
    use figment::providers::Toml;
    let ext = path.extension().map_or_else(
        || {
            anyhow::bail!(
                "Config path {} does not have a valid extension",
                path.to_string_lossy()
            )
        },
        |x| OsStr::to_str(x).ok_or(anyhow::anyhow!("Could not map to OsStr")),
    )?;
    match ext {
        "json5" | "json" => Ok(fig.merge(JsonProvider::file(path))),
        "toml" => Ok(fig.merge(Toml::file(path))),
        _ => Err(anyhow::anyhow!("Unrecognized file extension {ext}")),
    }
}

/// Return the default location for the config file (for *nix, Linux and `MacOS`), this will use
/// $`XDG_CONFIG/.config`, where `$XDG_CONFIG` is `$HOME/.config` by default.
#[cfg(not(target_os = "windows"))]
fn default_config_file_path() -> Result<PathBuf> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(APP_NAME);
    let file_path = xdg_dirs.place_config_file(CFG_FILE_NAME)?;
    Ok(file_path)
}

/// Return the default location for the config file (for windows), this will use
/// $XDG_CONFIG_HOME/.config, where `$XDG_CONFIG_HOME` is `$HOME/.config` by default.
#[cfg(target_os = "windows")]
fn default_config_file_path() -> Result<PathBuf> {
    use anyhow::ensure;

    let proj_dirs = ProjectDirs::from("io", "afnan", APP_NAME);
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
    use rstest::*;
    use std::env;

    // Tests the sample config that's in the docs
    #[test]
    fn test_sample_config() {
        let repo_root =
            env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| env::var("BUILD_DIR").unwrap());
        assert!(!repo_root.is_empty());
        let sample_config_path = [repo_root, "assets".into(), "sample_config.json5".into()]
            .iter()
            .collect::<PathBuf>();
        assert!(sample_config_path.exists());
        Config::try_from_file(Some(sample_config_path).as_ref(), false).unwrap();
    }

    // NOTE: we have to provide the file paths explicitly in the code, otherwise Rust won't know to
    // rerun if we add a new test case, for example. This is also the most ergonomic way to
    // parametrize on each file name so we can easily see which case failed.
    #[rstest]
    #[case("empty_dict.json5")]
    #[case("partial_section_1.json")]
    #[case("partial_section_2.json")]
    #[case("partial_section_3.json")]
    #[case("empty_config.toml")]
    #[case("partial_section_1.toml")]
    fn test_config_init(#[case] filename: &str) {
        let mut config_file_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        config_file_path.push("resources/test_configs");
        assert!(
            config_file_path.is_dir(),
            "test resource directory `{}` was not found",
            config_file_path.to_string_lossy()
        );
        config_file_path.push(filename);
        assert!(
            config_file_path.is_file(),
            "Expected test case file {} doesn't exist",
            config_file_path.to_string_lossy()
        );

        // We add the context so if there is an error you'll see the actual deserialization
        // error from serde and which file it failed on, which makes for a much more
        // informative error message in the test logs.
        Config::try_from_file(Some(&config_file_path), false)
            .with_context(|| {
                format!(
                    "Error parsing file: {}",
                    &config_file_path.file_name().unwrap().to_string_lossy()
                )
            })
            .unwrap();
    }
}
