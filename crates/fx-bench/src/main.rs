//! fx-bench: fixture generator and benchmarks (docs/PERFORMANCE.md).
//!
//! ```text
//! fx-bench fixtures           create missing fixtures in %TEMP%\fx-fixtures
//! fx-bench fixtures --clean   delete %TEMP%\fx-fixtures (and nothing else)
//! fx-bench enum               folder enumeration strategies on flat-100k (T0.3)
//! fx-bench list               sort and filter 100k entries in memory (T0.4)
//! fx-bench open               P2: 10 openings of flat-100k in fx.exe (T0.5)
//! fx-bench scroll             P7: frame times while scrolling (moves the mouse!) (T0.5)
//! fx-bench responsive <dir>   window responsiveness while <dir> loads (T0.5)
//! fx-bench type               P4: key -> frame latency typing to filter (sends keys!) (T0.6)
//! fx-bench nav                P4: arrows, Enter and Backspace on flat-100k (sends keys!) (T1.2)
//! fx-bench path               P4: path bar autocompletion and history (sends keys!) (T1.3)
//! fx-bench tabs               P2 (b) for new tabs and P4 for tab keys (sends keys!) (T1.4)
//! fx-bench drives             new drive letter -> sidebar, with subst (T1.5)
//! fx-bench copy               P4 and P7 while copying flat-10k-media (sends keys, moves the mouse!) (T1.13)
//! fx-bench menu               right click -> context menu, 20 menus (moves the mouse!) (T1.16)
//! fx-bench memory             P8: private memory idle on a small folder (T2.3)
//! fx-bench features           palette re-sort, disk search, Quick Look with real input (T1.20-T1.24)
//! fx-bench grid               grid view: P2 (b) and P7 on flat-10k-media (sends keys, moves the mouse!) (T1.7)
//! fx-bench index --volume C:  P6/P9: index the real MFT (needs Administrator) (T0.7)
//! fx-bench index --synthetic N  index N generated files in memory (T0.7)
//! fx-bench search (--volume C: | --synthetic N) [--interactive]  P5 (T0.8)
//! fx-bench all                every benchmark that needs neither Administrator nor
//!                             control of the mouse/keyboard (enum, list, open,
//!                             index and search on a synthetic 2M index)
//! ```
//! Run from the repository root: results go to `bench/results/`.

mod enum_bench;
mod fixtures;
mod index_bench;
mod list_bench;
mod names;
mod png;
mod report;
mod rng;
mod search_bench;
mod ui_bench;

use std::process::ExitCode;
use std::time::Instant;

use fixtures::{Outcome, Spec};

const USAGE: &str = "usage: fx-bench fixtures [--clean] | enum | list | open | scroll | responsive <folder> | type | nav | path | tabs | drives | grid | copy | menu | features | memory | index (--volume C: | --synthetic N) | search (--volume C: | --synthetic N) [--interactive] | all";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["fixtures"] => run_fixtures(),
        ["fixtures", "--clean"] => run_clean(),
        ["enum"] => enum_bench::run(),
        ["list"] => list_bench::run(),
        ["open"] => ui_bench::open(),
        ["scroll"] => ui_bench::scroll(),
        ["type"] => ui_bench::type_keys(),
        ["nav"] => ui_bench::nav_keys(),
        ["path"] => ui_bench::path_keys(),
        ["tabs"] => ui_bench::tab_keys(),
        ["drives"] => ui_bench::drive_latency(),
        ["grid"] => ui_bench::grid(),
        ["copy"] => ui_bench::copy_while_using(),
        ["menu"] => ui_bench::menu_latency(),
        ["features"] => ui_bench::features(),
        ["memory"] => ui_bench::memory(),
        ["all"] => run_all(),
        ["index", "--volume", volume] => index_bench::volume(volume),
        ["search", "--volume", v] => {
            search_bench::run(&search_bench::Source::Volume((*v).to_owned()), false)
        }
        ["search", "--volume", v, "--interactive"] => {
            search_bench::run(&search_bench::Source::Volume((*v).to_owned()), true)
        }
        ["search", "--synthetic", n, rest @ ..] if rest.is_empty() || rest == ["--interactive"] => {
            match n.parse() {
                Ok(n) => search_bench::run(&search_bench::Source::Synthetic(n), !rest.is_empty()),
                Err(_) => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(2);
                }
            }
        }
        ["index", "--synthetic", n] => match n.parse() {
            Ok(n) => index_bench::synthetic(n),
            Err(_) => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        },
        ["responsive", folder] => ui_bench::responsive(std::path::Path::new(folder), 10),
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

/// Benchmarks that run unattended: no Administrator, no synthetic input.
/// `scroll` (P7) and `type` (P4) move the mouse / send keys, and `--volume C:`
/// (P5/P6/P9 on the real disk) needs Administrator, so they are run separately.
fn run_all() -> std::io::Result<()> {
    let synthetic = search_bench::Source::Synthetic(2_000_000);
    let steps: [(&str, &dyn Fn() -> std::io::Result<()>); 5] = [
        ("enum", &enum_bench::run),
        ("list", &list_bench::run),
        ("open", &ui_bench::open),
        ("index --synthetic 2000000", &|| {
            index_bench::synthetic(2_000_000)
        }),
        ("search --synthetic 2000000", &|| {
            search_bench::run(&synthetic, false)
        }),
    ];
    let mut failed = Vec::new();
    for (name, step) in steps {
        println!("== fx-bench {name}");
        if let Err(e) = step() {
            eprintln!("fx-bench {name}: {e}");
            failed.push(name);
        }
    }
    println!(
        "== skipped (run by hand): scroll (moves the mouse), type, nav, path and tabs (send keys), index/search --volume C: (Administrator)"
    );
    if failed.is_empty() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "failed: {}",
            failed.join(", ")
        )))
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
