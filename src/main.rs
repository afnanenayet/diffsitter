mod ast;
mod cli;
mod parse;

use anyhow::Result;
use ast::{DiffVector, Edit};
use cli::Args;
use colour::{dark_green, red};
use std::fs;
use structopt::StructOpt;

fn main() -> Result<()> {
    let args: Args = Args::from_args();
    let text_a = fs::read_to_string(&args.a)?;
    let text_b = fs::read_to_string(&args.b)?;
    let file_type: Option<&str> = args.file_type.as_ref().map(|x| x.as_str());
    let ast_a = parse::parse_file(&args.a, file_type)?;
    let ast_b = parse::parse_file(&args.b, file_type)?;

    let diff_vec_a = DiffVector::from_ts_tree(&ast_a, &text_a);
    let diff_vec_b = DiffVector::from_ts_tree(&ast_b, &text_b);
    let entries = ast::min_edit(&diff_vec_a, &diff_vec_b);

    // Iterate through each edit and print it out
    for entry in entries {
        match entry {
            Edit::Addition(entry) => {
                dark_green!("+{}\n", entry.text);
            }
            Edit::Deletion(entry) => {
                red!("-{}\n", entry.text);
            }
            Edit::Substitution { old, new } => {
                red!("-{}\n", old.text);
                dark_green!("+{}\n", new.text);
            }
            _ => (),
        }
    }
    Ok(())
}
