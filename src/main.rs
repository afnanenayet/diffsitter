mod ast;
mod cli;
mod config;
mod diff;
mod formatting;
mod parse;

use anyhow::Result;
use ast::{AstVector, AstVectorData};
use cli::{list_supported_languages, set_term_colors, Args};
use config::{Config, ConfigReadError};
use console::Term;
use formatting::{DisplayParameters, DocumentDiffData};
use log::{debug, error, info, warn, LevelFilter};
use rayon::iter::ParallelIterator;
use rayon::prelude::*;
use serde_json as json;
use std::{
    collections::HashMap,
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
};

/// Return an instance of [Config] from a config file path (or the inferred default path)
///
/// If a config path isn't provided or there is some other failure, fall back to the default
/// config. This will error out if a config is found but is found to be an invalid config.
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
                warn!("{} - falling back to default config", e);
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

/// Create an AST vector from a path
///
/// This returns an AstVector and a pinned struct with the owned data, which the AstVector
/// references.
///
/// `data` is used as an out-parameter. We need some external struct we can reference because the
/// return type references the data in that struct.
fn generate_ast_vector_data(
    path: PathBuf,
    file_type: Option<&str>,
    file_associations: Option<&HashMap<String, String>>,
) -> Result<AstVectorData> {
    let text = fs::read_to_string(&path)?;
    let file_name = path.to_string_lossy();
    debug!("Reading {} to string", file_name);

    if let Some(file_type) = file_type {
        info!(
            "Using user-set filetype \"{}\" for {}",
            file_type, file_name
        );
    } else {
        info!("Will deduce filetype from file extension");
    };
    let tree = parse::parse_file(&path, file_type, file_associations)?;
    Ok(AstVectorData { tree, text, path })
}

/// Generate an AST vector from the underlying data
///
/// This is split off into a function so we can handle things like logging and keep the code DRY
fn generate_ast_vector(data: &AstVectorData) -> AstVector<'_> {
    let ast_vec = AstVector::from_ts_tree(&data.tree, &data.text);
    info!(
        "Constructed a diff vector with {} nodes for {}",
        ast_vec.len(),
        data.path.to_string_lossy(),
    );
    ast_vec
}

/// Take the diff of two files
fn run_diff(args: &Args) -> Result<()> {
    let config = derive_config(args)?;

    let file_type = args.file_type.as_deref();
    let file_associations = config.file_associations.as_ref();
    let path_a = args.old.as_ref().unwrap();
    let path_b = args.new.as_ref().unwrap();
    let paths = vec![path_a, path_b];
    let ast_data: Result<Vec<AstVectorData>> = paths
        .par_iter()
        .map(|p| generate_ast_vector_data(p.to_path_buf(), file_type, file_associations))
        .collect();
    let ast_data = ast_data.map_err(|e| anyhow::format_err!(e))?;
    let diff_vec: Vec<_> = ast_data
        .iter()
        .map(|data| generate_ast_vector(data))
        .collect();

    // This looks a bit weird because a the ast vectors and some other data reference data in the
    // AstVectorData structs. Because of that, we can't make a function that generates the ast vectors in
    // one shot.

    let (old_hunks, new_hunks) = ast::edit_hunks(&diff_vec[0], &diff_vec[1])?;
    let params = DisplayParameters {
        old: DocumentDiffData {
            filename: &ast_data[0].path.to_string_lossy(),
            hunks: &old_hunks,
            text: &ast_data[0].text,
        },
        new: DocumentDiffData {
            filename: &ast_data[1].path.to_string_lossy(),
            hunks: &new_hunks,
            text: &ast_data[1].text,
        },
    };
    // Use a buffered terminal instead of a normal unbuffered terminal so we can amortize the cost of printing. It
    // doesn't really how frequently the terminal prints to stdout because the user just cares about the output at the
    // end, we don't care about how frequently the terminal does partial updates or anything like that. If the user is
    // curious about progress, they can enable logging and see when hunks are processed and written to the buffer.
    let mut buf_writer = BufWriter::new(Term::stdout());
    config.formatting.print(&mut buf_writer, &params)?;
    // Just in case we forgot to flush anything in the `print` function
    buf_writer.flush()?;
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

    // Users can supply a command that will *not* run a diff, which we handle here
    if let Some(cmd) = args.cmd {
        match cmd {
            Command::List => list_supported_languages(),
            Command::DumpDefaultConfig => dump_default_config()?,
        }
    } else {
        let log_level = if args.debug {
            LevelFilter::Trace
        } else {
            LevelFilter::Off
        };
        pretty_env_logger::formatted_timed_builder()
            .filter_level(log_level)
            .init();
        set_term_colors(args.color_output);
        run_diff(&args)?;
    }
    Ok(())
}
