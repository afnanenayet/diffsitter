//! Code related to the CLI

use crate::parse::supported_languages;
use std::path::PathBuf;
use structopt::StructOpt;
use strum_macros::{EnumString, ToString};

#[derive(Debug, Eq, PartialEq, Clone, StructOpt)]
#[structopt(
    name = "diffsitter",
    about = "AST based diffs",
    settings = &[
        structopt::clap::AppSettings::ColoredHelp,
        structopt::clap::AppSettings::ColorAuto,
        structopt::clap::AppSettings::GlobalVersion,
    ]
)]
pub struct Args {
    /// Print debug output
    ///
    /// This will print debug logs at the trace level. This is useful for debugging and bug reports
    /// should contain debug logging info.
    #[structopt(short, long)]
    pub debug: bool,
    /// Run a subcommand that doesn't perform a diff. Valid options are: "list",
    /// "dump_default_config".
    ///
    /// "list" will list all of the filetypes/languages that this program was compiled with support
    /// for. "dump_default_config" will dump the default configuration to stdout.
    #[structopt(long)]
    pub cmd: Option<Command>,
    /// The first file to compare against
    ///
    /// Text that is in this file but is not in the new file is considered a deletion
    #[structopt(name = "OLD", parse(from_os_str), required_unless = "cmd")]
    pub old: Option<PathBuf>,
    /// The file that the old file is compared against
    ///
    /// Text that is in this file but is not in the old file is considered an addition
    #[structopt(name = "NEW", parse(from_os_str), required_unless = "cmd")]
    pub new: Option<PathBuf>,
    /// Manually set the file type for the given files
    ///
    /// This will dictate which parser is used with the difftool. You can list all of the valid
    /// file type strings with `diffsitter --cmd list`
    #[structopt(short = "t", long = "filetype")]
    pub file_type: Option<String>,
    /// Use the config provided at the given path
    ///
    /// By default, diffsitter attempts to find the config at `$XDG_CONFIG_HOME/diffsitter.json5`.
    /// On Windows the app will look in the standard config path.
    #[structopt(short, long, env = "DIFFSITTER_CONFIG")]
    pub config: Option<PathBuf>,
    /// Set the color output policy. Valid values are: "auto", "on", "off".
    ///
    /// "auto" will automatically detect whether colors should be applied by trying to determine
    /// whether the process is outputting to a TTY. "on" will enable output and "off" will
    /// disable color output regardless of whether the process detects a TTY.
    #[structopt(long = "color", default_value)]
    pub color_output: ColorOutputPolicy,
    /// Ignore any config files and use the default config
    ///
    /// This will cause the app to ignore any configs and all config values will use the their
    /// default settings.
    #[structopt(short, long)]
    pub no_config: bool,
}

/// Commands related to the configuration
#[derive(Debug, Eq, PartialEq, Clone, Copy, StructOpt, EnumString)]
#[strum(serialize_all = "snake_case")]
#[structopt(
    settings = &[
        structopt::clap::AppSettings::ColoredHelp,
        structopt::clap::AppSettings::ColorAuto,
        structopt::clap::AppSettings::GlobalVersion,
    ]
)]
pub enum Command {
    /// List the languages that this program was compiled for
    List,
    /// Dump the default config to stdout
    DumpDefaultConfig,
}

/// Print a list of the languages that this instance of diffsitter was compiled with
pub fn list_supported_languages() {
    let languages = supported_languages();

    println!("This program was compiled with support for:");

    for language in languages {
        println!("- {}", language);
    }
}

/// Whether the output to the terminal should be colored
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, ToString)]
#[strum(serialize_all = "snake_case")]
pub enum ColorOutputPolicy {
    /// Automatically enable color if printing to a TTY, otherwise disable color
    Auto,
    /// Force plaintext output
    Off,
    /// Force color output
    On,
}

impl Default for ColorOutputPolicy {
    fn default() -> Self {
        ColorOutputPolicy::Auto
    }
}

/// Set whether the terminal should display colors based on the user's preferences
///
/// This method will set the terminal output policy *for the current thread*.
pub fn set_term_colors(color_opt: ColorOutputPolicy) {
    match color_opt {
        ColorOutputPolicy::Off => (console::set_colors_enabled(false)),
        ColorOutputPolicy::On => (console::set_colors_enabled(true)),
        _ => (),
    };
}
