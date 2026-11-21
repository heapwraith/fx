//! `fx-bench search` (task T0.8): fuzzy search latency per keystroke (P5).
//!
//! The index comes from `--volume C:` (needs Administrator) or `--synthetic N`.
//! By default a fixed list of queries (1 to 12 characters) is typed one key at a
//! time and every keystroke's search is timed. `--interactive` reads queries from
//! stdin and prints, for each key, the time and the first 10 results.

use std::io::BufRead;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime};

use fx_index::{Hit, Index, search};

use crate::index_bench::synthetic_records;
use crate::report::{commit, json_string, machine, median_p95, results_dir, utc_date};

const TARGET_P5_MS: f64 = 50.0;

/// Typed one character at a time; each prefix is one keystroke (1 to 12 chars).
pub const QUERIES: &[&str] = &[
    "a",
    "readme",
    "report-1",
    "img",
    "setup.exe",
    "kernel32.dll",
    "relatorio",
    "photo",
    "notes",
    "😀",
    "x86_64-pc-wi",
    "budget final",
];

pub enum Source {
    Volume(String),
    Synthetic(usize),
}

fn load(source: &Source) -> std::io::Result<(Index, String)> {
    let start = Instant::now();
    let (index, label) = match source {
        Source::Volume(v) => (fx_index::read_volume(v)?, format!("volume {v}")),
        Source::Synthetic(n) => {
            let records = synthetic_records(*n);
            let mut index = Index::with_capacity(5, records.len());
            for r in &records {
                index.insert(r).map_err(std::io::Error::other)?;
            }
            (index, format!("{n} synthetic files"))
        }
    };
    eprintln!(
        "index of {label}: {} files in {:.2} s",
        index.len(),
        start.elapsed().as_secs_f64()
    );
    Ok((index, label))
}

fn timed(index: &Index, query: &str) -> (Duration, Vec<Hit>) {
    let start = Instant::now();
    let hits = search(index, query, &AtomicBool::new(false), None).unwrap_or_default();
    (start.elapsed(), hits)
}

fn prefixes(q: &str) -> impl Iterator<Item = &str> {
    q.char_indices().map(|(i, c)| &q[..i + c.len_utf8()])
}

pub fn run(source: &Source, interactive: bool) -> std::io::Result<()> {
    let (index, label) = load(source)?;
    if interactive {
        return interactive_loop(&index);
    }
    // Warm-up: touch the whole index once so the first query is not penalized
    // by page faults.
    let _ = timed(&index, "zzzz");

    let mut samples = Vec::new();
    println!(
        "P5: fuzzy search per keystroke on {label} ({} files), release",
        index.len()
    );
    for q in QUERIES {
        let mut worst = Duration::ZERO;
        let mut last = Vec::new();
        for p in prefixes(q) {
            let (t, hits) = timed(&index, p);
            worst = worst.max(t);
            samples.push(t);
            last = hits;
        }
        let top: Vec<&str> = last.iter().take(3).map(|h| index.name(h.slot)).collect();
        println!(
            "  {:<14} {:>2} keys  worst {:>6.2} ms  {} hits  top: {top:?}",
            format!("{q:?}"),
            prefixes(q).count(),
            worst.as_secs_f64() * 1000.0,
            last.len()
        );
    }
    let (median, p95) = median_p95(&samples);
    let max = samples
        .iter()
        .max()
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
    let real = matches!(source, Source::Volume(_));
    let pass = p95 <= TARGET_P5_MS;
    println!(
        "  {} keystrokes: median {median:.2} ms  p95 {p95:.2} ms  max {max:.2} ms   target ≤ {TARGET_P5_MS} ms  {}{}",
        samples.len(),
        if pass { "pass" } else { "FAIL" },
        if real {
            ""
        } else {
            "  (synthetic index: P5 needs the real C: index)"
        }
    );

    let date = utc_date(SystemTime::now());
    let id = if real {
        "T0.8-P5-volume"
    } else {
        "T0.8-P5-synthetic"
    };
    let json = format!(
        concat!(
            "{{\n  \"id\": \"{id}\",\n  \"source\": {source},\n  \"files\": {files},\n",
            "  \"keystrokes\": {n},\n  \"latency_ms\": {{ \"median\": {m:.3}, \"p95\": {p:.3}, \"max\": {x:.3}, \"target\": {t} }},\n",
            "  \"p5_applies\": {real},\n  \"pass\": {pass},\n",
            "  \"machine\": {machine},\n  \"commit\": {commit},\n  \"date\": {date}\n}}\n"
        ),
        id = id,
        source = json_string(&label),
        files = index.len(),
        n = samples.len(),
        m = median,
        p = p95,
        x = max,
        t = TARGET_P5_MS,
        real = real,
        pass = pass,
        machine = json_string(&machine()),
        commit = json_string(commit()),
        date = json_string(&date),
    );
    std::fs::write(results_dir()?.join(format!("{date}-{id}.json")), json)
}

fn interactive_loop(index: &Index) -> std::io::Result<()> {
    eprintln!(
        "type a query and press Enter; each prefix is searched as one keystroke (empty line quits)"
    );
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.is_empty() {
            return Ok(());
        }
        for p in prefixes(&line) {
            let (t, hits) = timed(index, p);
            println!(
                "{p:?}: {:.2} ms, {} hits",
                t.as_secs_f64() * 1000.0,
                hits.len()
            );
            for h in hits.iter().take(10) {
                let path = index.path(h.slot).map_or_else(
                    || index.name(h.slot).to_owned(),
                    |p| p.display().to_string(),
                );
                println!("    {:>5}  {path}", h.score);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_are_1_to_12_characters_and_prefixes_cover_them() {
        for q in QUERIES {
            let n = q.chars().count();
            assert!((1..=12).contains(&n), "{q:?} has {n} chars");
            let p: Vec<&str> = prefixes(q).collect();
            assert_eq!(p.len(), n);
            assert_eq!(p.last(), Some(q));
        }
        assert_eq!(prefixes("😀a").collect::<Vec<_>>(), ["😀", "😀a"]);
    }
}
