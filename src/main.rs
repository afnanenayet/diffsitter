mod ast;
mod cli;
mod formatting;
mod parse;

use anyhow::Result;
use ast::AstVector;
use cli::{list_supported_languages, Args};
use console::Term;
use formatting::DisplayParameters;
use formatting::Options;
use paw;
use std::fs;

#[paw::main]
fn main(args: Args) -> Result<()> {
    if args.list {
        list_supported_languages();
        return Ok(());
    }

    let path_a = args.a.unwrap();
    let path_b = args.b.unwrap();

    let old_text = fs::read_to_string(&path_a)?;
    let new_text = fs::read_to_string(&path_b)?;
    let file_type: Option<&str> = args.file_type.as_ref().map(|x| x.as_str());
    let ast_a = parse::parse_file(&path_a, file_type)?;
    let ast_b = parse::parse_file(&path_b, file_type)?;

    let diff_vec_a = AstVector::from_ts_tree(&ast_a, &old_text);
    let diff_vec_b = AstVector::from_ts_tree(&ast_b, &new_text);
    let entries = ast::min_edit(&diff_vec_a, &diff_vec_b);

    // Set up display options
    let options = Options::default();
    let params = DisplayParameters {
        diff: &entries,
        old_text: &old_text,
        new_text: &new_text,
    };
    let mut term = Term::stdout();
    options.line_by_line(&mut term, &params)?;
    Ok(())
}
