//! `fx-bench list`: sorting and filtering 100k entries in `fx_core::FileList`
//! (task T0.4). Entries are built in memory with the same names as `flat-100k`
//! (plus 5% folders), so no disk access is measured.

use std::ffi::OsString;
use std::time::{Duration, Instant, SystemTime};

use fx_core::{Attributes, Direction, Entry, FileList, SortKey};

use crate::fixtures::{Spec, item_rng};
use crate::names::flat_name;
use crate::report::{commit, json_string, machine, median_p95, results_dir, utc_date};

const RUNS: usize = 10;
const TARGET_SORT_MS: f64 = 50.0;
const TARGET_FILTER_MS: f64 = 5.0;
/// Typed one key at a time; every prefix is one "filter per keystroke" sample.
const TYPED: &str = "report";

fn entries(n: usize) -> Vec<Entry> {
    (0..n)
        .map(|i| {
            let mut rng = item_rng(1, i);
            let name = flat_name(&mut rng, i);
            let folder = i % 20 == 0;
            Entry {
                name: OsString::from(name),
                size: if folder {
                    0
                } else {
                    rng.next_u64() % 10_000_000
                },
                modified: SystemTime::UNIX_EPOCH
                    + Duration::from_secs(1_500_000_000 + rng.next_u64() % 300_000_000),
                attributes: if folder {
                    Attributes::DIRECTORY
                } else {
                    Attributes::empty()
                },
            }
        })
        .collect()
}

fn fresh_list(source: &[Entry]) -> FileList {
    let mut list = FileList::new();
    list.extend(source.iter().cloned());
    list
}

pub fn run() -> std::io::Result<()> {
    let out_dir = results_dir()?;
    let n = Spec::FULL.flat_files;
    let source = entries(n);

    // Append (informative): the cost of storing one folder's entries.
    let mut extend = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let start = Instant::now();
        let list = fresh_list(&source);
        extend.push(start.elapsed());
        assert_eq!(list.len(), n);
    }

    // Sort by name from arrival order (a fresh list each run).
    let mut sort = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let mut list = fresh_list(&source);
        let start = Instant::now();
        list.sort(SortKey::Name, Direction::Ascending);
        sort.push(start.elapsed());
    }

    // Filter per keystroke on the sorted list: r, re, rep, ... (worst case is the
    // first key, which matches the most entries).
    let mut list = fresh_list(&source);
    list.sort(SortKey::Name, Direction::Ascending);
    let mut filter = Vec::with_capacity(RUNS * TYPED.len());
    let mut matches = Vec::new();
    for _ in 0..RUNS {
        matches.clear();
        for k in 1..=TYPED.len() {
            let start = Instant::now();
            let hits = list.filter(&TYPED[..k]);
            filter.push(start.elapsed());
            matches.push(hits.len());
        }
    }

    let (extend_med, extend_p95) = median_p95(&extend);
    let (sort_med, sort_p95) = median_p95(&sort);
    let (filter_med, filter_p95) = median_p95(&filter);
    let filter_max = filter
        .iter()
        .max()
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
    let pass = sort_p95 <= TARGET_SORT_MS && filter_p95 <= TARGET_FILTER_MS;

    println!("FileList with {n} entries (5% folders), {RUNS} runs, release");
    println!("  append            {extend_med:>8.2} ms  p95 {extend_p95:>8.2} ms   (informative)");
    println!(
        "  sort by name      {sort_med:>8.2} ms  p95 {sort_p95:>8.2} ms   target ≤ {TARGET_SORT_MS} ms"
    );
    println!(
        "  filter per key    {filter_med:>8.2} ms  p95 {filter_p95:>8.2} ms   target ≤ {TARGET_FILTER_MS} ms (max {filter_max:.2} ms; typed {TYPED:?}, matches {matches:?})"
    );
    println!("  {}", if pass { "pass" } else { "FAIL" });

    let date = utc_date(SystemTime::now());
    let json = format!(
        concat!(
            "{{\n",
            "  \"id\": \"T0.4-list\",\n",
            "  \"items\": {n},\n",
            "  \"runs\": {runs},\n",
            "  \"append_ms\": {{ \"median\": {em:.3}, \"p95\": {ep:.3} }},\n",
            "  \"sort_by_name_ms\": {{ \"median\": {sm:.3}, \"p95\": {sp:.3}, \"target\": {st} }},\n",
            "  \"filter_per_key_ms\": {{ \"median\": {fm:.3}, \"p95\": {fp:.3}, \"max\": {fx:.3}, \"target\": {ft}, \"typed\": {typed} }},\n",
            "  \"pass\": {pass},\n",
            "  \"machine\": {machine},\n",
            "  \"commit\": {commit},\n",
            "  \"date\": {date}\n",
            "}}\n"
        ),
        n = n,
        runs = RUNS,
        em = extend_med,
        ep = extend_p95,
        sm = sort_med,
        sp = sort_p95,
        st = TARGET_SORT_MS,
        fm = filter_med,
        fp = filter_p95,
        fx = filter_max,
        ft = TARGET_FILTER_MS,
        typed = json_string(TYPED),
        pass = pass,
        machine = json_string(&machine()),
        commit = json_string(commit()),
        date = json_string(&date),
    );
    std::fs::write(out_dir.join(format!("{date}-T0.4-list.json")), json)?;
    println!("results in {}", out_dir.display());
    Ok(())
}
