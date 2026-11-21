//! fx-bench: fixture generator and benchmarks (docs/PERFORMANCE.md).
//!
//! ```text
//! fx-bench fixtures           create missing fixtures in %TEMP%\fx-fixtures
//! fx-bench fixtures --clean   delete %TEMP%\fx-fixtures (and nothing else)
//! ```
//! Benchmarks are added in tasks T0.3, T0.7 and T0.8.

mod fixtures;
mod names;
mod png;
mod rng;

use std::process::ExitCode;
use std::time::Instant;

use fixtures::{Outcome, Spec};

const USAGE: &str = "usage: fx-bench fixtures [--clean]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["fixtures"] => run_fixtures(),
        ["fixtures", "--clean"] => run_clean(),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fx-bench: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_fixtures() -> std::io::Result<()> {
    let root = fixtures::default_root();
    println!("fixtures in {}", root.display());
    let start = Instant::now();
    for r in fixtures::generate(&root, &Spec::FULL)? {
        let what = match r.outcome {
            Outcome::Created => "created",
            Outcome::AlreadyComplete => "already complete",
        };
        println!(
            "  {:<16} {:>7} files  {:<16} {:>8.2} s",
            r.name,
            r.files,
            what,
            r.elapsed.as_secs_f64()
        );
    }
    println!("total {:.2} s", start.elapsed().as_secs_f64());
    Ok(())
}

fn run_clean() -> std::io::Result<()> {
    let root = fixtures::default_root();
    if fixtures::clean(&root)? {
        println!("deleted {}", root.display());
    } else {
        println!("nothing to delete: {} does not exist", root.display());
    }
    Ok(())
}
