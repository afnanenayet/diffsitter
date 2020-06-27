mod ast;
mod cli;
mod parse;

use anyhow::Result;
use ast::{DiffVector, Edit};
use cli::Args;
use std::fs;
use structopt::StructOpt;
use tree_sitter::Parser;

fn main() -> Result<()> {
    let args: Args = Args::from_args();
    let text_a = fs::read_to_string(args.a)?;
    let text_b = fs::read_to_string(args.b)?;
    let language = unsafe { parse::tree_sitter_rust() };
    let mut parser_a = Parser::new();
    let mut parser_b = Parser::new();
    parser_a.set_language(language).unwrap();
    parser_b.set_language(language).unwrap();
    let ast_a = parser_a.parse(&text_a, None).unwrap();
    let ast_b = parser_b.parse(&text_b, None).unwrap();

    let diff_vec_a = DiffVector::from_ts_tree(&ast_a, &text_a);
    let diff_vec_b = DiffVector::from_ts_tree(&ast_b, &text_b);
    let edits = ast::min_edit(&diff_vec_a, &diff_vec_b);

    if edits.len() == 0 {
        println!("asts match");
    } else {
        println!("asts don't match");
    }

    for edit in edits {
        match edit {
            Edit::Addition(entry) => {
                print!("\n++ {}\n", entry.text);
            }
            Edit::Deletion(entry) => {
                print!("\n-- {}\n", entry.text);
            }
            Edit::Substitution { old, new } => {
                print!("\n-- {}\n++ {}\n", old.text, new.text);
            }
            _ => (),
        }
    }
    Ok(())
}
