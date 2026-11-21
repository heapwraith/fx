//! Engine tests (task T1.11). Everything happens inside `tempfile` folders.

use super::*;
use std::sync::mpsc::Receiver;

const WAIT: Duration = Duration::from_secs(30);

fn engine() -> (Engine, Receiver<Event>) {
    let (tx, rx) = channel();
    (Engine::start(tx).unwrap(), rx)
}

/// Events of `id` until it finishes.
fn run(engine: &Engine, rx: &Receiver<Event>, job: Job) -> (Vec<Event>, Outcome, Report) {
    let id = engine.submit(job);
    collect(rx, id)
}

fn collect(rx: &Receiver<Event>, id: JobId) -> (Vec<Event>, Outcome, Report) {
    let mut seen = Vec::new();
    loop {
        let e = rx.recv_timeout(WAIT).expect("job did not finish");
        if let Event::Finished {
            job,
            outcome,
            report,
        } = &e
            && *job == id
        {
            return (seen, outcome.clone(), report.clone());
        }
        seen.push(e);
    }
}

fn write(p: &Path, content: &[u8]) {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(p, content).unwrap();
}

/// No temporary file of ours anywhere under `dir`.
fn assert_no_temp(dir: &Path) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        assert!(!name.contains(".fx-"), "temporary file left: {name}");
        if e.file_type().unwrap().is_dir() {
            assert_no_temp(&e.path());
        }
    }
}

#[test]
fn keep_both_names() {
    let taken = |p: &Path| {
        let n = p.file_name().unwrap().to_string_lossy();
        n == "a (2).txt" || n == "pasta (2)"
    };
    let d = Path::new(r"C:\x");
    assert_eq!(keep_both_name(d, "a.txt", taken), d.join("a (3).txt"));
    assert_eq!(keep_both_name(d, "pasta", taken), d.join("pasta (3)"));
    assert_eq!(
        keep_both_name(d, ".gitignore", |_| false),
        d.join(".gitignore (2)")
    );
    assert_eq!(
        keep_both_name(d, "b.tar.gz", |_| false),
        d.join("b.tar (2).gz")
    );
}

#[test]
fn volume_keys() {
    assert_eq!(volume_key(Path::new(r"c:\a\b")), "C:");
    assert_eq!(
        volume_key(Path::new(r"\\server\share\x")),
        r"\\SERVER\SHARE"
    );
}

#[test]
fn copies_unicode_names_empty_files_and_trees() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("origem");
    write(&src.join("relatório 日本.txt"), "conteúdo".as_bytes());
    write(&src.join("vazio.bin"), b"");
    write(&src.join("sub").join("fundo.txt"), b"deep");
    let dest = t.path().join("destino");
    std::fs::create_dir(&dest).unwrap();
    let (e, rx) = engine();
    let (events, outcome, report) = run(&e, &rx, Job::copy(vec![src.clone()], dest.clone()));
    assert_eq!(outcome, Outcome::Done);
    let out = dest.join("origem");
    assert_eq!(
        std::fs::read(out.join("relatório 日本.txt")).unwrap(),
        "conteúdo".as_bytes()
    );
    assert_eq!(std::fs::metadata(out.join("vazio.bin")).unwrap().len(), 0);
    assert_eq!(
        std::fs::read(out.join("sub").join("fundo.txt")).unwrap(),
        b"deep"
    );
    assert!(
        src.join("sub").join("fundo.txt").exists(),
        "copy leaves the source"
    );
    assert_eq!(report.copied, [(src, out)]);
    let bytes = ("conteúdo".len() + "deep".len()) as u64;
    assert!(matches!(events[0], Event::Started { files: 3, bytes: b, .. } if b == bytes));
    assert_no_temp(&dest);
}

#[test]
fn paths_longer_than_260_characters() {
    let t = tempfile::tempdir().unwrap();
    let mut deep = t.path().join("longo");
    while deep.as_os_str().len() < 300 {
        deep = deep.join("pasta-com-nome-bem-comprido-para-passar-do-limite");
    }
    let file = deep.join("arquivo final.txt");
    write(&file, b"longe");
    let dest = t.path().join("destino");
    std::fs::create_dir(&dest).unwrap();
    let (e, rx) = engine();
    let (_, outcome, _) = run(
        &e,
        &rx,
        Job::copy(vec![t.path().join("longo")], dest.clone()),
    );
    assert_eq!(outcome, Outcome::Done);
    let rel = file.strip_prefix(t.path()).unwrap();
    let copied = dest.join(rel);
    assert!(copied.as_os_str().len() > 260);
    assert_eq!(std::fs::read(copied).unwrap(), b"longe");
}

#[test]
fn a_conflict_never_overwrites() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("a.txt");
    write(&src, b"new");
    let dest = t.path().join("destino");
    write(&dest.join("a.txt"), b"old");
    write(&dest.join("a (2).txt"), b"older");
    let (e, rx) = engine();
    let (events, outcome, report) = run(&e, &rx, Job::copy(vec![src.clone()], dest.clone()));
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), b"old");
    assert_eq!(std::fs::read(dest.join("a (2).txt")).unwrap(), b"older");
    assert_eq!(std::fs::read(dest.join("a (3).txt")).unwrap(), b"new");
    let conflict = events.iter().find_map(|e| match e {
        Event::Conflict {
            resolution,
            placed_at,
            ..
        } => Some((*resolution, placed_at.clone())),
        _ => None,
    });
    assert_eq!(
        conflict,
        Some((Resolution::KeepBoth, Some(dest.join("a (3).txt"))))
    );
    assert!(report.replaced.is_empty());
}

#[test]
fn skip_and_replace_only_on_request() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("a.txt");
    write(&src, b"new");
    let dest = t.path().join("destino");
    write(&dest.join("a.txt"), b"old");
    let (e, rx) = engine();
    let job = Job::copy(vec![src.clone()], dest.clone()).on_conflict(Resolution::Skip);
    let (_, _, report) = run(&e, &rx, job);
    assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), b"old");
    assert_eq!(report.skipped, std::slice::from_ref(&src));
    let job = Job::copy(vec![src.clone()], dest.clone()).on_conflict(Resolution::Replace);
    let (_, outcome, report) = run(&e, &rx, job);
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(std::fs::read(dest.join("a.txt")).unwrap(), b"new");
    assert_eq!(report.replaced, [dest.join("a.txt")]);
    assert_no_temp(&dest);
}

/// Cancelling in the middle of a big file: no temporary file, no partial
/// destination, the source byte-for-byte intact.
#[test]
fn cancel_mid_copy_leaves_no_temp_and_keeps_the_source() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("grande.bin");
    let content: Vec<u8> = (0..64 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    write(&src, &content);
    let dest = t.path().join("destino");
    std::fs::create_dir(&dest).unwrap();
    let (e, rx) = engine();
    let id = e.submit(Job::copy(vec![src.clone()], dest.clone()));
    // Cancel as soon as the copy is under way.
    loop {
        match rx.recv_timeout(WAIT).unwrap() {
            Event::Progress { job, .. } | Event::Started { job, .. } if job == id => {
                e.cancel(id);
                break;
            }
            _ => {}
        }
    }
    let (_, outcome, report) = collect(&rx, id);
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(report.copied.is_empty());
    assert!(!dest.join("grande.bin").exists());
    assert_no_temp(&dest);
    assert_eq!(std::fs::read(&src).unwrap(), content, "source intact");
}

#[test]
fn move_on_the_same_volume_is_a_rename() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("pasta");
    write(&src.join("x.txt"), b"x");
    let dest = t.path().join("destino");
    std::fs::create_dir(&dest).unwrap();
    let (e, rx) = engine();
    let (_, outcome, report) = run(&e, &rx, Job::move_to(vec![src.clone()], dest.clone()));
    assert_eq!(outcome, Outcome::Done);
    assert!(!src.exists());
    assert_eq!(
        std::fs::read(dest.join("pasta").join("x.txt")).unwrap(),
        b"x"
    );
    assert_eq!(report.moved, [(src, dest.join("pasta"))]);
}

/// Across volumes a move is copy, verify, delete. Simulated on one volume.
#[test]
fn move_across_volumes_deletes_the_source_only_after_the_copy() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("pasta");
    write(&src.join("a.txt"), b"aaa");
    write(&src.join("b").join("c.txt"), b"ccc");
    let dest = t.path().join("destino");
    std::fs::create_dir(&dest).unwrap();
    let (e, rx) = engine();
    let mut job = Job::move_to(vec![src.clone()], dest.clone());
    job.force_copy = true;
    let (_, outcome, report) = run(&e, &rx, job);
    assert_eq!(outcome, Outcome::Done);
    assert!(!src.exists(), "source removed after the verified copy");
    assert_eq!(
        std::fs::read(dest.join("pasta").join("b").join("c.txt")).unwrap(),
        b"ccc"
    );
    assert_eq!(report.moved.len(), 1);
    assert_no_temp(&dest);
}

#[test]
fn cancelled_cross_volume_move_keeps_the_source() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("grande.bin");
    let content = vec![7u8; 48 * 1024 * 1024];
    write(&src, &content);
    let dest = t.path().join("destino");
    std::fs::create_dir(&dest).unwrap();
    let (e, rx) = engine();
    let mut job = Job::move_to(vec![src.clone()], dest.clone());
    job.force_copy = true;
    let id = e.submit(job);
    loop {
        if let Event::Started { job, .. } = rx.recv_timeout(WAIT).unwrap()
            && job == id
        {
            e.cancel(id);
            break;
        }
    }
    let (_, outcome, _) = collect(&rx, id);
    assert_eq!(outcome, Outcome::Cancelled);
    assert_eq!(std::fs::read(&src).unwrap(), content, "source intact");
    assert!(!dest.join("grande.bin").exists());
    assert_no_temp(&dest);
}

#[test]
fn a_folder_cannot_go_inside_itself() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("pasta");
    write(&src.join("x.txt"), b"x");
    let inside = src.join("sub");
    std::fs::create_dir(&inside).unwrap();
    let (e, rx) = engine();
    let (_, outcome, _) = run(&e, &rx, Job::copy(vec![src.clone()], inside.clone()));
    assert!(matches!(outcome, Outcome::Failed(_)));
    assert!(!inside.join("pasta").exists());
}

#[test]
fn jobs_to_the_same_volume_run_in_order() {
    let t = tempfile::tempdir().unwrap();
    let dest = t.path().join("destino");
    std::fs::create_dir(&dest).unwrap();
    let (e, rx) = engine();
    let ids: Vec<JobId> = (0..3)
        .map(|i| {
            let src = t.path().join(format!("f{i}.bin"));
            write(&src, &vec![1u8; 2 * 1024 * 1024]);
            e.submit(Job::copy(vec![src], dest.clone()))
        })
        .collect();
    let mut order = Vec::new();
    while order.len() < 6 {
        match rx.recv_timeout(WAIT).unwrap() {
            Event::Started { job, .. } => order.push(("start", job)),
            Event::Finished { job, .. } => order.push(("end", job)),
            _ => {}
        }
    }
    let expected: Vec<_> = ids
        .iter()
        .flat_map(|&id| [("start", id), ("end", id)])
        .collect();
    assert_eq!(order, expected, "one job at a time on one disk");
}
