//! Rusty command-line tooling.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match rusty_cli::run(arguments) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            if error.is_usage() {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}
