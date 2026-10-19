//! Command-line distribution entry point for Egrissa.

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("--help") | Some("-h") | Some("help") => {
            println!(
                "Egrissa {}\n\nUsage: egrissa [--version]",
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::SUCCESS
        }
        Some("--version") | Some("-V") | Some("version") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(argument) => {
            eprintln!("unknown argument: {argument}\nRun `egrissa --help` for usage.");
            ExitCode::from(2)
        }
    }
}
