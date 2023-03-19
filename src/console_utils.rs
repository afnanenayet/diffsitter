//! Helper functions for dealing with the terminal

use console::{set_colors_enabled, set_colors_enabled_stderr};
use strum::{Display, EnumString};

/// Whether the output to the terminal should be colored
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
#[derive(Default)]
pub enum ColorOutputPolicy {
    /// Automatically enable color if printing to a TTY, otherwise disable color
    #[default]
    Auto,
    /// Force plaintext output
    Off,
    /// Force color output
    On,
}

/// Set terminal color settings based on the output policy.
pub fn set_term_colors(setting: ColorOutputPolicy) {
    if setting == ColorOutputPolicy::Auto {
        return;
    }
    let colors_enabled = match setting {
        ColorOutputPolicy::On => true,
        ColorOutputPolicy::Off => false,
        ColorOutputPolicy::Auto => {
            panic!("Color output policy is auto, this case should have been already handled")
        }
    };
    set_colors_enabled(colors_enabled);
    set_colors_enabled_stderr(colors_enabled);
}
