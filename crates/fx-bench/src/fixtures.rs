//! Fixture folders from docs/PERFORMANCE.md, generated under `%TEMP%\fx-fixtures`.
//!
//! Safety (docs/SAFETY.md): this module only ever deletes `<root>` or
//! `<root>/<fixture>`, and `<root>` must pass [`guard_fixtures_root`].

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crate::names::{flat_name, long_dir_name, unicode_name};
use crate::png::thumbnail_64;
use crate::rng::{FIXTURE_SEED, Rng};

/// The only directory name this module will ever delete.
pub const ROOT_NAME: &str = "fx-fixtures";

/// Bumped when the content of any fixture changes, so stale fixtures are rebuilt.
const FORMAT_VERSION: u32 = 1;

/// Sizes of every fixture. [`Spec::FULL`] is the one in docs/PERFORMANCE.md; tests
/// use smaller ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub flat_files: usize,
    pub media_files: usize,
    pub deep_levels: usize,
    pub deep_files: usize,
    pub unicode_files: usize,
    pub long_path_files: usize,
}

impl Spec {
    pub const FULL: Spec = Spec {
        flat_files: 100_000,
        media_files: 10_000,
        deep_levels: 20,
        deep_files: 50_000,
        unicode_files: 1_000,
        long_path_files: 100,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Created,
    AlreadyComplete,
}

#[derive(Debug)]
pub struct Report {
    pub name: &'static str,
    pub outcome: Outcome,
    pub files: usize,
    pub elapsed: Duration,
}

/// `%TEMP%\fx-fixtures` of the running (Windows) process.
pub fn default_root() -> PathBuf {
    std::env::temp_dir().join(ROOT_NAME)
}

/// Accepts `path` only if it is absolute and its last component is exactly
/// `fx-fixtures`. Every deletion in this module goes through this check.
pub fn guard_fixtures_root(path: &Path) -> io::Result<&Path> {
    let ok = path.is_absolute() && path.file_name().is_some_and(|name| name == ROOT_NAME);
    if ok {
        Ok(path)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "refusing to touch {}: only an absolute path ending in `{ROOT_NAME}` is allowed",
                path.display()
            ),
        ))
    }
}

/// Deletes the whole fixtures root. Returns `false` if it did not exist.
pub fn clean(root: &Path) -> io::Result<bool> {
    let root = guard_fixtures_root(root)?;
    match fs::remove_dir_all(root) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

struct Fixture {
    name: &'static str,
    expected_files: fn(&Spec) -> usize,
    build: fn(&Path, &Spec) -> io::Result<()>,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "flat-100k",
        expected_files: |s| s.flat_files,
        build: build_flat,
    },
    Fixture {
        name: "flat-10k-media",
        expected_files: |s| s.media_files,
        build: build_media,
    },
    Fixture {
        name: "deep-tree",
        expected_files: |s| s.deep_files,
        build: build_deep,
    },
    Fixture {
        name: "unicode-long",
        expected_files: |s| s.unicode_files + s.long_path_files,
        build: build_unicode_long,
    },
];

/// Creates every fixture that is missing or incomplete. A fixture is complete when
/// its marker file `<root>/.complete-<name>` matches the current format and spec;
/// markers live in the root so they never add an entry to the fixture folders.
pub fn generate(root: &Path, spec: &Spec) -> io::Result<Vec<Report>> {
    let root = guard_fixtures_root(root)?;
    fs::create_dir_all(root)?;
    let mut reports = Vec::with_capacity(FIXTURES.len());
    for fixture in FIXTURES {
        let start = Instant::now();
        let files = (fixture.expected_files)(spec);
        let marker = root.join(format!(".complete-{}", fixture.name));
        let signature = format!("v{FORMAT_VERSION} {spec:?}\n");

        let outcome = if fs::read_to_string(&marker).is_ok_and(|s| s == signature) {
            Outcome::AlreadyComplete
        } else {
            // Incomplete or stale: rebuild from scratch. The marker goes first so a
            // crash mid-rebuild can never leave a marker next to partial content.
            remove_if_exists(&marker, false)?;
            let dir = root.join(fixture.name);
            remove_if_exists(&dir, true)?;
            fs::create_dir(&dir)?;
            (fixture.build)(&dir, spec)?;
            fs::write(&marker, signature)?;
            Outcome::Created
        };
        reports.push(Report {
            name: fixture.name,
            outcome,
            files,
            elapsed: start.elapsed(),
        });
    }
    Ok(reports)
}

/// Only called with `<guarded root>/<fixed name>`.
fn remove_if_exists(path: &Path, dir: bool) -> io::Result<()> {
    let result = if dir {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match result {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Runs `job(i)` for `i in 0..n` on all cores. Each item must be independent
/// (derive its RNG from `i`), so the result does not depend on scheduling.
fn par_for(n: usize, job: impl Fn(usize) -> io::Result<()> + Sync) -> io::Result<()> {
    let threads = thread::available_parallelism()
        .map_or(4, |p| p.get())
        .min(16);
    let chunk = n.div_ceil(threads).max(1);
    thread::scope(|s| {
        let handles: Vec<_> = (0..n)
            .step_by(chunk)
            .map(|start| {
                let job = &job;
                s.spawn(move || (start..(start + chunk).min(n)).try_for_each(job))
            })
            .collect();
        handles.into_iter().try_for_each(|h| {
            h.join()
                .map_err(|_| io::Error::other("fixture worker thread panicked"))?
        })
    })
}

/// Per-fixture, per-item RNG.
pub(crate) fn item_rng(fixture: u64, index: usize) -> Rng {
    Rng::derive(
        FIXTURE_SEED ^ fixture.wrapping_mul(0xA24B_AED4_963E_E407),
        index as u64,
    )
}

fn build_flat(dir: &Path, spec: &Spec) -> io::Result<()> {
    par_for(spec.flat_files, |i| {
        fs::File::create(dir.join(flat_name(&mut item_rng(1, i), i))).map(drop)
    })
}

fn build_media(dir: &Path, spec: &Spec) -> io::Result<()> {
    par_for(spec.media_files, |i| {
        let png = thumbnail_64(item_rng(2, i).next_u64());
        fs::write(dir.join(format!("img-{i:05}.png")), png)
    })
}

/// A spine of `deep_levels` nested folders (`level-01/level-02/...`); next to each
/// `level-NN` there are 4 sibling folders at the same depth. Files are spread evenly over levels and folders.
fn build_deep(dir: &Path, spec: &Spec) -> io::Result<()> {
    const SIDES: usize = 4;
    let levels = spec.deep_levels.max(1);
    let mut level_dirs: Vec<Vec<PathBuf>> = Vec::with_capacity(levels);
    let mut spine = dir.to_path_buf();
    for level in 1..=levels {
        // Side folders are siblings of `level-NN`, so they sit at the same depth.
        let parent = spine.clone();
        spine.push(format!("level-{level:02}"));
        fs::create_dir(&spine)?;
        let mut here = vec![spine.clone()];
        for side in 1..=SIDES {
            let side_dir = parent.join(format!("side-{level:02}-{side}"));
            fs::create_dir(&side_dir)?;
            here.push(side_dir);
        }
        level_dirs.push(here);
    }
    let per_level = spec.deep_files.div_ceil(levels).max(1);
    par_for(spec.deep_files, |i| {
        let folders = &level_dirs[(i / per_level).min(levels - 1)];
        let folder = &folders[i % folders.len()];
        fs::File::create(folder.join(flat_name(&mut item_rng(3, i), i))).map(drop)
    })
}

/// Unicode names in the top folder, plus a chain of 3 folders with ~110-unit
/// names, so files at the bottom have paths well over 260 characters.
fn build_unicode_long(dir: &Path, spec: &Spec) -> io::Result<()> {
    par_for(spec.unicode_files, |i| {
        fs::File::create(dir.join(unicode_name(&mut item_rng(4, i), i))).map(drop)
    })?;
    let mut rng = item_rng(5, 0);
    let mut deep = dir.to_path_buf();
    for level in 0..3 {
        deep.push(long_dir_name(&mut rng, level, 110));
    }
    // std turns paths over MAX_PATH into `\\?\` paths on Windows.
    fs::create_dir_all(&deep)?;
    par_for(spec.long_path_files, |i| {
        fs::File::create(deep.join(unicode_name(&mut item_rng(6, i), i))).map(drop)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: Spec = Spec {
        flat_files: 300,
        media_files: 20,
        deep_levels: 20,
        deep_files: 500,
        unicode_files: 50,
        long_path_files: 10,
    };

    /// Relative paths of every file and folder under `root` (markers excluded), sorted.
    fn listing(root: &Path) -> Vec<String> {
        fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                if rel.starts_with(".complete-") {
                    continue;
                }
                out.push(rel);
                if path.is_dir() {
                    walk(base, &path, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out.sort();
        out
    }

    fn count_files(dir: &Path) -> usize {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .map(|p| if p.is_dir() { count_files(&p) } else { 1 })
            .sum()
    }

    fn max_depth(dir: &Path) -> usize {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .map(|p| 1 + max_depth(&p))
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn guard_refuses_anything_not_ending_in_fx_fixtures() {
        let tmp = std::env::temp_dir();
        for bad in [
            tmp.clone(),
            tmp.join("fx-fixtures-old"),
            tmp.join("my-fx-fixtures"),
            tmp.join("fx-fixtures").join("flat-100k"),
            tmp.join("FX-FIXTURES2"),
            PathBuf::from("fx-fixtures"), // relative
            PathBuf::from(""),
            PathBuf::from(r"C:\"),
            PathBuf::from(r"C:\Users"),
        ] {
            assert!(guard_fixtures_root(&bad).is_err(), "accepted {bad:?}");
            assert!(clean(&bad).is_err(), "clean accepted {bad:?}");
            assert!(generate(&bad, &SMALL).is_err(), "generate accepted {bad:?}");
        }
        assert!(guard_fixtures_root(&tmp.join("fx-fixtures")).is_ok());
    }

    #[test]
    fn clean_refuses_and_leaves_other_folders_intact() {
        let tmp = tempfile::tempdir().unwrap();
        let other = tmp.path().join("precious");
        fs::create_dir(&other).unwrap();
        fs::write(other.join("keep.txt"), b"x").unwrap();
        assert!(clean(&other).is_err());
        assert!(other.join("keep.txt").exists());
    }

    #[test]
    fn generation_is_deterministic_idempotent_and_matches_spec() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let root_a = a.path().join(ROOT_NAME);
        let root_b = b.path().join(ROOT_NAME);

        let first = generate(&root_a, &SMALL).unwrap();
        assert!(first.iter().all(|r| r.outcome == Outcome::Created));
        generate(&root_b, &SMALL).unwrap();
        let listing_a = listing(&root_a);
        assert_eq!(
            listing_a,
            listing(&root_b),
            "same seed must give same names"
        );

        // Second run on a complete root does not recreate anything.
        let probe = root_a.join("flat-100k").join("probe-not-from-generator");
        fs::write(&probe, b"").unwrap();
        let second = generate(&root_a, &SMALL).unwrap();
        assert!(second.iter().all(|r| r.outcome == Outcome::AlreadyComplete));
        assert!(probe.exists(), "complete fixture was recreated");
        fs::remove_file(&probe).unwrap();

        // Sizes match the spec.
        assert_eq!(count_files(&root_a.join("flat-100k")), SMALL.flat_files);
        assert_eq!(
            count_files(&root_a.join("flat-10k-media")),
            SMALL.media_files
        );
        assert_eq!(count_files(&root_a.join("deep-tree")), SMALL.deep_files);
        assert_eq!(max_depth(&root_a.join("deep-tree")), SMALL.deep_levels);
        assert_eq!(
            count_files(&root_a.join("unicode-long")),
            SMALL.unicode_files + SMALL.long_path_files
        );
        let longest = listing_a
            .iter()
            .filter(|p| p.starts_with("unicode-long"))
            .map(|p| p.encode_utf16().count())
            .max()
            .unwrap();
        assert!(longest > 260, "longest relative path has {longest} units");
        assert!(
            listing_a
                .iter()
                .any(|p| p.contains("日本語") || p.contains("中文"))
        );
    }

    #[test]
    fn incomplete_fixture_is_rebuilt() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(ROOT_NAME);
        generate(&root, &SMALL).unwrap();
        let expected = listing(&root);

        // Simulate an interrupted run: marker gone, content partially deleted.
        fs::remove_file(root.join(".complete-deep-tree")).unwrap();
        fs::remove_dir_all(root.join("deep-tree").join("level-01").join("level-02")).unwrap();
        let reports = generate(&root, &SMALL).unwrap();
        let deep = reports.iter().find(|r| r.name == "deep-tree").unwrap();
        assert_eq!(deep.outcome, Outcome::Created);
        assert_eq!(listing(&root), expected);

        // A different spec (stale marker) also triggers a rebuild.
        let bigger = Spec {
            flat_files: SMALL.flat_files + 1,
            ..SMALL
        };
        let reports = generate(&root, &bigger).unwrap();
        assert!(reports.iter().all(|r| r.outcome == Outcome::Created));
        assert_eq!(count_files(&root.join("flat-100k")), bigger.flat_files);
    }

    #[test]
    fn clean_removes_root_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(ROOT_NAME);
        let sibling = tmp.path().join("sibling");
        fs::create_dir(&sibling).unwrap();
        generate(&root, &SMALL).unwrap();
        assert!(clean(&root).unwrap());
        assert!(!root.exists());
        assert!(sibling.exists());
        assert!(!clean(&root).unwrap());
    }
}
