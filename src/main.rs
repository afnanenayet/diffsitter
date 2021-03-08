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
use formatting::{DisplayParameters, DocumentDiffData};
use log::{error, info, warn, LevelFilter};
use serde_json as json;
use std::fs;

/// Return an instance of [Config] from a config file path (or the inferred default path)
///
/// If a config path isn't provided or otherwise fails, fall back to the default config
fn derive_config(args: &Args) -> Result<Config> {
    if args.no_config {
        info!("`no_config` specified, falling back to default config");
        return Ok(Config::default());
    }
    match Config::try_from_file(args.config.as_ref()) {
        // If the config was parsed correctly with no issue, we don't have to do anything
        Ok(config) => Ok(config),
        // If there was an error, we need to figure out whether to propagate the error or fall
        // back to the default config
        Err(e) => match e {
            // If it is a recoverable error, ex: not being able to find the default file path or
            // not finding a file at all isn't a hard error, it makes sense for us to use the
            // default config.
            ConfigReadError::ReadFileFailure(_) | ConfigReadError::NoDefault => {
                warn!("{:#?} - falling back to default config", e);
                Ok(Config::default())
            }
            // If we *do* find a config file and it doesn't parse correctly, we should return an
            // error and let the user know that their config is incorrect. This isn't a browser,
            // we can't just silently march forward and hope for the best.
            ConfigReadError::DeserializationFailure(e) => {
                error!("Failed to deserialize config file: {}", e);
                Err(anyhow::anyhow!(e))
            }
        },
    }
}

/// Take the diff of two files
fn run_diff(args: &Args) -> Result<()> {
    let config = derive_config(args)?;
    let path_old = args.old.as_ref().unwrap();
    let path_old_name = path_old.to_string_lossy();
    let path_new = args.new.as_ref().unwrap();
    let path_new_name = path_new.to_string_lossy();

    let old_text = fs::read_to_string(&path_old)?;
    info!("Reading {} to string", &path_old_name);
    let new_text = fs::read_to_string(&path_new)?;
    info!("Reading {} to string", &path_new_name);
    let file_type: Option<&str> = args.file_type.as_deref();

    if let Some(file_type) = file_type {
        info!("Using user-set filetype: {}", file_type);
    } else {
        info!("Will deduce filetype from file extension");
    }
    let ast_a = parse::parse_file(&path_old, file_type, config.file_associations.as_ref())?;
    let ast_b = parse::parse_file(&path_new, file_type, config.file_associations.as_ref())?;
    let diff_vec_a = AstVector::from_ts_tree(&ast_a, &old_text);
    let diff_vec_b = AstVector::from_ts_tree(&ast_b, &new_text);
    let (old_hunks, new_hunks) = ast::edit_hunks(&diff_vec_a, &diff_vec_b)?;
    let params = DisplayParameters {
        old: DocumentDiffData {
            filename: &path_old_name,
            hunks: &old_hunks,
            text: &old_text,
        },
        new: DocumentDiffData {
            filename: &path_new_name,
            hunks: &new_hunks,
            text: &new_text,
        },
    };
    // Use a buffered terminal instead of a normal unbuffered terminal so we can amortize the cost of printing. It
    // doesn't really how frequently the terminal prints to stdout because the user just cares about the output at the
    // end, we don't care about how frequently the terminal does partial updates or anything like that. If the user is
    // curious about progress, they can enable logging and see when hunks are processed and written to the buffer.
    let mut term = Term::buffered_stdout();
    config.formatting.print(&mut term, &params)?;
    // Just in case we forgot to flush anything in the `print` function
    term.flush()?;
    Ok(())
}

/// Serialize the default options struct to a json file and print that to stdout
fn dump_default_config() -> Result<()> {
    let config = Config::default();
    println!("{}", json::to_string_pretty(&config)?);
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
