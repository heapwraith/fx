//! `fx-bench index` (task T0.7):
//! - `--volume C:`: reads the real MFT (needs Administrator) — P6 and P9.
//! - `--synthetic N`: builds an index of N generated files in memory, without
//!   Administrator — measures index build cost and P9, but not disk reading.

use std::time::{Duration, Instant, SystemTime};

use fx_index::{FileId, Index, Record};

use crate::fixtures::item_rng;
use crate::names::flat_name;
use crate::report::{commit, json_string, machine, results_dir, utc_date};

const TARGET_P6_S: f64 = 10.0;
const TARGET_P9_MB_PER_MILLION: f64 = 150.0;
const ROOT: FileId = 5;

/// A file tree of `n` entries: ~8% folders, each file under a random earlier
/// folder, names from the fixture generator (numbers, Unicode, extensions).
pub fn synthetic_records(n: usize) -> Vec<Record> {
    let mut folders: Vec<FileId> = vec![ROOT];
    (0..n)
        .map(|i| {
            let mut rng = item_rng(7, i);
            let id = 24 + i as FileId;
            let parent = folders[rng.below(folders.len())];
            let is_dir = rng.below(12) == 0;
            if is_dir {
                folders.push(id);
            }
            Record {
                id,
                parent,
                name: flat_name(&mut rng, i).encode_utf16().collect(),
                is_dir,
                size: if is_dir {
                    0
                } else {
                    rng.next_u64() % 50_000_000
                },
                modified: 133_000_000_000_000_000 + rng.next_u64() % 10_000_000_000_000_000,
            }
        })
        .collect()
}

fn build(records: &[Record]) -> std::io::Result<Index> {
    let mut index = Index::with_capacity(ROOT, records.len());
    for r in records {
        index.insert(r).map_err(std::io::Error::other)?;
    }
    index.shrink_to_fit();
    Ok(index)
}

struct Measured {
    source: String,
    files: usize,
    elapsed: Duration,
    heap_bytes: usize,
}

fn report(m: &Measured, real_volume: bool) -> std::io::Result<()> {
    let secs = m.elapsed.as_secs_f64();
    let mb = m.heap_bytes as f64 / (1024.0 * 1024.0);
    let mb_per_million = mb / (m.files as f64 / 1_000_000.0);
    let p6_pass = secs <= TARGET_P6_S;
    let p9_pass = mb_per_million <= TARGET_P9_MB_PER_MILLION;
    let verdict = |ok: bool| if ok { "pass" } else { "FAIL" };
    println!("Index of {} (release)", m.source);
    println!("  files indexed   {}", m.files);
    println!(
        "  time            {secs:.2} s{}",
        if real_volume {
            format!("   P6 target ≤ {TARGET_P6_S} s: {}", verdict(p6_pass))
        } else {
            "   (in-memory build only, no disk reading: not P6)".to_owned()
        }
    );
    println!(
        "  index memory    {mb:.1} MB = {mb_per_million:.1} MB per million files   P9 target ≤ {TARGET_P9_MB_PER_MILLION}: {}",
        verdict(p9_pass)
    );

    let date = utc_date(SystemTime::now());
    let id = if real_volume {
        "T0.7-index-volume"
    } else {
        "T0.7-index-synthetic"
    };
    let json = format!(
        concat!(
            "{{\n  \"id\": \"{id}\",\n  \"source\": {source},\n  \"files\": {files},\n",
            "  \"seconds\": {secs:.3},\n  \"p6_applies\": {real},\n  \"p6_target_s\": {p6t},\n",
            "  \"heap_mb\": {mb:.1},\n  \"mb_per_million\": {mbm:.1},\n  \"p9_target\": {p9t},\n",
            "  \"p6_pass\": {p6},\n  \"p9_pass\": {p9},\n",
            "  \"machine\": {machine},\n  \"commit\": {commit},\n  \"date\": {date}\n}}\n"
        ),
        id = id,
        source = json_string(&m.source),
        files = m.files,
        secs = secs,
        real = real_volume,
        p6t = TARGET_P6_S,
        mb = mb,
        mbm = mb_per_million,
        p9t = TARGET_P9_MB_PER_MILLION,
        p6 = real_volume && p6_pass,
        p9 = p9_pass,
        machine = json_string(&machine()),
        commit = json_string(commit()),
        date = json_string(&date),
    );
    std::fs::write(results_dir()?.join(format!("{date}-{id}.json")), json)
}

/// `fx-bench index --volume C:` (needs Administrator).
pub fn volume(volume: &str) -> std::io::Result<()> {
    let start = Instant::now();
    let (index, stats) = fx_index::read_volume_with_stats(volume)?;
    println!(
        "  breakdown: load MFT {:.2} s ({} records, {:.0} MB) · parse {:.2} s · merge {:.2} s",
        stats.load.as_secs_f64(),
        stats.records,
        stats.mft_bytes as f64 / (1024.0 * 1024.0),
        stats.parse.as_secs_f64(),
        stats.merge.as_secs_f64()
    );
    report(
        &Measured {
            source: format!("volume {volume}"),
            files: index.len(),
            elapsed: start.elapsed(),
            heap_bytes: index.heap_bytes(),
        },
        true,
    )
}

/// `fx-bench index --synthetic N`.
pub fn synthetic(n: usize) -> std::io::Result<()> {
    let records = synthetic_records(n);
    let start = Instant::now();
    let index = build(&records)?;
    let elapsed = start.elapsed();
    drop(records);
    report(
        &Measured {
            source: format!("{n} synthetic files"),
            files: index.len(),
            elapsed,
            heap_bytes: index.heap_bytes(),
        },
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_tree_is_deterministic_and_has_paths() {
        let a = synthetic_records(2_000);
        assert_eq!(a, synthetic_records(2_000));
        let index = build(&a).unwrap();
        assert_eq!(index.len(), 2_000);
        let folders = a.iter().filter(|r| r.is_dir).count();
        assert!((80..=250).contains(&folders), "{folders} folders");
        // Every entry hangs off the root through existing folders.
        for slot in 0..index.slots() as u32 {
            assert!(index.path(slot).is_some(), "no path for slot {slot}");
        }
    }
}
