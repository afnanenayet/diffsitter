//! Code related to the CLI

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "diffsitter", about = "AST based diffs")]
pub struct Args {
    /// The first file to compare against
    #[structopt(name = "A", parse(from_os_str))]
    pub a: PathBuf,

    /// The second file that is compared against A
    #[structopt(name = "B", parse(from_os_str))]
    pub b: PathBuf,

    /// You can manually set the file type. If this is not set, the file type will be deduced from
    /// the file's extension.
    #[structopt(short = "t", long = "file-type")]
    pub file_type: Option<String>,

    /// List the file types supported by this build
    #[structopt(short, long)]
    pub list_file_types: bool,

    /// Output the parsed AST as a graphviz dot file
    #[structopt(short, long)]
    pub show_graph: bool,
}
