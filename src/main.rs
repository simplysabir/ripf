mod cli;
mod config;
mod open;

use std::dbg;

use clap::Parser;
fn main() {
    let cli = cli::Cli::parse();
    dbg!(&cli); // TEMPORARY - will be replaced later
}
