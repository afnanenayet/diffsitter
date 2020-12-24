mod ast;
mod cli;
mod parse;

use anyhow::Result;
use ast::{DiffVector, Edit};
use cli::{list_supported_languages, Args};
use console::{style, Term};
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

    let text_a = fs::read_to_string(&path_a)?;
    let text_b = fs::read_to_string(&path_b)?;
    let file_type: Option<&str> = args.file_type.as_ref().map(|x| x.as_str());
    let ast_a = parse::parse_file(&path_a, file_type)?;
    let ast_b = parse::parse_file(&path_b, file_type)?;

    let diff_vec_a = DiffVector::from_ts_tree(&ast_a, &text_a);
    let diff_vec_b = DiffVector::from_ts_tree(&ast_b, &text_b);
    let entries = ast::min_edit(&diff_vec_a, &diff_vec_b);

    // Set up terminal output/formatting
    let term = Term::stdout();
    // The style to apply to added text
    let addition_style = console::Style::new().green();
    // The style to apply to deleted text
    let deletion_style = console::Style::new().red();

    // Iterate through each edit and print it out
    for entry in entries {
        match entry {
            Edit::Addition(new) => {
                term.write_line(
                    &addition_style
                        .apply_to(format!("> {}", new.text))
                        .to_string(),
                )?;
            }
            Edit::Deletion(old) => {
                term.write_line(
                    &deletion_style
                        .apply_to(format!("< {}", old.text))
                        .to_string(),
                )?;
            }
            Edit::Substitution { old, new } => {
                term.write_line(
                    &deletion_style
                        .apply_to(format!("< {}", old.text))
                        .to_string(),
                )?;
                term.write_line(
                    &addition_style
                        .apply_to(format!("> {}", new.text))
                        .to_string(),
                )?;
            }
            _ => (),
        }
    }
    Ok(())
}
