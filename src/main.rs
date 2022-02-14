mod ast;
mod cli;
mod config;
mod diff;
mod formatting;
mod neg_idx_vec;
mod parse;

use crate::parse::supported_languages;
use anyhow::Result;
use ast::{AstVector, AstVectorData};
use clap::IntoApp;
use clap::Parser;
use cli::{Args, ColorOutputPolicy};
use config::{Config, ConfigReadError};
use console::Term;
use formatting::{DisplayParameters, DocumentDiffData};
use log::{debug, error, info, warn, LevelFilter};
use parse::{generate_language, language_from_ext, GrammarConfig};
use serde_json as json;
use std::{
    fs,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Child, Command},
};

#[cfg(feature = "better-build-info")]
build_info::build_info!(fn build_info);

#[cfg(feature = "jemallocator")]
use jemallocator::Jemalloc;

#[cfg(feature = "jemallocator")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

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
/// This returns an `AstVector` and a pinned struct with the owned data, which the `AstVector`
/// references.
///
/// `data` is used as an out-parameter. We need some external struct we can reference because the
/// return type references the data in that struct.
fn generate_ast_vector_data(
    path: PathBuf,
    file_type: Option<&str>,
    grammar_config: &GrammarConfig,
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
    let tree = parse::parse_file(&path, file_type, grammar_config)?;
    Ok(AstVectorData { text, tree, path })
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

/// Check if the input files are supported by this program.
///
/// If the user provides a language override, this will check that the language is supported by the
/// program. If the user supplies any extension mappings, this will check to see if the extension
/// is in the mapping or if it's one of the user-defined ones.
///
/// This is used to determine whether the program should fall back to another diff utility.
fn are_input_files_supported(args: &Args, config: &Config) -> bool {
    let paths = vec![&args.old, &args.new];

    // If there's a user override at the command line, that takes priority over everything else.
    if let Some(file_type) = &args.file_type {
        return generate_language(file_type, &config.grammar).is_ok();
    }

    // For each path, attempt to create a parser for that given extension, checking for any
    // possible overrides.
    for path in paths.into_iter().flatten() {
        debug!("Checking if {} can be parsed", path.display());
        let ext = path.extension();

        if ext.is_none() {
            return false;
        }

        let ext = ext.unwrap();

        if ext.to_str().is_none() {
            warn!("No filetype deduced for {}", path.display());
            return false;
        }

        let ext = ext.to_str().unwrap();

        if language_from_ext(ext, &config.grammar).is_err() {
            error!("Extension {} not supported", ext);
            return false;
        }
    }
    debug!("Extensions for both input files are supported");
    true
}

/// Take the diff of two files
fn run_diff(args: &Args, config: &Config) -> Result<()> {
    let file_type = args.file_type.as_deref();
    let path_a = args.old.as_ref().unwrap();
    let path_b = args.new.as_ref().unwrap();

    // This looks a bit weird because the ast vectors and some other data reference data in the
    // AstVectorData structs. Because of that, we can't make a function that generates the ast
    // vectors in one shot.

    let ast_data_a = generate_ast_vector_data(path_a.clone(), file_type, &config.grammar)?;
    let ast_data_b = generate_ast_vector_data(path_b.clone(), file_type, &config.grammar)?;

    let diff_vec_a = generate_ast_vector(&ast_data_a);
    let diff_vec_b = generate_ast_vector(&ast_data_b);

    let (old_hunks, new_hunks) = ast::compute_edit_script(&diff_vec_a, &diff_vec_b);
    let params = DisplayParameters {
        old: DocumentDiffData {
            filename: &ast_data_a.path.to_string_lossy(),
            hunks: &old_hunks,
            text: &ast_data_a.text,
        },
        new: DocumentDiffData {
            filename: &ast_data_b.path.to_string_lossy(),
            hunks: &new_hunks,
            text: &ast_data_b.text,
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

/// Print extended version information to the terminal
#[cfg(feature = "better-build-info")]
fn print_build_info() {
    println!(
        "{}",
        build_info::format!("{} v{} ({} {}) built with {}",
            $.crate_info.name,
            $.crate_info.version,
            $.version_control?.git()?.commit_short_id,
            $.timestamp,
            $.compiler
        )
    );
}

/// Run the diff fallback command using the command and the given paths.
fn diff_fallback(cmd: &str, old: &Path, new: &Path) -> io::Result<Child> {
    debug!("Spawning diff fallback process");
    Command::new(cmd).args([old, new]).spawn()
}

/// Print a list of the languages that this instance of diffsitter was compiled with
pub fn list_supported_languages() {
    #[cfg(feature = "static-grammar-libs")]
    {
        let languages = supported_languages();
        println!("This program was compiled with support for:");

        for language in languages {
            println!("- {}", language);
        }
    }

    #[cfg(feature = "dynamic-grammar-libs")]
    {
        println!("This program will dynamically load grammars from shared libraries");
    }
}

/// Set whether the terminal should display colors based on the user's preferences
///
/// This method will set the terminal output policy *for the current thread*.
fn set_term_colors(color_opt: ColorOutputPolicy) {
    match color_opt {
        ColorOutputPolicy::Off => (console::set_colors_enabled(false)),
        ColorOutputPolicy::On => (console::set_colors_enabled(true)),
        _ => (),
    };
}

/// Print shell completion scripts to `stdout`.
///
/// This is a basic wrapper for the subcommand.
fn print_shell_completion(shell: clap_complete::Shell) {
    let mut app = cli::Args::into_app();
    clap_complete::generate(shell, &mut app, "diffsitter", &mut io::stdout());
}

fn main() -> Result<()> {
    use cli::Command;
    let args = Args::parse();

    // We parse the config as early as possible so users can get quick feedback if anything is off
    // with their config.
    let config = derive_config(&args)?;

    // Users can supply a command that will *not* run a diff, which we handle here
    if let Some(cmd) = args.cmd {
        match cmd {
            Command::List => list_supported_languages(),
            Command::DumpDefaultConfig => dump_default_config()?,

            #[cfg(feature = "better-build-info")]
            Command::BuildInfo => print_build_info(),
            Command::GenCompletion { shell } => {
                print_shell_completion(shell.into());
            }
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

        // First check if the input files can be parsed with tree-sitter.
        let files_supported = are_input_files_supported(&args, &config);

        // If the files are supported by our grammars, awesome. Otherwise fall back to a diff
        // utility if one is specified.
        if files_supported {
            run_diff(&args, &config)?;
        } else if let Some(cmd) = config.fallback_cmd {
            info!("Input files are not supported but user has configured diff fallback");
            diff_fallback(&cmd, &args.old.unwrap(), &args.new.unwrap())?;
        } else {
            anyhow::bail!("Unsupported file type with no fallback command specified.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;
    use test_case::test_case;

    /// Get paths to input files for tests
    fn get_test_paths(test_type: &str, test_name: &str, ext: &str) -> (PathBuf, PathBuf) {
        let test_data_root = PathBuf::from(format!("./test_data/{}/{}", test_type, test_name));
        let path_a = test_data_root.join(format!("a.{}", ext));
        let path_b = test_data_root.join(format!("b.{}", ext));
        assert!(
            path_a.exists(),
            "test data path {} does not exist",
            path_a.to_str().unwrap()
        );
        assert!(
            path_b.exists(),
            "test data path {} does not exist",
            path_b.to_str().unwrap()
        );
        (path_a, path_b)
    }

    #[test_case("short", "rust", "rs")]
    #[test_case("short", "python", "py")]
    fn diff_hunks_snapshot(test_type: &str, name: &str, ext: &str) {
        let (path_a, path_b) = get_test_paths(test_type, name, ext);
        let config = GrammarConfig::default();
        let ast_data_a = generate_ast_vector_data(path_a, None, &config).unwrap();
        let ast_data_b = generate_ast_vector_data(path_b, None, &config).unwrap();
        let diff_vec_a = generate_ast_vector(&ast_data_a);
        let diff_vec_b = generate_ast_vector(&ast_data_b);
        let diff_hunks = ast::compute_edit_script(&diff_vec_a, &diff_vec_b);

        // We have to set the snapshot name manually, otherwise there appear to be threading issues
        // and we end up with more snapshot files than there are tests, which cause
        // nondeterministic errors.
        let snapshot_name = format!("{test_type}_{name}");
        assert_debug_snapshot!(snapshot_name, diff_hunks);
    }
}
