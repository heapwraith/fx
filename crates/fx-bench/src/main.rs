//! fx-bench: fixture generator and benchmarks (docs/PERFORMANCE.md).
//!
//! ```text
//! fx-bench fixtures           create missing fixtures in %TEMP%\fx-fixtures
//! fx-bench fixtures --clean   delete %TEMP%\fx-fixtures (and nothing else)
//! fx-bench enum               folder enumeration strategies on flat-100k (T0.3)
//! fx-bench list               sort and filter 100k entries in memory (T0.4)
//! ```
//! Run from the repository root: results go to `bench/results/`.

mod enum_bench;
mod fixtures;
mod list_bench;
mod names;
mod png;
mod report;
mod rng;

use std::process::ExitCode;
use std::time::Instant;

use fixtures::{Outcome, Spec};

const USAGE: &str = "usage: fx-bench fixtures [--clean] | enum | list";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["fixtures"] => run_fixtures(),
        ["fixtures", "--clean"] => run_clean(),
        ["enum"] => enum_bench::run(),
        ["list"] => list_bench::run(),
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
