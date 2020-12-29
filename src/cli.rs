//! Code related to the CLI

use crate::parse::supported_languages;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "diffsitter",
    about = "AST based diffs",
    setting = structopt::clap::AppSettings::ColoredHelp
)]
pub struct Args {
    /// List the file types supported by this build
    #[structopt(short, long = "list")]
    pub list: bool,

    /// The first file to compare against
    #[structopt(name = "A", parse(from_os_str), required_unless = "list")]
    pub a: Option<PathBuf>,

    /// The second file that is compared against A
    #[structopt(name = "B", parse(from_os_str), required_unless = "list")]
    pub b: Option<PathBuf>,

    /// You can manually set the file type. If this is not set, the file type will be deduced from
    /// the file's extension.
    #[structopt(short = "t", long = "file-type")]
    pub file_type: Option<String>,

    /// Output the parsed AST as a graphviz dot file
    #[structopt(short, long)]
    pub show_graph: bool,
}

/// Print a list of the languages that this instance of diffsitter was compiled with
pub fn list_supported_languages() {
    let languages = supported_languages();

    println!("This program was compiled with support for:");

    for language in languages {
        println!("- {}", language);
    }
}
