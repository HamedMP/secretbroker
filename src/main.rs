#![forbid(unsafe_code)]

use clap::Parser;
use secretbroker::cli::{Cli, execute};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match execute(cli).await {
        Ok(0) => {}
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("secretbroker: {error:#}");
            std::process::exit(1);
        }
    }
}
