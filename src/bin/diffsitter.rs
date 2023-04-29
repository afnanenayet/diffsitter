use ::console::Term;
use anyhow::Result;
use clap::CommandFactory;
use clap::FromArgMatches;
#[cfg(panic = "unwind")]
use human_panic::setup_panic;
use libdiffsitter::cli;
use libdiffsitter::cli::Args;
use libdiffsitter::config::{Config, ReadError};
use libdiffsitter::console_utils;
use libdiffsitter::diff;
use libdiffsitter::generate_ast_vector_data;
#[cfg(feature = "static-grammar-libs")]
use libdiffsitter::parse::supported_languages;
use libdiffsitter::parse::{generate_language, language_from_ext};
use libdiffsitter::render::{DisplayData, DocumentDiffData, Renderer};
use log::{debug, error, info, warn, LevelFilter};
use serde_json as json;
use std::{
    io,
    path::Path,
    process::{Child, Command},
};

#[cfg(feature = "better-build-info")]
use shadow_rs::shadow;

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
            ReadError::ReadFileFailure(_) | ReadError::NoDefault => {
                warn!("{} - falling back to default config", e);
                Ok(Config::default())
            }
            // If we *do* find a config file and it doesn't parse correctly, we should return an
            // error and let the user know that their config is incorrect. This isn't a browser,
            // we can't just silently march forward and hope for the best.
            ReadError::DeserializationFailure(e) => {
                error!("Failed to deserialize config file: {}", e);
                Err(anyhow::anyhow!(e))
            }
        },
    }
}

/// Check if the input files are supported by this program.
///
/// If the user provides a language override, this will check that the language is supported by the
/// program. If the user supplies any extension mappings, this will check to see if the extension
/// is in the mapping or if it's one of the user-defined ones.
///
/// This is used to determine whether the program should fall back to another diff utility.
fn are_input_files_supported(args: &Args, config: &Config) -> bool {
    let paths = [&args.old, &args.new];

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
fn run_diff(args: Args, config: Config) -> Result<()> {
    // Check whether we can get the renderer up front. This is more ergonomic than running the diff
    // and then informing the user their renderer choice is incorrect/that the config is invalid.
    let render_config = config.formatting;
    let render_param = args.renderer;
    let renderer = render_config.get_renderer(render_param)?;

    let file_type = args.file_type.as_deref();
    let path_a = args.old.as_ref().unwrap();
    let path_b = args.new.as_ref().unwrap();

    // This looks a bit weird because the ast vectors and some other data reference data in the
    // AstVectorData structs. Because of that, we can't make a function that generates the ast
    // vectors in one shot.

    let ast_data_a = generate_ast_vector_data(path_a.clone(), file_type, &config.grammar)?;
    let ast_data_b = generate_ast_vector_data(path_b.clone(), file_type, &config.grammar)?;
    let diff_vec_a = config
        .input_processing
        .process(&ast_data_a.tree, &ast_data_a.text);
    let diff_vec_b = config
        .input_processing
        .process(&ast_data_b.tree, &ast_data_b.text);

    let hunks = diff::compute_edit_script(&diff_vec_a, &diff_vec_b)?;
    let params = DisplayData {
        hunks,
        old: DocumentDiffData {
            filename: &ast_data_a.path.to_string_lossy(),
            text: &ast_data_a.text,
        },
        new: DocumentDiffData {
            filename: &ast_data_b.path.to_string_lossy(),
            text: &ast_data_b.text,
        },
    };
    // Use a buffered terminal instead of a normal unbuffered terminal so we can amortize the cost
    // of printing. It doesn't really matter how frequently the terminal prints to stdout because
    // the user just cares about the output at the end, we don't care about how frequently the
    // terminal does partial updates or anything like that. If the user is curious about progress,
    // they can enable logging and see when hunks are processed and written to the buffer.
    let mut buf_writer = Term::buffered_stdout();
    let term_info = buf_writer.clone();
    renderer.render(&mut buf_writer, &params, Some(&term_info))?;
    buf_writer.flush()?;
    Ok(())
}

/// Serialize the default options struct to a json file and print that to stdout
fn dump_default_config() -> Result<()> {
    let config = Config::default();
    println!("{}", json::to_string_pretty(&config)?);
    Ok(())
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
            println!("- {language}");
        }
    }

    #[cfg(feature = "dynamic-grammar-libs")]
    {
        println!("This program will dynamically load grammars from shared libraries");
    }
}

/// Print shell completion scripts to `stdout`.
///
/// This is a basic wrapper for the subcommand.
fn print_shell_completion(shell: clap_complete::Shell) {
    let mut app = cli::Args::command();
    clap_complete::generate(shell, &mut app, "diffsitter", &mut io::stdout());
}

fn main() -> Result<()> {
    // Set up a panic handler that will yield more human-readable errors.
    #[cfg(panic = "unwind")]
    setup_panic!();

    #[cfg(feature = "better-build-info")]
    shadow!(build);

    use cli::Command;

    #[cfg(feature = "better-build-info")]
    let command = Args::command().version(build::CLAP_LONG_VERSION);

    #[cfg(not(feature = "better-build-info"))]
    let command = Args::command();
    let matches = command.get_matches();
    let args = Args::from_arg_matches(&matches)?;

    // We parse the config as early as possible so users can get quick feedback if anything is off
    // with their config.
    let config = derive_config(&args)?;

    // Users can supply a command that will *not* run a diff, which we handle here
    if let Some(cmd) = args.cmd {
        match cmd {
            Command::List => list_supported_languages(),
            Command::DumpDefaultConfig => dump_default_config()?,
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
        console_utils::set_term_colors(args.color_output);
        // First check if the input files can be parsed with tree-sitter.
        let files_supported = are_input_files_supported(&args, &config);

        // If the files are supported by our grammars, awesome. Otherwise fall back to a diff
        // utility if one is specified.
        if files_supported {
            run_diff(args, config)?;
        } else if let Some(cmd) = config.fallback_cmd {
            info!("Input files are not supported but user has configured diff fallback");
            diff_fallback(&cmd, &args.old.unwrap(), &args.new.unwrap())?;
        } else {
            anyhow::bail!("Unsupported file type with no fallback command specified.");
        }
    }
    Ok(())
}
