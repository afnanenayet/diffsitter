mod cli;

use cli::Args;
use structopt::StructOpt;

fn main() {
    let args = Args::from_args();
    println!("{:?}", args);
}
