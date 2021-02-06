mod ast;
mod cli;
mod diff;
mod formatting;
mod parse;

use anyhow::{anyhow, Result};
use ast::AstVector;
use cli::{default_config_file, list_supported_languages, set_term_colors, Args};
use console::Term;
use formatting::DisplayParameters;
use formatting::Options;
use log::{info, LevelFilter};
use std::fs;

/// Return an instance of [Options] froma config file path
///
/// If a config path isn't provided, then this will use the [default
/// path](cli::default_config_file).
fn derive_options(args: &Args) -> Result<Options> {
    if args.no_config {
        info!("`no_config` specified -- falling back to optional config");
        return Ok(Options::default());
    }
    let config_fp = if let Some(path) = args.config.as_ref() {
        path.clone()
    } else {
        default_config_file()
    };
    info!("Reading config at {:#?}", config_fp);

    if let Ok(config_str) = fs::read_to_string(config_fp) {
        return toml::from_str(&config_str).map_err(|e| anyhow!(e));
    }
    Ok(Options::default())
}

/// Take the diff of two files
fn run_diff(args: &Args) -> Result<()> {
    let options = derive_options(args)?;
    let path_a = args.old.as_ref().unwrap();
    let path_b = args.new.as_ref().unwrap();

    let old_text = fs::read_to_string(&path_a)?;
    info!("Reading {:#?} to string", &path_a);
    let new_text = fs::read_to_string(&path_b)?;
    info!("Reading {:#?} to string", &path_b);
    let file_type: Option<&str> = args.file_type.as_deref();

    if let Some(file_type) = file_type {
        info!("Using user-set filetype: {}", file_type);
    } else {
        info!("Will deduce filetype from file extension");
    }
    let ast_a = parse::parse_file(&path_a, file_type)?;
    let ast_b = parse::parse_file(&path_b, file_type)?;
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
    options.print(&mut term, &params)?;
    Ok(())
}

/// Serialize the default options struct to a TOML file and print that to stdout
fn dump_default_config() -> Result<()> {
    let config = Options::default();
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
