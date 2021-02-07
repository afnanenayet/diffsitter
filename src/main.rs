mod ast;
mod cli;
mod config;
mod diff;
mod formatting;
mod parse;

use anyhow::Result;
use ast::AstVector;
use cli::{list_supported_languages, set_term_colors, Args};
use config::{Config, ConfigReadError};
use console::Term;
use formatting::DisplayParameters;
use log::{error, info, warn, LevelFilter};
use std::fs;

/// Return an instance of [Config] from a config file path (or the inferred default path)
///
/// If a config path isn't provided or otherwise fails, fall back to the default config
fn derive_config(args: &Args) -> Result<Config> {
    if args.no_config {
        info!("`no_config` specified, falling back to default config");
        return Ok(Config::default());
    }
    let config = match Config::try_from_file(args.config.as_ref()) {
        Ok(config) => config,
        Err(e) => match e {
            ConfigReadError::ReadFileFailure(e) => {
                warn!("{}, falling back to default config", e);
                Config::default()
            }
            ConfigReadError::DeserializationFailure(e) => {
                error!("Failed to deserialize config file: {}", e);
                return Err(anyhow::anyhow!(e));
            }
        },
    };
    Ok(config)
}

/// Take the diff of two files
fn run_diff(args: &Args) -> Result<()> {
    let config = derive_config(args)?;
    let path_a = args.old.as_ref().unwrap();
    let path_b = args.new.as_ref().unwrap();

    let old_text = fs::read_to_string(&path_a)?;
    info!("Reading {} to string", &path_a.to_string_lossy());
    let new_text = fs::read_to_string(&path_b)?;
    info!("Reading {} to string", &path_b.to_string_lossy());
    let file_type: Option<&str> = args.file_type.as_deref();

    if let Some(file_type) = file_type {
        info!("Using user-set filetype: {}", file_type);
    } else {
        info!("Will deduce filetype from file extension");
    }
    let ast_a = parse::parse_file(&path_a, file_type, config.file_associations.as_ref())?;
    let ast_b = parse::parse_file(&path_b, file_type, config.file_associations.as_ref())?;
    let diff_vec_a = AstVector::from_ts_tree(&ast_a, &old_text);
    let diff_vec_b = AstVector::from_ts_tree(&ast_b, &new_text);
    let (old_hunks, new_hunks) = ast::edit_hunks(&diff_vec_a, &diff_vec_b)?;
    let params = DisplayParameters {
        old_hunks: &old_hunks,
        new_hunks: &new_hunks,
        old_text: &old_text,
        new_text: &new_text,
    };
    let mut term = Term::stdout();
    config.formatting.print(&mut term, &params)?;
    Ok(())
}

/// Serialize the default options struct to a TOML file and print that to stdout
fn dump_default_config() -> Result<()> {
    let config = Config::default();
    let s = toml::to_string_pretty(&config)?;
    println!("{}", s);
    Ok(())
}

#[paw::main]
fn main(args: Args) -> Result<()> {
    use cli::Command;

    let log_level = if args.debug {
        LevelFilter::Trace
    } else {
        LevelFilter::Off
    };
    pretty_env_logger::formatted_timed_builder()
        .filter_level(log_level)
        .init();

    if let Some(cmd) = args.cmd {
        match cmd {
            Command::List => list_supported_languages(),
            Command::DumpDefaultConfig => dump_default_config()?,
        }
    } else {
        set_term_colors(args.color_output);
        run_diff(&args)?;
    }
    Ok(())
}
