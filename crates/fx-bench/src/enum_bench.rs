//! `fx-bench enum`: folder enumeration strategies on `flat-100k` (task T0.3).

use std::path::Path;
use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use fx_fs::Strategy;

use crate::fixtures;
use crate::report::{commit, json_string, machine, median_p95, results_dir, utc_date};

const FOLDER: &str = "flat-100k";
/// Measured runs; one extra warm-up run is reported separately ("first run").
const RUNS: usize = 10;
const TARGET_FIRST_BATCH_MS: f64 = 20.0;
const TARGET_TOTAL_MS: f64 = 150.0;

struct Run {
    first_batch: Duration,
    total: Duration,
    items: usize,
}

/// Time to first batch and to the end, as seen by the receiver (the UI's view).
fn run_once(strategy: Strategy, dir: &Path) -> std::io::Result<Run> {
    let (tx, rx) = channel();
    let start = Instant::now();
    let worker = {
        let dir = dir.to_path_buf();
        thread::spawn(move || fx_fs::enumerate(strategy, &dir, &tx))
    };
    let mut first_batch = None;
    let mut items = 0;
    for batch in rx {
        first_batch.get_or_insert_with(|| start.elapsed());
        items += batch.len();
    }
    let total = start.elapsed();
    worker
        .join()
        .map_err(|_| std::io::Error::other("enumeration thread panicked"))??;
    Ok(Run {
        first_batch: first_batch.unwrap_or(total),
        total,
        items,
    })
}

pub fn run() -> std::io::Result<()> {
    let dir = fixtures::default_root().join(FOLDER);
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} missing: run `fx-bench fixtures` first", dir.display()),
        ));
    }
    let out_dir = results_dir()?;
    let date = utc_date(SystemTime::now());
    let machine = machine();
    let expected = fixtures::Spec::FULL.flat_files;

    println!("{} — {RUNS} runs after 1 warm-up, release", dir.display());
    println!(
        "  {:<22} {:>14} {:>14} {:>14} {:>14}   targets: first ≤ {TARGET_FIRST_BATCH_MS} ms, total ≤ {TARGET_TOTAL_MS} ms",
        "strategy", "first med/p95", "", "total med/p95", ""
    );
    for strategy in Strategy::ALL {
        let warmup = run_once(strategy, &dir)?;
        let runs: Vec<Run> = (0..RUNS)
            .map(|_| run_once(strategy, &dir))
            .collect::<Result<_, _>>()?;
        if let Some(bad) = runs.iter().map(|r| r.items).find(|&n| n != expected) {
            return Err(std::io::Error::other(format!(
                "{}: got {bad} entries, expected {expected}",
                strategy.id()
            )));
        }
        let firsts: Vec<Duration> = runs.iter().map(|r| r.first_batch).collect();
        let totals: Vec<Duration> = runs.iter().map(|r| r.total).collect();
        let (first_med, first_p95) = median_p95(&firsts);
        let (total_med, total_p95) = median_p95(&totals);
        let pass = first_p95 <= TARGET_FIRST_BATCH_MS && total_p95 <= TARGET_TOTAL_MS;
        println!(
            "  {:<22} {:>11.2} ms {:>11.2} ms {:>11.2} ms {:>11.2} ms   {}",
            strategy.id(),
            first_med,
            first_p95,
            total_med,
            total_p95,
            if pass { "pass" } else { "FAIL" }
        );

        let json = format!(
            concat!(
                "{{\n",
                "  \"id\": \"T0.3-enum\",\n",
                "  \"strategy\": {strategy},\n",
                "  \"folder\": {folder},\n",
                "  \"items\": {items},\n",
                "  \"runs\": {runs},\n",
                "  \"mode\": \"warm (1 warm-up discarded)\",\n",
                "  \"first_batch_ms\": {{ \"median\": {fm:.3}, \"p95\": {fp:.3}, \"target\": {ft} }},\n",
                "  \"total_ms\": {{ \"median\": {tm:.3}, \"p95\": {tp:.3}, \"target\": {tt} }},\n",
                "  \"warmup_run_ms\": {{ \"first_batch\": {wf:.3}, \"total\": {wt:.3} }},\n",
                "  \"pass\": {pass},\n",
                "  \"machine\": {machine},\n",
                "  \"commit\": {commit},\n",
                "  \"date\": {date}\n",
                "}}\n"
            ),
            strategy = json_string(strategy.id()),
            folder = json_string(FOLDER),
            items = expected,
            runs = RUNS,
            fm = first_med,
            fp = first_p95,
            ft = TARGET_FIRST_BATCH_MS,
            tm = total_med,
            tp = total_p95,
            tt = TARGET_TOTAL_MS,
            wf = warmup.first_batch.as_secs_f64() * 1000.0,
            wt = warmup.total.as_secs_f64() * 1000.0,
            pass = pass,
            machine = json_string(&machine),
            commit = json_string(commit()),
            date = json_string(&date),
        );
        std::fs::write(
            out_dir.join(format!("{date}-T0.3-{}.json", strategy.id())),
            json,
        )?;
    }
    println!("results in {}", out_dir.display());
    Ok(())
}
